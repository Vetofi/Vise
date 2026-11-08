use super::Game;
use vise_client::{Result, items::ItemStack, text::Text};
use vise_core::position::BlockPos;
use vise_world::level::flags;
use winit::keyboard::KeyCode;

const DEBUG_STICK: &str = "minecraft:debug_stick";

fn is_debug_stick(stack: &ItemStack) -> bool {
    &*stack.item == DEBUG_STICK
}

fn relative_index(len: usize, current: Option<usize>, backward: bool) -> usize {
    match (current, backward) {
        (Some(0), true) => len - 1,
        (Some(i), true) => i - 1,
        (Some(i), false) => (i + 1) % len,
        (None, true) => len - 1,
        (None, false) => 0,
    }
}

fn text(key: &str, args: &[&str]) -> Text {
    let args: Vec<Text> = args.iter().map(|arg| Text::literal(*arg)).collect();
    Text::translate(key, &args)
}

impl Game<'_> {
    pub(super) fn debug_stick_use_on(&mut self, pos: [i32; 3]) -> Result<Option<bool>> {
        if !self.held().is_some_and(is_debug_stick) {
            return Ok(None);
        }
        Ok(Some(self.debug_stick_interact(pos, true)?))
    }

    pub(super) fn debug_stick_can_destroy(&mut self, pos: [i32; 3]) -> Result<bool> {
        if !self.held().is_some_and(is_debug_stick) {
            return Ok(true);
        }
        let _ = self.debug_stick_interact(pos, false)?;
        Ok(false)
    }

    fn debug_stick_interact(&mut self, pos: [i32; 3], cycle: bool) -> Result<bool> {
        if !self.can_use_game_master_blocks() {
            return Ok(false);
        }
        let state_id = self.world.block(pos[0], pos[1], pos[2]);
        let Some(state) = self.pack.assets.registry.state(state_id).cloned() else {
            return Ok(false);
        };
        if state.properties.is_empty() {
            self.debug_stick_message(text(
                "item.minecraft.debug_stick.empty",
                &[state.name.as_str()],
            ));
            return Ok(false);
        }

        let selected = self
            .held()
            .and_then(ItemStack::data)
            .and_then(|data| data.debug_stick_property(&state.name));
        let backward = self.keys.contains(&KeyCode::ShiftLeft);

        if cycle {
            let (property, value) = {
                let registry = &self.pack.assets.registry;
                let property = selected
                    .as_deref()
                    .filter(|property| state.properties.contains_key(*property))
                    .map(str::to_owned)
                    .unwrap_or_else(|| state.properties.keys().next().unwrap().clone());
                let values = registry.property_values(&state.name, &property);
                if values.is_empty() {
                    return Ok(false);
                }
                let current = state.properties.get(&property).map(String::as_str);
                let current = current.and_then(|value| values.iter().position(|v| *v == value));
                let value = values[relative_index(values.len(), current, backward)].to_owned();
                (property, value)
            };
            let mut next = state;
            next.properties.insert(property.clone(), value.clone());
            let Ok(next_id) = self.pack.assets.registry.resolve(&next) else {
                return Ok(false);
            };
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            self.with_level(|b, level, ticks| {
                b.set_block(
                    level,
                    ticks,
                    at,
                    next_id,
                    flags::UPDATE_CLIENTS | flags::UPDATE_KNOWN_SHAPE,
                );
            });
            self.process_level_events()?;
            self.debug_stick_message(text(
                "item.minecraft.debug_stick.update",
                &[property.as_str(), value.as_str()],
            ));
        } else {
            let keys: Vec<&String> = state.properties.keys().collect();
            let current = selected
                .as_deref()
                .and_then(|selected| keys.iter().position(|key| key.as_str() == selected));
            let property = keys[relative_index(keys.len(), current, backward)].clone();
            let value = state.properties.get(&property).cloned().unwrap_or_default();
            self.set_debug_stick_property(&state.name, &property);
            self.debug_stick_message(text(
                "item.minecraft.debug_stick.select",
                &[property.as_str(), value.as_str()],
            ));
        }
        Ok(true)
    }

    fn set_debug_stick_property(&mut self, block: &str, property: &str) {
        let Some(stack) = self.held_mut().as_mut() else {
            return;
        };
        stack.data_mut().set_debug_stick_property(block, property);
        stack.tidy();
    }

    fn debug_stick_message(&mut self, message: Text) {
        if self.lan_acting.is_some() {
            self.guest_packets
                .push(vise_protocol::packets::play::Clientbound::SystemChat(
                    vise_protocol::packets::play::SystemChat {
                        content: vise_client::lan::data::component_nbt(&message.component()),
                        overlay: true,
                    },
                ));
        } else if self.options.dedicated.is_none() {
            self.titles.set_overlay(message, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::relative_index;

    #[test]
    fn relative_index_wraps_like_util() {
        assert_eq!(relative_index(4, None, false), 0);
        assert_eq!(relative_index(4, None, true), 3);
        assert_eq!(relative_index(4, Some(0), false), 1);
        assert_eq!(relative_index(4, Some(0), true), 3);
        assert_eq!(relative_index(4, Some(2), false), 3);
        assert_eq!(relative_index(4, Some(2), true), 1);
        assert_eq!(relative_index(4, Some(3), false), 0);
        assert_eq!(relative_index(4, Some(3), true), 2);
    }
}
