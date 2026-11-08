use crate::chunk::block_entity::{BlockEntity, ModContainer};
use crate::fast_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;
use std::sync::Arc;

pub type Pos = (i32, i32, i32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Ticking {
    #[default]
    No,
    Yes,
    Idle,
}

#[derive(Default)]
pub struct BlockEntities {
    map: HashMap<Pos, BlockEntity>,
    tickers: Arc<Vec<(Pos, u32)>>,
    live: FxHashMap<Pos, u32>,
    added: Vec<(Pos, u32)>,
    pending: FxHashMap<Pos, u32>,
    next_slot: u32,
    freed: bool,
    out: FxHashSet<Pos>,
    replaced: FxHashSet<Pos>,
    states: Vec<Ticking>,
    mod_containers: FxHashMap<u32, Arc<ModContainer>>,
}

impl std::ops::Deref for BlockEntities {
    type Target = HashMap<Pos, BlockEntity>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl<'a> IntoIterator for &'a BlockEntities {
    type Item = (&'a Pos, &'a BlockEntity);
    type IntoIter = std::collections::hash_map::Iter<'a, Pos, BlockEntity>;
    fn into_iter(self) -> Self::IntoIter {
        self.map.iter()
    }
}

fn java_hash((x, y, z): Pos) -> i32 {
    y.wrapping_add(z.wrapping_mul(31))
        .wrapping_mul(31)
        .wrapping_add(x)
}
fn mix(x: i32) -> usize {
    let h = (x as u32).wrapping_mul(0x9E37_79B9);
    (h ^ (h >> 16)) as usize
}

pub fn chunk_map_order(positions: impl IntoIterator<Item = Pos>) -> Vec<Pos> {
    let mut slots: Vec<Option<Pos>> = vec![None; 32];
    let max_fill = |n: usize| (n * 3).div_ceil(4).min(n - 1);
    let mut size = 0;
    for p in positions {
        let mask = slots.len() - 1;
        let mut at = mix(java_hash(p)) & mask;
        let mut known = false;
        while let Some(k) = slots[at] {
            if k == p {
                known = true;
                break;
            }
            at = (at + 1) & mask;
        }
        if known {
            continue;
        }
        slots[at] = Some(p);
        size += 1;
        if size > max_fill(slots.len()) {
            let n = ((size + 1) * 4).div_ceil(3).next_power_of_two();
            let old = std::mem::replace(&mut slots, vec![None; n]);
            for k in old.into_iter().rev().flatten() {
                let mut at = mix(java_hash(k)) & (n - 1);
                while slots[at].is_some() {
                    at = (at + 1) & (n - 1);
                }
                slots[at] = Some(k);
            }
        }
    }
    slots.into_iter().rev().flatten().collect()
}

impl BlockEntities {
    pub fn has_states(&self) -> bool {
        !self.states.is_empty()
    }
    pub fn set_states(&mut self, states: Vec<Ticking>) {
        self.states = states;
    }
    pub fn ticking(&self, state: u32) -> Ticking {
        self.states.get(state as usize).copied().unwrap_or_default()
    }
    pub fn set_mod_containers(&mut self, containers: FxHashMap<u32, Arc<ModContainer>>) {
        self.mod_containers = containers;
    }
    pub fn mod_container(&self, state: u32) -> Option<&Arc<ModContainer>> {
        self.mod_containers.get(&state)
    }

    fn next(&mut self) -> u32 {
        let n = self.next_slot;
        self.next_slot = n.wrapping_add(1);
        n
    }
    fn add(&mut self, pos: Pos) {
        let token = self.next();
        self.pending.insert(pos, token);
        self.added.push((pos, token));
    }
    fn cancel(&mut self, pos: &Pos) {
        self.freed |= self.live.remove(pos).is_some();
        self.pending.remove(pos);
    }

    pub fn get_mut(&mut self, pos: &Pos) -> Option<&mut BlockEntity> {
        self.map.get_mut(pos)
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Pos, &mut BlockEntity)> {
        self.map.iter_mut()
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut BlockEntity> {
        self.map.values_mut()
    }
    pub fn insert(&mut self, pos: Pos, entity: BlockEntity) -> Option<BlockEntity> {
        let old = self.map.insert(pos, entity);
        if old.is_none() {
            self.add(pos);
        }
        old
    }
    pub fn get_or_insert_with(
        &mut self,
        pos: Pos,
        make: impl FnOnce() -> BlockEntity,
    ) -> &mut BlockEntity {
        if !self.map.contains_key(&pos) {
            self.add(pos);
        }
        self.map.entry(pos).or_insert_with(make)
    }
    pub fn remove(&mut self, pos: &Pos) -> Option<BlockEntity> {
        self.cancel(pos);
        self.map.remove(pos)
    }
    pub fn take(&mut self, pos: &Pos) -> Option<BlockEntity> {
        let taken = self.map.remove(pos);
        if taken.is_some() {
            self.out.insert(*pos);
        }
        taken
    }
    pub fn put(&mut self, pos: Pos, entity: BlockEntity) {
        self.out.remove(&pos);
        if self.map.insert(pos, entity).is_none()
            && !self.live.contains_key(&pos)
            && !self.pending.contains_key(&pos)
        {
            self.add(pos);
        }
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Pos, &mut BlockEntity) -> bool) {
        let (live, pending, freed) = (&mut self.live, &mut self.pending, &mut self.freed);
        self.map.retain(|p, e| {
            let kept = keep(p, e);
            if !kept {
                *freed |= live.remove(p).is_some();
                pending.remove(p);
            }
            kept
        });
    }
    pub fn clear(&mut self) {
        self.map.clear();
        self.tickers = Arc::default();
        self.live.clear();
        self.added.clear();
        self.pending.clear();
        self.out.clear();
        self.replaced.clear();
        self.freed = false;
    }
    pub fn load_chunk(&mut self, entities: Vec<(Pos, BlockEntity)>) {
        let order = chunk_map_order(entities.iter().map(|(p, _)| *p));
        for (p, e) in entities {
            if self.map.insert(p, e).is_some() {
                self.replaced.insert(p);
            }
        }
        for p in order {
            self.add(p);
        }
    }
    pub fn state_changed(&mut self, pos: Pos, old: u32, new: u32) {
        if self.states.is_empty() || !self.map.contains_key(&pos) {
            return;
        }
        match self.ticking(new) {
            Ticking::Yes => {
                if self.ticking(old) != Ticking::Yes && !self.live.contains_key(&pos) {
                    self.add(pos);
                }
            }
            Ticking::Idle => self.cancel(&pos),
            Ticking::No => {}
        }
    }
    pub fn tickers(&mut self, state: impl Fn(Pos) -> u32) -> Arc<Vec<(Pos, u32)>> {
        for (pos, token) in std::mem::take(&mut self.added) {
            if self.pending.get(&pos) != Some(&token) {
                continue;
            }
            self.pending.remove(&pos);
            if self.live.contains_key(&pos)
                || !self.map.contains_key(&pos)
                || self.ticking(state(pos)) != Ticking::Yes
            {
                continue;
            }
            let slot = self.next();
            self.live.insert(pos, slot);
            Arc::make_mut(&mut self.tickers).push((pos, slot));
        }
        for pos in std::mem::take(&mut self.replaced) {
            if self.live.contains_key(&pos) && self.ticking(state(pos)) != Ticking::Yes {
                self.cancel(&pos);
            }
        }
        if !std::mem::take(&mut self.freed) && self.out.is_empty() {
            return Arc::clone(&self.tickers);
        }
        self.out.clear();
        let (live, map) = (&mut self.live, &self.map);
        Arc::make_mut(&mut self.tickers).retain(|(p, slot)| {
            let mine = live.get(p) == Some(slot);
            if mine && !map.contains_key(p) {
                live.remove(p);
            }
            mine && map.contains_key(p)
        });
        Arc::clone(&self.tickers)
    }
    pub fn is_live(&self, (pos, slot): (Pos, u32)) -> bool {
        self.live.get(&pos) == Some(&slot) && self.map.contains_key(&pos)
    }
    pub fn forget(&mut self, pos: &Pos) {
        self.cancel(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hopper() -> BlockEntity {
        BlockEntity::Chest(vec![None; 5])
    }
    fn level() -> BlockEntities {
        let mut b = BlockEntities::default();
        b.set_states(vec![Ticking::No, Ticking::Yes, Ticking::Idle]);
        b
    }
    fn order(b: &mut BlockEntities, states: &HashMap<Pos, u32>) -> Vec<Pos> {
        b.tickers(|p| states.get(&p).copied().unwrap_or(0))
            .iter()
            .map(|(p, _)| *p)
            .collect()
    }

    #[test]
    fn slots_follow_vanilla_lifecycle() {
        let mut b = level();
        let mut s = HashMap::new();
        for p in [(0, 0, 0), (1, 0, 0), (2, 0, 0)] {
            s.insert(p, 1);
            b.insert(p, hopper());
        }
        assert_eq!(order(&mut b, &s), [(0, 0, 0), (1, 0, 0), (2, 0, 0)]);
        b.insert((0, 0, 0), hopper());
        b.remove(&(1, 0, 0));
        b.insert((1, 0, 0), hopper());
        assert_eq!(order(&mut b, &s), [(0, 0, 0), (2, 0, 0), (1, 0, 0)]);
        let e = b.take(&(0, 0, 0)).unwrap();
        b.put((0, 0, 0), e);
        assert_eq!(order(&mut b, &s), [(0, 0, 0), (2, 0, 0), (1, 0, 0)]);
        s.insert((2, 0, 0), 2);
        b.state_changed((2, 0, 0), 1, 2);
        assert_eq!(order(&mut b, &s), [(0, 0, 0), (1, 0, 0)]);
        s.insert((2, 0, 0), 1);
        b.state_changed((2, 0, 0), 2, 1);
        assert_eq!(order(&mut b, &s), [(0, 0, 0), (1, 0, 0), (2, 0, 0)]);
        let slots = b.tickers(|p| s.get(&p).copied().unwrap_or(0));
        b.retain(|p, _| *p != (1, 0, 0));
        assert!(b.is_live(slots[0]) && !b.is_live(slots[1]));
        b.take(&(0, 0, 0));
        assert_eq!(order(&mut b, &s), [(2, 0, 0)]);
        assert_eq!(order(&mut b, &s), [(2, 0, 0)]);
    }

    #[test]
    fn chunk_loads_in_open_hash_map_order() {
        let positions = [
            (0, 64, 0),
            (1, 64, 0),
            (0, 64, 1),
            (15, -64, 15),
            (3, 70, 9),
        ];
        let slot = |p: &Pos| mix(java_hash(*p)) & 31;
        let mut expected = positions.to_vec();
        expected.sort_by_key(|p| std::cmp::Reverse(slot(p)));
        let distinct: std::collections::HashSet<usize> = positions.iter().map(slot).collect();
        assert_eq!(distinct.len(), positions.len());
        assert_eq!(chunk_map_order(positions), expected);
        let many: Vec<Pos> = (0..100).map(|i| (i % 16, i / 16, 3)).collect();
        let order = chunk_map_order(many.iter().copied().chain(many.iter().copied()));
        assert_eq!(order.len(), 100);
    }

    #[test]
    fn loaded_chunk_replaces_block_entity_by_state() {
        let mut b = level();
        let mut s = HashMap::new();
        let (p, q) = ((0, 0, 0), (1, 0, 0));
        for pos in [p, q] {
            s.insert(pos, 1);
        }
        b.load_chunk(vec![(p, hopper())]);
        b.load_chunk(vec![(q, hopper())]);
        assert_eq!(order(&mut b, &s), [p, q]);
        b.load_chunk(vec![(p, hopper())]);
        assert_eq!(order(&mut b, &s), [p, q]);
        s.insert(p, 2);
        b.load_chunk(vec![(p, BlockEntity::Chest(vec![None; 27]))]);
        assert_eq!(order(&mut b, &s), [q]);
        assert!(matches!(b.get(&p), Some(BlockEntity::Chest(items)) if items.len() == 27));
        s.insert(p, 1);
        b.state_changed(p, 2, 1);
        assert_eq!(order(&mut b, &s), [q, p]);
    }
}
