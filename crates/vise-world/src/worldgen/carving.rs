use super::{
    material_condition::{
        MaterialConditions, MaterialDensitySources, MaterialRule, MaterialSurfaceQueries,
    },
    material_stage::{SurfaceError, TopMaterialInputs, top_material_cell},
    terrain::{TerrainAquifer, TerrainBlockStates, TerrainChunk, TerrainError},
};
use crate::{
    heightmap::HeightmapState,
    registry::{BlockRegistry, BlockStateId},
};
use vise_core::{
    carving::CarvingMask, density::SamplerContext, noise::DensityVolume, position::BlockPos,
    random::RandomSource,
};
use std::collections::BTreeSet;

pub struct CarvingBlocks {
    flags: Vec<u8>,
}
impl CarvingBlocks {
    pub fn from_registry(
        registry: &BlockRegistry,
        uncarvable: &BTreeSet<String>,
    ) -> Result<Self, TerrainError> {
        for name in uncarvable.iter().map(String::as_str).chain([
            "minecraft:grass_block",
            "minecraft:mycelium",
            "minecraft:dirt",
        ]) {
            if registry.default_state(name).is_none() {
                return Err(TerrainError::InvalidState);
            }
        }
        let flags = (0..registry.len())
            .map(|index| {
                let name = &registry.state(BlockStateId(index as u32)).unwrap().name;
                u8::from(uncarvable.contains(name))
                    | (u8::from(matches!(
                        name.as_str(),
                        "minecraft:grass_block" | "minecraft:mycelium"
                    )) << 1)
                    | (u8::from(name == "minecraft:dirt") << 2)
            })
            .collect();
        Ok(Self { flags })
    }
    fn flags(&self, state: BlockStateId) -> Result<u8, TerrainError> {
        self.flags
            .get(state.0 as usize)
            .copied()
            .ok_or(TerrainError::InvalidState)
    }
}

pub fn apply_carving_mask<R: RandomSource, S: MaterialSurfaceQueries, B: TerrainBlockStates>(
    chunk: &mut TerrainChunk<'_, B>,
    mask: &CarvingMask,
    rule: MaterialRule,
    inputs: TopMaterialInputs<'_, R, S>,
    aquifer: &mut (impl TerrainAquifer + ?Sized),
    blocks: &CarvingBlocks,
    mut filter: impl FnMut([i32; 3]) -> bool,
) -> Result<(), SurfaceError>
where
    R::Positional: Clone,
{
    let TopMaterialInputs {
        generation,
        root,
        surface,
        preliminary_surface,
        density_context,
    } = inputs;
    let volume =
        DensityVolume::new([1; 3], [0; 3], [1; 3]).map_err(|_| SurfaceError::InvalidVolume)?;
    let sources = MaterialDensitySources::new(
        surface,
        volume,
        preliminary_surface,
        SamplerContext::default(),
    )?;
    let mut worker =
        MaterialConditions::<R, _>::with_possible_biomes(generation, root, sources, None);
    let rule = worker.add_rule(rule)?;
    let cx = chunk.position().x.wrapping_shl(4);
    let cz = chunk.position().z.wrapping_shl(4);
    for column in mask.columns() {
        let mut found_surface = false;
        let x = cx.wrapping_add(column.x);
        let z = cz.wrapping_add(column.z);
        for y in (column.min_y..=column.max_y).rev() {
            if !filter([column.x, y, column.z]) {
                continue;
            }
            let flags = blocks.flags(chunk.block(x, y, z))?;
            if flags & 1 != 0 {
                continue;
            }
            found_surface |= flags & 2 != 0;
            let Some(state) = aquifer
                .compute_substance(density_context, [x, y, z], 0.)
                .map_err(TerrainError::from)?
            else {
                continue;
            };
            chunk.set_block_worldgen(x, y, z, state)?;
            if aquifer.should_schedule_fluid_update() && chunk.block_properties(state).has_fluid() {
                chunk.mark_postprocessing(x, y, z);
            }
            let below = y.wrapping_sub(1);
            if found_surface && blocks.flags(chunk.block(x, below, z))? & 4 != 0 {
                let submerged = chunk.block_properties(state).has_fluid();
                worker.retarget_surface_densities([x, below, z])?;
                let replacement = top_material_cell(
                    &mut worker,
                    chunk,
                    rule,
                    BlockPos::new(x, below, z),
                    submerged,
                )?;
                if let Some(replacement) = replacement {
                    chunk.set_block_worldgen(x, below, z, replacement)?;
                    if chunk.block_properties(replacement).has_fluid() {
                        chunk.mark_postprocessing(x, below, z);
                    }
                }
            }
        }
    }
    Ok(())
}
