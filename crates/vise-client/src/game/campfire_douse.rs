use super::Game;
use vise_client::{Result, animate_tick::make_campfire_smoke, items::ItemStack};
use vise_core::position::BlockPos;
use vise_world::chunk::BlockState;
use winit::keyboard::KeyCode;

impl Game<'_> {
    fn campfire_to_douse(&self, pos: [i32; 3], held: Option<&ItemStack>) -> Option<BlockState> {
        held.filter(|s| {
            self.res
                .items()
                .item_tags
                .get("minecraft:douses_campfires")
                .is_some_and(|members| members.contains(&*s.item))
        })?;
        self.pack
            .assets
            .registry
            .state(self.world.block(pos[0], pos[1], pos[2]))
            .filter(|s| matches!(&*s.name, "minecraft:campfire" | "minecraft:soul_campfire"))
            .filter(|s| s.properties.get("lit").map(String::as_str) == Some("true"))
            .cloned()
    }

    pub(super) fn predict_campfire_douse(
        &mut self,
        pos: [i32; 3],
        held: Option<&ItemStack>,
        seq: i32,
    ) -> bool {
        if self.keys.contains(&KeyCode::ShiftLeft) {
            return false;
        }
        let Some(mut state) = self.campfire_to_douse(pos, held) else {
            return false;
        };
        let signal = state.properties.get("signal_fire").map(String::as_str) == Some("true");
        for _ in 0..20 {
            make_campfire_smoke(&mut self.particles, pos, signal, true);
        }
        state.properties.insert("lit".into(), "false".into());
        if let Ok(unlit) = self.pack.assets.registry.resolve(&state) {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            self.predict_blocks(&vec![(at, unlit)], seq);
        }
        true
    }

    pub(super) fn use_campfire_douse(
        &mut self,
        pos: [i32; 3],
        held: Option<&ItemStack>,
    ) -> Result<bool> {
        let (Some(used), Some(mut state)) = (held.cloned(), self.campfire_to_douse(pos, held))
        else {
            return Ok(false);
        };
        let registry = self.pack.assets.registry;
        self.level_event_at(1009, pos, 0);
        if self.lan_acting.is_none() {
            let signal = state.properties.get("signal_fire").map(String::as_str) == Some("true");
            for _ in 0..20 {
                make_campfire_smoke(&mut self.particles, pos, signal, true);
            }
        }
        self.post_block_game_event("minecraft:block_change", pos, self.sculk_actor(), None);
        state.properties.insert("lit".into(), "false".into());
        if let Ok(unlit) = registry.resolve(&state) {
            self.level_set(pos, unlit)?;
        }
        self.damage_held(1);
        self.trigger_block("minecraft:item_used_on_block", pos, &used);
        Ok(true)
    }
}
