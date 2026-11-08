use super::Game;
use vise_assets::item_model::DisplayContext;
use vise_client::{
    Result, block_entities::Pos, block_entity_render::vertex_light, containers::BlockEntity,
    items::ItemStack, mesher::Vertex, mobs::Who,
};
use vise_core::position::BlockPos;

const KIND: &str = "minecraft:campfire_cooking";

fn is_campfire(name: &str) -> bool {
    name == "minecraft:campfire" || name == "minecraft:soul_campfire"
}

impl Game<'_> {
    pub(super) fn use_on_campfire(&mut self, pos: [i32; 3]) -> Result<Option<bool>> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !is_campfire(&self.res.facts().get(state).name) {
            return Ok(None);
        }
        let Some(held) = self.held().cloned() else {
            return Ok(None);
        };
        let Some(time) = self.recipes().cook(KIND, &held).map(|r| r.time as i32) else {
            return Ok(None);
        };
        if self.remote.is_some() {
            return Ok(Some(true));
        }
        let key = (pos[0], pos[1], pos[2]);
        let placed = match self.world.block_entities.get_mut(&key) {
            Some(BlockEntity::Campfire(c)) => {
                let mut one = held.clone();
                c.place_food(&mut one, time)
            }
            _ => false,
        };
        if placed {
            self.consume_held();
            self.stats.custom("interact_with_campfire", 1);
            self.post_block_game_event(
                "minecraft:block_change",
                pos,
                Some(Who::Player),
                Some(state),
            );
            self.mark_block_entity_updated(key);
        }
        Ok(Some(placed))
    }

    pub(super) fn tick_campfire(&mut self, pos: Pos) -> Result<()> {
        let state = self.world.block(pos.0, pos.1, pos.2);
        let lit = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("lit"))
            .is_some_and(|v| v == "true");
        let Some(BlockEntity::Campfire(c)) = self.world.block_entities.get_mut(&pos) else {
            return Ok(());
        };
        let changed = if lit {
            c.items.iter().any(Option::is_some)
        } else {
            c.cooldown_tick()
        };
        if changed {
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
        }
        if !lit {
            return Ok(());
        }
        let Some(BlockEntity::Campfire(c)) = self.world.block_entities.get_mut(&pos) else {
            return Ok(());
        };
        let done = c.cook_tick();
        for (_, item) in done {
            let result = self
                .res
                .recipes()
                .cook(KIND, &item)
                .map_or_else(|| item.clone(), |r| r.result.clone());
            self.campfire_drop(pos, result);
            self.mark_block_entity_updated(pos);
            self.post_block_game_event(
                "minecraft:block_change",
                [pos.0, pos.1, pos.2],
                None,
                Some(state),
            );
        }
        Ok(())
    }

    pub(super) fn campfire_geometry(&self, camera: glam::DVec3, out: &mut Vec<Vertex>) {
        for (key, entity) in &self.world.block_entities {
            let BlockEntity::Campfire(c) = entity else {
                continue;
            };
            if c.items.iter().all(Option::is_none) {
                continue;
            }
            let at = glam::DVec3::new(f64::from(key.0), f64::from(key.1), f64::from(key.2));
            if at.distance_squared(camera) >= 64.0 * 64.0 {
                continue;
            }
            let state = self.world.block(key.0, key.1, key.2);
            let facing = match self
                .pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get("facing"))
                .map(String::as_str)
            {
                Some("west") => 1,
                Some("north") => 2,
                Some("east") => 3,
                _ => 0,
            };
            let light = vertex_light(self.world.light(key.0, key.1, key.2));
            let packed = BlockPos::new(key.0, key.1, key.2).pack();
            for (i, stack) in c.items.iter().enumerate() {
                let Some(stack) = stack else {
                    continue;
                };
                let seed = (packed as i32).wrapping_add(i as i32);
                let item = self.resolve_item(stack, DisplayContext::Fixed, None, seed);
                if item.is_empty() {
                    continue;
                }
                let yaw = -(((i + facing) % 4) as f32 * 90.0);
                let pose = glam::Mat4::from_translation(
                    (at - camera).as_vec3() + glam::Vec3::new(0.5, 0.449_218_75, 0.5),
                ) * glam::Mat4::from_rotation_y(yaw.to_radians())
                    * glam::Mat4::from_rotation_x(90f32.to_radians())
                    * glam::Mat4::from_translation(glam::Vec3::new(-0.3125, -0.3125, 0.0))
                    * glam::Mat4::from_scale(glam::Vec3::splat(0.375));
                self.submit_item_world(&item, pose, light, out);
            }
        }
    }

    fn campfire_drop(&mut self, pos: Pos, stack: ItemStack) {
        let at = BlockPos::new(pos.0, pos.1, pos.2);
        for piece in
            vise_world::level::removal::drop_item_stack(&mut self.world.random, at, stack.count)
        {
            self.spawn_item(
                ItemStack {
                    count: piece.count,
                    ..stack.clone()
                },
                piece.pos,
                piece.motion,
                0,
            );
        }
    }
}
