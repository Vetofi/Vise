use crate::{
    Error, Result,
    buf::{BitSet, Bytes, Codec, Longs, Reader, VarInt, Writer},
    packet,
};
use vise_world::nbt::Tag;

pub const BLOCKS: usize = 4096;
pub const BIOMES: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    Blocks,
    Biomes,
}
impl Strategy {
    fn entries(self) -> usize {
        match self {
            Self::Blocks => BLOCKS,
            Self::Biomes => BIOMES,
        }
    }
    fn configuration(self, bits: u8, global_bits: u8) -> (bool, u8) {
        match (self, bits) {
            (_, 0) => (true, 0),
            (Self::Blocks, 1..=4) => (true, 4),
            (Self::Blocks, 5..=8) => (true, bits),
            (Self::Biomes, 1..=3) => (true, bits),
            _ => (false, global_bits.max(bits)),
        }
    }
}

pub fn storage_get(data: &[i64], bits: u8, i: usize) -> u32 {
    if bits == 0 {
        return 0;
    }
    let per = 64 / bits as usize;
    let (cell, offset) = (i / per, (i % per) * bits as usize);
    data.get(cell)
        .map_or(0, |&l| ((l as u64 >> offset) & ((1u64 << bits) - 1)) as u32)
}
pub fn storage_len(bits: u8, entries: usize) -> usize {
    if bits == 0 {
        0
    } else {
        entries.div_ceil(64 / bits as usize)
    }
}

fn pack<T>(values: &[T], bits: u8, id: impl Fn(&T) -> u64) -> Vec<i64> {
    let per = 64 / bits as usize;
    values
        .chunks(per)
        .map(|word| {
            word.iter()
                .enumerate()
                .fold(0u64, |w, (k, v)| w | id(v) << (k * bits as usize)) as i64
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paletted {
    pub bits: u8,
    pub palette: Vec<i32>,
    pub data: Vec<i64>,
}
impl Paletted {
    pub fn read(r: &mut Reader, strategy: Strategy, global_bits: u8) -> Result<Self> {
        let wire = r.u8()?;
        let (has_palette, bits) = strategy.configuration(wire, global_bits);
        if bits > 32 {
            return Err(Error::new(format!("{bits} bits per entry, more than 32")));
        }
        let palette = if !has_palette {
            Vec::new()
        } else if bits == 0 {
            vec![r.varint()?]
        } else {
            let n = r.count(1 << bits)?;
            (0..n).map(|_| r.varint()).collect::<Result<_>>()?
        };
        let data = r.fixed_longs(storage_len(bits, strategy.entries()))?;
        Ok(Self {
            bits,
            palette,
            data,
        })
    }
    pub fn write(&self, w: &mut Writer) {
        w.u8(self.bits);
        if !self.palette.is_empty() {
            if self.bits != 0 {
                w.varint(self.palette.len() as i32);
            }
            for &p in &self.palette {
                w.varint(p);
            }
        }
        for &l in &self.data {
            w.i64(l);
        }
    }
    pub fn get(&self, i: usize) -> i32 {
        if self.bits == 0 {
            return self.palette.first().copied().unwrap_or(0);
        }
        let v = storage_get(&self.data, self.bits, i);
        if self.palette.is_empty() {
            v as i32
        } else {
            self.palette.get(v as usize).copied().unwrap_or(0)
        }
    }
    pub fn from_values(values: &[i32], strategy: Strategy, global_bits: u8) -> Self {
        thread_local! {
            static INDEX: std::cell::RefCell<Vec<u32>> = const { std::cell::RefCell::new(Vec::new()) };
        }
        let max = values.iter().copied().max().unwrap_or(0);
        if values.iter().any(|&v| v < 0) || max >= 1 << 20 {
            return Self::from_values_slow(values, strategy, global_bits);
        }
        let mut palette: Vec<i32> = Vec::new();
        let mut ids = Vec::with_capacity(values.len());
        INDEX.with_borrow_mut(|index| {
            if index.len() <= max as usize {
                index.resize(max as usize + 1, u32::MAX);
            }
            let (mut last, mut last_id) = (-1i32, 0u32);
            for &v in values {
                if v != last {
                    let slot = &mut index[v as usize];
                    if *slot == u32::MAX {
                        *slot = palette.len() as u32;
                        palette.push(v);
                    }
                    (last, last_id) = (v, *slot);
                }
                ids.push(last_id);
            }
            for &v in &palette {
                index[v as usize] = u32::MAX;
            }
        });
        let needed = if palette.len() <= 1 {
            0
        } else {
            (usize::BITS - (palette.len() - 1).leading_zeros()) as u8
        };
        let (has_palette, bits) = strategy.configuration(needed, global_bits);
        if bits == 0 {
            return Self {
                bits: 0,
                palette: vec![palette.first().copied().unwrap_or(0)],
                data: Vec::new(),
            };
        }
        let data = if has_palette {
            pack(&ids, bits, |&id| u64::from(id))
        } else {
            pack(values, bits, |&v| v as u64)
        };
        Self {
            bits,
            palette: if has_palette { palette } else { Vec::new() },
            data,
        }
    }

    fn from_values_slow(values: &[i32], strategy: Strategy, global_bits: u8) -> Self {
        let mut palette: Vec<i32> = Vec::new();
        for &v in values {
            if !palette.contains(&v) {
                palette.push(v);
            }
        }
        let needed = if palette.len() <= 1 {
            0
        } else {
            (usize::BITS - (palette.len() - 1).leading_zeros()) as u8
        };
        let (has_palette, bits) = strategy.configuration(needed, global_bits);
        if bits == 0 {
            return Self {
                bits: 0,
                palette: vec![palette.first().copied().unwrap_or(0)],
                data: Vec::new(),
            };
        }
        let mut data = vec![0i64; storage_len(bits, values.len())];
        let per = 64 / bits as usize;
        for (i, &v) in values.iter().enumerate() {
            let id = if has_palette {
                palette.iter().position(|&p| p == v).unwrap_or(0) as u64
            } else {
                v as u64
            };
            data[i / per] |= (id << ((i % per) * bits as usize)) as i64;
        }
        Self {
            bits,
            palette: if has_palette { palette } else { Vec::new() },
            data,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub non_empty: i16,
    pub fluids: i16,
    pub states: Paletted,
    pub biomes: Paletted,
}
impl Section {
    pub fn read(r: &mut Reader, block_bits: u8, biome_bits: u8) -> Result<Self> {
        Ok(Self {
            non_empty: r.i16()?,
            fluids: r.i16()?,
            states: Paletted::read(r, Strategy::Blocks, block_bits)?,
            biomes: Paletted::read(r, Strategy::Biomes, biome_bits)?,
        })
    }
    pub fn write(&self, w: &mut Writer) {
        w.i16(self.non_empty);
        w.i16(self.fluids);
        self.states.write(w);
        self.biomes.write(w);
    }
}

packet!(
    BlockEntityInfo {
        packed_xz: i8 => crate::buf::Byte,
        y: i16 => crate::buf::Short,
        kind: i32 => VarInt,
        tag: Tag => crate::buf::Nbt,
    }
);
pub struct Heightmaps;
impl Codec<Vec<(i32, Vec<i64>)>> for Heightmaps {
    fn read(r: &mut Reader) -> Result<Vec<(i32, Vec<i64>)>> {
        let n = r.count(16)?;
        (0..n).map(|_| Ok((r.varint()?, r.long_array()?))).collect()
    }
    fn write(v: &Vec<(i32, Vec<i64>)>, w: &mut Writer) {
        w.varint(v.len() as i32);
        for (kind, data) in v {
            w.varint(*kind);
            Longs::write(data, w);
        }
    }
}
packet!(
    ChunkData {
        heightmaps: Vec<(i32, Vec<i64>)> => Heightmaps,
        buffer: Vec<u8> => Bytes,
        block_entities: Vec<BlockEntityInfo> => crate::buf::List<BlockEntityInfo>,
    }
);
impl ChunkData {
    pub fn sections(&self, count: usize, block_bits: u8, biome_bits: u8) -> Result<Vec<Section>> {
        let mut r = Reader::new(&self.buffer);
        (0..count)
            .map(|_| Section::read(&mut r, block_bits, biome_bits))
            .collect()
    }
    pub fn from_sections(sections: &[Section]) -> Vec<u8> {
        let mut w = Writer::new();
        for s in sections {
            s.write(&mut w);
        }
        w.data
    }
}
pub struct Layers;
impl Codec<Vec<Vec<u8>>> for Layers {
    fn read(r: &mut Reader) -> Result<Vec<Vec<u8>>> {
        let n = r.count(r.remaining())?;
        (0..n).map(|_| r.byte_array(2048)).collect()
    }
    fn write(v: &Vec<Vec<u8>>, w: &mut Writer) {
        w.varint(v.len() as i32);
        for l in v {
            w.byte_array(l);
        }
    }
}
packet!(
    LightData {
        sky_mask: Vec<u8> => BitSet,
        block_mask: Vec<u8> => BitSet,
        empty_sky_mask: Vec<u8> => BitSet,
        empty_block_mask: Vec<u8> => BitSet,
        sky: Vec<Vec<u8>> => Layers,
        block: Vec<Vec<u8>> => Layers,
    }
);
impl LightData {
    pub fn layers(&self, light_sections: usize) -> Vec<SectionLight> {
        use crate::buf::bit;
        let (mut sky, mut block) = (self.sky.iter(), self.block.iter());
        (0..light_sections)
            .map(|i| {
                let s = if bit(&self.sky_mask, i) {
                    sky.next().cloned()
                } else if bit(&self.empty_sky_mask, i) {
                    Some(vec![0; 2048])
                } else {
                    None
                };
                let b = if bit(&self.block_mask, i) {
                    block.next().cloned()
                } else if bit(&self.empty_block_mask, i) {
                    Some(vec![0; 2048])
                } else {
                    None
                };
                (s, b)
            })
            .collect()
    }
}
pub type SectionLight = (Option<Vec<u8>>, Option<Vec<u8>>);
pub fn nibble(layer: &[u8], i: usize) -> u8 {
    layer
        .get(i >> 1)
        .map_or(0, |b| if i & 1 == 0 { b & 15 } else { b >> 4 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_values_matches_the_linear_search() {
        let mut seed = 12345u64;
        let mut next = |n: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % n) as i32
        };
        for distinct in [1u64, 2, 16, 17, 200, 300, 5000] {
            let values: Vec<i32> = (0..BLOCKS).map(|_| next(distinct) * 7).collect();
            assert_eq!(
                Paletted::from_values(&values, Strategy::Blocks, 16),
                Paletted::from_values_slow(&values, Strategy::Blocks, 16)
            );
            let biomes: Vec<i32> = values[..BIOMES].to_vec();
            assert_eq!(
                Paletted::from_values(&biomes, Strategy::Biomes, 7),
                Paletted::from_values_slow(&biomes, Strategy::Biomes, 7)
            );
        }
    }

    #[test]
    fn bit_storage_matches_simple_bit_storage() {
        let values: Vec<i32> = (0..BLOCKS as i32).map(|i| i % 20).collect();
        let p = Paletted::from_values(&values, Strategy::Blocks, 16);
        assert_eq!(p.bits, 5);
        assert_eq!(p.data.len(), 4096usize.div_ceil(12));
        for i in [0, 11, 12, 13, 4095] {
            assert_eq!(p.get(i), values[i]);
        }
    }

    #[test]
    fn palettes_follow_the_strategies() {
        let single = Paletted::from_values(&[7; BLOCKS], Strategy::Blocks, 16);
        assert_eq!(
            (single.bits, single.palette.clone(), single.data.len()),
            (0, vec![7], 0)
        );
        let mut two = vec![0; BLOCKS];
        two[100] = 9;
        let p = Paletted::from_values(&two, Strategy::Blocks, 16);
        assert_eq!((p.bits, p.data.len()), (4, 256));
        let mut biomes = vec![3; BIOMES];
        biomes[5] = 4;
        assert_eq!(Paletted::from_values(&biomes, Strategy::Biomes, 7).bits, 1);
        let many: Vec<i32> = (0..BIOMES as i32).map(|i| i % 9).collect();
        let g = Paletted::from_values(&many, Strategy::Biomes, 7);
        assert!(g.palette.is_empty() && g.bits == 7);
        assert_eq!(g.get(10), 1);
        let wide: Vec<i32> = (0..BLOCKS as i32).map(|i| i * 7).collect();
        let g = Paletted::from_values(&wide, Strategy::Blocks, 16);
        assert!(g.palette.is_empty() && g.bits == 16);
        assert_eq!(g.get(4095), 4095 * 7);
    }

    #[test]
    fn wire_bits_wider_than_storage_are_rejected() {
        for wire in [33u8, 200] {
            let data = [wire];
            let mut r = Reader::new(&data);
            assert!(Paletted::read(&mut r, Strategy::Blocks, 16).is_err());
        }
        for (wire, global) in [(15u8, 15u8), (16, 16)] {
            let mut data = vec![wire];
            data.resize(1 + 1024 * 8, 0);
            let mut r = Reader::new(&data);
            let p = Paletted::read(&mut r, Strategy::Blocks, global).unwrap();
            assert_eq!(p.bits, wire);
        }
    }

    #[test]
    fn chunk_data_round_trips() {
        let mut states = vec![0; BLOCKS];
        for (i, s) in states.iter_mut().enumerate().take(256) {
            *s = (i % 3) as i32 + 1;
        }
        let section = Section {
            non_empty: 256,
            fluids: 0,
            states: Paletted::from_values(&states, Strategy::Blocks, 16),
            biomes: Paletted::from_values(&[2; BIOMES], Strategy::Biomes, 7),
        };
        let empty = Section {
            non_empty: 0,
            fluids: 0,
            states: Paletted::from_values(&[0; BLOCKS], Strategy::Blocks, 16),
            biomes: Paletted::from_values(&[2; BIOMES], Strategy::Biomes, 7),
        };
        let data = ChunkData {
            heightmaps: vec![(1, vec![1, 2, 3]), (4, vec![5])],
            buffer: ChunkData::from_sections(&[section.clone(), empty.clone()]),
            block_entities: vec![BlockEntityInfo {
                packed_xz: 0x3f,
                y: -60,
                kind: 2,
                tag: Tag::End,
            }],
        };
        let mut w = Writer::new();
        ChunkData::write(&data, &mut w);
        let back = ChunkData::read(&mut Reader::new(&w.data)).unwrap();
        assert_eq!(back, data);
        let sections = back.sections(2, 16, 7).unwrap();
        assert_eq!(sections, vec![section, empty]);
        assert_eq!(sections[0].states.get(4), 2);
        assert_eq!(sections[1].states.get(4), 0);
    }

    #[test]
    fn light_layers_follow_masks() {
        let mut light = LightData {
            sky_mask: vec![],
            block_mask: vec![],
            empty_sky_mask: vec![],
            empty_block_mask: vec![],
            sky: vec![vec![0xF0; 2048]],
            block: vec![],
        };
        crate::buf::set_bit(&mut light.sky_mask, 3);
        crate::buf::set_bit(&mut light.empty_block_mask, 1);
        let mut w = Writer::new();
        LightData::write(&light, &mut w);
        let back = LightData::read(&mut Reader::new(&w.data)).unwrap();
        assert_eq!(back, light);
        let layers = back.layers(26);
        assert_eq!(layers.len(), 26);
        assert_eq!(nibble(layers[3].0.as_ref().unwrap(), 1), 15);
        assert_eq!(nibble(layers[3].0.as_ref().unwrap(), 0), 0);
        assert_eq!(layers[1].1.as_deref(), Some(&[0u8; 2048][..]));
        assert!(layers[0].0.is_none() && layers[0].1.is_none());
    }
}
