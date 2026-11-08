use crate::fluid::{FluidHeightWorld, FluidKind, FluidState};
use vise_core::{
    geometry::{Aabb, Vec3, VoxelShape},
    position::BlockPos,
};

pub struct FluidShapeCache {
    shapes: Vec<Option<VoxelShape>>,
}
impl Default for FluidShapeCache {
    fn default() -> Self {
        Self {
            shapes: vec![None; 5 * 18],
        }
    }
}
impl FluidShapeCache {
    pub fn cached_states(&self) -> usize {
        self.shapes.iter().filter(|v| v.is_some()).count()
    }
    pub fn shape<W: FluidHeightWorld>(
        &mut self,
        world: &mut W,
        state: FluidState,
        position: BlockPos,
    ) -> Result<VoxelShape, W::Error> {
        if state.kind() == FluidKind::Empty {
            return Ok(VoxelShape::empty());
        }
        let index = state.kind() as usize * 18
            + usize::from(state.falling()) * 9
            + usize::from(state.amount());
        if let Some(shape) = &self.shapes[index] {
            return Ok(shape.clone());
        }
        let height = f64::from(state.height_at(world, [position.x, position.y, position.z])?);
        let shape = VoxelShape::from_box(Aabb::new(Vec3::ZERO, Vec3::new(1.0, height, 1.0)))
            .expect("validated pinned fluid height");
        self.shapes[index] = Some(shape.clone());
        Ok(shape)
    }
}
