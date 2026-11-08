use super::Game;
use vise_client::{
    Result, block_entity_item, block_entity_nbt, containers::BlockEntity, item_data,
    items::ItemStack, save,
};
use vise_world::{
    level::classes::Class,
    nbt::{Compound, JavaString, Tag},
    registry::BlockStateId,
};

fn components(stack: &ItemStack) -> Compound {
    match item_data::encode_components(stack) {
        Some(Tag::Compound(c)) => c,
        _ => Compound::new(),
    }
}

impl Game<'_> {
    fn saved_block_entity(&self, key: (i32, i32, i32)) -> Option<Tag> {
        let entity = self.world.block_entities.get(&key)?;
        let block = &self
            .res
            .facts()
            .get(self.world.block(key.0, key.1, key.2))
            .name;
        let mut tag = save::encode_block_entity(key, entity, block);
        if let (Some(name), Tag::Compound(map)) = (self.world.block_names.get(&key), &mut tag) {
            map.insert("CustomName".into(), Tag::String(name.as_str().into()));
        }
        block_entity_nbt::canonical(&tag)
    }

    pub(super) fn picked_stack(&self, pos: [i32; 3], include_data: bool) -> Option<ItemStack> {
        let key = (pos[0], pos[1], pos[2]);
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let facts = self.res.facts().get(state);
        let head = self
            .pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.name.as_str() == "minecraft:piston_head")
            .map(|s| match s.properties.get("type") {
                Some(t) if t == "sticky" => "minecraft:sticky_piston",
                _ => "minecraft:piston",
            });
        let item = head.unwrap_or(&*facts.item);
        if item == "minecraft:air" {
            return None;
        }
        let behaviours = self.res.behaviours();
        let plant = behaviours.data(state);
        let plant_item = plant
            .is(Class::FlowerPot)
            .then_some(plant.link)
            .flatten()
            .map(|block| behaviours.block_data(block).item.clone())
            .filter(|item| &**item != "minecraft:air");
        let mut stack = ItemStack::new(plant_item.as_deref().unwrap_or(item), 1);
        let mut patch = Compound::new();
        if include_data
            && let Some(level) = self
                .pack
                .assets
                .registry
                .state(state)
                .filter(|s| matches!(s.name.as_str(), "minecraft:beehive" | "minecraft:bee_nest"))
                .and_then(|s| s.properties.get("honey_level"))
        {
            patch.insert(
                "minecraft:block_state".into(),
                Tag::compound([("honey_level", Tag::String(level.as_str().into()))]),
            );
        }
        if let Some(pose) = self
            .pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.name.ends_with("copper_golem_statue"))
            .and_then(|s| s.properties.get("copper_golem_pose"))
        {
            patch.insert(
                "minecraft:block_state".into(),
                Tag::compound([("copper_golem_pose", Tag::String(pose.as_str().into()))]),
            );
        }
        if let Some(level) = self
            .pack
            .assets
            .registry
            .state(state)
            .filter(|s| s.name.as_str() == "minecraft:light")
            .and_then(|s| s.properties.get("level"))
        {
            patch.insert(
                "minecraft:block_state".into(),
                Tag::compound([("level", Tag::String(level.as_str().into()))]),
            );
        }
        let banner = matches!(
            self.world.block_entities.get(&key),
            Some(BlockEntity::Banner(_))
        );
        let pot = &*facts.name == "minecraft:decorated_pot";
        if (include_data || banner || pot)
            && let Some(saved) = self.saved_block_entity(key)
        {
            let mut picked = block_entity_item::pick(&saved);
            if pot && !include_data {
                picked.retain(|k, _| *k == JavaString::from("minecraft:pot_decorations"));
            }
            patch.extend(picked);
        }
        if !patch.is_empty() {
            let mut all = components(&stack);
            all.extend(patch);
            (stack.damage, stack.extra) = item_data::decode_components(Some(&Tag::Compound(all)));
        }
        Some(stack)
    }

    pub(super) fn apply_item_block_state(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        stack: &ItemStack,
    ) -> Result<BlockStateId> {
        let Some(Tag::Compound(saved)) =
            stack.data().and_then(|d| d.other("minecraft:block_state"))
        else {
            return Ok(state);
        };
        let Some(current) = self.pack.assets.registry.state(state) else {
            return Ok(state);
        };
        let props: Vec<(String, String)> = saved
            .iter()
            .filter_map(|(k, v)| match v {
                Tag::String(v) => Some((k.to_string_lossy(), v.to_string_lossy())),
                _ => None,
            })
            .filter(|(k, v)| current.properties.get(k).is_some_and(|old| old != v))
            .collect();
        let props: Vec<(&str, &str)> = props.iter().map(|(k, v)| (&**k, &**v)).collect();
        match self.with_props(state, &props) {
            Some(new) if !props.is_empty() && new != state => {
                self.level_set(pos, new)?;
                Ok(new)
            }
            _ => Ok(state),
        }
    }

    pub(super) fn apply_placed_item(
        &mut self,
        pos: [i32; 3],
        stack: &ItemStack,
        game_master: bool,
    ) {
        let key = (pos[0], pos[1], pos[2]);
        self.pending.banner.remove(&pos);
        let Some(saved) = self.saved_block_entity(key) else {
            return;
        };
        let placed = block_entity_item::place(&saved, &components(stack), game_master);
        if placed == saved {
            return;
        }
        let Some((_, mut entity)) = save::decode_block_entity(&placed) else {
            return;
        };
        if let (BlockEntity::Crafter(new), Some(BlockEntity::Crafter(old))) =
            (&mut entity, self.world.block_entities.get(&key))
        {
            new.state_triggered = old.state_triggered;
        }
        if !matches!(entity, BlockEntity::Banner(_) | BlockEntity::Sign(_)) {
            match save::text(placed.get("CustomName")) {
                Some(name) => self.world.block_names.insert(key, name),
                None => self.world.block_names.remove(&key),
            };
        }
        self.world.block_entities.insert(key, entity);
        self.update_spawner_display(key);
    }
}
