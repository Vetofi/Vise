use super::Game;
use vise_client::{
    health::{BlockOutcome, Blocking},
    items::ItemStack,
};
use vise_core::random::RandomSource;
use vise_world::blocking_rules::{BlockingRules, DamageReduction, ItemDamageFunction};

pub(super) const OFFHAND_SLOT: usize = 40;

impl Game<'_> {
    pub(super) fn stack_in(&self, slot: usize) -> Option<&ItemStack> {
        if slot == OFFHAND_SLOT {
            self.offhand.as_ref()
        } else {
            self.inventory.get(slot)?.as_ref()
        }
    }
    pub(super) fn stack_in_mut(&mut self, slot: usize) -> &mut Option<ItemStack> {
        if slot == OFFHAND_SLOT {
            &mut self.offhand
        } else {
            &mut self.inventory[slot]
        }
    }
    pub(super) fn blocks_attacks(
        &self,
        stack: &ItemStack,
    ) -> Option<&vise_client::items::BlocksAttacks> {
        self.items().get(&stack.item)?.blocks_attacks.as_ref()
    }
    pub(super) fn raise_shield(&mut self, slot: usize) -> bool {
        let Some(stack) = self.stack_in(slot) else {
            return false;
        };
        if self.blocks_attacks(stack).is_none() {
            return false;
        }
        if self.shield_cooldown == 0 && self.using.is_none() {
            self.using = Some((slot, 0));
        }
        true
    }
    pub(super) fn player_blocking(
        &self,
        from: Option<[f64; 3]>,
        damage_type: &str,
        pierce: bool,
    ) -> Option<Blocking> {
        let (slot, ticks) = self.using?;
        let component = self.blocks_attacks(self.stack_in(slot)?)?;
        if (ticks as i32) < (component.delay_seconds * 20.0).round() as i32 {
            return None;
        }
        let tags = &self.stations().enchantments.damage_tags;
        let matches = |spec: &str| match spec.strip_prefix('#') {
            Some(tag) => tags.get(tag).is_some_and(|t| t.contains(damage_type)),
            None => spec == damage_type,
        };
        let p = self.player.as_ref()?;
        let angle = match from {
            Some(src) => {
                let yaw = (-self.yaw).to_radians();
                let view = [yaw.sin() as f64, yaw.cos() as f64];
                let (dx, dz) = (src[0] - p.position.x, src[2] - p.position.z);
                let len = (dx * dx + dz * dz).sqrt();
                let to = if len < 1.0e-4 {
                    [0.0, 0.0]
                } else {
                    [dx / len, dz / len]
                };
                (to[0] * view[0] + to[1] * view[1]).acos()
            }
            None => f64::from(std::f32::consts::PI),
        };
        let (threshold, base, factor) = component.item_damage;
        Some(Blocking {
            rules: BlockingRules {
                delay_seconds: component.delay_seconds,
                disable_scale: component.disable_scale,
                reductions: component
                    .reductions
                    .iter()
                    .map(|r| DamageReduction {
                        horizontal_angle: r.horizontal_angle,
                        types: r.types.as_ref().map(|t| t.iter().any(|s| matches(s))),
                        base: r.base,
                        factor: r.factor,
                    })
                    .collect(),
                item_damage: ItemDamageFunction {
                    threshold,
                    base,
                    factor,
                },
            },
            bypassed: component.bypassed_by.as_deref().is_some_and(matches),
            pierce,
            angle,
        })
    }
    pub(super) fn after_block(
        &mut self,
        outcome: BlockOutcome,
        damage: f32,
        attacker: Option<u32>,
        projectile: bool,
    ) {
        let Some((slot, _)) = self.using else { return };
        let Some(stack) = self.stack_in(slot).cloned() else {
            return;
        };
        let Some(component) = self.blocks_attacks(&stack).cloned() else {
            return;
        };
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        if outcome.sound
            && let Some(sound) = &component.block_sound
        {
            let pitch = 0.8 + self.world.random.next_float() * 0.4;
            self.audio.play(sound, at, 1.0, pitch);
        }
        if outcome.blocked <= 0.0 {
            return;
        }
        let (threshold, base, factor) = component.item_damage;
        let item_damage = if outcome.blocked < threshold {
            0
        } else {
            (base + factor * outcome.blocked).floor() as i32
        };
        if item_damage > 0 && !self.creative {
            let loss = self.wear(&stack, item_damage as u32);
            let max = self
                .res
                .items()
                .get(&stack.item)
                .map_or(0, |i| i.max_damage);
            let cell = self.stack_in_mut(slot);
            if let Some(s) = cell.as_mut()
                && max > 0
            {
                s.damage += loss;
                if s.damage >= max {
                    *cell = None;
                    self.using = None;
                    self.audio.play(
                        "minecraft:item.shield.break",
                        at,
                        0.8,
                        0.8 + self.world.random.next_float() * 0.4,
                    );
                    return;
                }
            }
        }
        if projectile {
            return;
        }
        let Some(id) = attacker else {
            if let Some((from, disable)) = self.combat.player_attacker {
                let fully = outcome.blocked >= damage;
                self.blocked_by_attacker(&component, [from[0], from[2]], fully, disable, at);
            }
            return;
        };
        let fully = outcome.blocked >= damage;
        let Some(m) = self.mobs.iter().find(|m| m.id == id) else {
            return;
        };
        let (mx, mz) = (m.position.x, m.position.z);
        let disable = if m.species() == vise_client::mobs::Species::Warden {
            5.0
        } else {
            m.extra
                .main_hand
                .as_deref()
                .and_then(|item| self.items().get(item))
                .map_or(0.0, |i| i.disable_blocking_seconds)
        };
        if m.species() == vise_client::mobs::Species::Ravager {
            let mut events = Vec::new();
            let roaring = m.extra.anim_ticks != 0;
            let stunned = self
                .mobs
                .iter_mut()
                .find(|m| m.id == id)
                .is_some_and(|m| vise_client::mobs::ravager::blocked(m, &mut events));
            if !roaring && let Some(p) = self.player.as_mut() {
                let (xd, zd) = (p.position.x - mx, p.position.z - mz);
                let push = if stunned {
                    let d = (xd * xd + zd * zd).sqrt().max(0.01);
                    let k = (1.0 / d).min(1.0) * 0.05;
                    vise_core::geometry::Vec3::new(xd / d * k, 0.0, zd / d * k)
                } else {
                    let dd = (xd * xd + zd * zd).max(0.001);
                    vise_core::geometry::Vec3::new(xd / dd * 4.0, 0.2, zd / dd * 4.0)
                };
                p.velocity = p.velocity.add_vector(push);
            }
            let _ = self.handle_mob_events(events);
            return;
        }
        self.blocked_by_attacker(&component, [mx, mz], fully, disable, at);
    }

    fn blocked_by_attacker(
        &mut self,
        component: &vise_client::items::BlocksAttacks,
        attacker: [f64; 2],
        fully: bool,
        disable: f32,
        at: Option<[f64; 3]>,
    ) {
        let [mx, mz] = attacker;
        if !fully && let Some(p) = self.player.as_mut() {
            let (xd, zd) = (p.position.x - mx, p.position.z - mz);
            let len = (xd * xd + zd * zd).sqrt();
            if len >= 1.0e-5 {
                let (dx, dz) = (xd / len * 0.5, zd / len * 0.5);
                let v = p.velocity;
                let y = if p.ground.on_ground {
                    (v.y / 2.0 + 0.5).min(0.4)
                } else {
                    v.y
                };
                p.velocity = vise_core::geometry::Vec3::new(v.x / 2.0 - dx, y, v.z / 2.0 - dz);
            }
        }
        let ticks = disable * component.disable_scale;
        let ticks = if ticks > 0.0 {
            (ticks * 20.0).round() as u32
        } else {
            0
        };
        if ticks > 0 {
            self.shield_cooldown = ticks;
            self.using = None;
            if let Some(sound) = &component.disabled_sound {
                let pitch = 0.8 + self.world.random.next_float() * 0.4;
                self.audio.play(sound, at, 0.8, pitch);
            }
        }
    }
}
