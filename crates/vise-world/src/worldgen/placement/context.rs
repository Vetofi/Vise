use crate::worldgen::feature_sort::FeatureIdentity;
use crate::{heightmap::HeightmapType, worldgen::anchor::WorldGenerationContext};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlacementQueryError {
    Unavailable,
    WorkLimit,
    Failed(String),
}
impl std::fmt::Display for PlacementQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkLimit => f.write_str("predicate work limit exceeded"),
            Self::Unavailable => f.write_str("placement world query is unavailable"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for PlacementQueryError {}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerBlock {
    Empty,
    Bedrock,
    Ground,
}
pub trait PlacementContext {
    fn update_shape_at_face(
        &mut self,
        _position: [i32; 3],
        _direction: vise_core::geometry::Direction,
        _flags: i32,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn update_from_neighbour_shapes(
        &mut self,
        _position: [i32; 3],
        _flags: i32,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn ensure_can_write(&mut self, _position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        Ok(true)
    }
    fn section_block(
        &mut self,
        position: [i32; 3],
    ) -> Result<Option<crate::registry::BlockStateId>, PlacementQueryError> {
        if self.outside_build_height(position[1])? {
            return Ok(None);
        }
        self.block_state(position).map(Some)
    }
    fn set_section_block(
        &mut self,
        _position: [i32; 3],
        _state: crate::registry::BlockStateId,
    ) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn set_chunk_block(
        &mut self,
        position: [i32; 3],
        state: crate::registry::BlockStateId,
    ) -> Result<(), PlacementQueryError> {
        self.set_section_block(position, state)
    }
    fn collision_full_block(
        &mut self,
        _state: crate::registry::BlockStateId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn spawn_end_crystal(
        &mut self,
        _position: [f64; 3],
        _yaw: f32,
        _beam_target: Option<[i32; 3]>,
        _invulnerable: bool,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn world_seed(&mut self) -> Result<i64, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn configure_spawner(
        &mut self,
        _position: [i32; 3],
        _entity: &str,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn store_beehive_occupants(
        &mut self,
        _position: [i32; 3],
        _ticks_in_hive: &[i32],
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn state_collision_empty(
        &mut self,
        _state: crate::registry::BlockStateId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn set_block_entity_loot_table(
        &mut self,
        _position: [i32; 3],
        _table: &str,
        _random: &mut impl vise_core::random::RandomSource,
    ) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn configure_end_gateway(
        &mut self,
        _position: [i32; 3],
        _exit: [i32; 3],
        _exact: bool,
    ) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn biome_should_freeze(
        &mut self,
        _biome: crate::worldgen::biome::BiomeId,
        _position: [i32; 3],
        _require_edge: bool,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn biome_should_snow(
        &mut self,
        _biome: crate::worldgen::biome::BiomeId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn block_brightness(&mut self, _position: [i32; 3]) -> Result<i32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn fluid_state(
        &mut self,
        _position: [i32; 3],
    ) -> Result<crate::fluid::FluidState, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn destroy_block_without_entity(
        &mut self,
        _position: [i32; 3],
        _drop_resources: bool,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn sea_level(&mut self) -> Result<i32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn schedule_fluid_tick(
        &mut self,
        _position: [i32; 3],
        _fluid: crate::fluid::FluidKind,
        _delay: i32,
    ) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn mark_postprocessing(&mut self, _position: [i32; 3]) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn water_at(&mut self, _position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn can_occlude(
        &mut self,
        _state: crate::registry::BlockStateId,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn attachment_at(
        &mut self,
        _position: [i32; 3],
        _direction: vise_core::geometry::Direction,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn next_world_boolean(&mut self) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn next_world_float(&mut self) -> Result<f32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn next_world_int(&mut self, _bound: i32) -> Result<i32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn schedule_block_tick(
        &mut self,
        _position: [i32; 3],
        _block: crate::registry::BlockStateId,
        _delay: i32,
    ) -> Result<(), PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }

    fn block_survives(
        &mut self,
        _state: crate::registry::BlockStateId,
        _position: [i32; 3],
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn state_face_sturdy(
        &mut self,
        _state: crate::registry::BlockStateId,
        _position: [i32; 3],
        _direction: vise_core::geometry::Direction,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn face_sturdy(
        &mut self,
        _position: [i32; 3],
        _direction: vise_core::geometry::Direction,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn center_sturdy(
        &mut self,
        _position: [i32; 3],
        _direction: vise_core::geometry::Direction,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn unobstructed_block(&mut self, _position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn biome_identity(
        &mut self,
        _position: [i32; 3],
    ) -> Result<crate::worldgen::biome::BiomeId, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn top_feature(&self) -> Option<FeatureIdentity> {
        None
    }
    fn biome_allows_feature(
        &mut self,
        _position: [i32; 3],
        _feature: FeatureIdentity,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn block_state(
        &mut self,
        _position: [i32; 3],
    ) -> Result<crate::registry::BlockStateId, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn block_matches_tag(
        &mut self,
        _position: [i32; 3],
        _tag: &str,
    ) -> Result<bool, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn inside_build_height(&mut self, position: [i32; 3]) -> Result<bool, PlacementQueryError> {
        Ok(position[1] >= self.min_y()? && position[1] <= self.max_y()?)
    }
    fn outside_build_height(&mut self, y: i32) -> Result<bool, PlacementQueryError> {
        Ok(y < self.min_y()? || y > self.max_y()?)
    }
    fn layer_block(&mut self, _position: [i32; 3]) -> Result<LayerBlock, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
    fn add_entity(&mut self, _nbt: crate::nbt::Compound) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn set_block_entity(
        &mut self,
        _position: [i32; 3],
        _nbt: crate::nbt::Compound,
    ) -> Result<(), PlacementQueryError> {
        Ok(())
    }
    fn generation(&self) -> WorldGenerationContext;
    fn predicate_generation(&mut self) -> Result<WorldGenerationContext, PlacementQueryError> {
        Ok(self.generation())
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError>;
    fn height(&mut self, kind: HeightmapType, x: i32, z: i32) -> Result<i32, PlacementQueryError>;
    fn any_height_at_least(
        &mut self,
        _kind: HeightmapType,
        _corner: [i32; 2],
        _size: i32,
        _y: i32,
    ) -> Result<Option<bool>, PlacementQueryError> {
        Ok(None)
    }
}
impl PlacementContext for WorldGenerationContext {
    fn max_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.min_y.wrapping_add(self.height).wrapping_sub(1))
    }
    fn generation(&self) -> WorldGenerationContext {
        *self
    }
    fn min_y(&mut self) -> Result<i32, PlacementQueryError> {
        Ok(self.min_y)
    }
    fn height(&mut self, _: HeightmapType, _: i32, _: i32) -> Result<i32, PlacementQueryError> {
        Err(PlacementQueryError::Unavailable)
    }
}
