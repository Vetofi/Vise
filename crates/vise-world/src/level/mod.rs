use crate::{
    fluid::{FluidBlockBindings, FluidKind, FluidState},
    registry::{BlockRegistry, BlockStateId},
};
use vise_core::{
    geometry::{Aabb, Direction, VoxelShape},
    position::{BlockPos, ChunkPos},
    random::LegacyRandom,
};
use std::{collections::HashMap, io, path::Path, sync::Arc, sync::Mutex};

pub mod classes;
pub mod data;
pub mod states;
pub mod ticks;

mod bonemeal;
mod buckets;
mod creaking;
mod falling;
mod feature_world;
mod fluids;
mod growth;
mod hooks;
mod interact;
pub mod pistons;
mod place;
mod portal;
pub mod portal_travel;
mod query;
pub(crate) use query::data_2d;
mod rails;
mod redstone;
pub mod removal;
pub mod sculk;
mod shapes;
mod shelf;
mod signals;
mod speleothem;
mod survive;
mod tick;
mod timeline;
mod update;
mod weather;

#[cfg(test)]
mod growth_tests;
#[cfg(test)]
mod tests;

pub use buckets::BucketContent;
pub use data::{BlockData, ItemBinding, ItemKind, Named, Props, PushReaction, StateFacts, Tags};
pub use falling::Landing;
pub use interact::{InteractionResult, PlayerContext, PlayerEffect, hit_slot};
pub use pistons::{BlockEventData, MovingPiston};
pub use place::{PlaceContext, PlaceOutcome};
pub use states::{BlockId, PropKey, StateTable};
pub use tick::{Precipitation, PrecipitationKind};
pub use ticks::Ticks;

pub mod flags {
    pub const UPDATE_NEIGHBORS: i32 = 1;
    pub const UPDATE_CLIENTS: i32 = 2;
    pub const UPDATE_INVISIBLE: i32 = 4;
    pub const UPDATE_KNOWN_SHAPE: i32 = 16;
    pub const UPDATE_SUPPRESS_DROPS: i32 = 32;
    pub const UPDATE_MOVE_BY_PISTON: i32 = 64;
    pub const UPDATE_SKIP_SHAPE_UPDATE_ON_WIRE: i32 = 128;
    pub const UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS: i32 = 256;
    pub const UPDATE_SKIP_ON_PLACE: i32 = 512;
    pub const UPDATE_NONE: i32 = 260;
    pub const UPDATE_ALL: i32 = 3;
    pub const UPDATE_ALL_IMMEDIATE: i32 = 11;
    pub const UPDATE_SKIP_ALL_SIDEEFFECTS: i32 = 816;
    pub const UPDATE_LIMIT: i32 = 512;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameRules {
    pub water_source_conversion: bool,
    pub lava_source_conversion: bool,
    pub random_tick_speed: i32,
    pub block_drops: bool,
    pub mob_griefing: bool,
    pub max_snow_accumulation_height: i32,
    pub spawn_mobs: bool,
    pub spawn_monsters: bool,
    pub fire_spread_radius_around_player: i32,
    pub spread_vines: bool,
}
impl Default for GameRules {
    fn default() -> Self {
        Self {
            water_source_conversion: true,
            lava_source_conversion: false,
            random_tick_speed: 3,
            block_drops: true,
            mob_griefing: true,
            max_snow_accumulation_height: 1,
            spawn_mobs: true,
            spawn_monsters: true,
            fire_spread_radius_around_player: 128,
            spread_vines: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityQuery {
    Everything,
    Mobs,
    Minecart,
    Arrows,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LevelEvent {
    LevelEvent { id: i32, pos: BlockPos, data: i32 },
    Sound {
        pos: BlockPos,
        sound: Arc<str>,
        volume: f32,
        pitch: f32,
    },
    DropResources { pos: BlockPos, state: BlockStateId },
    FallingBlock {
        pos: BlockPos,
        state: BlockStateId,
        hurt_per_distance: Option<f32>,
    },
    GrowTree {
        pos: BlockPos,
        state: BlockStateId,
        grower: Arc<str>,
    },
    PlaceFeature {
        pos: BlockPos,
        state: BlockStateId,
        feature: Arc<str>,
    },
    PrimeTnt { pos: BlockPos, rot: f64 },
    BlockEvent {
        pos: BlockPos,
        state: BlockStateId,
        kind: &'static str,
    },
    BlockEntityEvent {
        pos: BlockPos,
        state: BlockStateId,
        a: i32,
        b: i32,
    },
    Player {
        pos: BlockPos,
        state: BlockStateId,
        effect: PlayerEffect,
    },
    GameEvent {
        pos: BlockPos,
        event: &'static str,
        state: Option<BlockStateId>,
    },
}

pub trait LevelAccess {
    fn observe_block_changed(&self, _pos: BlockPos, _old: BlockStateId, _new: BlockStateId) {}
    fn observe_neighbor_changed(
        &self,
        _pos: BlockPos,
        _state: BlockStateId,
        _changed: BlockId,
        _moved: bool,
    ) {
    }
    fn is_client_side(&self) -> bool {
        false
    }
    fn block_state(&self, pos: BlockPos) -> BlockStateId;
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId>;
    fn is_loaded(&self, pos: BlockPos) -> bool;
    fn min_y(&self) -> i32 {
        -64
    }
    fn max_y(&self) -> i32 {
        320
    }
    fn world_surface(&self, _x: i32, _z: i32) -> i32 {
        self.max_y()
    }
    fn random(&mut self) -> &mut LegacyRandom;
    fn game_time(&self) -> i64;
    fn block_light(&self, _pos: BlockPos) -> i32 {
        0
    }
    fn sky_light(&self, _pos: BlockPos) -> i32 {
        15
    }
    fn sky_darken(&self) -> i32 {
        0
    }
    fn sun_angle(&self, _pos: BlockPos) -> f32 {
        0.0
    }
    fn is_raining(&self) -> bool {
        false
    }
    fn is_thundering(&self) -> bool {
        false
    }
    fn is_raining_at(&self, _pos: BlockPos) -> bool {
        false
    }
    fn dimension(&self) -> Dimension {
        Dimension::Overworld
    }
    fn game_rules(&self) -> GameRules {
        GameRules::default()
    }
    fn difficulty(&self) -> i32 {
        2
    }
    fn can_spread_fire_around(&self, _pos: BlockPos) -> bool {
        true
    }
    fn increased_fire_burnout(&self, _pos: BlockPos) -> bool {
        false
    }
    fn is_unobstructed(&self, _pos: BlockPos, _state: BlockStateId) -> bool {
        true
    }
    fn block_entity_analog_signal(
        &self,
        _pos: BlockPos,
        _state: BlockStateId,
        _direction: Direction,
    ) -> i32 {
        0
    }
    fn item_frame_analog_output(&self, _pos: BlockPos, _facing: Direction) -> Option<i32> {
        None
    }
    fn recheck_container_openers(&mut self, _pos: BlockPos) -> Option<(i32, i32)> {
        None
    }
    fn set_command_block_powered(
        &mut self,
        _pos: BlockPos,
        _powered: bool,
    ) -> Option<(bool, bool)> {
        None
    }
    fn reset_potent_sulfur_countdown(&mut self, _pos: BlockPos) {}
    fn block_entity_signal(
        &self,
        _pos: BlockPos,
        _state: BlockStateId,
        _direction: Direction,
    ) -> i32 {
        0
    }
    fn precipitation(&self, _pos: BlockPos) -> Option<Precipitation> {
        None
    }
    fn count_entities(&self, _area: Aabb, _query: EntityQuery) -> usize {
        0
    }
    fn is_ticking_chunk(&self, _chunk: ChunkPos) -> bool {
        true
    }
    fn emit(&mut self, _event: LevelEvent) {}
    fn remove_block_entity(
        &mut self,
        _pos: BlockPos,
        _old: BlockStateId,
        _side_effects: bool,
    ) -> u8 {
        0
    }
    fn load_block_entity(&mut self, _pos: BlockPos, _nbt: crate::nbt::Compound) {}
    fn biome_name(&self, _pos: BlockPos) -> Option<&str> {
        None
    }
    fn world_seed(&self) -> i64 {
        0
    }
    fn day_time(&self) -> i64 {
        6000
    }
}

pub fn raw_brightness<L: LevelAccess + ?Sized>(level: &L, pos: BlockPos, sky_darken: i32) -> i32 {
    (level.sky_light(pos) - sky_darken).max(level.block_light(pos))
}

pub trait GameplayStateSource {
    fn flag(&self, name: &str) -> bool;
    fn unsigned(&self, name: &str) -> u64;
    fn number(&self, name: &str) -> f64;
    fn text(&self, name: &str) -> &str;
    fn sound_number(&self, name: &str) -> f64;
    fn sound_id(&self, name: &str) -> &str;
}

pub struct Behaviours {
    pub table: StateTable,
    pub props: Props,
    pub named: Named,
    pub tags: Tags,
    all_tags: HashMap<String, Arc<data::BlockSet>>,
    facts: Vec<StateFacts>,
    shapes: Vec<VoxelShape>,
    shape_info: Vec<data::ShapeInfo>,
    pub fluid_tags: data::FluidTags,
    blocks: Vec<BlockData>,
    items: HashMap<String, ItemBinding>,
    liquid_blocks: FluidBlockBindings,
    dynamic: HashMap<BlockStateId, crate::dynamic_support::DynamicSupport>,
    occlusion_cache: Mutex<HashMap<(i32, i32, u8), bool>>,
    growth: growth::Growth,
}

impl Behaviours {
    pub fn load(registry: &BlockRegistry, resources_dir: &Path) -> io::Result<Self> {
        data::load(registry, resources_dir)
    }
    pub fn load_with_gameplay<G: GameplayStateSource>(
        registry: &BlockRegistry,
        resources_dir: &Path,
        behaviour: &serde_json::Value,
        gameplay: &[G],
    ) -> io::Result<Self> {
        data::load_with_gameplay(registry, resources_dir, behaviour, gameplay)
    }

    pub fn load_with_behaviour_json(
        registry: &BlockRegistry,
        resources_dir: &Path,
        behaviour: &serde_json::Value,
    ) -> io::Result<Self> {
        data::load_with_behaviour_json(registry, resources_dir, behaviour)
    }

    pub fn set_cache_dir(dir: std::path::PathBuf) {
        data::set_cache_dir(dir);
    }
    pub fn facts(&self, state: BlockStateId) -> &StateFacts {
        &self.facts[state.0 as usize]
    }
    pub fn block_data(&self, block: BlockId) -> &BlockData {
        &self.blocks[block.0 as usize]
    }
    pub fn data(&self, state: BlockStateId) -> &BlockData {
        self.block_data(self.table.block(state))
    }
    pub fn block(&self, state: BlockStateId) -> BlockId {
        self.table.block(state)
    }
    pub fn item_binding(&self, item: &str) -> Option<&ItemBinding> {
        self.items.get(item)
    }
    pub fn tag(&self, name: &str) -> Option<&data::BlockSet> {
        self.all_tags.get(name).map(|s| &**s)
    }
    pub fn fluid(&self, state: BlockStateId) -> FluidState {
        self.facts(state).fluid
    }
    pub fn legacy_block(&self, fluid: FluidState) -> BlockStateId {
        self.liquid_blocks.block(fluid)
    }
    pub fn collision_shape(&self, state: BlockStateId) -> Option<&VoxelShape> {
        let index = self.facts(state).shape;
        (index >= 0).then(|| &self.shapes[index as usize])
    }
    pub fn shape_info(&self, state: BlockStateId) -> data::ShapeInfo {
        let index = self.facts(state).shape;
        if index < 0 {
            data::ShapeInfo::default()
        } else {
            self.shape_info[index as usize]
        }
    }
    pub(crate) fn has_flag(&self, state: BlockStateId, flag: u32) -> bool {
        self.facts(state).flags & flag != 0
    }
    pub fn is_air(&self, state: BlockStateId) -> bool {
        self.has_flag(state, data::state_flags::AIR)
    }
    pub fn is_solid(&self, state: BlockStateId) -> bool {
        self.has_flag(state, data::state_flags::SOLID)
    }
    pub fn is_replaceable(&self, state: BlockStateId) -> bool {
        self.has_flag(state, data::state_flags::REPLACEABLE)
    }
    pub fn is_randomly_ticking(&self, state: BlockStateId) -> bool {
        self.has_flag(state, data::state_flags::RANDOM_TICKS)
    }
    pub fn is_face_sturdy(&self, state: BlockStateId, direction: Direction) -> bool {
        self.facts(state).sturdy & (1 << direction as u32) != 0
    }
    pub fn is_face_sturdy_center(&self, state: BlockStateId, direction: Direction) -> bool {
        self.facts(state).sturdy & (1 << (6 + direction as u32)) != 0
    }
    pub fn is_face_sturdy_rigid(&self, state: BlockStateId, direction: Direction) -> bool {
        self.facts(state).sturdy & (1 << (12 + direction as u32)) != 0
    }
    pub fn fluid_kind(&self, state: BlockStateId) -> FluidKind {
        self.fluid(state).kind()
    }
}

impl Behaviours {
    pub fn set_block<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        state: BlockStateId,
        flags: i32,
    ) -> bool {
        self.sim(level, ticks).set_block(pos, state, flags)
    }
    pub fn copper_chest_from_block<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        copper: BlockStateId,
        facing: Direction,
        pos: BlockPos,
    ) -> Option<BlockStateId> {
        self.sim(level, ticks)
            .copper_chest_from_block(copper, facing, pos)
    }
    pub fn remove_block<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        moved_by_piston: bool,
    ) -> bool {
        self.sim(level, ticks).remove_block(pos, moved_by_piston)
    }
    pub fn destroy_block<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        drop: bool,
    ) -> bool {
        self.sim(level, ticks)
            .destroy_block(pos, drop, flags::UPDATE_LIMIT)
    }
    pub fn player_destroy_block<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        creative: bool,
        correct_tool: bool,
    ) -> bool {
        self.sim(level, ticks)
            .player_destroy(pos, creative, correct_tool)
    }
    pub fn ice_destroyed_by_player<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        destroyed: BlockStateId,
        prevents_melting: bool,
    ) {
        self.sim(level, ticks)
            .ice_destroyed_by_player(pos, destroyed, prevents_melting);
    }
    pub fn turtle_egg_destroyed_by_player<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        destroyed: BlockStateId,
    ) {
        self.sim(level, ticks)
            .turtle_egg_destroyed_by_player(pos, destroyed);
    }
    pub fn fire_state_for<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) -> BlockStateId {
        self.sim(level, ticks).fire_state_for(pos)
    }
    pub fn fire_can_be_placed_at<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        forward: vise_core::geometry::Direction,
    ) -> bool {
        self.sim(level, ticks).fire_can_be_placed_at(pos, forward)
    }
    pub fn can_survive<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        self.sim(level, ticks).can_survive(state, pos)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn update_shape<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        state: BlockStateId,
        pos: BlockPos,
        direction: Direction,
        neighbour_pos: BlockPos,
        neighbour: BlockStateId,
    ) -> BlockStateId {
        self.sim(level, ticks)
            .update_shape(state, pos, direction, neighbour_pos, neighbour)
    }
    pub fn update_from_neighbour_shapes<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockStateId {
        self.sim(level, ticks)
            .update_from_neighbour_shapes(state, pos)
    }
    pub fn update_neighbors_at<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        block: BlockId,
    ) {
        self.sim(level, ticks).update_neighbors_at(pos, block);
    }
    pub fn update_neighbour_for_output_signal<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) {
        let mut sim = self.sim(level, ticks);
        let block = self.block(sim.state(pos));
        sim.update_neighbour_for_output_signal(pos, block);
    }
    pub fn schedule_tick<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        delay: i32,
    ) {
        let mut sim = self.sim(level, ticks);
        let block = self.block(sim.state(pos));
        sim.schedule_tick(pos, block, delay);
    }
    pub fn handle_neighbor_changed<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        changed: BlockId,
    ) {
        let mut sim = self.sim(level, ticks);
        let state = sim.state(pos);
        sim.neighbor_changed(state, pos, changed, false);
    }
    pub fn entity_inside<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) {
        self.sim(level, ticks).entity_inside(pos);
    }
    pub fn boat_inside<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) {
        self.sim(level, ticks).boat_inside(pos);
    }
    pub fn tick_daylight_detector<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) {
        if level.dimension() == Dimension::Nether || level.game_time() % 20 != 0 {
            return;
        }
        let mut sim = self.sim(level, ticks);
        let state = sim.state(pos);
        if sim.b.data(state).is(classes::Class::DaylightDetector) {
            sim.daylight_detector_update_signal(pos, state);
        }
    }
    pub fn entity_on_big_dripleaf<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        on_ground: bool,
        feet_y: f64,
    ) {
        self.sim(level, ticks)
            .big_dripleaf_entity_inside(pos, on_ground, feet_y);
    }
    pub fn bell_projectile_hit<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        face: Direction,
        hit_y: f64,
    ) -> bool {
        let mut sim = self.sim(level, ticks);
        let state = sim.state(pos);
        self.data(state).is(classes::Class::Bell)
            && sim.bell_hit(pos, state, face, hit_y - f64::from(pos.y))
    }
    pub fn bell_attempt_to_ring<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) -> bool {
        let mut sim = self.sim(level, ticks);
        let state = sim.state(pos);
        if !self.data(state).is(classes::Class::Bell) {
            return false;
        }
        let facing = sim.b.facing(state);
        sim.bell_ring(pos, state, facing);
        true
    }
    pub fn has_neighbor_signal<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) -> bool {
        self.sim(level, ticks).has_neighbor_signal(pos)
    }
    pub fn best_neighbor_signal<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
    ) -> i32 {
        self.sim(level, ticks).best_neighbor_signal(pos)
    }
    pub fn target_projectile_hit<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        location: [f64; 3],
        face: [i32; 3],
        arrow: bool,
    ) -> Option<i32> {
        self.sim(level, ticks)
            .target_projectile_hit(pos, location, face, arrow)
    }
}

pub(crate) struct Sim<'a, L: LevelAccess + ?Sized> {
    pub b: &'a Behaviours,
    pub level: &'a mut L,
    pub ticks: &'a mut Ticks,
    pub wire_signal_off: std::cell::Cell<bool>,
}
impl Behaviours {
    pub(crate) fn sim<'a, L: LevelAccess + ?Sized>(
        &'a self,
        level: &'a mut L,
        ticks: &'a mut Ticks,
    ) -> Sim<'a, L> {
        Sim {
            b: self,
            level,
            ticks,
            wire_signal_off: std::cell::Cell::new(false),
        }
    }
}
