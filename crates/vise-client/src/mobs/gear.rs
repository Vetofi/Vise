use super::{DamageKind, Mob, Species};
use std::sync::Arc;

pub const MAIN: usize = 0;
pub const OFF: usize = 1;
pub const HEAD: usize = 2;
pub const CHEST: usize = 3;
pub const LEGS: usize = 4;
pub const FEET: usize = 5;
pub const ARMOR: [usize; 4] = [HEAD, CHEST, LEGS, FEET];
pub const KEYS: [&str; 6] = ["mainhand", "offhand", "head", "chest", "legs", "feet"];

pub type Levels = Vec<(Arc<str>, u32)>;
pub type Trim = (Arc<str>, Arc<str>);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Gear {
    pub slots: [Levels; 6],
    pub trims: [Option<Trim>; 6],
}
impl Gear {
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(Vec::is_empty) && self.trims.iter().all(Option::is_none)
    }
    pub fn level(&self, slot: usize, id: &str) -> u32 {
        self.slots[slot]
            .iter()
            .find(|(e, _)| &**e == id)
            .map_or(0, |(_, l)| *l)
    }
    pub fn upgrade(&mut self, slot: usize, id: &str, level: u32) {
        match self.slots[slot].iter_mut().find(|(e, _)| &**e == id) {
            Some((_, l)) => *l = (*l).max(level),
            None => self.slots[slot].push((Arc::from(id), level)),
        }
    }
}

pub fn gear(m: &Mob) -> Option<&Gear> {
    m.extra.gear.as_deref()
}
pub fn level(m: &Mob, slot: usize, id: &str) -> u32 {
    gear(m).map_or(0, |g| g.level(slot, id))
}
pub fn enchanted(m: &Mob, slot: usize) -> bool {
    gear(m).is_some_and(|g| !g.slots[slot].is_empty())
}
pub fn set(m: &mut Mob, slot: usize, levels: Levels) {
    let g = m.extra.gear.get_or_insert_with(Box::default);
    g.slots[slot] = levels;
    g.trims[slot] = None;
    if g.is_empty() {
        m.extra.gear = None;
    }
}
pub fn set_trim(m: &mut Mob, slot: usize, trim: Option<Trim>) {
    let g = m.extra.gear.get_or_insert_with(Box::default);
    g.trims[slot] = trim;
    if g.is_empty() {
        m.extra.gear = None;
    }
}
pub fn upgrade(m: &mut Mob, slot: usize, id: &str, level: u32) {
    m.extra
        .gear
        .get_or_insert_with(Box::default)
        .upgrade(slot, id, level);
}

fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
            | Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::WitherSkeleton
            | Species::Wither
            | Species::Phantom
            | Species::Zoglin
            | Species::SkeletonHorse
            | Species::ZombieHorse
    )
}
fn arthropod(s: Species) -> bool {
    matches!(
        s,
        Species::Spider
            | Species::CaveSpider
            | Species::Silverfish
            | Species::Endermite
            | Species::Bee
    )
}
pub fn damage_bonus(levels: &[(Arc<str>, u32)], target: Option<Species>) -> f32 {
    let mut bonus = 0.0;
    for (id, l) in levels {
        let l = *l as f32;
        match &**id {
            "minecraft:sharpness" => bonus += 1.0 + 0.5 * (l - 1.0),
            "minecraft:smite" if target.is_some_and(undead) => bonus += 2.5 * l,
            "minecraft:bane_of_arthropods" if target.is_some_and(arthropod) => bonus += 2.5 * l,
            _ => {}
        }
    }
    bonus
}
pub fn melee_bonus(m: &Mob, target: Option<Species>) -> f32 {
    gear(m).map_or(0.0, |g| damage_bonus(&g.slots[MAIN], target))
}
pub fn fire_aspect(m: &Mob) -> i32 {
    4 * level(m, MAIN, "minecraft:fire_aspect") as i32
}
pub fn knockback(m: &Mob) -> u32 {
    level(m, MAIN, "minecraft:knockback")
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArrowBonus {
    pub damage: f32,
    pub punch: u32,
    pub flame: bool,
    pub pierce: u32,
}
pub fn arrow_bonus(m: &Mob) -> ArrowBonus {
    let power = level(m, MAIN, "minecraft:power");
    ArrowBonus {
        damage: if power > 0 {
            1.0 + 0.5 * (power as f32 - 1.0)
        } else {
            0.0
        },
        punch: level(m, MAIN, "minecraft:punch"),
        flame: level(m, MAIN, "minecraft:flame") > 0,
        pierce: level(m, MAIN, "minecraft:piercing"),
    }
}
pub fn protection(m: &Mob, kind: DamageKind) -> f32 {
    let Some(g) = gear(m) else { return 0.0 };
    protection_points(g, kind).min(20.0)
}
pub fn protection_points(g: &Gear, kind: DamageKind) -> f32 {
    if kind == DamageKind::Generic || kind.bypasses_invulnerability() {
        return 0.0;
    }
    let fire = matches!(
        kind,
        DamageKind::OnFire
            | DamageKind::InFire
            | DamageKind::Campfire
            | DamageKind::HotFloor
            | DamageKind::Lava
            | DamageKind::Fireball
            | DamageKind::SulfurCubeHot
    );
    let projectile = matches!(
        kind,
        DamageKind::Arrow | DamageKind::Thrown | DamageKind::Fireball | DamageKind::WindCharge
    );
    let mut total = 0.0;
    for slot in ARMOR {
        for (id, l) in &g.slots[slot] {
            let l = *l as f32;
            total += match &**id {
                "minecraft:protection" => l,
                "minecraft:fire_protection" if fire => 2.0 * l,
                "minecraft:blast_protection"
                    if matches!(kind, DamageKind::Explosion | DamageKind::Fireworks) =>
                {
                    2.0 * l
                }
                "minecraft:projectile_protection" if projectile => 2.0 * l,
                "minecraft:feather_falling"
                    if matches!(kind, DamageKind::Fall | DamageKind::Stalagmite) =>
                {
                    3.0 * l
                }
                _ => 0.0,
            };
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(list: &[(&str, u32)]) -> Levels {
        list.iter().map(|(e, l)| (Arc::from(*e), *l)).collect()
    }
    #[test]
    fn sharpness_adds_half_a_heart_per_level_above_one() {
        let s = levels(&[("minecraft:sharpness", 3)]);
        assert_eq!(damage_bonus(&s, Some(Species::Cow)), 2.0);
    }
    #[test]
    fn smite_and_bane_need_the_right_target() {
        let l = levels(&[("minecraft:smite", 2), ("minecraft:bane_of_arthropods", 1)]);
        assert_eq!(damage_bonus(&l, Some(Species::Zombie)), 5.0);
        assert_eq!(damage_bonus(&l, Some(Species::Spider)), 2.5);
        assert_eq!(damage_bonus(&l, None), 0.0);
    }
    #[test]
    fn protections_count_by_damage_kind() {
        let mut g = Gear::default();
        g.upgrade(HEAD, "minecraft:protection", 4);
        g.upgrade(FEET, "minecraft:feather_falling", 4);
        g.upgrade(CHEST, "minecraft:fire_protection", 2);
        assert_eq!(protection_points(&g, DamageKind::MobAttack), 4.0);
        assert_eq!(protection_points(&g, DamageKind::Fall), 16.0);
        assert_eq!(protection_points(&g, DamageKind::OnFire), 8.0);
        assert_eq!(protection_points(&g, DamageKind::Generic), 0.0);
    }
    #[test]
    fn upgrading_keeps_the_higher_level() {
        let mut g = Gear::default();
        g.upgrade(MAIN, "minecraft:power", 2);
        g.upgrade(MAIN, "minecraft:power", 1);
        assert_eq!(g.level(MAIN, "minecraft:power"), 2);
        assert!(!g.is_empty());
    }
}
