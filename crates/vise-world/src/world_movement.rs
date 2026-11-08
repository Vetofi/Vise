use crate::{
    block_collision_scan::{BlockCollisionScan, BlockCollisionWorld, CollisionScanError},
    collision_context::CollisionContext,
    entity_obstruction::EntityIdentity,
};
use vise_core::geometry::{
    Aabb, Vec3, VoxelShape, entity_collision_bounds, try_resolve_step_movement,
};

pub trait EntityMovementWorld {
    type Error;
    fn entity_collisions(
        &mut self,
        entity: EntityIdentity,
        bounds: Aabb,
    ) -> Result<Vec<VoxelShape>, Self::Error>;
    fn movement_border(
        &mut self,
        entity: EntityIdentity,
        bounds: Aabb,
    ) -> Result<Option<VoxelShape>, Self::Error>;
}

pub struct MovementRequest<'a> {
    pub entity: EntityIdentity,
    pub context: CollisionContext<'a>,
    pub bounds: Aabb,
    pub movement: Vec3,
    pub max_step: f32,
    pub on_ground: bool,
    pub candidate_budget: usize,
}

pub fn resolve_world_movement<W, E>(
    world: &mut W,
    request: MovementRequest<'_>,
) -> Result<Vec3, CollisionScanError<E>>
where
    W: BlockCollisionWorld<Error = E> + EntityMovementWorld<Error = E>,
{
    let entities = world
        .entity_collisions(
            request.entity,
            entity_collision_bounds(request.movement, request.bounds, request.max_step),
        )
        .map_err(CollisionScanError::World)?;
    try_resolve_step_movement(
        request.movement,
        request.bounds,
        request.max_step,
        request.on_ground,
        |bounds| {
            let mut shapes = entities.clone();
            if let Some(border) = world
                .movement_border(request.entity, bounds)
                .map_err(CollisionScanError::World)?
            {
                shapes.push(border);
            }
            for hit in BlockCollisionScan::new(
                world,
                request.context,
                bounds,
                false,
                request.candidate_budget,
            )? {
                shapes.push(hit?.shape);
            }
            Ok(shapes)
        },
    )
}

pub struct CollisionCheckRequest<'a> {
    pub entity: EntityIdentity,
    pub context: CollisionContext<'a>,
    pub bounds: Aabb,
    pub candidate_budget: usize,
}
pub fn no_world_collision<W, E>(
    world: &mut W,
    request: CollisionCheckRequest<'_>,
) -> Result<bool, CollisionScanError<E>>
where
    W: BlockCollisionWorld<Error = E> + EntityMovementWorld<Error = E>,
{
    for hit in BlockCollisionScan::new(
        world,
        request.context,
        request.bounds,
        false,
        request.candidate_budget,
    )? {
        if !hit?.shape.is_empty() {
            return Ok(false);
        }
    }
    if !world
        .entity_collisions(request.entity, request.bounds)
        .map_err(CollisionScanError::World)?
        .is_empty()
    {
        return Ok(false);
    }
    if let Some(border) = world
        .movement_border(request.entity, request.bounds)
        .map_err(CollisionScanError::World)?
    {
        let entity = VoxelShape::from_box(request.bounds).map_err(CollisionScanError::Shape)?;
        return border
            .join_is_not_empty(&entity, vise_core::geometry::ShapeOp::AND)
            .map(|overlap| !overlap)
            .map_err(CollisionScanError::Shape);
    }
    Ok(true)
}
