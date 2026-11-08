use super::Game;
use vise_client::{
    first_person::Hand,
    mobs::{Species, equine, llama},
};

impl Game<'_> {
    pub(super) fn remote_mob_takes(&self, i: usize, hand: Hand) -> bool {
        let Some(m) = self.mobs.get(i).filter(|m| m.alive()) else {
            return false;
        };
        let stack = match hand {
            Hand::Main => self.stack_in(self.selected),
            Hand::Off => self.offhand.as_ref(),
        };
        let item = stack.map_or("", |s| &*s.item);
        let baby = m.is_baby();
        if item == "minecraft:lead"
            || item == "minecraft:name_tag" && stack.is_some_and(|s| s.custom_name().is_some())
            || item.strip_suffix("_spawn_egg") == Some(m.kind.id.as_str())
        {
            return true;
        }
        if self.age_lock_takes(m, item) {
            return true;
        }
        if m.kind.flags.animal && m.kind.food.iter().any(|f| f.as_str() == item) {
            return true;
        }
        let sneaking = self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft);
        match m.species() {
            Species::Villager | Species::WanderingTrader => item != "minecraft:villager_spawn_egg",
            s if equine::is_equine(s)
                || llama::is_llama(s)
                || matches!(s, Species::Camel | Species::CamelHusk) =>
            {
                !baby
            }
            Species::Pig | Species::Strider => m.extra.saddled && !sneaking,
            Species::HappyGhast => !baby && m.extra.head.is_some(),
            Species::Wolf | Species::Cat | Species::Parrot => m.extra.tame,
            Species::Allay => !item.is_empty() || m.extra.main_hand.is_some(),
            Species::Cow | Species::Goat => item == "minecraft:bucket" && !baby,
            Species::Mooshroom => {
                matches!(
                    item,
                    "minecraft:bucket" | "minecraft:bowl" | "minecraft:shears"
                ) && !baby
            }
            Species::Sheep => item == "minecraft:shears" || item.ends_with("_dye"),
            Species::SnowGolem => item == "minecraft:shears" && !m.extra.sheared,
            Species::Cod
            | Species::Salmon
            | Species::TropicalFish
            | Species::Pufferfish
            | Species::Axolotl => item == "minecraft:water_bucket",
            Species::Tadpole => {
                item == "minecraft:water_bucket"
                    || !m.extra.age_locked && m.kind.food.iter().any(|f| f.as_str() == item)
            }
            Species::Armadillo => item == "minecraft:brush",
            Species::ZombieVillager => item == "minecraft:golden_apple",
            Species::Creeper => {
                matches!(item, "minecraft:flint_and_steel" | "minecraft:fire_charge")
            }
            Species::Piglin => {
                !baby
                    && self
                        .res
                        .items()
                        .item_tags
                        .get("minecraft:piglin_loved")
                        .is_some_and(|tag| tag.contains(item))
            }
            _ => false,
        }
    }
    pub(super) fn remote_feed_particles(&mut self, i: usize, hand: Hand) {
        let Some(m) = self.mobs.get(i).filter(|m| m.alive() && m.is_baby()) else {
            return;
        };
        let stack = match hand {
            Hand::Main => self.stack_in(self.selected),
            Hand::Off => self.offhand.as_ref(),
        };
        let item = stack.map_or("", |s| &*s.item);
        let ages = match m.species() {
            s if llama::is_llama(s) => matches!(item, "minecraft:wheat" | "minecraft:hay_block"),
            Species::Camel => m.kind.food.iter().any(|f| f.as_str() == item),
            Species::Horse | Species::Donkey | Species::Mule => matches!(
                item,
                "minecraft:wheat"
                    | "minecraft:sugar"
                    | "minecraft:hay_block"
                    | "minecraft:apple"
                    | "minecraft:carrot"
                    | "minecraft:golden_carrot"
                    | "minecraft:golden_apple"
                    | "minecraft:enchanted_golden_apple"
            ),
            _ => false,
        };
        if ages {
            let id = m.id;
            self.particles_around_mob(
                id,
                "happy_villager",
                vise_client::particles::Burst::HORSE_EATING,
            );
        }
    }
}
