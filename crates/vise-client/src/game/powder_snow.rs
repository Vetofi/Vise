use super::Game;
use vise_client::{
    Result,
    items::ItemStack,
    mobs::Who,
    particles::{ParticleOptions, ParticleType},
};
use vise_core::{geometry::Vec3, position::BlockPos, random::RandomSource};

impl Game<'_> {
    pub(super) fn snowflake(&mut self, position: Vec3, velocity: Vec3) {
        let r = self.particles.random();
        let offset = [
            r.next_double() * 0.0,
            r.next_double() * 0.0,
            r.next_double() * 0.0,
        ];
        let options = ParticleOptions::simple(ParticleType::Snowflake);
        self.particles.add(
            &options,
            [
                position.x + offset[0],
                position.y + offset[1],
                position.z + offset[2],
            ],
            [velocity.x, velocity.y, velocity.z],
        );
    }
    pub(super) fn powder_snow_fire(&mut self) -> Result<()> {
        let Some(player) = self.player.as_mut() else {
            return Ok(());
        };
        let entered = std::mem::take(&mut player.powder_snow_entered);
        let at = [player.position.x, player.position.y, player.position.z];
        if entered.is_empty() || player.fire_ticks <= 0 {
            return Ok(());
        }
        for pos in entered {
            self.with_level(|b, level, ticks| b.destroy_block(level, ticks, pos, false));
            self.world.modified.insert((pos.x >> 4, pos.z >> 4));
        }
        self.process_level_events()?;
        if let Some(player) = self.player.as_mut() {
            player.fire_ticks = 0;
        }
        self.extinguish_sound(at);
        Ok(())
    }
    pub(super) fn extinguish_sound(&mut self, at: [f64; 3]) {
        let r = self.particles.random();
        let pitch = 1.6 + (r.next_float() - r.next_float()) * 0.4;
        self.audio.play(
            "minecraft:entity.generic.extinguish_fire",
            Some(at),
            0.7,
            pitch,
        );
    }
    pub(super) fn rain_extinguish_player(&mut self) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let (at, top) = (player.position, player.bounds().max_position().y);
        if player.fire_ticks <= 0 || !self.in_rain(at, top) {
            return;
        }
        if let Some(player) = self.player.as_mut() {
            player.fire_ticks = 0;
        }
        self.extinguish_sound([at.x, at.y, at.z]);
    }
    pub(super) fn rain_extinguish_in_frame(&mut self) {
        if self.remote.is_none() {
            self.rain_extinguish_player();
        }
    }
    pub(super) fn is_powder_snow(&self, pos: [i32; 3]) -> bool {
        &*self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            == "minecraft:powder_snow"
    }
    pub(super) fn pick_up_powder_snow(&mut self, pos: [i32; 3]) -> Result<()> {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let creative = self.creative;
        let picked = self.with_level(|b, level, ticks| b.bucket_pickup(level, ticks, at, creative));
        self.process_level_events()?;
        let Some(filled) = picked else {
            return Ok(());
        };
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        self.stats.add("used", "minecraft:bucket", 1);
        if let Some(eye) = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z])
        {
            self.audio.play(
                "minecraft:item.bucket.fill_powder_snow",
                Some(eye),
                1.0,
                1.0,
            );
        }
        self.post_block_game_event("minecraft:fluid_pickup", pos, Some(Who::Player), None);
        let filled = ItemStack::new(filled, 1);
        self.filled_result(filled.clone());
        self.trigger_item("minecraft:filled_bucket", &filled);
        self.swing_arm();
        Ok(())
    }
    pub(super) fn filled_result(&mut self, filled: ItemStack) {
        if self.creative {
            let has = self
                .inventory
                .iter()
                .flatten()
                .any(|s| s.same_item_same_components(&filled));
            if !has {
                let _ = self.give(filled);
            }
            return;
        }
        let held = self.held_mut();
        let last = held.as_ref().is_none_or(|s| s.count <= 1);
        if last {
            *held = Some(filled);
            return;
        }
        if let Some(s) = held.as_mut() {
            s.count -= 1;
        }
        if let Some(rest) = self.give(filled) {
            self.player_drop(rest);
        }
    }
}
