use crate::{
    chunk::{Chunk, ColumnLayout, LevelShape, Light, Section},
    containers::BlockEntity,
    fast_hash::FxHashMap,
    mobs::spawner::{SpawnDataExt, SpawnerExt},
    section_mask::SectionMask,
};
use vise_protocol::{
    buf::{BitSet, Codec, List, Writer, set_bit},
    chunk::{
        BIOMES, BLOCKS, BlockEntityInfo, ChunkData, Heightmaps, LightData, Paletted, Strategy,
    },
    packets::play::{ChunkBiomeData, LevelChunkWithLight},
    registry::{Builtin, ceil_log2},
};
use vise_world::nbt::Tag;
use std::{
    collections::HashMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
};

const NON_AIR: u8 = 1;
const FLUID: u8 = 2;
const ENTITY: u8 = 4;

pub fn block_entity_type(name: &str) -> Option<&'static str> {
    let n = name.strip_prefix("minecraft:")?;
    Some(match n {
        "chest" => "minecraft:chest",
        "trapped_chest" => "minecraft:trapped_chest",
        "ender_chest" => "minecraft:ender_chest",
        "barrel" => "minecraft:barrel",
        "furnace" => "minecraft:furnace",
        "blast_furnace" => "minecraft:blast_furnace",
        "smoker" => "minecraft:smoker",
        "hopper" => "minecraft:hopper",
        "dispenser" => "minecraft:dispenser",
        "dropper" => "minecraft:dropper",
        "brewing_stand" => "minecraft:brewing_stand",
        "beacon" => "minecraft:beacon",
        "bell" => "minecraft:bell",
        "campfire" | "soul_campfire" => "minecraft:campfire",
        "enchanting_table" => "minecraft:enchanting_table",
        "jukebox" => "minecraft:jukebox",
        "lectern" => "minecraft:lectern",
        "spawner" => "minecraft:mob_spawner",
        "comparator" => "minecraft:comparator",
        "daylight_detector" => "minecraft:daylight_detector",
        "conduit" => "minecraft:conduit",
        "decorated_pot" => "minecraft:decorated_pot",
        "moving_piston" => "minecraft:piston",
        "structure_block" => "minecraft:structure_block",
        "jigsaw" => "minecraft:jigsaw",
        "command_block" | "chain_command_block" | "repeating_command_block" => {
            "minecraft:command_block"
        }
        "end_portal" => "minecraft:end_portal",
        "end_gateway" => "minecraft:end_gateway",
        "sculk_sensor" => "minecraft:sculk_sensor",
        "calibrated_sculk_sensor" => "minecraft:calibrated_sculk_sensor",
        "sculk_catalyst" => "minecraft:sculk_catalyst",
        "sculk_shrieker" => "minecraft:sculk_shrieker",
        "crafter" => "minecraft:crafter",
        "potent_sulfur" => "minecraft:potent_sulfur",
        "chiseled_bookshelf" => "minecraft:chiseled_bookshelf",
        "trial_spawner" => "minecraft:trial_spawner",
        "vault" => "minecraft:vault",
        "beehive" | "bee_nest" => "minecraft:beehive",
        "suspicious_sand" | "suspicious_gravel" => "minecraft:brushable_block",
        "creaking_heart" => "minecraft:creaking_heart",
        "test_block" => "minecraft:test_block",
        "test_instance_block" => "minecraft:test_instance_block",
        n if n.ends_with("shulker_box") => "minecraft:shulker_box",
        n if n.ends_with("_wall_hanging_sign") || n.ends_with("_hanging_sign") => {
            "minecraft:hanging_sign"
        }
        n if n.ends_with("_sign") => "minecraft:sign",
        n if n.ends_with("_banner") => "minecraft:banner",
        n if n.ends_with("_head") || n.ends_with("_skull") => "minecraft:skull",
        n if n.ends_with("_shelf") => "minecraft:shelf",
        n if n.ends_with("copper_golem_statue") => "minecraft:copper_golem_statue",
        _ => return None,
    })
}

pub struct ChunkEncoder {
    coder: Arc<SectionCoder>,
    entity_types: Vec<i16>,
    pub block_ids: Vec<i32>,
    pub tables: Option<Arc<crate::save::SaveTables>>,
    cache: std::sync::Mutex<FxHashMap<usize, Encoded>>,
    heights: std::sync::Mutex<HashMap<(i32, i32), Heights>>,
    sections: std::ops::Range<usize>,
    column: ColumnLayout,
    sky: bool,
}

struct SectionCoder {
    flags: Vec<u8>,
    biome_ids: Vec<i32>,
    biome_bits: u8,
}

struct Heights {
    sections: Vec<std::sync::Weak<Section>>,
    maps: ColumnHeightmaps,
}

type ColumnHeightmaps = Arc<Vec<(i32, Vec<i64>)>>;

fn cached_heights(
    heights: &HashMap<(i32, i32), Heights>,
    chunk: &Chunk,
) -> Option<ColumnHeightmaps> {
    let h = heights.get(&(chunk.x, chunk.z))?;
    let same = h.sections.len() == chunk.sections.len()
        && h.sections
            .iter()
            .zip(&chunk.sections)
            .all(|(w, s)| std::ptr::eq(w.as_ptr(), Arc::as_ptr(s)));
    same.then(|| h.maps.clone())
}

fn keep_heights(
    heights: &mut HashMap<(i32, i32), Heights>,
    key: (i32, i32),
    sections: &[Arc<Section>],
    maps: &ColumnHeightmaps,
) {
    if heights.len() >= CACHE_SWEEP / 4 {
        heights.retain(|_, h| h.sections.iter().all(|w| w.strong_count() > 0));
        if heights.len() >= CACHE_SWEEP / 8 {
            heights.clear();
        }
    }
    heights.insert(
        key,
        Heights {
            sections: sections.iter().map(Arc::downgrade).collect(),
            maps: maps.clone(),
        },
    );
}

struct Encoded {
    section: std::sync::Weak<Section>,
    non_air: bool,
    wire: Option<(Arc<[u8]>, Entities)>,
    light: Option<Arc<Layers>>,
}

type Entities = Arc<[(u16, u32)]>;

struct Layers {
    sky: Vec<u8>,
    block: Vec<u8>,
    sky_any: bool,
    block_any: bool,
}
impl Layers {
    fn of(light: &Light) -> Self {
        let (mut sky, mut block) = (vec![0u8; 2048], vec![0u8; 2048]);
        let (sky_any, block_any) = match light {
            Light::Uniform(v) => {
                let (sk, bl) = (v >> 4, v & 15);
                sky.fill(sk | sk << 4);
                block.fill(bl | bl << 4);
                (sk != 0, bl != 0)
            }
            Light::Full(a) => {
                for ((pair, sky), block) in
                    a.as_chunks::<2>().0.iter().zip(&mut sky).zip(&mut block)
                {
                    let [even, odd] = *pair;
                    *sky = even >> 4 | (odd & 0xF0);
                    *block = (even & 15) | odd << 4;
                }
                let all = a.iter().fold(0u8, |acc, &v| acc | v);
                (all & 0xF0 != 0, all & 15 != 0)
            }
        };
        Self {
            sky,
            block,
            sky_any,
            block_any,
        }
    }
}

impl SectionCoder {
    fn wire(&self, section: &Section) -> (Arc<[u8]>, Entities) {
        let (encoded, entities) = self.section(section);
        let mut w = vise_protocol::buf::Writer::new();
        encoded.write(&mut w);
        (w.data.into(), entities.into())
    }

    fn section(&self, section: &Section) -> (vise_protocol::chunk::Section, Vec<(u16, u32)>) {
        let mut values = Vec::with_capacity(BLOCKS);
        let (mut non_empty, mut fluids, mut entities) = (0i16, 0i16, Vec::new());
        let mut count = |state: u32, n: i16| {
            let f = self.flags.get(state as usize).copied().unwrap_or(0);
            if f & NON_AIR != 0 {
                non_empty += n;
                if f & FLUID != 0 {
                    fluids += n;
                }
            }
            f & ENTITY != 0
        };
        let uniform = section.blocks.uniform();
        if let Some(v) = uniform {
            if count(v, BLOCKS as i16) {
                entities = (0..BLOCKS as u16).map(|i| (i, v)).collect();
            }
        } else {
            let mut unpacked = [0u32; BLOCKS];
            section.unpack(&mut unpacked);
            for (i, &v) in unpacked.iter().enumerate() {
                if count(v, 1) {
                    entities.push((i as u16, v));
                }
                values.push(v as i32);
            }
        }
        (
            vise_protocol::chunk::Section {
                non_empty,
                fluids,
                states: match uniform {
                    Some(v) => Paletted {
                        bits: 0,
                        palette: vec![v as i32],
                        data: Vec::new(),
                    },
                    None => Paletted::from_values(
                        &values,
                        Strategy::Blocks,
                        crate::chunk::block_strategy().global_bits,
                    ),
                },
                biomes: self.biomes(section),
            },
            entities,
        )
    }

    fn biomes(&self, section: &Section) -> Paletted {
        let values: Vec<i32> = (0..BIOMES)
            .map(|j| {
                let internal = section.biomes.get(j) as u16;
                self.biome_ids
                    .get(usize::from(internal))
                    .copied()
                    .unwrap_or(0)
            })
            .collect();
        Paletted::from_values(&values, Strategy::Biomes, self.biome_bits)
    }
}

const CACHE_SWEEP: usize = 1 << 15;

const WARM_MIN: usize = 16;

struct Warm {
    coder: Arc<SectionCoder>,
    heightmaps: Option<(Arc<crate::save::SaveTables>, i32, i32, i32)>,
    columns: Vec<((i32, i32), Vec<Arc<Section>>)>,
    heights: Vec<OnceLock<ColumnHeightmaps>>,
    sections: Vec<Arc<Section>>,
    next: AtomicUsize,
    done: Vec<OnceLock<Warmed>>,
}
type Warmed = ((Arc<[u8]>, Entities), Option<Arc<Layers>>);
impl Warm {
    fn run(&self) {
        loop {
            let i = self.next.fetch_add(1, Ordering::Relaxed);
            if let Some((_, sections)) = self.columns.get(i) {
                if let Some((tables, column_min_y, min_y, height)) = &self.heightmaps {
                    let maps = crate::save::client_heightmaps_in(
                        sections,
                        *column_min_y,
                        tables,
                        *min_y,
                        *height,
                    );
                    let _ = self.heights[i].set(Arc::new(maps));
                }
                continue;
            }
            let i = i - self.columns.len();
            let Some(s) = self.sections.get(i) else {
                return;
            };
            let light = matches!(s.light, Light::Full(_)).then(|| Arc::new(Layers::of(&s.light)));
            let _ = self.done[i].set((self.coder.wire(s), light));
        }
    }
}

struct WarmPool {
    job: std::sync::Mutex<(u64, Option<Arc<Warm>>)>,
    ready: std::sync::Condvar,
}

fn warm_pool() -> Option<&'static WarmPool> {
    static POOL: OnceLock<Option<Arc<WarmPool>>> = OnceLock::new();
    POOL.get_or_init(|| {
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()) - 1;
        let pool = Arc::new(WarmPool {
            job: Default::default(),
            ready: Default::default(),
        });
        let started = (0..threads)
            .filter(|i| {
                let pool = pool.clone();
                std::thread::Builder::new()
                    .name(format!("vise-chunk-encode-{i}"))
                    .spawn(move || {
                        let mut done = 0;
                        loop {
                            let job = {
                                let mut slot = pool.job.lock().unwrap();
                                while slot.0 == done {
                                    slot = pool.ready.wait(slot).unwrap();
                                }
                                done = slot.0;
                                slot.1.clone()
                            };
                            if let Some(job) = job {
                                job.run();
                            }
                        }
                    })
                    .is_ok()
            })
            .count();
        (started > 0).then_some(pool)
    })
    .as_deref()
}
impl ChunkEncoder {
    pub fn new(
        states: impl IntoIterator<Item = (Arc<str>, bool)>,
        builtin: &Builtin,
        biome_names: &[String],
        biome_registry: &[String],
    ) -> Self {
        let types = builtin.get("minecraft:block_entity_type");
        let blocks = builtin.get("minecraft:block");
        let mut flags = Vec::new();
        let mut entity_types = Vec::new();
        let mut block_ids = Vec::new();
        for (name, has_fluid) in states {
            let air = matches!(
                &*name,
                "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
            );
            let mut f = 0u8;
            if !air {
                f |= NON_AIR;
                if has_fluid {
                    f |= FLUID;
                }
            }
            let t = block_entity_type(&name)
                .and_then(|t| types.and_then(|ids| ids.id(t)))
                .map_or(-1, |v| v as i16);
            if t >= 0 {
                f |= ENTITY;
            }
            flags.push(f);
            entity_types.push(t);
            block_ids.push(blocks.and_then(|b| b.id(&name)).unwrap_or(0));
        }
        let biome_ids = biome_names
            .iter()
            .map(|n| {
                biome_registry
                    .iter()
                    .position(|b| b == n)
                    .map_or(0, |i| i as i32)
            })
            .collect();
        Self {
            coder: Arc::new(SectionCoder {
                flags,
                biome_ids,
                biome_bits: ceil_log2(biome_registry.len()),
            }),
            entity_types,
            block_ids,
            tables: None,
            cache: Default::default(),
            heights: Default::default(),
            sections: 0..ColumnLayout::VANILLA.sections(),
            column: ColumnLayout::VANILLA,
            sky: true,
        }
    }

    pub fn biome_data(&self, chunk: &Chunk) -> ChunkBiomeData {
        let biomes: Vec<Paletted> = chunk.sections[self.sections.clone()]
            .iter()
            .map(|s| self.coder.biomes(s))
            .collect();
        ChunkBiomeData::from_biomes([chunk.x, chunk.z], &biomes)
    }

    fn heightmaps(&self, chunk: &Chunk, tables: &crate::save::SaveTables) -> ColumnHeightmaps {
        let mut heights = self.heights.lock().unwrap();
        if let Some(maps) = cached_heights(&heights, chunk) {
            return maps;
        }
        let (min_y, height) = self.level();
        let maps = Arc::new(crate::save::client_heightmaps_in(
            &chunk.sections,
            chunk.min_y(),
            tables,
            min_y,
            height,
        ));
        keep_heights(&mut heights, (chunk.x, chunk.z), &chunk.sections, &maps);
        maps
    }

    fn level(&self) -> (i32, i32) {
        (
            self.column.section_y(self.sections.start),
            16 * self.sections.len() as i32,
        )
    }

    fn cached<T>(&self, section: &Arc<Section>, read: impl FnOnce(&mut Encoded) -> T) -> T {
        read(self.entry(&mut self.cache.lock().unwrap(), section))
    }

    fn entry<'a>(
        &self,
        cache: &'a mut FxHashMap<usize, Encoded>,
        section: &Arc<Section>,
    ) -> &'a mut Encoded {
        if cache.len() >= CACHE_SWEEP {
            cache.retain(|_, e| e.section.strong_count() > 0);
            if cache.len() >= CACHE_SWEEP / 2 {
                cache.clear();
            }
        }
        cache
            .entry(Arc::as_ptr(section) as usize)
            .or_insert_with(|| Encoded {
                section: Arc::downgrade(section),
                non_air: self.non_air(section),
                wire: None,
                light: None,
            })
    }

    fn non_air(&self, section: &Section) -> bool {
        let non_air =
            |v: u32| self.coder.flags.get(v as usize).copied().unwrap_or(0) & NON_AIR != 0;
        if !section.blocks.maybe_has(non_air) {
            false
        } else if let Some(v) = section.blocks.uniform() {
            non_air(v)
        } else {
            (0..BLOCKS).any(|i| non_air(section.blocks.get(i)))
        }
    }

    fn section_wire(&self, section: &Arc<Section>) -> (Arc<[u8]>, Entities) {
        if let Some(wire) = self.cached(section, |e| e.wire.clone()) {
            return wire;
        }
        let wire = self.coder.wire(section);
        self.cached(section, |e| e.wire = Some(wire.clone()));
        wire
    }
    pub fn set_dimension(&mut self, shape: LevelShape) {
        let column = shape.column();
        let lo = usize::try_from((shape.min_y - column.min_y()) >> 4).unwrap_or(0);
        let n = usize::try_from(shape.height >> 4).unwrap_or(0);
        self.column = column;
        self.sections = lo.min(column.sections())..(lo + n).min(column.sections());
        self.sky = shape.sky_light;
        self.heights.get_mut().unwrap().clear();
    }
    pub fn entity_type(&self, state: u32) -> Option<i32> {
        self.entity_types
            .get(state as usize)
            .filter(|t| **t >= 0)
            .map(|t| i32::from(*t))
    }
    pub fn is_air(&self, state: u32) -> bool {
        self.coder
            .flags
            .get(state as usize)
            .is_none_or(|f| f & NON_AIR == 0)
    }

    fn occupied(&self, chunk: &Chunk) -> SectionMask {
        let mut bits = SectionMask::EMPTY;
        let mut cache = self.cache.lock().unwrap();
        for (i, s) in chunk.sections[self.sections.clone()].iter().enumerate() {
            if self.entry(&mut cache, s).non_air {
                bits.insert(i);
            }
        }
        bits
    }

    fn around(
        &self,
        chunk: &Chunk,
        world: &crate::world::World,
        protos: &dyn Fn(i32, i32) -> Option<SectionMask>,
    ) -> SectionMask {
        let mut around = SectionMask::EMPTY;
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (x, z) = (chunk.x + dx, chunk.z + dz);
                if let Some(c) = world.chunks.get(&(x, z)) {
                    around |= self.occupied(c);
                } else if let Some(sections) = protos(x, z) {
                    for i in sections.iter() {
                        if self.sections.contains(&i) {
                            around.insert(i - self.sections.start);
                        }
                    }
                }
            }
        }
        around
    }

    pub fn body_in(&self, chunk: &Chunk, world: &crate::world::World) -> Vec<u8> {
        self.body_with(chunk, world, &|_, _| None)
    }
    pub fn body_with(
        &self,
        chunk: &Chunk,
        world: &crate::world::World,
        protos: &dyn Fn(i32, i32) -> Option<SectionMask>,
    ) -> Vec<u8> {
        self.body(
            chunk,
            &world.block_entities,
            self.around(chunk, world, protos),
        )
    }

    pub fn warm(&self, columns: &[&Chunk]) {
        let Some(job) = self.job(columns) else {
            return;
        };
        let Some(pool) = warm_pool() else {
            return;
        };
        let job = Arc::new(job);
        {
            let mut slot = pool.job.lock().unwrap();
            slot.0 += 1;
            slot.1 = Some(job.clone());
        }
        pool.ready.notify_all();
        job.run();
        {
            let mut slot = pool.job.lock().unwrap();
            if slot.1.as_ref().is_some_and(|j| Arc::ptr_eq(j, &job)) {
                slot.1 = None;
            }
        }
        self.keep(&job);
    }

    fn job(&self, columns: &[&Chunk]) -> Option<Warm> {
        let stale: Vec<((i32, i32), Vec<Arc<Section>>)> = match &self.tables {
            Some(_) => {
                let heights = self.heights.lock().unwrap();
                let mut seen = crate::fast_hash::FxHashSet::default();
                columns
                    .iter()
                    .filter(|c| seen.insert((c.x, c.z)) && cached_heights(&heights, c).is_none())
                    .map(|c| ((c.x, c.z), c.sections.clone()))
                    .collect()
            }
            None => Vec::new(),
        };
        let sections: Vec<Arc<Section>> = {
            let cache = self.cache.lock().unwrap();
            columns
                .iter()
                .flat_map(|c| &c.sections[self.sections.clone()])
                .filter(|s| {
                    cache
                        .get(&(Arc::as_ptr(s) as usize))
                        .is_none_or(|e| e.wire.is_none())
                })
                .cloned()
                .collect()
        };
        if sections.len() + stale.len() < WARM_MIN {
            return None;
        }
        let (min_y, height) = self.level();
        Some(Warm {
            coder: self.coder.clone(),
            heightmaps: self
                .tables
                .clone()
                .map(|t| (t, self.column.min_y(), min_y, height)),
            heights: stale.iter().map(|_| OnceLock::new()).collect(),
            columns: stale,
            done: sections.iter().map(|_| OnceLock::new()).collect(),
            sections,
            next: AtomicUsize::new(0),
        })
    }

    fn keep(&self, job: &Warm) {
        {
            let mut heights = self.heights.lock().unwrap();
            for ((key, sections), done) in job.columns.iter().zip(&job.heights) {
                if let Some(maps) = done.get() {
                    keep_heights(&mut heights, *key, sections, maps);
                }
            }
        }
        let mut cache = self.cache.lock().unwrap();
        for (s, done) in job.sections.iter().zip(&job.done) {
            if let Some((wire, light)) = done.get() {
                let e = self.entry(&mut cache, s);
                e.wire = Some(wire.clone());
                if light.is_some() {
                    e.light = light.clone();
                }
            }
        }
    }

    fn body(
        &self,
        chunk: &Chunk,
        block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
        around: SectionMask,
    ) -> Vec<u8> {
        let (wires, entities) = self.blocks(chunk, block_entities);
        let heightmaps = self.heightmaps_of(chunk);
        let lights = self.light_sections(chunk, around);
        let buffer: usize = wires.iter().map(|w| w.len()).sum();
        let mut tags = Writer::new();
        List::<BlockEntityInfo>::write(&entities, &mut tags);
        let size = 64
            + heightmaps
                .iter()
                .map(|(_, h)| 10 + 8 * h.len())
                .sum::<usize>()
            + buffer
            + tags.data.len()
            + lights.len() * 2 * (2048 + 3);
        let mut w = Writer {
            data: Vec::with_capacity(size),
        };
        w.varint(vise_protocol::ids::play::clientbound::LEVEL_CHUNK_WITH_LIGHT);
        w.i32(chunk.x);
        w.i32(chunk.z);
        Heightmaps::write(&*heightmaps, &mut w);
        w.varint(buffer as i32);
        for wire in &wires {
            w.bytes(wire);
        }
        w.bytes(&tags.data);
        write_light(&lights, self.sky, &mut w);
        w.data
    }

    pub fn light_update(&self, chunk: &Chunk, changed: SectionMask) -> Vec<u8> {
        let first = self.sections.start;
        let lights: Vec<(usize, SectionLight)> = self
            .light_sections(chunk, SectionMask::ALL)
            .into_iter()
            .filter(|&(b, _)| {
                (1..=self.sections.len()).contains(&b) && changed.contains(first + b - 1)
            })
            .collect();
        vise_protocol::packets::play::Clientbound::LightUpdate(
            vise_protocol::packets::play::LightUpdate {
                x: chunk.x,
                z: chunk.z,
                light: light_data_of(&lights, self.sky),
            },
        )
        .encode()
    }

    pub fn encode(
        &self,
        chunk: &Chunk,
        block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
    ) -> LevelChunkWithLight {
        let mut packet = self.encode_blocks(chunk, block_entities);
        packet.light = self.light_data(chunk, SectionMask::ALL);
        packet
    }

    fn encode_blocks(
        &self,
        chunk: &Chunk,
        block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
    ) -> LevelChunkWithLight {
        let (wires, block_entities) = self.blocks(chunk, block_entities);
        LevelChunkWithLight {
            x: chunk.x,
            z: chunk.z,
            chunk: ChunkData {
                heightmaps: self.heightmaps_of(chunk).to_vec(),
                buffer: wires.concat(),
                block_entities,
            },
            light: LightData {
                sky_mask: Vec::new(),
                block_mask: Vec::new(),
                empty_sky_mask: Vec::new(),
                empty_block_mask: Vec::new(),
                sky: Vec::new(),
                block: Vec::new(),
            },
        }
    }

    fn heightmaps_of(&self, chunk: &Chunk) -> ColumnHeightmaps {
        self.tables
            .as_ref()
            .map(|t| self.heightmaps(chunk, t))
            .unwrap_or_default()
    }

    fn blocks(
        &self,
        chunk: &Chunk,
        block_entities: &HashMap<(i32, i32, i32), BlockEntity>,
    ) -> (Vec<Arc<[u8]>>, Vec<BlockEntityInfo>) {
        let mut wires = Vec::with_capacity(self.sections.len());
        let mut entity_infos = Vec::new();
        for (i, s) in chunk
            .sections
            .iter()
            .enumerate()
            .skip(self.sections.start)
            .take(self.sections.len())
        {
            let (encoded, entities) = self.section_wire(s);
            for &(index, state) in entities.iter() {
                let Some(kind) = self.entity_type(state) else {
                    continue;
                };
                let (x, y, z) = (
                    i32::from(index & 15),
                    i32::from(index >> 8),
                    i32::from(index >> 4 & 15),
                );
                let wy = chunk.min_y() + (i as i32) * 16 + y;
                let pos = (chunk.x * 16 + x, wy, chunk.z * 16 + z);
                entity_infos.push(BlockEntityInfo {
                    packed_xz: ((x << 4) | z) as i8,
                    y: wy as i16,
                    kind,
                    tag: block_entities
                        .get(&pos)
                        .and_then(entity_tag)
                        .or_else(|| pot_tag(chunk, pos))
                        .unwrap_or(Tag::End),
                });
            }
            wires.push(encoded);
        }
        if let Some(order) = chunk.saved.as_ref().map(|s| &s.block_entity_order)
            && !order.is_empty()
        {
            let at = |e: &BlockEntityInfo| {
                let pos = (
                    chunk.x * 16 + i32::from(e.packed_xz as u8 >> 4),
                    i32::from(e.y),
                    chunk.z * 16 + i32::from(e.packed_xz as u8 & 15),
                );
                order.iter().position(|p| *p == pos).unwrap_or(usize::MAX)
            };
            entity_infos.sort_by_key(at);
        }
        java_hash_order(&mut entity_infos, |e| {
            (
                chunk.x * 16 + i32::from(e.packed_xz as u8 >> 4),
                i32::from(e.y),
                chunk.z * 16 + i32::from(e.packed_xz as u8 & 15),
            )
        });
        (wires, entity_infos)
    }

    fn light_data(&self, chunk: &Chunk, around: SectionMask) -> LightData {
        light_data_of(&self.light_sections(chunk, around), self.sky)
    }

    fn light_sections(&self, chunk: &Chunk, around: SectionMask) -> Vec<(usize, SectionLight)> {
        light_sections(&chunk.sections[self.sections.clone()], around, |s| {
            if let Light::Uniform(v) = s.light {
                return SectionLight::Uniform(v);
            }
            SectionLight::Layers(self.cached(s, |e| e.light.clone()).unwrap_or_else(|| {
                let layers = Arc::new(Layers::of(&s.light));
                self.cached(s, |e| e.light = Some(layers.clone()));
                layers
            }))
        })
    }
}

fn java_hash_order<T>(items: &mut Vec<T>, pos: impl Fn(&T) -> (i32, i32, i32)) {
    let hash = |&(x, y, z): &(i32, i32, i32)| {
        y.wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x)
    };
    let keys: Vec<i32> = items.iter().map(|i| hash(&pos(i))).collect();
    let mut capacity = 16usize;
    while keys.len() * 4 > capacity * 3 {
        capacity *= 2;
    }
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by_key(|&i| {
        ((keys[i] ^ ((keys[i] as u32) >> 16) as i32) as u32 as usize) & (capacity - 1)
    });
    let mix = |h: i32| {
        let h = h.wrapping_mul(0x9E37_79B9_u32 as i32) as u32;
        (h ^ (h >> 16)) as usize
    };
    let put = |table: &mut Vec<Option<usize>>, i: usize| {
        let mask = table.len() - 1;
        let mut at = mix(keys[i]) & mask;
        while table[at].is_some() {
            at = (at + 1) & mask;
        }
        table[at] = Some(i);
    };
    let mut table: Vec<Option<usize>> = vec![None; 32];
    for (size, &i) in order.iter().enumerate() {
        put(&mut table, i);
        let max_fill = (table.len() * 3).div_ceil(4).min(table.len() - 1);
        if size >= max_fill {
            let grown = ((size + 2) * 4).div_ceil(3).next_power_of_two().max(2);
            let old = std::mem::replace(&mut table, vec![None; grown]);
            for &j in old.iter().rev().flatten() {
                put(&mut table, j);
            }
        }
    }
    let iteration: Vec<usize> = table.iter().rev().flatten().copied().collect();
    let mut slots: Vec<Option<T>> = items.drain(..).map(Some).collect();
    items.extend(iteration.into_iter().filter_map(|i| slots[i].take()));
}

pub fn entity_tag(entity: &BlockEntity) -> Option<Tag> {
    match entity {
        BlockEntity::Sign(sign) => Some(Tag::compound(sign.encode())),
        BlockEntity::Spawner(s) => Some(s.update_tag()),
        BlockEntity::Brushable(b) => {
            let mut entries = Vec::new();
            if let Some(face) = b.hit_direction {
                entries.push(("hit_direction", Tag::Byte(face as i8)));
            }
            if let Some(item) = &b.item {
                entries.push(("item", crate::save::item_to_nbt(item)));
            }
            Some(Tag::compound(entries))
        }
        BlockEntity::Shelf(s) => Some(Tag::compound([
            ("Items", crate::save::encode_items(&s.items)),
            (
                "align_items_to_bottom",
                Tag::Byte(i8::from(s.align_to_bottom)),
            ),
        ])),
        BlockEntity::Banner(b) => {
            let mut entries = Vec::new();
            if !b.layers.is_empty() {
                entries.push(("patterns", crate::banner::layers_to_nbt(&b.layers)));
            }
            if let Some(name) = &b.name {
                entries.push(("CustomName", Tag::String(name.as_str().into())));
            }
            entries.push(("components", Tag::Compound(b.components.clone())));
            Some(Tag::compound(entries))
        }
        BlockEntity::Vault(v) => Some(v.update_tag()),
        BlockEntity::TrialSpawner(t) => t
            .next_spawn_data
            .as_ref()
            .map(|d| Tag::compound([("spawn_data", d.encode())])),
        BlockEntity::Skull(s) => {
            let mut entries = Vec::new();
            if let Some(profile) = &s.profile {
                entries.push(("profile", profile.clone()));
            }
            if let Some(sound) = &s.note_block_sound {
                entries.push(("note_block_sound", Tag::String(sound.as_str().into())));
            }
            if let Some(name) = &s.custom_name {
                entries.push(("custom_name", name.clone()));
            }
            Some(Tag::compound(entries))
        }
        BlockEntity::Jukebox(_) | BlockEntity::Lectern(_) => None,
        BlockEntity::Campfire(c) => Some(Tag::compound([(
            "Items",
            crate::save::encode_items(&c.items),
        )])),
        _ => None,
    }
}

pub fn pot_tag(chunk: &Chunk, pos: (i32, i32, i32)) -> Option<Tag> {
    let at = chunk.saved.as_ref()?.block_entities.iter().find(|t| {
        t.get("id") == Some(&Tag::String("minecraft:decorated_pot".into()))
            && t.get("x") == Some(&Tag::Int(pos.0))
            && t.get("y") == Some(&Tag::Int(pos.1))
            && t.get("z") == Some(&Tag::Int(pos.2))
    })?;
    let entries: Vec<_> = ["sherds", "LootTable", "LootTableSeed", "item"]
        .into_iter()
        .filter_map(|k| Some((k, at.get(k)?.clone())))
        .collect();
    (!entries.is_empty()).then(|| Tag::compound(entries))
}

#[cfg(test)]
fn light_data(sections: &[Arc<Section>], around: SectionMask) -> LightData {
    let lights = light_sections(sections, around, |s| {
        SectionLight::Layers(Arc::new(Layers::of(&s.light)))
    });
    light_data_of(&lights, true)
}

enum SectionLight {
    Layers(Arc<Layers>),
    Uniform(u8),
}
impl SectionLight {
    fn any(&self) -> (bool, bool) {
        match self {
            Self::Layers(l) => (l.sky_any, l.block_any),
            Self::Uniform(v) => (v >> 4 != 0, v & 15 != 0),
        }
    }
    fn put(&self, sky: bool, out: &mut Vec<u8>) {
        match self {
            Self::Layers(l) => out.extend_from_slice(if sky { &l.sky } else { &l.block }),
            Self::Uniform(v) => {
                let n = if sky { v >> 4 } else { v & 15 };
                out.resize(out.len() + 2048, n | n << 4);
            }
        }
    }
}

fn light_sections(
    sections: &[Arc<Section>],
    around: SectionMask,
    section: impl Fn(&Arc<Section>) -> SectionLight,
) -> Vec<(usize, SectionLight)> {
    let n = sections.len();
    let exists =
        |b: usize| (b.saturating_sub(2)..=b.min(n - 1)).any(|s| s + 2 >= b && around.contains(s));
    (0..n + 2)
        .filter(|&b| exists(b))
        .map(|b| {
            let light = match b {
                0 => SectionLight::Uniform(0),
                _ if b == n + 1 => SectionLight::Uniform(0xF0),
                _ => section(&sections[b - 1]),
            };
            (b, light)
        })
        .collect()
}

fn light_masks(lights: &[(usize, SectionLight)], sky: bool) -> [Vec<u8>; 4] {
    let mut masks: [Vec<u8>; 4] = Default::default();
    for (b, light) in lights {
        let (sky_any, block_any) = light.any();
        if sky {
            set_bit(&mut masks[if sky_any { 0 } else { 2 }], *b);
        }
        set_bit(&mut masks[if block_any { 1 } else { 3 }], *b);
    }
    masks
}

fn sent_layers(
    lights: &[(usize, SectionLight)],
    sky: bool,
) -> impl Iterator<Item = &SectionLight> + Clone {
    lights.iter().map(|(_, l)| l).filter(move |l| {
        let (sky_any, block_any) = l.any();
        if sky { sky_any } else { block_any }
    })
}

fn light_data_of(lights: &[(usize, SectionLight)], sky: bool) -> LightData {
    let [sky_mask, block_mask, empty_sky_mask, empty_block_mask] = light_masks(lights, sky);
    let layers = |kind: bool| -> Vec<Vec<u8>> {
        sent_layers(lights, kind)
            .map(|l| {
                let mut layer = Vec::with_capacity(2048);
                l.put(kind, &mut layer);
                layer
            })
            .collect()
    };
    LightData {
        sky_mask,
        block_mask,
        empty_sky_mask,
        empty_block_mask,
        sky: if sky { layers(true) } else { Vec::new() },
        block: layers(false),
    }
}

fn write_light(lights: &[(usize, SectionLight)], sky: bool, w: &mut Writer) {
    for mask in light_masks(lights, sky) {
        BitSet::write(&mask, w);
    }
    for kind in [true, false] {
        let layers = sent_layers(lights, kind).filter(|_| sky || !kind);
        w.varint(layers.clone().count() as i32);
        for l in layers {
            w.varint(2048);
            l.put(kind, &mut w.data);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::{MIN_Y, SECTIONS};

    #[test]
    fn block_entity_blocks_are_recognised() {
        assert_eq!(
            block_entity_type("minecraft:oak_sign"),
            Some("minecraft:sign")
        );
        assert_eq!(
            block_entity_type("minecraft:oak_wall_hanging_sign"),
            Some("minecraft:hanging_sign")
        );
        assert_eq!(
            block_entity_type("minecraft:red_shulker_box"),
            Some("minecraft:shulker_box")
        );
        assert_eq!(block_entity_type("minecraft:stone"), None);
    }

    #[test]
    fn block_entities_come_from_the_cached_section_in_scan_order() {
        let builtin = Builtin::parse(
            r#"{"minecraft:block_entity_type": {"entries": {
                "minecraft:chest": {"protocol_id": 1},
                "minecraft:sign": {"protocol_id": 7}}}}"#,
        )
        .unwrap();
        let states = [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:chest",
            "minecraft:oak_sign",
        ]
        .map(|n| (Arc::<str>::from(n), false));
        let encoder = ChunkEncoder::new(states, &builtin, &[], &[]);
        let mut chunk = Chunk::empty(2, -3);
        for (x, y, z, state) in [(15, 3, 0, 2), (0, 3, 15, 3), (4, 3, 4, 1), (7, 40, 2, 2)] {
            chunk.set(x, MIN_Y + y, z, state);
        }
        let scan = |chunk: &Chunk| {
            let mut found = Vec::new();
            for (i, s) in chunk.sections.iter().enumerate() {
                for y in 0..16 {
                    for z in 0..16 {
                        for x in 0..16 {
                            let state = s.get(crate::chunk::index(x, y, z));
                            if let Some(kind) = encoder.entity_type(state) {
                                found.push((x, MIN_Y + i as i32 * 16 + y, z, kind));
                            }
                        }
                    }
                }
            }
            java_hash_order(&mut found, |e| {
                (chunk.x * 16 + e.0, e.1, chunk.z * 16 + e.2)
            });
            found
        };
        let sent = |chunk: &Chunk| {
            let packet = encoder.encode(chunk, &HashMap::new());
            let infos = packet.chunk.block_entities;
            infos
                .iter()
                .map(|e| {
                    let xz = e.packed_xz as u8;
                    (
                        i32::from(xz >> 4),
                        i32::from(e.y),
                        i32::from(xz & 15),
                        e.kind,
                    )
                })
                .collect::<Vec<_>>()
        };
        let expected = scan(&chunk);
        assert_eq!(expected.len(), 3);
        assert_eq!(sent(&chunk), expected);
        assert_eq!(sent(&chunk), expected);
        chunk.set(15, MIN_Y + 3, 0, 0);
        chunk.set(9, MIN_Y + 3, 9, 3);
        assert_eq!(sent(&chunk), scan(&chunk));
    }

    fn encoder() -> ChunkEncoder {
        let states = ["minecraft:air", "minecraft:stone", "minecraft:water"]
            .map(|n| (Arc::<str>::from(n), n.ends_with("water")));
        ChunkEncoder::new(states, &Builtin::default(), &[], &[])
    }

    fn fresh(encoder: &ChunkEncoder, section: &Section) -> Vec<u8> {
        let mut w = vise_protocol::buf::Writer::new();
        encoder.coder.section(section).0.write(&mut w);
        w.data
    }

    #[test]
    fn cached_sections_follow_edits() {
        let encoder = encoder();
        let mut chunk = Chunk::empty(0, 0);
        chunk.set(1, MIN_Y + 2, 3, 1);
        let before = encoder.encode(&chunk, &HashMap::new());
        assert_eq!(encoder.occupied(&chunk), SectionMask::single(0));
        assert_eq!(
            &*encoder.section_wire(&chunk.sections[0]).0,
            fresh(&encoder, &chunk.sections[0])
        );
        chunk.set(4, MIN_Y + 20, 4, 2);
        chunk.set(1, MIN_Y + 2, 3, 0);
        assert_eq!(encoder.occupied(&chunk), SectionMask::single(1));
        let after = encoder.encode(&chunk, &HashMap::new());
        assert_ne!(before.chunk.buffer, after.chunk.buffer);
        let mut expected = Vec::new();
        for s in &chunk.sections {
            expected.extend(fresh(&encoder, s));
        }
        assert_eq!(after.chunk.buffer, expected);
    }

    #[test]
    fn light_layers_pack_two_blocks_per_byte() {
        let mut chunk = Chunk::empty(0, 0);
        let mut light = Box::new([0u8; 4096]);
        for (i, v) in light.iter_mut().enumerate() {
            *v = (i * 37 % 251) as u8;
        }
        Arc::make_mut(&mut chunk.sections[3]).light = Light::Full(light.clone());
        let data = light_data(&chunk.sections, SectionMask::ALL);
        let (sky, block) = (&data.sky[3], &data.block[0]);
        for j in 0..4096 {
            let (s, b) = if j & 1 == 0 {
                (sky[j >> 1] & 15, block[j >> 1] & 15)
            } else {
                (sky[j >> 1] >> 4, block[j >> 1] >> 4)
            };
            assert_eq!((s, b), (light[j] >> 4, light[j] & 15), "block {j}");
        }
    }

    #[test]
    fn cached_light_follows_edits() {
        let encoder = encoder();
        let mut chunk = Chunk::empty(0, 0);
        let mut light = Box::new([0x5Au8; 4096]);
        Arc::make_mut(&mut chunk.sections[2]).light = Light::Full(light.clone());
        let first = encoder.encode(&chunk, &HashMap::new()).light;
        assert_eq!(first, light_data(&chunk.sections, SectionMask::ALL));
        light[7] = 0xF3;
        Arc::make_mut(&mut chunk.sections[2]).light = Light::Full(light);
        let second = encoder.encode(&chunk, &HashMap::new()).light;
        assert_ne!(first, second);
        assert_eq!(second, light_data(&chunk.sections, SectionMask::ALL));
    }

    #[test]
    fn a_proto_neighbour_counts_for_the_light_layers() {
        let encoder = encoder();
        let mut world = crate::world::World::new(vise_world::registry::BlockStateId(0), 0);
        for x in -1..=1 {
            for z in -1..=1 {
                let mut chunk = Chunk::empty(x, z);
                chunk.set(0, MIN_Y, 0, 1);
                world.chunks.insert((x, z), Arc::new(chunk));
            }
        }
        let mut tall = Chunk::empty(1, 0);
        tall.set(0, MIN_Y, 0, 1);
        tall.set(0, MIN_Y + 16 * 10, 0, 1);
        world.chunks.insert((1, 0), Arc::new(tall));
        let centre = world.chunks[&(0, 0)].clone();
        let loaded = encoder.body_in(&centre, &world);
        world.chunks.remove(&(1, 0));
        assert_ne!(encoder.body_in(&centre, &world), loaded);
        let mut sections = SectionMask::EMPTY;
        sections.insert(0);
        sections.insert(10);
        let proto = |x, z| ((x, z) == (1, 0)).then_some(sections);
        assert_eq!(encoder.body_with(&centre, &world, &proto), loaded);
    }

    #[test]
    fn light_masks_cover_the_world_plus_borders() {
        let chunk = Chunk::empty(0, 0);
        assert!(
            light_data(&chunk.sections, SectionMask::EMPTY)
                .sky
                .is_empty()
        );
        let data = light_data(&chunk.sections, SectionMask::ALL);
        assert_eq!(data.sky.len(), SECTIONS + 1);
        assert_eq!(data.block.len(), 0);
        assert_eq!(data.sky[0][0], 0xFF);
        assert!(vise_protocol::buf::bit(&data.empty_block_mask, 5));
    }

    #[test]
    fn the_body_is_the_packet_encoded() {
        let builtin = Builtin::parse(
            r#"{"minecraft:block_entity_type": {"entries": {
                "minecraft:chest": {"protocol_id": 1},
                "minecraft:sign": {"protocol_id": 7}}}}"#,
        )
        .unwrap();
        let states = [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:chest",
            "minecraft:oak_sign",
        ]
        .map(|n| (Arc::<str>::from(n), false));
        let mut encoder = ChunkEncoder::new(states, &builtin, &[], &[]);
        let mut chunk = Chunk::empty(-7, 12);
        let mut light = Box::new([0u8; 4096]);
        for (i, v) in light.iter_mut().enumerate() {
            *v = (i * 53 % 256) as u8;
        }
        Arc::make_mut(&mut chunk.sections[4]).light = Light::Full(light);
        Arc::make_mut(&mut chunk.sections[5]).light = Light::Uniform(0x07);
        Arc::make_mut(&mut chunk.sections[6]).light = Light::Uniform(0);
        Arc::make_mut(&mut chunk.sections[7]).light = Light::Full(Box::new([0x30; 4096]));
        for (x, y, z, state) in [
            (3, 70, 9, 1),
            (5, 70, 5, 2),
            (0, -60, 15, 2),
            (15, 300, 0, 1),
            (8, 64, 1, 3),
        ] {
            chunk.set(x, y, z, state);
        }
        let mut entities = HashMap::new();
        entities.insert(
            (-7 * 16 + 8, 64, 12 * 16 + 1),
            BlockEntity::Sign(crate::signs::SignData::default()),
        );
        let packet = |encoder: &ChunkEncoder, around: SectionMask| {
            let mut packet = encoder.encode_blocks(&chunk, &entities);
            packet.light = encoder.light_data(&chunk, around);
            vise_protocol::packets::play::Clientbound::LevelChunkWithLight(packet).encode()
        };
        let sent = encoder
            .encode_blocks(&chunk, &entities)
            .chunk
            .block_entities;
        assert_eq!(sent.len(), 3);
        assert!(sent.iter().any(|e| e.tag != Tag::End));
        for around in [
            SectionMask::EMPTY,
            SectionMask::single(0),
            SectionMask::from_iter([5, 7]),
            SectionMask::single(23),
            SectionMask::ALL,
        ] {
            assert_eq!(
                encoder.body(&chunk, &entities, around),
                packet(&encoder, around),
                "around {around:?}"
            );
        }
        encoder.set_dimension(LevelShape::NETHER);
        assert_eq!(
            encoder.body(&chunk, &entities, SectionMask::ALL),
            packet(&encoder, SectionMask::ALL)
        );
    }

    #[test]
    fn warmed_columns_give_the_same_bodies() {
        let builtin = Builtin::parse(
            r#"{"minecraft:block_entity_type": {"entries": {
                "minecraft:sign": {"protocol_id": 7}}}}"#,
        )
        .unwrap();
        let states = [
            "minecraft:air",
            "minecraft:stone",
            "minecraft:water",
            "minecraft:oak_sign",
        ]
        .map(|n| (Arc::<str>::from(n), n.ends_with("water")));
        let mut world = crate::world::World::new(vise_world::registry::BlockStateId(0), 0);
        for x in -2..2 {
            for z in -2..2 {
                let mut chunk = Chunk::empty(x, z);
                let n = (x * 7 + z * 13).rem_euclid(17);
                for i in 0..40 + n * 30 {
                    let (bx, bz) = ((i * 5 + n) % 16, (i * 11) % 16);
                    chunk.set(bx, MIN_Y + (i * 37 + n * 50) % 384, bz, 1 + (i % 3) as u32);
                }
                let mut light = Box::new([0u8; 4096]);
                for (i, v) in light.iter_mut().enumerate() {
                    *v = (i * 31 + n as usize) as u8;
                }
                Arc::make_mut(&mut chunk.sections[(n % 24) as usize]).light = Light::Full(light);
                world.block_entities.insert(
                    (x * 16 + 2, 70, z * 16 + 3),
                    BlockEntity::Sign(crate::signs::SignData::default()),
                );
                chunk.set(2, 70, 3, 3);
                world.chunks.insert((x, z), Arc::new(chunk));
            }
        }
        let mut keys: Vec<(i32, i32)> =
            (-2..2).flat_map(|x| (-2..2).map(move |z| (x, z))).collect();
        keys.push(keys[5]);
        let chunks: Vec<&Chunk> = keys
            .iter()
            .map(|k| &**world.chunks.get(k).unwrap())
            .collect();
        let tables = Arc::new(crate::save::SaveTables::for_heightmaps(
            crate::assets::Properties::for_tests(&[
                (true, false, false, false),
                (false, false, true, true),
                (false, true, true, true),
                (false, false, false, false),
            ]),
        ));
        let mut alone = ChunkEncoder::new(states.clone(), &builtin, &[], &[]);
        alone.tables = Some(tables.clone());
        let expected: Vec<Vec<u8>> = chunks.iter().map(|c| alone.body_in(c, &world)).collect();
        let mut warmed = ChunkEncoder::new(states.clone(), &builtin, &[], &[]);
        warmed.tables = Some(tables);
        let bodies = |encoder: &ChunkEncoder| -> Vec<Vec<u8>> {
            chunks.iter().map(|c| encoder.body_in(c, &world)).collect()
        };
        assert!(chunks.len() * SECTIONS >= WARM_MIN);
        warmed.warm(&chunks);
        assert_eq!(bodies(&warmed), expected);
        let mut cold = ChunkEncoder::new(states, &builtin, &[], &[]);
        cold.tables = alone.tables.clone();
        let job = cold.job(&chunks).unwrap();
        assert_eq!((job.columns.len(), job.sections.len()), (16, 17 * SECTIONS));
        job.run();
        cold.keep(&job);
        assert_eq!(cold.heights.lock().unwrap().len(), 16);
        assert_eq!(bodies(&cold), expected);
        warmed.warm(&chunks);
        assert_eq!(bodies(&warmed), expected);
        warmed.warm(&[]);
    }

    #[test]
    fn a_nether_column_sends_its_sixteen_sections_without_sky_light() {
        let chunk = Chunk::empty(0, 0);
        let mut encoder = ChunkEncoder::new(
            [("minecraft:air".into(), false)],
            &Builtin::default(),
            &[],
            &[],
        );
        let overworld = encoder.encode(&chunk, &HashMap::new());
        encoder.set_dimension(LevelShape::NETHER);
        let nether = encoder.encode(&chunk, &HashMap::new());
        let per_section = overworld.chunk.buffer.len() / SECTIONS;
        assert_eq!(nether.chunk.buffer.len(), per_section * 16);
        assert!(nether.light.sky.is_empty() && nether.light.sky_mask.is_empty());
        assert!(vise_protocol::buf::bit(
            &nether.light.empty_block_mask,
            17
        ));
        assert!(!vise_protocol::buf::bit(
            &nether.light.empty_block_mask,
            18
        ));
    }

    #[test]
    fn a_tall_column_sends_its_whole_height() {
        let shape = LevelShape::new(-64, 2096, 2096, true).unwrap();
        let mut chunk = Chunk::with_layout(0, 0, shape.column());
        chunk.set(3, 2000, 4, 1);
        let mut encoder = encoder();
        let vanilla = encoder.encode(&Chunk::empty(0, 0), &HashMap::new());
        encoder.set_dimension(shape);
        let tall = encoder.encode(&chunk, &HashMap::new());
        let per_section = vanilla.chunk.buffer.len() / SECTIONS;
        assert!(tall.chunk.buffer.len() > per_section * 131);
        assert_eq!(tall.light.sky.len(), 132);
    }
}
