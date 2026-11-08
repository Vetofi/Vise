use crate::{Result, world::World};
use vise_core::{
    geometry::{Aabb, Vec3, VoxelShape, border::BorderGeometry},
    position::{BlockPos, ChunkPos},
};
use vise_world::{
    block_collision::{BlockCollisionRegistry, DynamicCollisionWorld, ShulkerCollisionState},
    block_collision_scan::{BlockCollisionWorld, CollisionBlock},
    collision::CachedCollisionRegistry,
    collision_context::{CollisionContext, CollisionEntity, CollisionItem},
    dynamic_support::DynamicSupportWorld,
    entity_obstruction::EntityIdentity,
    fluid::{FluidKind, FluidState},
    liquid_collision::LiquidCollisionWorld,
    piston_shape::PistonCollisionWorld,
    powder_snow_collision::{PowderSnowEntity, PowderSnowWorld},
    registry::{BlockRegistry, BlockStateId, BlockTableFile},
    world_movement::{
        CollisionCheckRequest, EntityMovementWorld, MovementRequest, no_world_collision,
        resolve_world_movement,
    },
};
use std::{io, path::Path, sync::Arc};

pub fn load<'r>(root: &Path, registry: &'r BlockRegistry) -> Result<BlockCollisionRegistry<'r>> {
    let file = BlockTableFile::open(&root.join("cached-collision.nbt"), registry)?;
    let cached = CachedCollisionRegistry::from_table(file.table("collision")?)?;
    if !cached.has_canonical_identities() {
        return Err("scene collisions require canonical shape identities".into());
    }
    Ok(BlockCollisionRegistry::new(Arc::new(cached))?)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PowderSnowSubject {
    pub fall_distance: f64,
    pub falling_block: bool,
    pub leather_boots: bool,
    pub walkable_type: bool,
    pub lava_walker: bool,
}
fn strider_liquid_shape() -> VoxelShape {
    VoxelShape::from_cube([1, 2, 1], &[true, false]).expect("a valid half-block column")
}

thread_local! {
    static PISTON_NOCLIP: std::cell::Cell<Option<vise_core::geometry::Direction>> = const { std::cell::Cell::new(None) };
}

pub fn with_piston_noclip<T>(
    direction: vise_core::geometry::Direction,
    f: impl FnOnce() -> T,
) -> T {
    struct Reset(Option<vise_core::geometry::Direction>);
    impl Drop for Reset {
        fn drop(&mut self) {
            PISTON_NOCLIP.with(|value| value.set(self.0));
        }
    }
    let previous = PISTON_NOCLIP.with(|value| value.replace(Some(direction)));
    let _reset = Reset(previous);
    f()
}

pub struct SceneCollisions<'w, 'r> {
    pub world: &'w World,
    pub binding: &'w BlockCollisionRegistry<'r>,
    pub subject: Option<PowderSnowSubject>,
}
pub struct EntityCollisions<'a, W> {
    pub blocks: W,
    pub entities: &'a [Aabb],
    pub border: Option<(&'a BorderGeometry, [f64; 2])>,
}
impl<W: BlockCollisionWorld> BlockCollisionWorld for EntityCollisions<'_, W> {
    type Error = W::Error;
    type Chunk = W::Chunk;
    fn collision_chunk(
        &mut self,
        p: ChunkPos,
    ) -> std::result::Result<Option<Self::Chunk>, Self::Error> {
        self.blocks.collision_chunk(p)
    }
    fn collision_block(
        &mut self,
        chunk: Self::Chunk,
        p: BlockPos,
    ) -> std::result::Result<CollisionBlock, Self::Error> {
        self.blocks.collision_block(chunk, p)
    }
    fn is_suffocating(
        &mut self,
        chunk: Self::Chunk,
        block: CollisionBlock,
        p: BlockPos,
    ) -> std::result::Result<bool, Self::Error> {
        self.blocks.is_suffocating(chunk, block, p)
    }
    fn may_reach_out(
        &mut self,
        chunk: Self::Chunk,
        section_y: i32,
    ) -> std::result::Result<bool, Self::Error> {
        self.blocks.may_reach_out(chunk, section_y)
    }
    fn context_shape(
        &mut self,
        block: CollisionBlock,
        p: BlockPos,
        context: &CollisionContext<'_>,
    ) -> std::result::Result<VoxelShape, Self::Error> {
        self.blocks.context_shape(block, p, context)
    }
}
impl<W: EntityMovementWorld<Error = io::Error>> EntityMovementWorld for EntityCollisions<'_, W> {
    type Error = io::Error;
    fn entity_collisions(
        &mut self,
        entity: EntityIdentity,
        bounds: Aabb,
    ) -> io::Result<Vec<VoxelShape>> {
        let mut shapes = self.blocks.entity_collisions(entity, bounds)?;
        for &aabb in self.entities {
            if aabb.intersects(bounds.inflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7))) {
                shapes.push(VoxelShape::from_box(aabb).map_err(io::Error::other)?);
            }
        }
        Ok(shapes)
    }
    fn movement_border(
        &mut self,
        entity: EntityIdentity,
        bounds: Aabb,
    ) -> io::Result<Option<VoxelShape>> {
        let Some((border, [x, z])) = self.border else {
            return self.blocks.movement_border(entity, bounds);
        };
        if !border.is_inside_close_to_border(x, z, bounds) {
            return Ok(None);
        }
        border.collision_shape().map(Some).map_err(io::Error::other)
    }
}
impl<'a, W> EntityCollisions<'a, W>
where
    W: BlockCollisionWorld<Error = io::Error> + EntityMovementWorld<Error = io::Error>,
{
    pub fn with_border(blocks: W, border: &'a BorderGeometry, at: [f64; 2]) -> Self {
        Self {
            blocks,
            entities: &[],
            border: Some((border, at)),
        }
    }
    pub fn resolve(
        &mut self,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
    ) -> Result<Vec3> {
        Ok(resolve_world_movement(
            self,
            MovementRequest {
                entity: EntityIdentity(0),
                context: CollisionContext::position(bounds.min_position().y),
                bounds,
                movement,
                max_step,
                on_ground,
                candidate_budget: 1_000_000,
            },
        )?)
    }
}
impl SceneCollisions<'_, '_> {
    pub fn resolve(
        &mut self,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
    ) -> Result<Vec3> {
        self.resolve_with_context(bounds, movement, max_step, on_ground, self.context(bounds))
    }
    pub fn resolve_with_context(
        &mut self,
        bounds: Aabb,
        movement: Vec3,
        max_step: f32,
        on_ground: bool,
        context: CollisionContext<'_>,
    ) -> Result<Vec3> {
        Ok(resolve_world_movement(
            self,
            MovementRequest {
                entity: EntityIdentity(0),
                context,
                bounds,
                movement,
                max_step,
                on_ground,
                candidate_budget: 1_000_000,
            },
        )?)
    }
    fn context(&self, bounds: Aabb) -> CollisionContext<'static> {
        let y = bounds.min_position().y;
        if !self.subject.is_some_and(|s| s.lava_walker) {
            return CollisionContext::position(y);
        }
        CollisionContext::for_entity(
            CollisionEntity {
                identity: EntityIdentity(0),
                y,
                descending: false,
                main_hand: Some(CollisionItem::EMPTY),
            },
            false,
        )
    }
    pub fn unobstructed(&mut self, bounds: Aabb) -> Result<bool> {
        Ok(no_world_collision(
            self,
            CollisionCheckRequest {
                entity: EntityIdentity(0),
                context: CollisionContext::position(bounds.min_position().y),
                bounds,
                candidate_budget: 1_000_000,
            },
        )?)
    }
}
pub fn no_collision_empty<W, E>(world: &mut W, bounds: Aabb) -> bool
where
    W: BlockCollisionWorld<Error = E> + EntityMovementWorld<Error = E>,
{
    no_collision_in(world, bounds, false)
}
pub fn no_collision_or_liquid<W, E>(world: &mut W, bounds: Aabb) -> bool
where
    W: BlockCollisionWorld<Error = E> + EntityMovementWorld<Error = E>,
{
    no_collision_in(world, bounds, true)
}
fn no_collision_in<W, E>(world: &mut W, bounds: Aabb, fluids: bool) -> bool
where
    W: BlockCollisionWorld<Error = E> + EntityMovementWorld<Error = E>,
{
    no_world_collision(
        world,
        CollisionCheckRequest {
            entity: EntityIdentity(0),
            context: CollisionContext::empty(fluids),
            bounds,
            candidate_budget: 1_000_000,
        },
    )
    .unwrap_or(false)
}
impl<'w> BlockCollisionWorld for SceneCollisions<'w, '_> {
    type Error = io::Error;
    type Chunk = &'w crate::chunk::Chunk;
    fn collision_chunk(&mut self, p: ChunkPos) -> io::Result<Option<Self::Chunk>> {
        Ok(self.world.chunks.get(&(p.x, p.z)).map(|chunk| &**chunk))
    }
    fn collision_block(&mut self, chunk: Self::Chunk, p: BlockPos) -> io::Result<CollisionBlock> {
        self.binding.collision_block(chunk.block(p.x, p.y, p.z))
    }
    fn may_reach_out(&mut self, chunk: Self::Chunk, section_y: i32) -> io::Result<bool> {
        let blocks = usize::try_from(section_y - (chunk.min_y() >> 4))
            .ok()
            .and_then(|s| chunk.sections.get(s))
            .map(|section| &section.blocks);
        Ok(blocks.is_none_or(|blocks| {
            blocks.is_global() || blocks.maybe_has(|v| self.binding.reaches_out(BlockStateId(v)))
        }))
    }
    fn is_suffocating(
        &mut self,
        _: Self::Chunk,
        _: CollisionBlock,
        _: BlockPos,
    ) -> io::Result<bool> {
        Ok(false)
    }
    fn context_shape(
        &mut self,
        block: CollisionBlock,
        p: BlockPos,
        context: &CollisionContext<'_>,
    ) -> io::Result<VoxelShape> {
        self.binding
            .collision_shape_with_context(self, block.state, p, context)
            .map_err(io::Error::other)
    }
}
impl EntityMovementWorld for SceneCollisions<'_, '_> {
    type Error = io::Error;
    fn entity_collisions(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Vec<VoxelShape>> {
        Ok(Vec::new())
    }
    fn movement_border(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Option<VoxelShape>> {
        Ok(None)
    }
}
impl DynamicSupportWorld for SceneCollisions<'_, '_> {
    type Error = io::Error;
    fn shulker_is_closed(&mut self, _: [i32; 3]) -> io::Result<Option<bool>> {
        Ok(None)
    }
    fn moving_piston_shape(&mut self, p: [i32; 3]) -> io::Result<Option<VoxelShape>> {
        let Some(&(_, piston)) = self.world.moving_pistons.iter().find(|(q, _)| *q == p) else {
            return Ok(None);
        };
        let binding = self.binding;
        let Some(pistons) = binding.pistons() else {
            return Ok(None);
        };
        pistons
            .collision_shape(self, p, piston, PISTON_NOCLIP.with(|value| value.get()))
            .map(Some)
            .map_err(io::Error::other)
    }
}
impl PistonCollisionWorld for SceneCollisions<'_, '_> {
    type Error = io::Error;
    fn collision_shape(&mut self, state: BlockStateId, p: [i32; 3]) -> io::Result<VoxelShape> {
        let binding = self.binding;
        binding
            .collision_shape(self, state, p)
            .map_err(io::Error::other)
    }
}
impl DynamicCollisionWorld for SceneCollisions<'_, '_> {
    fn shulker_collision_state(
        &mut self,
        _: [i32; 3],
    ) -> io::Result<Option<ShulkerCollisionState>> {
        Ok(None)
    }
}
impl LiquidCollisionWorld for SceneCollisions<'_, '_> {
    type Error = io::Error;
    fn entity_liquid_shape(&mut self, _: EntityIdentity) -> io::Result<VoxelShape> {
        Ok(if self.subject.is_some_and(|s| s.lava_walker) {
            strider_liquid_shape()
        } else {
            VoxelShape::empty()
        })
    }
    fn fluid(&mut self, p: BlockPos) -> io::Result<FluidState> {
        FluidState::from_block(
            self.binding
                .registry()
                .state(self.world.block(p.x, p.y, p.z))
                .ok_or_else(|| io::Error::other("scene fluid state outside registry"))?,
        )
    }
    fn entity_can_stand(&mut self, _: EntityIdentity, fluid: FluidState) -> io::Result<bool> {
        Ok(self.subject.is_some_and(|s| s.lava_walker)
            && matches!(fluid.kind(), FluidKind::Lava | FluidKind::FlowingLava))
    }
}
impl PowderSnowWorld for SceneCollisions<'_, '_> {
    type Error = io::Error;
    fn powder_snow_entity(&mut self, _: EntityIdentity) -> io::Result<PowderSnowEntity> {
        let subject = self.subject.unwrap_or_default();
        Ok(PowderSnowEntity {
            fall_distance: subject.fall_distance,
            falling_block: subject.falling_block,
        })
    }
    fn powder_snow_walkable_tag(&mut self, _: EntityIdentity) -> io::Result<bool> {
        Ok(self.subject.is_some_and(|s| s.walkable_type))
    }
    fn feet_are_leather_boots(&mut self, _: EntityIdentity) -> io::Result<bool> {
        Ok(self.subject.is_some_and(|s| s.leather_boots))
    }
}

pub struct EmptyCollisions;

impl BlockCollisionWorld for EmptyCollisions {
    type Error = io::Error;
    type Chunk = ChunkPos;
    fn collision_chunk(&mut self, _: ChunkPos) -> io::Result<Option<ChunkPos>> {
        Ok(None)
    }
    fn collision_block(&mut self, _: ChunkPos, _: BlockPos) -> io::Result<CollisionBlock> {
        Err(io::Error::other("no chunk is resident"))
    }
    fn is_suffocating(&mut self, _: ChunkPos, _: CollisionBlock, _: BlockPos) -> io::Result<bool> {
        Ok(false)
    }
    fn context_shape(
        &mut self,
        _: CollisionBlock,
        _: BlockPos,
        _: &CollisionContext<'_>,
    ) -> io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
}

impl EntityMovementWorld for EmptyCollisions {
    type Error = io::Error;
    fn entity_collisions(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Vec<VoxelShape>> {
        Ok(Vec::new())
    }
    fn movement_border(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Option<VoxelShape>> {
        Ok(None)
    }
}

pub struct LookupCollisions<'w, 'r> {
    pub block: &'w dyn Fn(i32, i32, i32) -> vise_world::registry::BlockStateId,
    pub binding: &'w BlockCollisionRegistry<'r>,
}
impl BlockCollisionWorld for LookupCollisions<'_, '_> {
    type Error = io::Error;
    type Chunk = ChunkPos;
    fn collision_chunk(&mut self, p: ChunkPos) -> io::Result<Option<ChunkPos>> {
        Ok(Some(p))
    }
    fn collision_block(&mut self, _: ChunkPos, p: BlockPos) -> io::Result<CollisionBlock> {
        self.binding.collision_block((self.block)(p.x, p.y, p.z))
    }
    fn is_suffocating(&mut self, _: ChunkPos, _: CollisionBlock, _: BlockPos) -> io::Result<bool> {
        Ok(false)
    }
    fn context_shape(
        &mut self,
        block: CollisionBlock,
        p: BlockPos,
        context: &CollisionContext<'_>,
    ) -> io::Result<VoxelShape> {
        self.binding
            .collision_shape_with_context(self, block.state, p, context)
            .map_err(io::Error::other)
    }
}
impl EntityMovementWorld for LookupCollisions<'_, '_> {
    type Error = io::Error;
    fn entity_collisions(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Vec<VoxelShape>> {
        Ok(Vec::new())
    }
    fn movement_border(&mut self, _: EntityIdentity, _: Aabb) -> io::Result<Option<VoxelShape>> {
        Ok(None)
    }
}
impl DynamicSupportWorld for LookupCollisions<'_, '_> {
    type Error = io::Error;
    fn shulker_is_closed(&mut self, _: [i32; 3]) -> io::Result<Option<bool>> {
        Ok(None)
    }
    fn moving_piston_shape(&mut self, _: [i32; 3]) -> io::Result<Option<VoxelShape>> {
        Ok(None)
    }
}
impl DynamicCollisionWorld for LookupCollisions<'_, '_> {
    fn shulker_collision_state(
        &mut self,
        _: [i32; 3],
    ) -> io::Result<Option<ShulkerCollisionState>> {
        Ok(None)
    }
}
impl LiquidCollisionWorld for LookupCollisions<'_, '_> {
    type Error = io::Error;
    fn entity_liquid_shape(&mut self, _: EntityIdentity) -> io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
    fn fluid(&mut self, p: BlockPos) -> io::Result<FluidState> {
        FluidState::from_block(
            self.binding
                .registry()
                .state((self.block)(p.x, p.y, p.z))
                .ok_or_else(|| io::Error::other("fluid state outside registry"))?,
        )
    }
    fn entity_can_stand(&mut self, _: EntityIdentity, _: FluidState) -> io::Result<bool> {
        Ok(false)
    }
}
impl PowderSnowWorld for LookupCollisions<'_, '_> {
    type Error = io::Error;
    fn powder_snow_entity(&mut self, _: EntityIdentity) -> io::Result<PowderSnowEntity> {
        Ok(PowderSnowEntity {
            fall_distance: 0.0,
            falling_block: false,
        })
    }
    fn powder_snow_walkable_tag(&mut self, _: EntityIdentity) -> io::Result<bool> {
        Ok(false)
    }
    fn feet_are_leather_boots(&mut self, _: EntityIdentity) -> io::Result<bool> {
        Ok(false)
    }
}

#[cfg(test)]
mod piston_noclip_tests {
    use super::*;
    use vise_core::geometry::Direction;

    #[test]
    fn piston_noclip_is_scoped_and_restored_after_unwind() {
        assert_eq!(PISTON_NOCLIP.with(|value| value.get()), None);
        with_piston_noclip(Direction::East, || {
            assert_eq!(
                PISTON_NOCLIP.with(|value| value.get()),
                Some(Direction::East)
            );
            let _ =
                std::panic::catch_unwind(|| with_piston_noclip(Direction::West, || panic!("test")));
            assert_eq!(
                PISTON_NOCLIP.with(|value| value.get()),
                Some(Direction::East)
            );
        });
        assert_eq!(PISTON_NOCLIP.with(|value| value.get()), None);
    }
}

#[cfg(test)]
mod border_movement_tests {
    use super::*;

    fn border() -> BorderGeometry {
        BorderGeometry::new([0.0, 0.0], 10.0, 29_999_984).expect("a border")
    }

    #[test]
    fn a_body_next_to_the_border_stops_at_its_wall() {
        let bounds = Aabb::new(Vec3::new(4.0, 0.0, 0.0), Vec3::new(4.6, 1.8, 0.6));
        let border = border();
        let mut collisions = EntityCollisions::with_border(EmptyCollisions, &border, [4.3, 0.3]);
        let moved = collisions
            .resolve(bounds, Vec3::new(0.51, 0.0, 0.0), 0.0, false)
            .expect("the movement resolves");
        assert!(
            (moved.x - 0.4).abs() < 1.0e-9,
            "the body was not stopped by the border: {moved:?}"
        );
    }

    #[test]
    fn a_body_away_from_the_border_moves_whole() {
        let bounds = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.6, 1.8, 0.6));
        let border = border();
        let mut collisions = EntityCollisions::with_border(EmptyCollisions, &border, [0.3, 0.3]);
        let moved = collisions
            .resolve(bounds, Vec3::new(0.51, 0.0, 0.0), 0.0, false)
            .expect("the movement resolves");
        assert!(
            (moved.x - 0.51).abs() < 1.0e-9,
            "the body did not move the whole way: {moved:?}"
        );
    }
}
