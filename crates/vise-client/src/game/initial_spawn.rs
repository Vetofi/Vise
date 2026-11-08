use super::{Game, dispenser::entity_random};
use vise_client::{
    Result,
    assets::Properties,
    collisions::{SceneCollisions, no_collision_or_liquid},
    world::World,
    world_spawn::{
        PlayerSpawnFinder, SpawnCandidate, SpawnLevel, level_respawn_pos, spawn_pos_in_chunk,
    },
};
use vise_core::{
    geometry::{Aabb, Direction, ShapeOp, Vec3, VoxelShape},
    position::BlockPos,
    random::RandomSource,
};
use vise_world::{
    block_collision::BlockCollisionRegistry, collision_context::CollisionContext,
    heightmap::HeightmapType, level::flags::UPDATE_CLIENTS, registry::BlockStateId,
    worldgen::terrain::TerrainBlockStates,
};

struct Columns<'a, 'r> {
    world: &'a World,
    properties: &'a Properties,
    binding: &'a BlockCollisionRegistry<'r>,
}
impl SpawnLevel for Columns<'_, '_> {
    fn min_y(&self) -> i32 {
        self.world.bottom_y()
    }
    fn height(&self, kind: HeightmapType, x: i32, z: i32) -> i32 {
        (self.world.bottom_y()..self.world.top_y())
            .rev()
            .find(|&y| kind.is_opaque(self.properties.heightmap_state(self.world.block(x, y, z))))
            .map_or(self.world.bottom_y(), |y| y + 1)
    }
    fn has_fluid(&self, x: i32, y: i32, z: i32) -> bool {
        use vise_world::heightmap::HeightmapState;
        self.properties
            .heightmap_state(self.world.block(x, y, z))
            .has_fluid()
    }
    fn top_face_full(&self, x: i32, y: i32, z: i32) -> bool {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.binding,
            subject: None,
        };
        let state = self.world.block(x, y, z);
        let Ok(shape) = self.binding.collision_shape_with_context(
            &mut scene,
            state,
            BlockPos::new(x, y, z),
            &CollisionContext::empty(false),
        ) else {
            return false;
        };
        shape.face_shape(Direction::Up).is_ok_and(|face| {
            VoxelShape::block()
                .join_is_not_empty(&face, ShapeOp::XOR)
                .is_ok_and(|differs| !differs)
        })
    }
}

impl Game<'_> {
    fn spawn_columns(&self) -> Columns<'_, '_> {
        Columns {
            world: &self.world,
            properties: &self.pack.assets.properties,
            binding: self.collision,
        }
    }
    fn spawn_origin(&self) -> Result<((i32, i32), i32)> {
        if let Some(flat) =
            vise_client::save::world_gen_settings(&self.world_root).and_then(|s| {
                vise_client::flatgen::FlatTerrain::from_settings(
                    &s,
                    self.pack.assets.registry,
                    self.res.biomes(),
                    self.level_shapes.overworld,
                )
            })
        {
            return Ok(((0, 0), flat.spawn_height()));
        }
        if self.options.test_terrain {
            return Ok(((0, 0), 64));
        }
        let origin = match self.end.locator() {
            Some(terrain) => terrain.origin()?,
            None => vise_core::position::ChunkPos::new(0, 0),
        };
        Ok(((origin.x, origin.z), 64))
    }
    pub(super) fn find_world_spawn(&mut self) -> Result<Option<[i32; 3]>> {
        let ((ox, oz), height) = match self.spawn_origin {
            Some(origin) => origin,
            None => *self.spawn_origin.insert(self.spawn_origin()?),
        };
        while let Some((dx, dz)) = self.spawn_spiral.current() {
            if self.spawn_spiral.tested() {
                let (cx, cz) = (ox + dx, oz + dz);
                if self.standalone_server() {
                    if !self.world.chunks.contains_key(&(cx, cz)) {
                        let area = [((cx, cz), -1)];
                        if self.streaming.extra != area {
                            self.streaming.extra = area.to_vec();
                            self.streaming.rank_full_task(cx, cz);
                        }
                        return Ok(None);
                    }
                } else if !self.streaming.is_lit(cx, cz) {
                    self.streaming.center = (cx, cz);
                    return Ok(None);
                }
                if let Some(pos) = spawn_pos_in_chunk(&self.spawn_columns(), cx, cz) {
                    return Ok(Some(pos));
                }
            }
            self.spawn_spiral.advance();
        }
        Ok(Some([ox * 16 + 8, height, oz * 16 + 8]))
    }
    pub(super) fn place_bonus_chest(&mut self, origin: [i32; 3]) -> Result<()> {
        use vise_client::containers::BlockEntity;
        let (cx, cz) = (origin[0] >> 4, origin[2] >> 4);
        let registry = &self.pack.assets.registry;
        let (Some(chest), Some(torch)) = (
            registry.default_state("minecraft:chest"),
            registry.default_state("minecraft:torch"),
        ) else {
            return Ok(());
        };
        if !self.streaming.is_lit(cx, cz) {
            return Ok(());
        }
        let random = &mut self.world.random;
        let mut shuffled = |min: i32| -> [i32; 16] {
            let mut values: [i32; 16] = std::array::from_fn(|i| min + i as i32);
            for end in (2..=16usize).rev() {
                let other = random.next_int_bound(end as i32).unwrap_or(0) as usize;
                values.swap(end - 1, other);
            }
            values
        };
        let (xs, zs) = (shuffled(cx * 16), shuffled(cz * 16));
        for x in xs {
            for z in zs {
                let y = self
                    .spawn_columns()
                    .height(HeightmapType::MotionBlockingNoLeaves, x, z);
                if self.world.block(x, y, z) != self.world.air && !self.collision_empty([x, y, z]) {
                    continue;
                }
                self.set_block_clients([x, y, z], chest)?;
                let seed = self.world.random.next_long();
                self.world
                    .block_entities
                    .insert((x, y, z), BlockEntity::Chest(Vec::new()));
                self.load_block_entity_nbt(
                    [x, y, z],
                    &format!(
                        "{{LootTable:\"minecraft:chests/spawn_bonus_chest\",LootTableSeed:{seed}L}}"
                    ),
                );
                for [dx, dz] in [[0, -1], [1, 0], [0, 1], [-1, 0]] {
                    let at = [x + dx, y, z + dz];
                    let survives = self.with_level(|b, level, ticks| {
                        b.can_survive(level, ticks, BlockPos::new(at[0], at[1], at[2]), torch)
                    });
                    if survives {
                        self.set_block_clients(at, torch)?;
                    }
                }
                return Ok(());
            }
        }
        Ok(())
    }
    fn set_block_clients(&mut self, pos: [i32; 3], state: BlockStateId) -> Result<()> {
        self.with_level(|b, level, ticks| {
            b.set_block(
                level,
                ticks,
                BlockPos::new(pos[0], pos[1], pos[2]),
                state,
                UPDATE_CLIENTS,
            );
        });
        self.process_level_events()
    }
    fn collision_empty(&self, pos: [i32; 3]) -> bool {
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        self.collision
            .collision_shape_with_context(
                &mut scene,
                self.world.block(pos[0], pos[1], pos[2]),
                BlockPos::new(pos[0], pos[1], pos[2]),
                &CollisionContext::empty(false),
            )
            .is_ok_and(|shape| shape.is_empty())
    }
    pub(super) fn search_spawn(
        &self,
        suggestion: [i32; 3],
        state: &mut Option<PlayerSpawnFinder>,
    ) -> std::result::Result<Vec3, (i32, i32)> {
        let mut finder = state
            .take()
            .unwrap_or_else(|| self.spawn_finder(suggestion));
        let suggestion = finder.suggestion;
        loop {
            let Some(candidate) = finder.next_candidate() else {
                return Ok(self.fixup_spawn_height(suggestion));
            };
            let (x, z) = match candidate {
                SpawnCandidate::Column(x, z) => (x, z),
                SpawnCandidate::Suggestion => (suggestion[0], suggestion[2]),
            };
            if !self.streaming.is_lit(x >> 4, z >> 4) {
                finder.retry();
                *state = Some(finder);
                return Err((x >> 4, z >> 4));
            }
            match candidate {
                SpawnCandidate::Column(x, z) => {
                    if let Some(pos) = level_respawn_pos(&self.spawn_columns(), x, z)
                        && self.player_fits(pos)
                    {
                        return Ok(bottom_center(pos));
                    }
                }
                SpawnCandidate::Suggestion => return Ok(self.fixup_spawn_height(suggestion)),
            }
        }
    }
    pub(super) fn find_player_spawn(&mut self, suggestion: [i32; 3]) -> Result<Option<Vec3>> {
        let mut state = self.player_spawn.take();
        match self.search_spawn(suggestion, &mut state) {
            Ok(feet) => Ok(Some(feet)),
            Err(chunk) => {
                self.streaming.center = chunk;
                self.player_spawn = state;
                Ok(None)
            }
        }
    }
    fn spawn_finder(&self, suggestion: [i32; 3]) -> PlayerSpawnFinder {
        if self.level.game_mode == 2 {
            let mut finder = PlayerSpawnFinder::new(suggestion, 0, |_| 0);
            finder.next_candidate();
            return finder;
        }
        let mut radius = self.ui.rules.int("respawn_radius").max(0);
        let to_border = self
            .border
            .distance(f64::from(suggestion[0]), f64::from(suggestion[2]))
            .floor() as i32;
        if to_border < radius {
            radius = to_border;
        }
        if to_border <= 1 {
            radius = 1;
        }
        PlayerSpawnFinder::new(suggestion, radius, |n| {
            entity_random().next_int_bound(n).unwrap_or(0)
        })
    }
    pub(super) fn server_player_spawn(&self, spawn: [f64; 3]) -> [f64; 3] {
        let suggestion = spawn.map(|v| v.floor() as i32);
        let mut finder = self.spawn_finder(suggestion);
        while let Some(candidate) = finder.next_candidate() {
            let (x, z) = match candidate {
                SpawnCandidate::Column(x, z) => (x, z),
                SpawnCandidate::Suggestion => (suggestion[0], suggestion[2]),
            };
            if !self.streaming.is_lit(x >> 4, z >> 4) {
                continue;
            }
            let feet = match candidate {
                SpawnCandidate::Column(x, z) => level_respawn_pos(&self.spawn_columns(), x, z)
                    .filter(|&pos| self.player_fits(pos))
                    .map(bottom_center),
                SpawnCandidate::Suggestion => Some(self.fixup_spawn_height(suggestion)),
            };
            if let Some(feet) = feet {
                return [feet.x, feet.y, feet.z];
            }
        }
        spawn
    }
    fn player_fits(&self, pos: [i32; 3]) -> bool {
        let feet = bottom_center(pos);
        let bounds = Aabb::new(
            Vec3::new(feet.x - 0.3, feet.y, feet.z - 0.3),
            Vec3::new(feet.x + 0.3, feet.y + 1.8, feet.z + 0.3),
        );
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        no_collision_or_liquid(&mut scene, bounds)
    }
    fn fixup_spawn_height(&self, pos: [i32; 3]) -> Vec3 {
        let [x, mut y, z] = pos;
        while !self.player_fits([x, y, z]) && y < self.world.top_y() - 1 {
            y += 1;
        }
        y -= 1;
        while self.player_fits([x, y, z]) && y > self.world.bottom_y() {
            y -= 1;
        }
        bottom_center([x, y + 1, z])
    }
}

fn bottom_center(pos: [i32; 3]) -> Vec3 {
    Vec3::new(
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]),
        f64::from(pos[2]) + 0.5,
    )
}
