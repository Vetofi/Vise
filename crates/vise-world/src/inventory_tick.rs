use crate::{
    item_stack_refs::StackRef,
    player_inventory::{EquipmentSlot, PlayerInventory},
};
pub trait InventoryTickWorld<I, K, V> {
    type Error;
    type Level: Copy;
    fn level(
        &mut self,
        inventory: &mut PlayerInventory<I, K, V>,
    ) -> Result<Self::Level, Self::Error>;
    fn is_server_level(&self, level: Self::Level) -> bool;
    fn item_tick(
        &mut self,
        inventory: &mut PlayerInventory<I, K, V>,
        item: I,
        stack: StackRef<I, K, V>,
        level: Self::Level,
        slot: Option<EquipmentSlot>,
    ) -> Result<(), Self::Error>;
}
pub fn tick_stack<
    I: Copy + Eq,
    K: Clone + Eq,
    V: Clone + PartialEq,
    W: InventoryTickWorld<I, K, V>,
>(
    w: &mut W,
    inventory: &mut PlayerInventory<I, K, V>,
    stack: StackRef<I, K, V>,
    level: W::Level,
    slot: Option<EquipmentSlot>,
) -> Result<(), W::Error> {
    {
        let mut data = stack.write();
        if data.pop_time > 0 {
            data.pop_time = data.pop_time.wrapping_sub(1);
        }
    }
    if w.is_server_level(level) {
        let item = stack.read().item();
        w.item_tick(inventory, item, stack, level, slot)?;
    }
    Ok(())
}
pub fn tick_inventory<
    I: Copy + Eq,
    K: Clone + Eq,
    V: Clone + PartialEq,
    W: InventoryTickWorld<I, K, V>,
>(
    w: &mut W,
    inventory: &mut PlayerInventory<I, K, V>,
) -> Result<(), W::Error> {
    for index in 0..36 {
        let stack = inventory.get(index).expect("ordinary slot");
        if !stack.read().is_empty() {
            let level = w.level(inventory)?;
            let slot = (index as usize == inventory.selected()).then_some(EquipmentSlot::MainHand);
            tick_stack(w, inventory, stack, level, slot)?;
        }
    }
    Ok(())
}
pub fn tick_equipment<
    I: Copy + Eq,
    K: Clone + Eq,
    V: Clone + PartialEq,
    W: InventoryTickWorld<I, K, V>,
>(
    w: &mut W,
    inventory: &mut PlayerInventory<I, K, V>,
) -> Result<(), W::Error> {
    for slot in EquipmentSlot::ALL {
        let stack = inventory.equipment().get(slot);
        if !stack.read().is_empty() {
            let level = w.level(inventory)?;
            tick_stack(w, inventory, stack, level, Some(slot))?;
        }
    }
    Ok(())
}
