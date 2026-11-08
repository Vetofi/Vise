use super::Game;
use vise_client::{Result, blocks::Fluid};
use vise_core::position::BlockPos;
use std::sync::Arc;

const POSTPROCESS_FLAGS: i32 = 276;

impl Game<'_> {
    pub(super) fn postprocess_chunks(&mut self) -> Result<()> {
        let ready: Vec<(i32, i32)> = self
            .world
            .chunks
            .iter()
            .filter(|(_, c)| c.lit && !c.postprocess.is_empty())
            .map(|(k, _)| *k)
            .filter(|&(x, z)| {
                (-1..=1)
                    .all(|dz| (-1..=1).all(|dx| self.world.chunks.contains_key(&(x + dx, z + dz))))
            })
            .collect();
        if ready.is_empty() {
            return Ok(());
        }
        self.sync_tick_chunks();
        for key in ready {
            let Some(chunk) = self.world.chunks.get_mut(&key) else {
                continue;
            };
            let marks = std::mem::take(&mut Arc::make_mut(chunk).postprocess);
            self.with_level(|b, level, ticks| {
                for [x, y, z] in marks {
                    let pos = BlockPos::new(x, y, z);
                    let state = level.world.block(x, y, z);
                    let facts = level.facts.get(state);
                    let (fluid, liquid_block) = (
                        facts.fluid != Fluid::None,
                        matches!(&*facts.name, "minecraft:water" | "minecraft:lava"),
                    );
                    if fluid {
                        b.scheduled_fluid_tick(level, ticks, pos);
                    }
                    if liquid_block {
                        b.scheduled_block_tick(level, ticks, pos);
                    } else {
                        let updated = b.update_from_neighbour_shapes(level, ticks, pos, state);
                        if updated != state {
                            b.set_block(level, ticks, pos, updated, POSTPROCESS_FLAGS);
                        }
                    }
                }
            });
        }
        self.process_level_events()
    }
}
