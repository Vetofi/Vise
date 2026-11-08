use super::Game;
use vise_client::{
    items::ItemStack,
    lan::{data::component_nbt, menu::NativeMenuEvent},
    text::Text,
};

#[allow(dead_code)]
pub(crate) fn native_menu_inventory_slot(size: usize, slot: i32) -> Option<usize> {
    let n = usize::try_from(slot).ok()?.checked_sub(size)?;
    match n {
        0..27 => Some(n + 9),
        27..36 => Some(n - 27),
        _ => None,
    }
}

fn fit(
    inventory: &mut [Option<ItemStack>],
    gets: &[ItemStack],
    max: &impl Fn(&ItemStack) -> u32,
) -> bool {
    for stack in gets {
        let mut left = stack.count;
        let max = max(stack).max(1);
        for slot in inventory.iter_mut().take(36).flatten() {
            if left > 0 && slot.same_item_same_components(stack) && slot.count < max {
                let n = (max - slot.count).min(left);
                slot.count += n;
                left -= n;
            }
        }
        for slot in inventory.iter_mut().take(36) {
            if left > 0 && slot.is_none() {
                let n = max.min(left);
                *slot = Some(ItemStack {
                    count: n,
                    ..stack.clone()
                });
                left -= n;
            }
        }
        if left > 0 {
            return false;
        }
    }
    true
}

fn exchange(
    a: &mut Vec<Option<ItemStack>>,
    a_offer: &[(usize, ItemStack)],
    b: &mut Vec<Option<ItemStack>>,
    b_offer: &[(usize, ItemStack)],
    max: impl Fn(&ItemStack) -> u32,
) -> Result<(), String> {
    fn take(
        inv: &[Option<ItemStack>],
        offer: &[(usize, ItemStack)],
        who: &str,
    ) -> Result<(Vec<Option<ItemStack>>, Vec<ItemStack>), String> {
        let mut left = inv.to_vec();
        let mut taken = Vec::new();
        for (slot, expected) in offer {
            match left.get_mut(*slot).filter(|_| *slot < 36) {
                Some(s) if s.as_ref() == Some(expected) => taken.push(s.take().expect("checked")),
                _ => return Err(format!("{who} no longer holds their offer")),
            }
        }
        Ok((left, taken))
    }
    let (mut a_after, a_gives) = take(a, a_offer, "the first player")?;
    let (mut b_after, b_gives) = take(b, b_offer, "the second player")?;
    if !fit(&mut a_after, &b_gives, &max) {
        return Err("the first player has no room".into());
    }
    if !fit(&mut b_after, &a_gives, &max) {
        return Err("the second player has no room".into());
    }
    *a = a_after;
    *b = b_after;
    Ok(())
}

#[allow(dead_code)]
impl Game<'_> {
    fn native_conn(&self, uuid: u128) -> Option<u32> {
        let host = self.lan.as_ref()?;
        let g = host
            .hub
            .guests
            .iter()
            .find(|g| g.placed && g.uuid == uuid)?;
        Some(g.conn)
    }
    pub(super) fn native_open_menu(
        &mut self,
        uuid: u128,
        rows: u8,
        title: Text,
        items: Vec<Option<ItemStack>>,
    ) -> Option<i32> {
        let conn = self.native_conn(uuid)?;
        let host = self.lan.as_mut()?;
        host.hub
            .open_native_menu(conn, rows, component_nbt(&title.component()), items)
    }
    pub(super) fn native_set_menu_items(&mut self, uuid: u128, items: Vec<Option<ItemStack>>) {
        if let Some(conn) = self.native_conn(uuid)
            && let Some(host) = self.lan.as_mut()
        {
            host.hub.set_native_menu(conn, items);
        }
    }
    pub(super) fn native_close_menu(&mut self, uuid: u128) {
        if let Some(conn) = self.native_conn(uuid)
            && let Some(host) = self.lan.as_mut()
            && host
                .hub
                .guest(conn)
                .and_then(|g| g.menu.as_ref())
                .is_some_and(|m| m.slots == vise_client::lan::menu::Slots::Native)
        {
            host.hub.close_container(conn);
        }
    }
    pub(super) fn native_player_stacks(&self, uuid: u128) -> Option<Vec<Option<ItemStack>>> {
        let host = self.lan.as_ref()?;
        let g = host
            .hub
            .guests
            .iter()
            .find(|g| g.placed && g.uuid == uuid)?;
        (!g.dead && self.lan_acting != Some(g.conn)).then(|| {
            let mut stacks = g.inventory.clone();
            stacks.resize(36, None);
            stacks
        })
    }
    pub(super) fn native_exchange_items(
        &mut self,
        a: u128,
        a_offer: &[(usize, ItemStack)],
        b: u128,
        b_offer: &[(usize, ItemStack)],
    ) -> Result<(), String> {
        if a == b {
            return Err("a player can't trade with themselves".into());
        }
        let acting = self.lan_acting;
        let items = self.res.items();
        let Some(host) = self.lan.as_mut() else {
            return Err("no server".into());
        };
        let find = |uuid: u128| {
            host.hub
                .guests
                .iter()
                .position(|g| g.placed && !g.dead && g.uuid == uuid)
                .filter(|&i| acting != Some(host.hub.guests[i].conn))
        };
        let (Some(ia), Some(ib)) = (find(a), find(b)) else {
            return Err("both players must be here".into());
        };
        let mut inv_a = std::mem::take(&mut host.hub.guests[ia].inventory);
        let mut inv_b = std::mem::take(&mut host.hub.guests[ib].inventory);
        let result = exchange(&mut inv_a, a_offer, &mut inv_b, b_offer, |s| {
            items.get(&s.item).map_or(64, |i| i.max_stack)
        });
        host.hub.guests[ia].inventory = inv_a;
        host.hub.guests[ib].inventory = inv_b;
        result
    }
}

impl Game<'_> {
    pub(super) fn run_native_menu_events(&mut self) {
        loop {
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            if host.hub.native_menu_events.is_empty() {
                return;
            }
            match host.hub.native_menu_events.remove(0) {
                NativeMenuEvent::Click {
                    uuid,
                    window,
                    slot,
                    button,
                    mode,
                } => self.native_menu_clicked(uuid, window, slot, button, mode),
                NativeMenuEvent::Closed { uuid, window } => self.native_menu_closed(uuid, window),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stack(item: &str, count: u32) -> ItemStack {
        ItemStack {
            item: item.into(),
            count,
            damage: 0,
            extra: None,
        }
    }
    fn inventory(stacks: &[(usize, ItemStack)]) -> Vec<Option<ItemStack>> {
        let mut inv = vec![None; 36];
        for (i, s) in stacks {
            inv[*i] = Some(s.clone());
        }
        inv
    }
    fn max(s: &ItemStack) -> u32 {
        if &*s.item == "minecraft:diamond_sword" {
            1
        } else {
            64
        }
    }

    #[test]
    fn menu_clicks_map_to_the_inventory_below() {
        assert_eq!(native_menu_inventory_slot(54, 3), None);
        assert_eq!(native_menu_inventory_slot(54, 54), Some(9));
        assert_eq!(native_menu_inventory_slot(54, 80), Some(35));
        assert_eq!(native_menu_inventory_slot(54, 81), Some(0));
        assert_eq!(native_menu_inventory_slot(54, 89), Some(8));
        assert_eq!(native_menu_inventory_slot(54, 90), None);
        assert_eq!(native_menu_inventory_slot(54, -999), None);
        assert_eq!(native_menu_inventory_slot(9, 9), Some(9));
    }

    #[test]
    fn an_exchange_swaps_both_offers() {
        let mut a = inventory(&[
            (0, stack("minecraft:diamond", 5)),
            (1, stack("minecraft:dirt", 64)),
        ]);
        let mut b = inventory(&[
            (4, stack("minecraft:diamond_sword", 1)),
            (5, stack("minecraft:diamond", 60)),
        ]);
        let a_offer = [(0, stack("minecraft:diamond", 5))];
        let b_offer = [(4, stack("minecraft:diamond_sword", 1))];
        exchange(&mut a, &a_offer, &mut b, &b_offer, max).unwrap();
        assert_eq!(a[0], Some(stack("minecraft:diamond_sword", 1)));
        assert_eq!(a[1], Some(stack("minecraft:dirt", 64)));
        assert_eq!(b[0], Some(stack("minecraft:diamond", 1)));
        assert_eq!(b[4], None);
        assert_eq!(b[5], Some(stack("minecraft:diamond", 64)));
        let count = |inv: &[Option<ItemStack>], item: &str| -> u32 {
            inv.iter()
                .flatten()
                .filter(|s| &*s.item == item)
                .map(|s| s.count)
                .sum()
        };
        assert_eq!(
            count(&a, "minecraft:diamond") + count(&b, "minecraft:diamond"),
            65
        );
        assert_eq!(
            count(&a, "minecraft:diamond_sword") + count(&b, "minecraft:diamond_sword"),
            1
        );
    }

    #[test]
    fn an_exchange_that_cant_complete_moves_nothing() {
        let a0 = inventory(&[(0, stack("minecraft:diamond", 5))]);
        let b0 = inventory(&[(2, stack("minecraft:emerald", 3))]);
        let (mut a, mut b) = (a0.clone(), b0.clone());
        let stale = [(0, stack("minecraft:diamond", 6))];
        assert!(exchange(&mut a, &stale, &mut b, &[], max).is_err());
        assert_eq!((&a, &b), (&a0, &b0));
        assert!(
            exchange(
                &mut a,
                &[(7, stack("minecraft:diamond", 5))],
                &mut b,
                &[],
                max
            )
            .is_err()
        );
        assert!(
            exchange(
                &mut a,
                &[(36, stack("minecraft:diamond", 5))],
                &mut b,
                &[],
                max
            )
            .is_err()
        );
        assert_eq!((&a, &b), (&a0, &b0));
        let full: Vec<_> = (0..36).map(|_| Some(stack("minecraft:dirt", 64))).collect();
        let mut b = full.clone();
        let offer = [(0, stack("minecraft:diamond", 5))];
        assert!(exchange(&mut a, &offer, &mut b, &[], max).is_err());
        assert_eq!((&a, &b), (&a0, &full));
        let twice = [
            (0, stack("minecraft:diamond", 5)),
            (0, stack("minecraft:diamond", 5)),
        ];
        let mut b = b0.clone();
        assert!(exchange(&mut a, &twice, &mut b, &[], max).is_err());
        assert_eq!((&a, &b), (&a0, &b0));
    }
}
