use super::Game;
use vise_client::{Result, items::ItemStack, vanillagen};
use vise_core::{geometry::Direction, position::BlockPos};
use vise_world::worldgen::live::LiveFeatures;
use std::sync::Arc;

impl Game<'_> {
    pub(super) fn install_live_features(&self) {
        install_live_features(&self.res, &self.pack, &self.options);
    }
    pub(super) fn use_bone_meal(&mut self, stack: &ItemStack) -> Result<bool> {
        if &*stack.item != "minecraft:bone_meal" {
            return Ok(false);
        }
        let Some(hit) = self.pick() else {
            return Ok(false);
        };
        let face = match hit.face {
            [0, 1, 0] => Direction::Up,
            [0, -1, 0] => Direction::Down,
            [0, 0, -1] => Direction::North,
            [0, 0, 1] => Direction::South,
            [-1, 0, 0] => Direction::West,
            _ => Direction::East,
        };
        let pos = BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
        let used = self.with_level(|b, level, ticks| b.use_bone_meal(level, ticks, pos, face));
        if used
            && stack.count > 1
            && let Some(p) = self.player.as_ref().map(|p| p.position)
        {
            let ctx = self.player_context(None);
            self.post_game_event("minecraft:item_interact_finish", [p.x, p.y, p.z], ctx);
        }
        self.process_level_events()?;
        if used {
            self.swing_arm();
            self.stats.add("used", &stack.item, 1);
            self.consume_held();
        }
        Ok(used)
    }
}

pub(super) fn install_live_features(
    res: &super::Resources,
    pack: &super::PackResources,
    options: &super::Options,
) {
    if res.behaviours().live_features().is_some() {
        return;
    }
    let dir = options.resources.join("worldgen");
    let started = std::time::Instant::now();
    let features = vanillagen::shared_data(&dir, pack.assets.registry)
        .map_err(|e| e.to_string())
        .and_then(|data| LiveFeatures::new(&data).map_err(|e| e.to_string()));
    match features {
        Ok(Some(features)) => {
            res.behaviours().set_live_features(Arc::new(features));
            log::info!("Vise: live features ready in {:?}", started.elapsed());
        }
        Ok(None) => {
            log::warn!("Vise: the worldgen bundle has no features; saplings will not grow")
        }
        Err(e) => log::warn!("Vise: live features unavailable: {e}"),
    }
}
