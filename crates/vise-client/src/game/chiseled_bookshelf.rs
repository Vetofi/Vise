use super::{Game, Hit, machines::dir_from_name};
use vise_client::{containers::BlockEntity, items::ItemStack, mobs::Who};
use vise_core::{
    geometry::{Direction, Vec3},
    position::BlockPos,
};
use vise_world::level::{flags, hit_slot};

const BLOCK: &str = "minecraft:chiseled_bookshelf";

impl Game<'_> {
    pub(super) fn use_chiseled_bookshelf(
        &mut self,
        hit: &Hit,
        held: Option<&ItemStack>,
    ) -> Option<bool> {
        let pos = hit.pos;
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let info = self
            .pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.name == BLOCK)?;
        let facing = info
            .properties
            .get("facing")
            .and_then(|f| dir_from_name(f))?;
        let face = Direction::ALL
            .into_iter()
            .find(|d| d.normal() == hit.face)?;
        let at = Vec3::new(hit.point[0], hit.point[1], hit.point[2]);
        let slot = hit_slot(
            facing,
            face,
            BlockPos::new(pos[0], pos[1], pos[2]),
            at,
            2,
            3,
        )?;
        let occupied = info
            .properties
            .get(&format!("slot_{slot}_occupied"))
            .map(String::as_str)
            == Some("true");
        let book = held.filter(|s| {
            self.res
                .items()
                .item_tags
                .get("minecraft:bookshelf_books")
                .is_some_and(|m| m.contains(&*s.item))
        });
        let centre = pos.map(|v| f64::from(v) + 0.5);
        if let Some(book) = book
            && !occupied
        {
            let book = book.clone();
            self.stats.add("used", &book.item, 1);
            let sound = if &*book.item == "minecraft:enchanted_book" {
                "minecraft:block.chiseled_bookshelf.insert.enchanted"
            } else {
                "minecraft:block.chiseled_bookshelf.insert"
            };
            self.bookshelf_items(pos)?[slot] = Some(ItemStack {
                count: 1,
                ..book.clone()
            });
            self.consume_held();
            self.chiseled_bookshelf_update_state(pos, slot);
            self.audio.play(sound, Some(centre), 1.0, 1.0);
            self.trigger_block("minecraft:item_used_on_block", pos, &book);
            return Some(true);
        }
        if self.hand != vise_client::first_person::Hand::Main {
            return None;
        }
        if !occupied {
            return Some(false);
        }
        let retrieved = self.bookshelf_items(pos)?[slot].take()?;
        self.chiseled_bookshelf_update_state(pos, slot);
        let sound = if &*retrieved.item == "minecraft:enchanted_book" {
            "minecraft:block.chiseled_bookshelf.pickup.enchanted"
        } else {
            "minecraft:block.chiseled_bookshelf.pickup"
        };
        self.audio.play(sound, Some(centre), 1.0, 1.0);
        self.give_or_drop(retrieved);
        self.post_block_game_event("minecraft:block_change", pos, Some(Who::Player), None);
        Some(true)
    }

    fn bookshelf_items(&mut self, pos: [i32; 3]) -> Option<&mut [Option<ItemStack>; 6]> {
        let entity = self
            .world
            .block_entities
            .get_or_insert_with((pos[0], pos[1], pos[2]), || {
                BlockEntity::ChiseledBookshelf(Box::default())
            });
        match entity {
            BlockEntity::ChiseledBookshelf(b) => Some(&mut b.items),
            _ => None,
        }
    }

    fn chiseled_bookshelf_update_state(&mut self, pos: [i32; 3], slot: usize) {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::ChiseledBookshelf(b)) = self.world.block_entities.get_mut(&key)
        else {
            return;
        };
        b.last_slot = slot as i32;
        let occupied: Vec<bool> = b.items.iter().map(Option::is_some).collect();
        let registry = self.pack.assets.registry;
        let Some(mut state) = registry
            .state(self.world.block(pos[0], pos[1], pos[2]))
            .cloned()
        else {
            return;
        };
        for (i, occupied) in occupied.into_iter().enumerate() {
            state
                .properties
                .insert(format!("slot_{i}_occupied"), occupied.to_string());
        }
        let Some(new) = registry.id(&state) else {
            return;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.set_block(level, ticks, at, new, flags::UPDATE_ALL));
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        self.post_block_game_event("minecraft:block_change", pos, None, Some(new));
    }

    pub(super) fn chiseled_bookshelf_synced(&mut self, pos: (i32, i32, i32)) {
        let Some(BlockEntity::ChiseledBookshelf(b)) = self.world.block_entities.get(&pos) else {
            return;
        };
        let Some(info) = self
            .pack
            .assets
            .registry
            .state(self.world.block(pos.0, pos.1, pos.2))
        else {
            return;
        };
        let moved = (0..6).rev().find(|i| {
            let shown = info
                .properties
                .get(&format!("slot_{i}_occupied"))
                .map(String::as_str);
            b.items[*i].is_some() != (shown == Some("true"))
        });
        if let Some(slot) = moved {
            self.chiseled_bookshelf_update_state([pos.0, pos.1, pos.2], slot);
        }
    }
}
