use super::{Game, shield::OFFHAND_SLOT, view_vector};
use vise_client::{
    enchant::EffectContext,
    items::ItemStack,
    mobs::Who,
    projectiles::{Projectile, ProjectileKind},
};
use vise_core::{geometry::Vec3, random::RandomSource};

const CROSSBOW: &str = "minecraft:crossbow";
const FIREWORK_ROCKET: &str = "minecraft:firework_rocket";

fn is_arrow(stack: &ItemStack) -> bool {
    matches!(
        &*stack.item,
        "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:tipped_arrow"
    )
}

fn projectile_slot(
    weapon: &str,
    hands: [(usize, Option<&ItemStack>); 2],
    inventory: &[Option<ItemStack>],
) -> Option<usize> {
    let held = |s: &ItemStack| is_arrow(s) || weapon == CROSSBOW && &*s.item == FIREWORK_ROCKET;
    hands
        .into_iter()
        .find_map(|(slot, s)| s.is_some_and(held).then_some(slot))
        .or_else(|| {
            inventory
                .iter()
                .position(|s| s.as_ref().is_some_and(is_arrow))
        })
}

impl Game<'_> {
    pub(super) fn charge_ticks(stack: &ItemStack) -> u32 {
        let seconds = 1.25 - 0.25 * stack.enchantment_level("minecraft:quick_charge") as f32;
        (seconds.max(0.0) * 20.0).floor() as u32
    }
    fn charging_sounds(stack: &ItemStack) -> [&'static str; 3] {
        match stack.enchantment_level("minecraft:quick_charge") {
            0 => [
                "minecraft:item.crossbow.loading_start",
                "minecraft:item.crossbow.loading_middle",
                "minecraft:item.crossbow.loading_end",
            ],
            1 => [
                "minecraft:item.crossbow.quick_charge_1",
                "",
                "minecraft:item.crossbow.loading_end",
            ],
            2 => [
                "minecraft:item.crossbow.quick_charge_2",
                "",
                "minecraft:item.crossbow.loading_end",
            ],
            _ => [
                "minecraft:item.crossbow.quick_charge_3",
                "",
                "minecraft:item.crossbow.loading_end",
            ],
        }
    }
    pub(super) fn projectile_slot(&self, weapon: &str) -> Option<usize> {
        let hands = [OFFHAND_SLOT, self.selected].map(|slot| (slot, self.stack_in(slot)));
        projectile_slot(weapon, hands, &self.inventory)
    }
    pub(super) fn use_crossbow(&mut self, stack: &ItemStack) -> bool {
        if &*stack.item != CROSSBOW {
            return false;
        }
        if self.using.is_some() {
            return true;
        }
        if stack.data().is_some_and(|d| !d.charged.is_empty()) {
            self.shoot_crossbow();
        } else if (self.projectile_slot(CROSSBOW).is_some() || self.creative)
            && self.using.is_none()
        {
            self.using = Some((self.held_slot(), 0));
        }
        true
    }
    pub(super) fn tick_crossbow(&mut self, slot: usize, ticks: u32, stack: &ItemStack) {
        let ticks = ticks + 1;
        self.using = Some((slot, ticks));
        let charge = Self::charge_ticks(stack).max(1) as f32;
        let (before, now) = ((ticks - 1) as f32 / charge, ticks as f32 / charge);
        let sounds = Self::charging_sounds(stack);
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        if before < 0.2 && now >= 0.2 && !sounds[0].is_empty() {
            self.audio.play(sounds[0], at, 0.5, 1.0);
        }
        if before < 0.5 && now >= 0.5 && !sounds[1].is_empty() {
            self.audio.play(sounds[1], at, 0.5, 1.0);
        }
        let charged = stack.data().is_some_and(|d| !d.charged.is_empty());
        if now >= 1.0 && !charged {
            let shots = if stack.enchantment_level("minecraft:multishot") > 0 {
                3
            } else {
                1
            };
            let slot_ammo = self.projectile_slot(CROSSBOW);
            let ammo = slot_ammo
                .and_then(|i| self.stack_in(i).cloned())
                .unwrap_or_else(|| ItemStack::new("minecraft:arrow", 1));
            if !self.creative {
                let Some(i) = slot_ammo else { return };
                let cell = self.stack_in_mut(i);
                if let Some(s) = cell.as_mut() {
                    s.count -= 1;
                    if s.count == 0 {
                        *cell = None;
                    }
                }
            }
            let drawn = (0..shots)
                .map(|i| vise_client::arrows::drawn_ammo(&ammo, self.creative || i > 0))
                .collect();
            if let Some(bow) = self.stack_in_mut(slot).as_mut() {
                bow.data_mut().charged = drawn;
            }
            let pitch = 1.0 / (self.world.random.next_float() * 0.5 + 1.0) + 0.2;
            self.audio.play(sounds[2], at, 1.0, pitch);
        }
    }
    fn shoot_crossbow(&mut self) {
        let Some(eye) = self.eye_position() else {
            return;
        };
        let Some(bow) = self.held_mut().as_mut() else {
            return;
        };
        let charged = std::mem::take(&mut bow.data_mut().charged);
        bow.tidy();
        let weapon = bow.clone();
        let pierce = self
            .res
            .stations()
            .enchantments
            .modify_value(
                Some(&weapon),
                "minecraft:projectile_piercing",
                &EffectContext::default(),
                &mut self.world.random,
                0.0,
            )
            .clamp(0.0, 127.0) as u8;
        let count = charged.len();
        let power = if charged.iter().any(|s| &*s.item == FIREWORK_ROCKET) {
            1.6
        } else {
            3.15
        };
        let spread = if count > 1 { 10.0 } else { 0.0 };
        let view = glam::DVec3::from_array(view_vector(self.yaw, self.pitch));
        let up = glam::DVec3::from_array(view_vector(self.yaw, self.pitch - 90.0));
        for (index, ammo) in charged.iter().enumerate() {
            let angle = if count > 1 {
                -spread + 2.0 * spread * index as f64 / (count - 1) as f64
            } else {
                0.0
            };
            let angle = if count == 3 {
                [0.0, -spread, spread][index]
            } else {
                angle
            };
            let dir =
                glam::DQuat::from_axis_angle(up.normalize_or_zero(), angle.to_radians()) * view;
            let rocket = &*ammo.item == FIREWORK_ROCKET;
            if rocket {
                let at = Vec3::new(eye[0], eye[1] - f64::from(0.15f32), eye[2]);
                let mut shot = Projectile::new(ProjectileKind::Snowball, at, None);
                shot.shoot(
                    &mut super::dispenser::entity_random(),
                    dir.to_array(),
                    power,
                    1.0,
                );
                self.launch_angled_rocket(at, shot.velocity, ammo, Some(Who::Player));
            }
            if !rocket {
                let effect = super::dispenser::tipped_effect(ammo).or_else(|| {
                    (&*ammo.item == "minecraft:spectral_arrow")
                        .then_some(("minecraft:glowing", 200))
                });
                let kind = ProjectileKind::Arrow {
                    base_damage: 2.0,
                    critical: true,
                    effect,
                };
                let mut arrow = Projectile::new(
                    kind,
                    Vec3::new(eye[0], eye[1] - 0.1, eye[2]),
                    Some(Who::Player),
                );
                if let Some(a) = arrow.arrow.as_mut() {
                    a.set_item(ammo);
                    a.set_owner(Some(Who::Player));
                    a.fired_from_weapon = Some(weapon.clone());
                    a.pierce = pierce;
                }
                arrow.shoot(&mut self.world.random, dir.to_array(), power, 1.0);
                self.add_projectile(arrow);
            }
            self.player_projectile_shot();
            let pitch = if index == 0 {
                1.0
            } else {
                let high = index & 1 == 1;
                1.0 / (self.world.random.next_float() * 0.5 + 1.8) + if high { 0.63 } else { 0.43 }
            };
            self.audio
                .play("minecraft:item.crossbow.shoot", Some(eye), 1.0, pitch);
            self.damage_held(if rocket { 3 } else { 1 });
        }
        if let Some(stack) = self.held().cloned() {
            self.trigger_item("minecraft:shot_crossbow", &stack);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_projectiles_come_before_the_inventory() {
        let stack = |item: &str| Some(ItemStack::new(item, 1));
        let (crossbow, rocket, arrow) = (
            stack(CROSSBOW),
            stack(FIREWORK_ROCKET),
            stack("minecraft:arrow"),
        );
        let mut inventory = vec![None; 36];
        inventory[5] = arrow.clone();
        let slot = |weapon, off: &Option<ItemStack>, inventory: &[Option<ItemStack>]| {
            let hands = [(OFFHAND_SLOT, off.as_ref()), (0, crossbow.as_ref())];
            projectile_slot(weapon, hands, inventory)
        };
        assert_eq!(slot(CROSSBOW, &rocket, &inventory), Some(OFFHAND_SLOT));
        assert_eq!(slot("minecraft:bow", &rocket, &inventory), Some(5));
        assert_eq!(
            slot("minecraft:bow", &arrow, &inventory),
            Some(OFFHAND_SLOT)
        );
        assert_eq!(slot(CROSSBOW, &None, &inventory), Some(5));
        inventory[5] = rocket.clone();
        assert_eq!(slot(CROSSBOW, &None, &inventory), None);
        let hands = [(OFFHAND_SLOT, crossbow.as_ref()), (3, rocket.as_ref())];
        assert_eq!(projectile_slot(CROSSBOW, hands, &inventory), Some(3));

        let spectral = stack("minecraft:spectral_arrow");
        let tipped = stack("minecraft:tipped_arrow");
        inventory[7] = spectral.clone();
        assert_eq!(slot("minecraft:bow", &None, &inventory), Some(7));
        assert_eq!(
            slot("minecraft:bow", &tipped, &inventory),
            Some(OFFHAND_SLOT)
        );
        inventory[7] = None;
        assert_eq!(slot("minecraft:bow", &None, &inventory), None);
    }
}
