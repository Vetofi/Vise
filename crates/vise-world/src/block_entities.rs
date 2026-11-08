use crate::{compression::invalid, nbt::Tag};
use vise_core::position::{BlockPos, ChunkPos};
use std::{
    collections::{HashMap, HashSet},
    io,
};

pub fn position_from_tag(chunk: ChunkPos, tag: &Tag) -> io::Result<BlockPos> {
    if !matches!(tag, Tag::Compound(_)) {
        return Err(invalid("block entity is not a compound"));
    }
    let read = |name| match tag.get(name) {
        Some(Tag::Byte(v)) => i32::from(*v),
        Some(Tag::Short(v)) => i32::from(*v),
        Some(Tag::Int(v)) => *v,
        Some(Tag::Long(v)) => *v as i32,
        Some(Tag::Float(bits)) => f64::from(f32::from_bits(*bits)).floor() as i32,
        Some(Tag::Double(bits)) => f64::from_bits(*bits).floor() as i32,
        _ => 0,
    };
    let (mut x, y, mut z) = (read("x"), read("y"), read("z"));
    if x >> 4 != chunk.x || z >> 4 != chunk.z {
        x = chunk.x.wrapping_shl(4).wrapping_add(x & 15);
        z = chunk.z.wrapping_shl(4).wrapping_add(z & 15);
    }
    Ok(BlockPos::new(x, y, z))
}

pub struct ProtoBlockEntities<E> {
    position: ChunkPos,
    pending: HashMap<BlockPos, Tag>,
    active: HashMap<BlockPos, E>,
}
impl<E> ProtoBlockEntities<E> {
    pub fn new(position: ChunkPos) -> Self {
        Self {
            position,
            pending: HashMap::new(),
            active: HashMap::new(),
        }
    }
    pub fn set_pending(&mut self, tag: Tag) -> io::Result<()> {
        let position = position_from_tag(self.position, &tag)?;
        if !self.active.contains_key(&position) {
            self.pending.insert(position, tag);
        }
        Ok(())
    }
    pub fn set_active(&mut self, position: BlockPos, entity: E) -> Option<E> {
        self.pending.remove(&position);
        self.active.insert(position, entity)
    }
    pub fn active(&self, position: BlockPos) -> Option<&E> {
        self.active.get(&position)
    }
    pub fn pending(&self, position: BlockPos) -> Option<&Tag> {
        self.pending.get(&position)
    }
    pub fn pending_mut(&mut self, position: BlockPos) -> Option<&mut Tag> {
        self.pending.get_mut(&position)
    }
    pub fn pending_entries(&self) -> &HashMap<BlockPos, Tag> {
        &self.pending
    }
    pub fn positions(&self) -> HashSet<BlockPos> {
        self.pending
            .keys()
            .chain(self.active.keys())
            .copied()
            .collect()
    }
    pub fn remove(&mut self, position: BlockPos) -> Option<E> {
        self.pending.remove(&position);
        self.active.remove(&position)
    }
    pub fn for_saving<T>(
        &self,
        position: BlockPos,
        serialize: impl FnOnce(&E) -> Result<Tag, T>,
    ) -> Result<Option<Tag>, T> {
        if let Some(entity) = self.active.get(&position) {
            serialize(entity).map(Some)
        } else {
            Ok(self.pending.get(&position).cloned())
        }
    }
}
