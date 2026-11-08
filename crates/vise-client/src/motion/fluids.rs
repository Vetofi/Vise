use super::*;
use vise_world::{
    entity_fluid::{
        CurrentTarget, EntityFluidInteraction, FluidInteractionWorld, FluidMembership,
        FluidSections, FluidTagId,
    },
    entity_fluid_tick::{self, EntityFluidTickWorld},
    fluid::{FluidHeightWorld, FluidKind, FluidState, FluidStateTable},
    fluid_flow::{FluidFlowWorld, flow_at},
    fluid_shape::FluidShapeCache,
    water_splash::{self, SplashParticle, WaterSplashWorld},
};
use std::sync::Mutex;

pub const WATER: FluidTagId = FluidTagId(0);
pub const LAVA: FluidTagId = FluidTagId(1);

pub struct FluidResources {
    states: FluidStateTable,
    sturdy: Vec<u8>,
    blocks_flow: Vec<bool>,
    ice: Vec<bool>,
    pub shapes: Mutex<FluidShapeCache>,
}
impl FluidResources {
    pub fn new(assets: &Assets, facts: &crate::blocks::BlockFacts) -> Result<Self> {
        Ok(Self {
            states: FluidStateTable::new(assets.registry)?,
            sturdy: facts.states.iter().map(|s| s.sturdy_full).collect(),
            blocks_flow: facts
                .states
                .iter()
                .map(|s| assets.blocks_fluid_flow.contains(&*s.name))
                .collect(),
            ice: facts
                .states
                .iter()
                .map(|s| s.has_class("IceBlock"))
                .collect(),
            shapes: Mutex::new(FluidShapeCache::default()),
        })
    }
    pub fn fluid(&self, state: BlockStateId) -> FluidState {
        self.states.get(state).unwrap_or(FluidState::EMPTY)
    }
    pub fn sturdy(&self, state: BlockStateId, d: Direction) -> bool {
        self.sturdy
            .get(state.0 as usize)
            .is_some_and(|s| s & 1 << d as u8 != 0)
    }
    pub fn blocks_flow(&self, state: BlockStateId) -> bool {
        self.blocks_flow
            .get(state.0 as usize)
            .copied()
            .unwrap_or(false)
    }
    pub fn ice(&self, state: BlockStateId) -> bool {
        self.ice.get(state.0 as usize).copied().unwrap_or(false)
    }
}
pub struct PlayerFluids {
    pub interaction: EntityFluidInteraction,
    pub was_touching_water: bool,
    pub was_eye_in_water: bool,
    pub first_tick: bool,
    pub fast_lava: bool,
}
impl Default for PlayerFluids {
    fn default() -> Self {
        Self {
            interaction: EntityFluidInteraction::new([WATER, LAVA]),
            was_touching_water: false,
            was_eye_in_water: false,
            first_tick: true,
            fast_lava: false,
        }
    }
}
fn is_tagged(kind: FluidKind, tag: FluidTagId) -> bool {
    if tag == WATER {
        matches!(kind, FluidKind::Water | FluidKind::FlowingWater)
    } else {
        matches!(kind, FluidKind::Lava | FluidKind::FlowingLava)
    }
}
impl PlayerMotion {
    pub fn fluid_height(&self, tag: FluidTagId) -> f64 {
        self.fluids.interaction.fluid_height(&FluidTags, tag)
    }
    pub fn is_eye_in_fluid(&self, tag: FluidTagId) -> bool {
        self.fluids.interaction.is_eye_in_fluid(&FluidTags, tag)
    }
    pub fn is_under_water(&self) -> bool {
        self.fluids.was_eye_in_water
    }
    pub fn is_in_lava(&self) -> bool {
        !self.fluids.first_tick && self.fluids.interaction.is_in_fluid(&FluidTags, LAVA)
    }
}
pub fn bubble_column_push(
    world: &World,
    registry: &vise_world::registry::BlockRegistry,
    facts: &crate::blocks::BlockFacts,
    collision: &vise_world::block_collision::BlockCollisionRegistry<'_>,
    bounds: Aabb,
    velocity: &mut Vec3,
) -> bool {
    let b = bounds.deflate(Vec3::new(
        f64::from(1.0e-5f32),
        f64::from(1.0e-5f32),
        f64::from(1.0e-5f32),
    ));
    let (lo, hi) = (b.min_position(), b.max_position());
    let mut reset_fall = false;
    for x in lo.x.floor() as i32..=(hi.x.ceil() as i32 - 1) {
        for y in lo.y.floor() as i32..=(hi.y.ceil() as i32 - 1) {
            for z in lo.z.floor() as i32..=(hi.z.ceil() as i32 - 1) {
                if let Some(column) =
                    bubble_column_at(world, registry, facts, collision, BlockPos::new(x, y, z))
                {
                    reset_fall |= column.push(velocity);
                }
            }
        }
    }
    reset_fall
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BubbleColumn {
    pub drag: bool,
    pub surface: bool,
}
impl BubbleColumn {
    pub fn push(self, velocity: &mut Vec3) -> bool {
        velocity.y = match (self.surface, self.drag) {
            (true, true) => (-0.9_f64).max(velocity.y - 0.03),
            (true, false) => 1.8_f64.min(velocity.y + 0.1),
            (false, true) => (-0.3_f64).max(velocity.y - 0.03),
            (false, false) => 0.7_f64.min(velocity.y + 0.06),
        };
        !self.surface
    }
}
pub fn bubble_column_at(
    world: &World,
    registry: &vise_world::registry::BlockRegistry,
    facts: &crate::blocks::BlockFacts,
    collision: &vise_world::block_collision::BlockCollisionRegistry<'_>,
    pos: BlockPos,
) -> Option<BubbleColumn> {
    let state = world.block(pos.x, pos.y, pos.z);
    if &*facts.get(state).name != "minecraft:bubble_column" {
        return None;
    }
    let drag = registry
        .state(state)
        .and_then(|s| s.properties.get("drag"))
        .is_some_and(|d| d == "true");
    let above = world.block(pos.x, pos.y + 1, pos.z);
    let mut scene = SceneCollisions {
        world,
        binding: collision,
        subject: None,
    };
    let context = vise_world::collision_context::CollisionContext::empty(false);
    let surface = collision
        .collision_shape_with_context(&mut scene, above, pos, &context)
        .is_ok_and(|shape| shape.is_empty())
        && facts.get(above).fluid == crate::blocks::Fluid::None;
    Some(BubbleColumn { drag, surface })
}
pub struct FluidTags;
impl FluidMembership for FluidTags {
    fn fluid_in_tag(&self, kind: FluidKind, tag: FluidTagId) -> bool {
        is_tagged(kind, tag)
    }
}
impl MotionWorld<'_, '_, '_> {
    pub(super) fn fluid_at(&self, p: BlockPos) -> FluidState {
        self.resources.fluids.fluid(self.block(p))
    }
    pub fn fluid_base_tick(&mut self) -> Result<()> {
        self.player.fluids.was_eye_in_water = self.player.is_eye_in_fluid(WATER);
        self.update_fluids()?;
        self.player.swimming = !self.player.flying
            && self.player.sprinting
            && if self.player.swimming {
                self.player.fluids.was_touching_water
            } else {
                self.player.is_under_water()
                    && is_tagged(
                        self.fluid_at(BlockPos::containing(self.player.position))
                            .kind(),
                        WATER,
                    )
            };
        self.tick_remaining_fire()?;
        if self.player.is_in_lava() {
            self.player.fall_distance *= 0.5;
        }
        self.player.fluids.first_tick = false;
        Ok(())
    }
    pub fn update_fluids(&mut self) -> Result<bool> {
        let mut interaction = std::mem::take(&mut self.player.fluids.interaction);
        let result = entity_fluid_tick::update(&mut interaction, self, WATER, LAVA, 1_000_000);
        self.player.fluids.interaction = interaction;
        result.map_err(|e| e.to_string().into())
    }
    pub(super) fn water_clip(&mut self, from: Vec3, to: Vec3, p: BlockPos) -> Result<bool> {
        let fluid = self.fluid_at(p);
        if !is_tagged(fluid.kind(), WATER) {
            return Ok(false);
        }
        let resources = self.resources;
        let shape = resources
            .fluids
            .shapes
            .lock()
            .map_err(|_| "fluid shape cache poisoned")?
            .shape(self, fluid, p)?;
        Ok(shape.clip(from, to, p).is_some())
    }
}
impl FluidMembership for MotionWorld<'_, '_, '_> {
    fn fluid_in_tag(&self, kind: FluidKind, tag: FluidTagId) -> bool {
        is_tagged(kind, tag)
    }
}
impl CurrentTarget for MotionWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    fn is_player_class(&self) -> bool {
        true
    }
    fn velocity(&mut self) -> Result<Vec3> {
        Ok(self.player.velocity)
    }
    fn add_delta_movement(&mut self, delta: Vec3) -> Result<()> {
        let v = self.player.velocity;
        let sum = Vec3::new(v.x + delta.x, v.y + delta.y, v.z + delta.z);
        if delta.is_finite() && sum.is_finite() {
            self.player.velocity = sum;
        }
        Ok(())
    }
}
impl FluidInteractionWorld for MotionWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    type Chunk = ();
    fn interaction_box(&mut self) -> Result<Option<Aabb>> {
        Ok(Some(
            self.player.bounds().deflate(Vec3::new(0.001, 0.001, 0.001)),
        ))
    }
    fn loaded_chunk(&mut self, x: i32, z: i32) -> Result<Option<()>> {
        Ok(self.scene.chunks.contains_key(&(x, z)).then_some(()))
    }
    fn chunk_sections(&mut self, _: ()) -> Result<FluidSections<'_>> {
        let layout = self.scene.layout();
        Ok(FluidSections {
            minimum_section_y: layout.min_y() >> 4,
            has_fluid: crate::chunk::every_section(layout),
        })
    }
    fn bounding_box_min_y(&self) -> f64 {
        self.player.bounds().min_position().y
    }
    fn block_x(&self) -> i32 {
        self.player.position.x.floor() as i32
    }
    fn eye_y(&mut self) -> Result<f64> {
        Ok(self.player.position.y + f64::from(self.player.dimensions.eye_height))
    }
    fn block_z(&self) -> i32 {
        self.player.position.z.floor() as i32
    }
    fn fluid_state(&mut self, p: BlockPos) -> Result<FluidState> {
        Ok(self.fluid_at(p))
    }
    fn fluid_height(&mut self, f: FluidState, p: BlockPos) -> Result<f32> {
        f.height_at(self, [p.x, p.y, p.z])
    }
    fn camera_height(&mut self, f: FluidState, p: BlockPos) -> Result<f32> {
        f.camera_height_at(self, [p.x, p.y, p.z])
    }
    fn flow(&mut self, f: FluidState, p: BlockPos) -> Result<Vec3> {
        flow_at(self, f, p)
    }
}
impl EntityFluidTickWorld for MotionWorld<'_, '_, '_> {
    fn is_pushed_by_fluid(&mut self) -> Result<bool> {
        Ok(!self.player.flying)
    }
    fn reset_fall_distance(&mut self) -> Result<()> {
        self.player.fall_distance = 0.0;
        Ok(())
    }
    fn was_touching_water(&self) -> bool {
        self.player.fluids.was_touching_water
    }
    fn first_tick(&self) -> bool {
        self.player.fluids.first_tick
    }
    fn water_splash(&mut self) -> Result<()> {
        water_splash::water_splash(self, 1_000_000).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn set_was_touching_water(&mut self, value: bool) {
        self.player.fluids.was_touching_water = value;
    }
    fn fast_lava(&mut self) -> Result<bool> {
        Ok(self.player.fluids.fast_lava)
    }
}
impl FluidHeightWorld for MotionWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    fn fluid(&mut self, p: [i32; 3]) -> Result<FluidState> {
        Ok(self.fluid_at(BlockPos::new(p[0], p[1], p[2])))
    }
    fn sturdy_down(&mut self, p: [i32; 3]) -> Result<bool> {
        let state = MotionWorld::block(self, BlockPos::new(p[0], p[1], p[2]));
        Ok(self.resources.fluids.sturdy(state, Direction::Down))
    }
}
impl FluidFlowWorld for MotionWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    type Block = BlockStateId;
    fn fluid(&mut self, p: BlockPos) -> Result<FluidState> {
        Ok(self.fluid_at(p))
    }
    fn block(&mut self, p: BlockPos) -> Result<BlockStateId> {
        Ok(MotionWorld::block(self, p))
    }
    fn blocks_fluid_flow(&self, s: BlockStateId) -> bool {
        self.resources.fluids.blocks_flow(s)
    }
    fn is_ice_class(&self, s: BlockStateId) -> bool {
        self.resources.fluids.ice(s)
    }
    fn is_face_sturdy(&mut self, s: BlockStateId, _: BlockPos, d: Direction) -> Result<bool> {
        Ok(self.resources.fluids.sturdy(s, d))
    }
}
impl WaterSplashWorld for MotionWorld<'_, '_, '_> {
    type Error = Box<dyn std::error::Error>;
    type Entity = ();
    type Sound = bool;
    fn this_entity(&self) {}
    fn controlling_passenger(&mut self) -> Result<Option<()>> {
        Ok(None)
    }
    fn velocity(&mut self, _: ()) -> Result<Vec3> {
        Ok(self.player.velocity)
    }
    fn splash_sound(&mut self, high_speed: bool) -> Result<bool> {
        Ok(high_speed)
    }
    fn random_float(&mut self) -> Result<f32> {
        Ok(self.scene.random.next_float())
    }
    fn random_double(&mut self) -> Result<f64> {
        Ok(self.scene.random.next_double())
    }
    fn play_sound(&mut self, high_speed: bool, volume: f32, pitch: f32) -> Result<()> {
        self.player.events.push(MotionEvent::Splash {
            high_speed,
            volume,
            pitch,
        });
        Ok(())
    }
    fn position(&self) -> Vec3 {
        self.player.position
    }
    fn width(&self) -> f32 {
        self.player.dimensions.width
    }
    fn particle(&mut self, kind: SplashParticle, position: Vec3, velocity: Vec3) -> Result<()> {
        self.player.events.push(MotionEvent::SplashParticle {
            bubble: matches!(kind, SplashParticle::Bubble),
            position,
            velocity,
        });
        Ok(())
    }
    fn splash_event(&mut self) -> Result<()> {
        self.player.events.push(MotionEvent::SplashEvent);
        Ok(())
    }
}
