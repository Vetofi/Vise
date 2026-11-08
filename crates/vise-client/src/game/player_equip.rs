use super::Game;
use vise_client::items::ItemStack;

const ARMOR: [&str; 4] = ["head", "chest", "legs", "feet"];
const OFFHAND: &str = "offhand";

fn same(a: Option<&ItemStack>, b: Option<&ItemStack>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_item_same_components(b),
        _ => false,
    }
}

impl Game<'_> {
    pub(super) fn player_equipment(&self) -> [Option<ItemStack>; 5] {
        let [head, chest, legs, feet] = self.armor.clone();
        [head, chest, legs, feet, self.offhand.clone()]
    }

    pub(super) fn player_equipment_changed(&mut self, before: &[Option<ItemStack>; 5]) {
        let after = self.player_equipment();
        for (i, slot) in ARMOR.into_iter().chain([OFFHAND]).enumerate() {
            self.player_on_equip(slot, before[i].as_ref(), after[i].as_ref());
        }
    }

    pub(super) fn player_on_equip(
        &mut self,
        slot: &str,
        old: Option<&ItemStack>,
        new: Option<&ItemStack>,
    ) {
        if !same(old, new) {
            self.player_equipped(slot, new);
        }
    }

    pub(super) fn player_equipped(&mut self, slot: &str, new: Option<&ItemStack>) {
        if self.remote.is_some() || self.spectator() {
            return;
        }
        let Some(at) = self.player.as_ref().map(|p| p.position) else {
            return;
        };
        let at = [at.x, at.y, at.z];
        let equippable = new
            .and_then(|s| self.items().get(&s.item))
            .and_then(|info| {
                let own = info.equip_slot.as_deref()?;
                Some((own == slot, info.equip_sound.clone()))
            });
        if let Some((true, sound)) = &equippable {
            let sound = sound
                .as_deref()
                .unwrap_or("minecraft:item.armor.equip_generic");
            let players = vise_client::settings::SOUND_SOURCES
                .iter()
                .position(|s| *s == "player")
                .unwrap_or(0);
            self.audio.play_on(players, sound, Some(at), 1.0, 1.0);
        }
        if ARMOR.contains(&slot) {
            let event = if equippable.is_some() {
                "minecraft:equip"
            } else {
                "minecraft:unequip"
            };
            let ctx = self.player_context(None);
            self.post_game_event(event, at, ctx);
        }
    }
}
