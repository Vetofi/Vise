use crate::items::{ItemStack, Items};

pub fn removes_dye(items: &Items, item: &str) -> bool {
    items
        .item_tags
        .get("minecraft:cauldron_can_remove_dye")
        .is_some_and(|members| members.contains(item))
}
use vise_world::chunk::BlockState;

pub trait Player {
    fn creative(&self) -> bool;
    fn hand(&mut self) -> &mut Option<ItemStack>;
    fn add(&mut self, stack: &mut ItemStack) -> bool;
    fn contains(&self, stack: &ItemStack) -> bool;
    fn drop_stack(&mut self, stack: ItemStack);
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Success,
    Consume,
    Pass,
}

#[derive(Debug)]
pub struct Interaction {
    pub outcome: Outcome,
    pub block: Option<BlockState>,
    pub sound: Option<&'static str>,
    pub stat: Option<&'static str>,
}

fn state(name: &str, level: Option<u8>) -> BlockState {
    BlockState {
        name: name.to_owned(),
        properties: level
            .map(|l| ("level".to_owned(), l.to_string()))
            .into_iter()
            .collect(),
    }
}

fn pass() -> Interaction {
    Interaction {
        outcome: Outcome::Pass,
        block: None,
        sound: None,
        stat: None,
    }
}

fn filled(player: &mut dyn Player, new: ItemStack, limit_creative: bool) {
    let creative = player.creative();
    if limit_creative && creative {
        if !player.contains(&new) {
            let mut new = new;
            player.add(&mut new);
        }
        return;
    }
    let hand = player.hand();
    if !creative && let Some(held) = hand.as_mut() {
        held.count -= 1;
        if held.count == 0 {
            *hand = None;
        }
    }
    if hand.is_none() {
        *hand = Some(new);
        return;
    }
    let mut new = new;
    if !player.add(&mut new) {
        player.drop_stack(new);
    }
}

pub fn extinguished(cauldron: &BlockState) -> BlockState {
    if cauldron.name == "minecraft:powder_snow_cauldron" {
        let mut water = cauldron.clone();
        water.name = "minecraft:water_cauldron".into();
        lowered(&water)
    } else {
        lowered(cauldron)
    }
}

fn lowered(cauldron: &BlockState) -> BlockState {
    let level: u8 = cauldron
        .properties
        .get("level")
        .and_then(|l| l.parse().ok())
        .unwrap_or(1);
    if level <= 1 {
        state("minecraft:cauldron", None)
    } else {
        let mut next = cauldron.clone();
        next.properties
            .insert("level".into(), (level - 1).to_string());
        next
    }
}

fn changed(block: BlockState, sound: Option<&'static str>, stat: &'static str) -> Interaction {
    Interaction {
        outcome: Outcome::Success,
        block: Some(block),
        sound,
        stat: Some(stat),
    }
}

const COLORS: [&str; 16] = [
    "white",
    "orange",
    "magenta",
    "light_blue",
    "yellow",
    "lime",
    "pink",
    "gray",
    "light_gray",
    "cyan",
    "purple",
    "blue",
    "brown",
    "green",
    "red",
    "black",
];

pub fn interact(
    cauldron: &BlockState,
    under_water: bool,
    removes_dye: bool,
    player: &mut dyn Player,
) -> Interaction {
    let Some(held) = player.hand().clone() else {
        return pass();
    };
    let kind = match cauldron.name.as_str() {
        "minecraft:cauldron" => "empty",
        "minecraft:water_cauldron" => "water",
        "minecraft:lava_cauldron" => "lava",
        "minecraft:powder_snow_cauldron" => "powder_snow",
        _ => return pass(),
    };
    let level: u8 = cauldron
        .properties
        .get("level")
        .and_then(|l| l.parse().ok())
        .unwrap_or(0);
    let item = &*held.item;
    let water_potion = held.data().and_then(|d| d.potion.as_deref()) == Some("minecraft:water");
    if kind == "water" && removes_dye {
        if held.data().and_then(|d| d.dyed_color).is_none() {
            return pass();
        }
        if let Some(stack) = player.hand().as_mut() {
            stack.data_mut().dyed_color = None;
            stack.tidy();
        }
        return changed(lowered(cauldron), None, "clean_armor");
    }
    let fill_bucket = |player: &mut dyn Player, new: &str, can_fill: bool, sound| {
        if !can_fill {
            return pass();
        }
        filled(player, ItemStack::new(new, 1), true);
        changed(
            state("minecraft:cauldron", None),
            Some(sound),
            "use_cauldron",
        )
    };
    let empty_bucket = |player: &mut dyn Player, block: BlockState, sound| {
        filled(player, ItemStack::new("minecraft:bucket", 1), true);
        changed(block, Some(sound), "fill_cauldron")
    };
    match item {
        "minecraft:lava_bucket" => {
            return if under_water {
                Interaction {
                    outcome: Outcome::Consume,
                    block: None,
                    sound: None,
                    stat: None,
                }
            } else {
                empty_bucket(
                    player,
                    state("minecraft:lava_cauldron", None),
                    "minecraft:item.bucket.empty_lava",
                )
            };
        }
        "minecraft:water_bucket" => {
            return empty_bucket(
                player,
                state("minecraft:water_cauldron", Some(3)),
                "minecraft:item.bucket.empty",
            );
        }
        "minecraft:powder_snow_bucket" => {
            return if under_water {
                Interaction {
                    outcome: Outcome::Consume,
                    block: None,
                    sound: None,
                    stat: None,
                }
            } else {
                empty_bucket(
                    player,
                    state("minecraft:powder_snow_cauldron", Some(3)),
                    "minecraft:item.bucket.empty_powder_snow",
                )
            };
        }
        _ => {}
    }
    match (kind, item) {
        ("empty", "minecraft:potion") => {
            if !water_potion {
                return pass();
            }
            filled(player, ItemStack::new("minecraft:glass_bottle", 1), true);
            changed(
                state("minecraft:water_cauldron", Some(1)),
                Some("minecraft:item.bottle.empty"),
                "use_cauldron",
            )
        }
        ("water", "minecraft:bucket") => fill_bucket(
            player,
            "minecraft:water_bucket",
            level == 3,
            "minecraft:item.bucket.fill",
        ),
        ("water", "minecraft:glass_bottle") => {
            filled(
                player,
                ItemStack::new("minecraft:potion", 1).with_potion("minecraft:water"),
                true,
            );
            changed(
                lowered(cauldron),
                Some("minecraft:item.bottle.fill"),
                "use_cauldron",
            )
        }
        ("water", "minecraft:potion") => {
            if level == 3 || !water_potion {
                return pass();
            }
            filled(player, ItemStack::new("minecraft:glass_bottle", 1), true);
            let mut next = cauldron.clone();
            next.properties
                .insert("level".into(), (level % 3 + 1).to_string());
            changed(next, Some("minecraft:item.bottle.empty"), "use_cauldron")
        }
        ("water", name)
            if name
                .strip_prefix("minecraft:")
                .and_then(|n| n.strip_suffix("_banner"))
                .is_some_and(|c| COLORS.contains(&c)) =>
        {
            let layers = held.data().map_or(0, |d| d.banner_patterns.len());
            if layers == 0 {
                return pass();
            }
            let mut cleaned = held.clone();
            cleaned.count = 1;
            cleaned.data_mut().banner_patterns.pop();
            cleaned.tidy();
            filled(player, cleaned, false);
            changed(lowered(cauldron), None, "clean_banner")
        }
        ("water", name)
            if name
                .strip_prefix("minecraft:")
                .and_then(|n| n.strip_suffix("_shulker_box"))
                .is_some_and(|c| COLORS.contains(&c)) =>
        {
            filled(player, held.transmute("minecraft:shulker_box", 1), false);
            changed(lowered(cauldron), None, "clean_shulker_box")
        }
        ("lava", "minecraft:bucket") => fill_bucket(
            player,
            "minecraft:lava_bucket",
            true,
            "minecraft:item.bucket.fill_lava",
        ),
        ("powder_snow", "minecraft:bucket") => fill_bucket(
            player,
            "minecraft:powder_snow_bucket",
            level == 3,
            "minecraft:item.bucket.fill_powder_snow",
        ),
        _ => pass(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burning_entity_lowers_water_and_melts_powder_snow() {
        let water = |l| state("minecraft:water_cauldron", Some(l));
        assert_eq!(extinguished(&water(3)), water(2));
        assert_eq!(extinguished(&water(1)), state("minecraft:cauldron", None));
        assert_eq!(
            extinguished(&state("minecraft:powder_snow_cauldron", Some(3))),
            water(2)
        );
        assert_eq!(
            extinguished(&state("minecraft:powder_snow_cauldron", Some(1))),
            state("minecraft:cauldron", None)
        );
    }
}
