use super::Game;
use vise_client::{
    items::ItemStack,
    mobs::{Damage, Species, Who},
};
use vise_core::{
    geometry::{Aabb, Direction, Vec3},
    position::BlockPos,
};
use vise_world::{chunk::BlockState, registry::BlockStateId};

pub(super) fn thrown_block_hit(start: Vec3, end: Vec3, velocity: Vec3) -> ([i32; 3], Direction) {
    let cell = [end.x, end.y, end.z].map(|v| v.floor() as i32);
    let face = Aabb::clip_boxes(
        [Aabb::unit_cube(Vec3::ZERO)],
        start,
        end,
        BlockPos::new(cell[0], cell[1], cell[2]),
    )
    .map_or_else(
        || Direction::approximate_nearest(velocity.reverse()),
        |hit| hit.direction,
    );
    (cell, face)
}

impl Game<'_> {
    pub(super) fn potion_in_tag(&self, stack: &ItemStack, tag: &str) -> bool {
        stack.potion().is_some_and(|potion| {
            self.res
                .stations()
                .trades
                .potion_tags
                .get(tag)
                .is_some_and(|members| members.iter().any(|m| m.as_str() == potion))
        })
    }

    pub(super) fn potion_hit_block(&mut self, stack: &ItemStack, hit: [i32; 3], face: Direction) {
        if !self.potion_in_tag(stack, "minecraft:douses_fire") {
            return;
        }
        let relative = |pos: [i32; 3], direction: Direction| {
            let n = direction.normal();
            [pos[0] + n[0], pos[1] + n[1], pos[2] + n[2]]
        };
        let effect = relative(hit, face);
        self.douse_fire(effect);
        self.douse_fire(relative(effect, face.opposite()));
        for direction in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
        ] {
            self.douse_fire(relative(effect, direction));
        }
    }

    fn douse_fire(&mut self, pos: [i32; 3]) {
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let name = self.res.facts().get(state).name.clone();
        let fire = self.block_in_tag("minecraft:fire", &name);
        if fire {
            self.with_level(|b, level, ticks| b.destroy_block(level, ticks, at, false));
        }
        let candle =
            !fire && self.with_level(|b, level, ticks| b.extinguish_candle(level, ticks, at));
        if !fire
            && !candle
            && let Some(mut lit) = self.lit_campfire(state, &name)
        {
            self.level_event_at(1009, pos, 0);
            self.post_block_game_event("minecraft:block_change", pos, None, None);
            lit.properties.insert("lit".into(), "false".into());
            if let Ok(unlit) = self.pack.assets.registry.resolve(&lit) {
                let _ = self.level_set(pos, unlit);
            }
        }
        let _ = self.process_level_events();
    }

    fn lit_campfire(&self, state: BlockStateId, name: &str) -> Option<BlockState> {
        if !self.block_in_tag("minecraft:campfires", name) {
            return None;
        }
        self.pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.properties.get("lit").map(String::as_str) == Some("true"))
            .cloned()
    }

    pub(super) fn affect_entities_around(&mut self, stack: &ItemStack, at: Vec3, by_player: bool) {
        let hurts = self.potion_in_tag(stack, "minecraft:hurts_water_sensitive_entities");
        let extinguishes = self.potion_in_tag(stack, "minecraft:extinguishes_entities");
        let reach = Aabb::new(
            Vec3::new(at.x - 0.125, at.y, at.z - 0.125),
            Vec3::new(at.x + 0.125, at.y + 0.25, at.z + 0.125),
        )
        .inflate(Vec3::new(4.0, 2.0, 4.0));
        let near = |p: Vec3| {
            let (dx, dy, dz) = (p.x - at.x, p.y - at.y, p.z - at.z);
            dx * dx + dy * dy + dz * dz < 16.0
        };
        if hurts || extinguishes {
            let source =
                Damage::indirect_magic(by_player.then_some(Who::Player), [at.x, at.y, at.z]);
            for m in self.mobs.iter_mut().filter(|m| !m.removed) {
                let sensitive = m.kind.flags.water_sensitive;
                if !(sensitive || m.is_on_fire())
                    || !reach.intersects(m.bounds())
                    || !near(m.position)
                {
                    continue;
                }
                if hurts && sensitive {
                    let invulnerable = m.invulnerable_flag;
                    m.damage(1.0, source);
                    if m.species() == Species::Enderman
                        && !invulnerable
                        && m.alive()
                        && !m.extra.passenger
                    {
                        m.extra.counter2 = 64;
                    }
                }
                if extinguishes && m.is_on_fire() && m.alive() {
                    m.extinguish_fire();
                }
            }
        }
        if extinguishes {
            self.for_each_player(|g| g.extinguish_player_in_splash(reach, at));
            let mut put_out = Vec::new();
            for stand in self.armor_stands.iter_mut() {
                if stand.fire_ticks > 0
                    && stand.health > 0.5
                    && reach.intersects(stand.bounds())
                    && near(stand.position)
                {
                    stand.fire_ticks = 0;
                    put_out.push([stand.position.x, stand.position.y, stand.position.z]);
                }
            }
            for stand_at in put_out {
                self.extinguish_sound(stand_at);
            }
        }
        if self.potion_in_tag(stack, "minecraft:rehydrates_axolotls") {
            let (lo, hi) = (
                [at.x - 0.125 - 4.0, at.y - 2.0, at.z - 0.125 - 4.0],
                [at.x + 0.125 + 4.0, at.y + 0.25 + 2.0, at.z + 0.125 + 4.0],
            );
            for m in self
                .mobs
                .iter_mut()
                .filter(|m| m.species() == Species::Axolotl && !m.removed)
            {
                let (w, h, _) = m.dims();
                let half = w / 2.0;
                let (x, y, z) = (m.position.x, m.position.y, m.position.z);
                if x - half < hi[0]
                    && x + half > lo[0]
                    && y < hi[1]
                    && y + h > lo[1]
                    && z - half < hi[2]
                    && z + half > lo[2]
                {
                    m.air = (m.air + 1800).min(vise_client::mobs::axolotl::MAX_AIR);
                }
            }
        }
    }

    fn extinguish_player_in_splash(&mut self, reach: Aabb, at: Vec3) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let w = f64::from(p.dimensions.width) / 2.0;
        let bounds = Aabb::new(
            Vec3::new(p.position.x - w, p.position.y, p.position.z - w),
            Vec3::new(
                p.position.x + w,
                p.position.y + f64::from(p.dimensions.height),
                p.position.z + w,
            ),
        );
        let (dx, dy, dz) = (
            p.position.x - at.x,
            p.position.y - at.y,
            p.position.z - at.z,
        );
        if p.fire_ticks <= 0
            || p.health.dead_or_dying()
            || !reach.intersects(bounds)
            || dx * dx + dy * dy + dz * dz >= 16.0
        {
            return;
        }
        let feet = [p.position.x, p.position.y, p.position.z];
        self.extinguish_sound(feet);
        if let Some(p) = self.player.as_mut() {
            let ticks = p.fire_ticks.min(0);
            p.set_remaining_fire_ticks(ticks);
        }
    }
}
