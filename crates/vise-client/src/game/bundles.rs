use super::{Game, Screen};
use vise_client::{
    bundle::{Contents, toggle_selected},
    mouse::ScrollWheelHandler,
};
use vise_protocol::packets::play::{self, Serverbound as S};

#[derive(Default)]
pub(super) struct BundleMouse {
    wheel: ScrollWheelHandler,
    hovered: Option<usize>,
}

pub(super) fn takes_bundle_wheel(screen: Screen) -> bool {
    matches!(screen, Screen::Inventory | Screen::Creative)
}

impl Game<'_> {
    fn bundle_slot(&mut self, slot: usize) -> bool {
        (slot < 36 || slot == 504 || (300..500).contains(&slot))
            && (self.remote.is_none() || self.remote_storage_open())
            && (slot < 36 || slot == 504 || self.slots().contains(&slot))
    }

    pub(super) fn bundle_scroll(&mut self, x: f64, y: f64) -> bool {
        let Some(slot) = self.inventory_target().0 else {
            return false;
        };
        if !self.bundle_slot(slot) {
            return false;
        }
        let stack = self.slot_mut(slot).clone();
        let items = self.items();
        let Some(bundle) = stack.filter(|s| items.get(&s.item).is_some_and(|i| i.bundle)) else {
            return false;
        };
        let shown = Contents::of(items, &bundle).map_or(0, |c| c.shown_count());
        if shown == 0 {
            return false;
        }
        let (wheel_x, wheel_y) = self.bundle_mouse.wheel.on_mouse_scroll(x, y);
        let wheel = if wheel_y == 0 { -wheel_x } else { wheel_y };
        if wheel != 0 {
            let selected = bundle.data().map_or(-1, |d| d.bundle_selected.0);
            let next = ScrollWheelHandler::next_selection(f64::from(wheel), selected, shown as i32);
            if next != selected {
                self.select_bundle(slot, next);
            }
        }
        true
    }

    fn select_bundle(&mut self, slot: usize, index: i32) {
        if let Some(stack) = self.slot_mut(slot) {
            toggle_selected(stack, index);
        }
        if self.remote.is_some() {
            let (_, menu) = self.remote_menu();
            if let Some(menu_slot) = Self::menu_slot_index(&menu, slot) {
                self.send(S::SelectBundleItem(play::SelectBundleItem {
                    slot: menu_slot as i32,
                    index,
                }));
            }
        }
    }

    pub(super) fn unselect_bundle(&mut self, slot: usize) {
        if !self.bundle_slot(slot) {
            return;
        }
        let selected = self
            .slot_mut(slot)
            .as_ref()
            .and_then(|s| s.data())
            .is_some_and(|d| d.bundle_selected.0 >= 0);
        if selected {
            self.select_bundle(slot, -1);
        }
    }

    pub(super) fn tick_bundle_hover(&mut self) {
        let now = if takes_bundle_wheel(self.screen) {
            self.inventory_target().0
        } else {
            None
        };
        if let Some(left) = std::mem::replace(&mut self.bundle_mouse.hovered, now)
            && Some(left) != now
        {
            self.unselect_bundle(left);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_screens_offer_the_wheel_to_bundles() {
        assert!(takes_bundle_wheel(Screen::Inventory));
        assert!(takes_bundle_wheel(Screen::Creative));
        for screen in [
            Screen::Playing,
            Screen::Chat,
            Screen::Paused,
            Screen::Options,
        ] {
            assert!(!takes_bundle_wheel(screen));
        }
    }
}
