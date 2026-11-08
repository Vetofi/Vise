use crate::{
    chunk::{Chunk, ColumnLayout, LevelShape},
    fast_hash::{FxHashMap, FxHashSet},
    section_mask::SectionMask,
};
use vise_core::random::LegacyRandom;
use vise_world::registry::BlockStateId;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct DirtySections {
    columns: crate::fast_hash::FxHashMap<(i32, i32), SectionMask>,
}
impl DirtySections {
    pub fn insert(&mut self, (x, sy, z): (i32, i32, i32)) {
        debug_assert!((0..SectionMask::CAPACITY as i32).contains(&sy));
        self.columns.entry((x, z)).or_default().insert(sy as usize);
    }
    pub fn contains(&self, &(x, sy, z): &(i32, i32, i32)) -> bool {
        (0..SectionMask::CAPACITY as i32).contains(&sy)
            && self
                .columns
                .get(&(x, z))
                .is_some_and(|m| m.contains(sy as usize))
    }
    pub fn take_column(&mut self, x: i32, z: i32) -> SectionMask {
        self.columns.remove(&(x, z)).unwrap_or_default()
    }
    pub fn retain_columns(&mut self, mut keep: impl FnMut(i32, i32) -> bool) {
        self.columns.retain(|&(x, z), _| keep(x, z));
    }
    pub fn columns(&self) -> impl Iterator<Item = ((i32, i32), SectionMask)> + '_ {
        self.columns.iter().map(|(&c, &m)| (c, m))
    }
    pub fn clear(&mut self) {
        self.columns.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }
    pub fn len(&self) -> usize {
        self.columns.values().map(SectionMask::len).sum()
    }
}
pub struct World {
    pub shape: LevelShape,
    pub fast_lava: bool,
    pub chunks: crate::chunk_map::ChunkMap,
    pub random: LegacyRandom,
    pub revision: u64,
    pub health_rules: HealthRules,
    pub air: BlockStateId,
    pub dirty_sections: DirtySections,
    pub dirty_light: crate::fast_hash::FxHashSet<(i32, i32)>,
    pub light_edits: Vec<((i32, i32, i32), u32, u32)>,
    pub loaded_entities: Vec<((i32, i32), Vec<vise_world::nbt::Tag>)>,
    pub modified: FxHashSet<(i32, i32)>,
    pub block_entities: crate::block_entities::BlockEntities,
    pub chest_openers: FxHashMap<(i32, i32, i32), (i32, f64)>,
    pub block_names: FxHashMap<(i32, i32, i32), String>,
    pub watched: Vec<bool>,
    pub watched_writes: Vec<((i32, i32, i32), u32, u32)>,
    pub inhabited: FxHashMap<(i32, i32), i64>,
    pub game_time: i64,
    pub day_time: i64,
    pub weather: Weather,
    pub block_log: Option<Vec<(i32, i32, i32)>>,
    pub light_log: Option<crate::fast_hash::FxHashMap<(i32, i32), SectionMask>>,
    pub pois: crate::poi_sections::PoiSections<crate::poi_sections::Poi>,
    pub moving_piston_fallback: Vec<([i32; 3], vise_world::piston_shape::MovingPistonState)>,
    pub moving_pistons: Vec<([i32; 3], vise_world::piston_shape::MovingPistonState)>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Weather {
    pub clear_time: i32,
    pub rain_time: i32,
    pub thunder_time: i32,
    pub raining: bool,
    pub thundering: bool,
    pub rain_level: (f32, f32),
    pub thunder_level: (f32, f32),
}
impl Weather {
    pub fn tick_timers(&mut self, random: &mut vise_core::random::LegacyRandom) {
        use vise_core::random::RandomSource;
        let mut uniform =
            |min: i32, max: i32| min + random.next_int_bound(max - min + 1).unwrap_or(0);
        if self.clear_time > 0 {
            self.clear_time -= 1;
            self.thunder_time = if self.thundering { 0 } else { 1 };
            self.rain_time = if self.raining { 0 } else { 1 };
            self.thundering = false;
            self.raining = false;
        } else {
            if self.thunder_time > 0 {
                self.thunder_time -= 1;
                if self.thunder_time == 0 {
                    self.thundering = !self.thundering;
                }
            } else if self.thundering {
                self.thunder_time = uniform(3600, 15600);
            } else {
                self.thunder_time = uniform(12000, 180000);
            }
            if self.rain_time > 0 {
                self.rain_time -= 1;
                if self.rain_time == 0 {
                    self.raining = !self.raining;
                }
            } else if self.raining {
                self.rain_time = uniform(12000, 24000);
            } else {
                self.rain_time = uniform(12000, 180000);
            }
        }
    }
    pub fn tick_levels(&mut self) {
        self.thunder_level.0 = self.thunder_level.1;
        self.thunder_level.1 =
            (self.thunder_level.1 + if self.thundering { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
        self.rain_level.0 = self.rain_level.1;
        self.rain_level.1 =
            (self.rain_level.1 + if self.raining { 0.01 } else { -0.01 }).clamp(0.0, 1.0);
    }
    pub fn levels(&self, partial: f32) -> (f32, f32) {
        let rain = self.rain_level.0 + (self.rain_level.1 - self.rain_level.0) * partial;
        let thunder =
            self.thunder_level.0 + (self.thunder_level.1 - self.thunder_level.0) * partial;
        (rain, thunder * rain)
    }
}
impl World {
    pub fn new(air: BlockStateId, seed: i64) -> Self {
        Self {
            shape: LevelShape::OVERWORLD,
            chunks: Default::default(),
            random: LegacyRandom::new(seed),
            revision: 0,
            health_rules: HealthRules::default(),
            air,
            fast_lava: false,
            dirty_sections: Default::default(),
            dirty_light: Default::default(),
            light_edits: Vec::new(),
            loaded_entities: Vec::new(),
            modified: FxHashSet::default(),
            block_entities: Default::default(),
            chest_openers: FxHashMap::default(),
            block_names: FxHashMap::default(),
            watched: Vec::new(),
            watched_writes: Vec::new(),
            inhabited: FxHashMap::default(),
            game_time: 0,
            day_time: 1000,
            weather: Weather::default(),
            block_log: None,
            light_log: None,
            moving_piston_fallback: Vec::new(),
            moving_pistons: Vec::new(),
            pois: Default::default(),
        }
    }
    pub fn test_scene(
        assets: &crate::assets::Assets,
        render: &crate::assets::RenderAssets,
        radius: i32,
    ) -> crate::Result<Self> {
        let air = assets
            .registry
            .default_state("minecraft:air")
            .ok_or("missing air state")?;
        let mut world = Self::new(air, 0);
        for cz in -radius..=radius {
            for cx in -radius..=radius {
                world
                    .chunks
                    .insert((cx, cz), Arc::new(Chunk::empty(cx, cz)));
                for z in 0..16 {
                    for x in 0..16 {
                        for y in 0..4 {
                            let layer = match y {
                                0 => 0,
                                1 | 2 => 1,
                                _ if (cx + cz) & 1 == 0 => 2,
                                _ => 3,
                            };
                            world.set(
                                cx * 16 + x,
                                y,
                                cz * 16 + z,
                                assets.material_states[layer],
                            )?;
                        }
                    }
                }
            }
        }
        for y in 4..11 {
            for z in -10_i32..=10 {
                for x in -10_i32..=10 {
                    if x.abs().max(z.abs()) <= 10 - (y - 4) {
                        world.set(
                            x,
                            y,
                            z,
                            assets.material_states[if y & 1 == 0 { 4 } else { 5 }],
                        )?;
                    }
                }
            }
        }
        for (position, state) in &render.model_placements {
            world.set(position.x, position.y, position.z, *state)?;
        }
        world.revision = 0;
        Ok(world)
    }
    pub fn layout(&self) -> ColumnLayout {
        self.shape.column()
    }
    pub fn bottom_y(&self) -> i32 {
        self.layout().min_y()
    }
    pub fn top_y(&self) -> i32 {
        self.layout().max_y()
    }
    pub fn set(&mut self, x: i32, y: i32, z: i32, state: BlockStateId) -> crate::Result<()> {
        let Some(chunk) = self.chunks.get_mut(&(x >> 4, z >> 4)) else {
            return Err("write outside resident chunks".into());
        };
        let (min_y, sections) = (chunk.min_y(), chunk.sections.len() as i32);
        if chunk.section_index(y).is_none() {
            return Err("write outside world height".into());
        }
        let previous = Arc::make_mut(chunk).set(x & 15, y, z & 15, state.0);
        if previous != state.0 {
            self.block_entities
                .state_changed((x, y, z), previous, state.0);
            let watches = |id: u32| self.watched.get(id as usize).copied().unwrap_or(false);
            if watches(previous) || watches(state.0) {
                self.watched_writes.push(((x, y, z), previous, state.0));
            }
            self.revision = self.revision.wrapping_add(1);
            if let Some(log) = &mut self.block_log {
                log.push((x, y, z));
            }
            self.modified.insert((x >> 4, z >> 4));
            let sy = (y - min_y) >> 4;
            let local_y = y & 15;
            let mark = |dirty: &mut DirtySections, bx: i32, bz: i32| {
                dirty.insert((bx >> 4, sy, bz >> 4));
                if local_y == 0 && sy > 0 {
                    dirty.insert((bx >> 4, sy - 1, bz >> 4));
                }
                if local_y == 15 && sy + 1 < sections {
                    dirty.insert((bx >> 4, sy + 1, bz >> 4));
                }
            };
            let (bx, bz) = (x & 15, z & 15);
            mark(&mut self.dirty_sections, x, z);
            if bx == 0 {
                mark(&mut self.dirty_sections, x - 1, z);
            } else if bx == 15 {
                mark(&mut self.dirty_sections, x + 1, z);
            }
            if bz == 0 {
                mark(&mut self.dirty_sections, x, z - 1);
            } else if bz == 15 {
                mark(&mut self.dirty_sections, x, z + 1);
            }
            if (bx == 0 || bx == 15) && (bz == 0 || bz == 15) {
                mark(
                    &mut self.dirty_sections,
                    if bx == 0 { x - 1 } else { x + 1 },
                    if bz == 0 { z - 1 } else { z + 1 },
                );
            }
            if self.chunks.get(&(x >> 4, z >> 4)).is_some_and(|c| c.lit) {
                self.light_edits.push(((x, y, z), previous, state.0));
            } else {
                for dz in -1..=1 {
                    for dx in -1..=1 {
                        self.dirty_light.insert(((x >> 4) + dx, (z >> 4) + dz));
                    }
                }
            }
        }
        Ok(())
    }
    pub fn world_surface(&self, x: i32, z: i32, air: impl Fn(BlockStateId) -> bool) -> i32 {
        let bottom = self.layout().min_y();
        self.chunks.get(&(x >> 4, z >> 4)).map_or(bottom, |chunk| {
            chunk.world_surface(x, z, |v| air(BlockStateId(v)))
        })
    }
    pub fn motion_blocking(&self, x: i32, z: i32, blocks: impl Fn(BlockStateId) -> bool) -> i32 {
        let bottom = self.layout().min_y();
        self.chunks.get(&(x >> 4, z >> 4)).map_or(bottom, |chunk| {
            chunk.motion_blocking(x & 15, z & 15, |v| blocks(BlockStateId(v)))
        })
    }
    pub fn block(&self, x: i32, y: i32, z: i32) -> BlockStateId {
        self.chunks
            .get(&(x >> 4, z >> 4))
            .map_or(self.air, |chunk| chunk.block(x & 15, y, z & 15))
    }
    pub fn may_hold(
        &self,
        lo: [i32; 3],
        hi: [i32; 3],
        mut matches: impl FnMut(BlockStateId) -> bool,
    ) -> bool {
        let layout = self.layout();
        let (bottom, top) = (layout.min_y(), layout.max_y() - 1);
        if (lo[1] < bottom || hi[1] > top) && matches(BlockStateId(0)) {
            return true;
        }
        let sections = (lo[1].max(bottom) - bottom) >> 4..=(hi[1].min(top) - bottom) >> 4;
        for cx in lo[0] >> 4..=hi[0] >> 4 {
            for cz in lo[2] >> 4..=hi[2] >> 4 {
                let Some(chunk) = self.chunks.get(&(cx, cz)) else {
                    if matches(self.air) {
                        return true;
                    }
                    continue;
                };
                for s in sections.clone() {
                    let blocks = &chunk.sections[s as usize].blocks;
                    if blocks.is_global() || blocks.maybe_has(|v| matches(BlockStateId(v))) {
                        return true;
                    }
                }
            }
        }
        false
    }
    pub fn block_entity_tickers(
        &mut self,
    ) -> std::sync::Arc<Vec<(crate::block_entities::Pos, u32)>> {
        let (chunks, air) = (&self.chunks, self.air);
        self.block_entities.tickers(|(x, y, z)| {
            chunks
                .get(&(x >> 4, z >> 4))
                .map_or(air, |chunk| chunk.block(x & 15, y, z & 15))
                .0
        })
    }
    pub fn light(&self, x: i32, y: i32, z: i32) -> u8 {
        self.chunks
            .get(&(x >> 4, z >> 4))
            .map_or(0xF0, |chunk| chunk.light(x & 15, y, z & 15))
    }
    pub fn is_air(&self, state: BlockStateId) -> bool {
        state == self.air
    }
    pub fn loaded(&self, x: i32, z: i32) -> bool {
        self.chunks.contains_key(&(x >> 4, z >> 4))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct HealthRules {
    pub difficulty: vise_world::food::Difficulty,
    pub natural_regeneration: bool,
    pub drowning_damage: bool,
    pub fall_damage: bool,
    pub fire_damage: bool,
    pub freeze_damage: bool,
}
impl Default for HealthRules {
    fn default() -> Self {
        Self {
            difficulty: vise_world::food::Difficulty::Normal,
            natural_regeneration: true,
            drowning_damage: true,
            fall_damage: true,
            fire_damage: true,
            freeze_damage: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DirtySections, World};
    use crate::chunk::{Chunk, MIN_Y, SECTIONS};
    use vise_world::registry::BlockStateId;
    use std::sync::Arc;

    #[test]
    fn writes_mark_exactly_the_reachable_sections() {
        let mut world = World::new(BlockStateId(0), 0);
        world.chunks.insert((0, 0), Arc::new(Chunk::empty(0, 0)));
        for (x, y, z) in [
            (3, 70, 3),
            (0, 70, 3),
            (15, 70, 3),
            (3, 70, 0),
            (0, 70, 0),
            (15, 70, 15),
            (0, MIN_Y, 0),
            (0, 80, 0),
            (15, MIN_Y + SECTIONS as i32 * 16 - 1, 15),
        ] {
            world.dirty_sections.clear();
            world.set(x, y, z, BlockStateId(1)).unwrap();
            let mut expected = DirtySections::default();
            for dz in -1..=1 {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (bx, by, bz) = (x + dx, y + dy, z + dz);
                        let s = (by - MIN_Y) >> 4;
                        if (0..SECTIONS as i32).contains(&s) {
                            expected.insert((bx >> 4, s, bz >> 4));
                        }
                    }
                }
            }
            let mut got: Vec<_> = world.dirty_sections.columns().collect();
            let mut want: Vec<_> = expected.columns().collect();
            got.sort_unstable_by_key(|&(column, _)| column);
            want.sort_unstable_by_key(|&(column, _)| column);
            assert_eq!(got, want, "write at {x} {y} {z}");
            if (x & 15, z & 15) == (3, 3) {
                assert_eq!(world.dirty_sections.len(), 1);
            }
            if (x & 15, z & 15, y & 15) == (0, 0, 0) && y > MIN_Y {
                assert_eq!(world.dirty_sections.len(), 8);
            }
        }
    }

    #[test]
    fn may_hold_reads_the_palettes_of_the_reached_sections() {
        let mut world = World::new(BlockStateId(0), 0);
        let mut chunk = Chunk::empty(0, 0);
        chunk.set(3, 70, 3, 9);
        world.chunks.insert((0, 0), Arc::new(chunk));
        let nine = |s: BlockStateId| s.0 == 9;
        assert!(world.may_hold([0, 64, 0], [1, 65, 1], nine));
        assert!(world.may_hold([10, 79, 10], [12, 80, 12], nine));
        assert!(!world.may_hold([0, 50, 0], [15, 63, 15], nine));
        assert!(!world.may_hold([16, 70, 0], [17, 71, 1], nine));
        assert!(world.may_hold([16, 70, 0], [17, 71, 1], |s| s.0 == 0));
        assert!(!world.may_hold([0, 400, 0], [0, 401, 0], nine));
        assert!(world.may_hold([0, 400, 0], [0, 401, 0], |s| s.0 == 0));
    }
}
