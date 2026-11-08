use super::{Creative, Game, Screen};
use vise_client::{
    containers::BlockEntity,
    hud::{HudBatch, rgba},
    inv_sort,
    items::ItemStack,
    keybinds,
    menu::Input,
    stations::hover_name,
};
use winit::{event::MouseButton, keyboard::KeyCode};

const SORT_BUTTON: f32 = 9.0;

#[derive(Default)]
pub(super) struct Tweaks {
    drag: Option<Drag>,
    wheel: f32,
}
pub(super) fn tweak_menu(name: &str) -> bool {
    name == "inventory"
        || name.starts_with("minecraft:generic_9x")
        || matches!(
            name,
            "minecraft:shulker_box"
                | "minecraft:hopper"
                | "minecraft:generic_3x3"
                | "minecraft:furnace"
                | "minecraft:blast_furnace"
                | "minecraft:smoker"
                | "minecraft:crafting"
        )
}
#[derive(Clone, Debug)]
struct Drag {
    right: bool,
    held: Option<ItemStack>,
    active: bool,
    visited: Vec<usize>,
}

impl Game<'_> {
    pub(super) fn tweaks_on(&self) -> bool {
        if !self.settings.extras.mouse_tweaks
            || self.screen != Screen::Inventory
            || self.station.is_some()
            || self.mount_active()
        {
            return false;
        }
        if self.remote.is_some() {
            self.remote_menu_type().is_none_or(tweak_menu)
        } else {
            matches!(self.container_kind(), None | Some(BlockEntity::Chest(_)))
        }
    }
    pub(super) fn tweak_click(&mut self, slot: usize, button: i32, input: Input) -> bool {
        if self.remote.is_some() {
            return self.remote_menu_ops(&[(Some(slot), button, input)]);
        }
        if !self.menu_click(Some(slot), button, input) {
            return false;
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
        true
    }
    fn tweak_put(&mut self, slot: usize, right: bool) -> bool {
        let kind = i32::from(right) << 2;
        let ops = [
            (None, kind, Input::QuickCraft),
            (Some(slot), kind | 1, Input::QuickCraft),
            (None, kind | 2, Input::QuickCraft),
        ];
        if self.remote.is_some() {
            return self.remote_menu_ops(&ops);
        }
        if !self.menu_ops(&ops) {
            return false;
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
        true
    }
    pub(super) fn tweak_press(&mut self, slot: Option<usize>, right: bool, shift: bool) -> bool {
        self.tweaks.drag = None;
        let Some(slot) = slot.filter(|&s| s != 200 && s < Creative::DISPLAY) else {
            return false;
        };
        if !self.tweaks_on() {
            return false;
        }
        let held = self.carried.clone();
        if right {
            if shift || held.is_none() || self.tweak_result(slot) {
                return false;
            }
            let mut drag = Drag {
                right,
                held,
                active: true,
                visited: vec![slot],
            };
            self.tweak_place(&mut drag, slot);
            self.inv_drag = None;
            self.inv_clicks.skip_release();
            self.tweaks.drag = Some(drag);
            return true;
        }
        if held.is_some() || shift {
            self.tweaks.drag = Some(Drag {
                right,
                held,
                active: false,
                visited: vec![slot],
            });
        }
        false
    }
    fn tweak_place(&mut self, drag: &mut Drag, slot: usize) {
        let Some(held) = drag.held.clone() else {
            return;
        };
        if self.tweak_takes_one(slot, &held) && self.tweak_put(slot, true) {
            drag.held = (held.count > 1).then(|| ItemStack {
                count: held.count - 1,
                ..held
            });
        }
    }
    pub(super) fn tweak_drag_move(&mut self, slot: Option<usize>) -> bool {
        let Some(mut drag) = self.tweaks.drag.take() else {
            return false;
        };
        let active = drag.active;
        let entered =
            slot.filter(|s| *s != 200 && *s < Creative::DISPLAY && !drag.visited.contains(s));
        if let Some(slot) = entered {
            drag.visited.push(slot);
            let stack = self.slot_view(slot);
            let shift = self.has_shift_down();
            if drag.right {
                self.tweak_place(&mut drag, slot);
            } else if let Some(held) = drag.held.clone() {
                let spread = self.inv_drag.as_ref().is_some_and(|(_, s)| s.len() > 1);
                if let Some(s) = stack.filter(|s| s.same_item_same_components(&held))
                    && (drag.active || !spread)
                {
                    if !drag.active {
                        drag.active = true;
                        self.inv_drag = None;
                        self.inv_clicks.skip_release();
                    }
                    if shift {
                        self.tweak_click(slot, 0, Input::QuickMove);
                    } else if held.count + s.count <= self.max_stack(&s)
                        && self.tweak_click(slot, 0, Input::Pickup)
                        && self.tweak_click(slot, 0, Input::Pickup)
                    {
                        drag.held = Some(ItemStack {
                            count: held.count + s.count,
                            ..held
                        });
                    }
                }
            } else if shift && stack.is_some() {
                drag.active = true;
                self.tweak_click(slot, 0, Input::QuickMove);
            }
        }
        let owned = drag.active || active;
        self.tweaks.drag = Some(drag);
        owned
    }
    pub(super) fn tweak_release(&mut self) {
        self.tweaks.drag = None;
    }
    pub(super) fn tweak_scroll(&mut self, delta: f32, slot: Option<usize>) -> bool {
        if !self.settings.extras.mouse_tweaks_wheel || !self.tweaks_on() || self.carried.is_some() {
            return false;
        }
        let Some(slot) = slot else {
            self.tweaks.wheel = 0.0;
            return false;
        };
        self.tweaks.wheel += delta;
        let steps = self.tweaks.wheel.trunc();
        self.tweaks.wheel -= steps;
        for _ in 0..steps.abs() as u32 {
            if steps < 0.0 {
                self.tweak_push(slot);
            } else {
                self.tweak_pull(slot);
            }
        }
        true
    }
    fn tweak_result(&mut self, slot: usize) -> bool {
        let mut result = false;
        self.menu_session(|menu, _, to_menu| {
            result = to_menu(slot).is_some_and(|i| menu.is_result(i));
            Some(false)
        });
        result
    }
    fn tweak_takes_one(&mut self, slot: usize, stack: &ItemStack) -> bool {
        let mut takes = false;
        self.menu_session(|menu, env, to_menu| {
            takes = to_menu(slot).is_some_and(|i| menu.takes_one(env, i, stack));
            Some(false)
        });
        takes
    }
    fn tweak_landing(&mut self, slot: usize) -> Vec<usize> {
        let slots = self.slots();
        let mut landing = Vec::new();
        self.menu_session(|menu, env, to_menu| {
            let from = to_menu(slot)?;
            landing = menu
                .quick_move_landing(env, from)
                .into_iter()
                .filter_map(|i| slots.iter().copied().find(|&g| to_menu(g) == Some(i)))
                .collect();
            Some(false)
        });
        landing
    }
    fn other_side(&self, slot: usize) -> Vec<usize> {
        let table = self.craft_size == 3;
        let own: Vec<usize> = self
            .slots()
            .into_iter()
            .filter(|s| (300..500).contains(s) || (table && (100..109).contains(s)))
            .collect();
        let player = || (0..9).rev().chain((9..36).rev());
        match slot {
            100..109 | 300..500 => player().collect(),
            0..36 if !own.is_empty() => own,
            0..9 => (9..36).collect(),
            9..36 => (0..9).collect(),
            _ => Vec::new(),
        }
    }
    fn tweak_push(&mut self, slot: usize) {
        let Some(stack) = self.slot_view(slot) else {
            return;
        };
        let landing = self.tweak_landing(slot);
        let Some(&target) = landing.first() else {
            return;
        };
        if self.tweak_result(slot) {
            if self.tweak_click(slot, 0, Input::Pickup) {
                for target in landing {
                    self.tweak_put(target, false);
                }
            }
            return;
        }
        if self.tweak_click(slot, 0, Input::Pickup) {
            self.tweak_put(target, true);
            if stack.count > 1 {
                self.tweak_put(slot, false);
            }
        }
    }
    fn tweak_pull(&mut self, slot: usize) {
        let Some(stack) = self.slot_view(slot) else {
            return;
        };
        if !self.tweak_takes_one(slot, &stack) {
            return;
        }
        let mut source = None;
        for s in self.other_side(slot) {
            if let Some(v) = self.slot_view(s)
                && v.same_item_same_components(&stack)
                && !self.tweak_result(s)
            {
                source = Some((s, v.count));
                break;
            }
        }
        let Some((source, count)) = source else {
            return;
        };
        if self.tweak_click(source, 0, Input::Pickup) {
            self.tweak_put(slot, true);
            if count > 1 {
                self.tweak_put(source, false);
            }
        }
    }
}

impl Game<'_> {
    fn sort_buttons(&self) -> Vec<((f32, f32), Vec<usize>)> {
        if !self.settings.extras.sort
            || self.screen != Screen::Inventory
            || self.station.is_some()
            || self.mount_active()
        {
            return Vec::new();
        }
        let chest_like = if self.remote.is_some() {
            if !self.remote_storage_open() {
                return Vec::new();
            }
            true
        } else {
            match self.container_kind() {
                None => false,
                Some(BlockEntity::Chest(_)) => {
                    self.chest_rows().is_some() || self.container_block_is_shulker()
                }
                Some(_) => return Vec::new(),
            }
        };
        let mut buttons = Vec::new();
        let container: Vec<usize> = self
            .slots()
            .into_iter()
            .filter(|s| (300..500).contains(s))
            .collect();
        if chest_like && !container.is_empty() {
            buttons.push(((160.0, 5.0), container));
        }
        buttons.push(((160.0, self.screen_height() - 95.0), (9..36).collect()));
        buttons
    }
    pub(super) fn draw_sort_buttons(&self, batch: &mut HudBatch, ox: f32, oy: f32) {
        let atlas = &self.gui.atlas;
        for ((x, y), _) in self.sort_buttons() {
            let (x, y) = (ox + x, oy + y);
            let (cx, cy) = self.cursor;
            let hovered = cx >= x && cx < x + SORT_BUTTON && cy >= y && cy < y + SORT_BUTTON;
            let face = if hovered {
                rgba(220, 220, 220, 255)
            } else {
                rgba(160, 160, 160, 255)
            };
            batch.fill(atlas, x, y, SORT_BUTTON, SORT_BUTTON, rgba(55, 55, 55, 255));
            batch.fill(
                atlas,
                x + 1.0,
                y + 1.0,
                SORT_BUTTON - 2.0,
                SORT_BUTTON - 2.0,
                face,
            );
            for (i, w) in [5.0, 4.0, 3.0].into_iter().enumerate() {
                let by = y + 2.0 + 2.0 * i as f32;
                batch.fill(atlas, x + 2.0, by, w, 1.0, rgba(55, 55, 55, 255));
            }
        }
    }
    pub(super) fn sort_press(&mut self) -> bool {
        let Some(r) = self.renderer.as_ref() else {
            return false;
        };
        let batch = HudBatch::new([r.config.width, r.config.height]);
        let (ox, oy) = self.inventory_origin(&batch);
        let (cx, cy) = self.cursor;
        let hit = self.sort_buttons().into_iter().find(|((x, y), _)| {
            let (x, y) = (ox + x, oy + y);
            cx >= x && cx < x + SORT_BUTTON && cy >= y && cy < y + SORT_BUTTON
        });
        let Some((_, slots)) = hit else {
            return false;
        };
        self.sort_slots(&slots);
        true
    }
    pub(super) fn sort_key_press(&mut self, key: KeyCode) -> bool {
        keybinds::key_name(key).is_some_and(|name| self.sort_input(name))
    }
    pub(super) fn sort_mouse_press(&mut self, button: MouseButton) -> bool {
        self.sort_input(&keybinds::mouse_name(button))
    }
    fn sort_input(&mut self, name: &str) -> bool {
        let e = &self.settings.extras;
        if !e.sort || e.sort_key != name || self.recipe_search_focused() {
            return false;
        }
        let buttons = self.sort_buttons();
        let hovered = self.inventory_target().0;
        let slots = buttons
            .iter()
            .find(|(_, slots)| hovered.is_some_and(|h| slots.contains(&h)))
            .or(buttons.first())
            .map(|(_, slots)| slots.clone());
        let Some(slots) = slots else {
            return false;
        };
        self.sort_slots(&slots);
        true
    }
    fn sort_slots(&mut self, slots: &[usize]) {
        if self.carried.is_some() {
            return;
        }
        let views: Vec<Option<ItemStack>> = slots.iter().map(|&s| self.slot_view(s)).collect();
        let clicks = {
            let names = |id: &str| self.pack.names.display_item(id);
            let rank = |s: &ItemStack| {
                let order = self.creative_screen.order_of(s).unwrap_or(usize::MAX);
                (order, hover_name(&names, s))
            };
            let max = |s: &ItemStack| self.max_stack(s);
            inv_sort::plan(&views, &rank, &max)
        };
        for i in clicks {
            self.tweak_click(slots[i], 0, Input::Pickup);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::tweak_menu;

    #[test]
    fn the_tweaks_run_in_the_menus_with_plain_click_rules() {
        for menu in [
            "inventory",
            "minecraft:generic_9x1",
            "minecraft:generic_9x6",
            "minecraft:shulker_box",
            "minecraft:hopper",
            "minecraft:generic_3x3",
            "minecraft:furnace",
            "minecraft:blast_furnace",
            "minecraft:smoker",
            "minecraft:crafting",
        ] {
            assert!(tweak_menu(menu), "{menu}");
        }
        for menu in [
            "minecraft:anvil",
            "minecraft:enchantment",
            "minecraft:merchant",
            "minecraft:beacon",
            "minecraft:grindstone",
            "minecraft:stonecutter",
            "minecraft:loom",
            "minecraft:brewing_stand",
            "minecraft:crafter_3x3",
        ] {
            assert!(!tweak_menu(menu), "{menu}");
        }
    }
}
