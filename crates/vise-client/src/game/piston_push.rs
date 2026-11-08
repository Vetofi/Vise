use super::Game;
use vise_client::collisions::{EntityCollisions, SceneCollisions, with_piston_noclip};
use vise_core::geometry::{Aabb, Direction, Vec3, border::BorderGeometry};
use vise_core::position::BlockPos;
use vise_world::{
    block_collision_scan::BlockCollisionWorld, level::pistons::MovingPiston,
    piston_movement::PistonMovement, piston_push, world_movement::EntityMovementWorld,
};

pub(super) type Push = (BlockPos, MovingPiston, f32);

#[derive(Clone, Copy, Hash, PartialEq, Eq)]
enum Target {
    Player,
    Guest(u32),
    Mob(usize),
    Item(usize),
    Boat(usize),
    Cart(usize),
    ArmorStand(usize),
}

struct StepHeights {
    player: f32,
    guests: std::collections::HashMap<u32, f32>,
}

#[derive(Clone, Copy)]
struct Body {
    bounds: Aabb,
    position: Vec3,
    on_ground: bool,
    step_height: f32,
    supported: bool,
}

impl Game<'_> {
    pub(super) fn push_piston_entities(&mut self, pushes: Vec<Push>) {
        let mut accumulators = std::collections::HashMap::new();
        for push in pushes {
            self.push_for_piston(push, &mut accumulators);
        }
    }

    fn push_for_piston(
        &mut self,
        (pos, piston, new_progress): Push,
        accumulators: &mut std::collections::HashMap<Target, PistonMovement>,
    ) {
        let delta_progress = f64::from(new_progress - piston.progress);
        if delta_progress <= 0.0 {
            return;
        }
        let Some(bindings) = self.collision.pistons() else {
            return;
        };
        let Ok(state) = bindings.collision_related_state(piston.shape_state()) else {
            return;
        };
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let Ok(shape) = self
            .collision
            .collision_shape(&mut scene, state, [pos.x, pos.y, pos.z])
        else {
            return;
        };
        if shape.is_empty() {
            return;
        }
        let movement = piston.movement_direction();
        let boxes: Vec<Aabb> = shape.boxes().to_vec();
        let facts = self.res.facts();
        let slime = &*facts.get(piston.moved_state).name == "minecraft:slime_block";
        let honey = &*facts.get(piston.moved_state).name == "minecraft:honey_block";
        let sticky_top = if honey {
            let mut scene = SceneCollisions {
                world: &self.world,
                binding: self.collision,
                subject: None,
            };
            self.collision
                .collision_shape(&mut scene, piston.moved_state, [pos.x, pos.y, pos.z])
                .ok()
                .and_then(|s| s.bounds())
                .map_or(0.0, |b| b.max_position().y)
        } else {
            0.0
        };

        let steps = self.push_step_heights();
        let search = shape.bounds().map(|bounds| {
            piston_push::movement_search_area(
                piston_push::move_by_position_and_progress(pos, bounds, piston.shape_state()),
                movement,
                delta_progress,
            )
        });
        let targets = self.push_targets();
        for &target in &targets {
            let Some(body) = self.push_body(target, pos, &steps) else {
                continue;
            };
            let amount = piston_push::collided_amount(
                &boxes,
                pos,
                piston.shape_state(),
                new_progress,
                movement,
                body.bounds,
            );
            if slime && search.is_some_and(|area| area.intersects(body.bounds)) {
                self.bounce(target, movement);
            }
            if amount > 0.0 {
                let step = piston_push::step_vector(movement);
                let before = body.bounds;
                self.move_pushed(
                    target,
                    before,
                    [body.position.x, body.position.z],
                    body.step_height,
                    body.on_ground,
                    step.scale(amount),
                    movement,
                    accumulators,
                );
                if !piston.extending
                    && piston.source
                    && let Some(after) = self.push_body(target, pos, &steps)
                    && let Some(fix) = piston_push::fix_within_piston_base(
                        pos,
                        after.bounds,
                        movement,
                        delta_progress,
                    )
                {
                    let back = piston_push::step_vector(movement.opposite());
                    self.move_pushed(
                        target,
                        after.bounds,
                        [after.position.x, after.position.z],
                        after.step_height,
                        after.on_ground,
                        back.scale(fix),
                        movement,
                        accumulators,
                    );
                }
            }
        }
        if honey && movement.normal()[1] == 0 {
            let area = piston_push::sticky_area(pos, piston.shape_state(), sticky_top);
            for target in targets {
                let Some(body) = self.push_body(target, pos, &steps) else {
                    continue;
                };
                if piston_push::sticky_entity_selected(
                    area,
                    body.bounds,
                    vise_world::level::data::PushReaction::PushPull,
                    body.on_ground,
                    body.supported,
                    body.position,
                ) {
                    self.move_pushed(
                        target,
                        body.bounds,
                        [body.position.x, body.position.z],
                        body.step_height,
                        body.on_ground,
                        piston_push::step_vector(movement).scale(delta_progress),
                        movement,
                        accumulators,
                    );
                }
            }
        }
    }

    fn push_step_heights(&mut self) -> StepHeights {
        let player = self
            .player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:step_height").ok())
            .map_or(0.6, |v| v as f32);
        let mut guests = std::collections::HashMap::new();
        if let Some(host) = self.lan.as_mut() {
            for g in &mut host.hub.guests {
                let step = g
                    .motion
                    .as_mut()
                    .and_then(|m| m.attribute("minecraft:step_height").ok())
                    .map_or(0.6, |v| v as f32);
                guests.insert(g.conn, step);
            }
        }
        StepHeights { player, guests }
    }

    fn push_targets(&self) -> Vec<Target> {
        let mut targets = Vec::new();
        if self.player.is_some() {
            targets.push(Target::Player);
        }
        if let Some(host) = self.lan.as_ref() {
            targets.extend(
                host.hub
                    .guests
                    .iter()
                    .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
                    .map(|g| Target::Guest(g.conn)),
            );
        }
        targets.extend((0..self.mobs.len()).map(Target::Mob));
        targets.extend((0..self.dropped.len()).map(Target::Item));
        targets.extend((0..self.boats.len()).map(Target::Boat));
        targets.extend((0..self.carts.len()).map(Target::Cart));
        targets.extend((0..self.armor_stands.len()).map(Target::ArmorStand));
        targets
    }

    fn push_body(&self, target: Target, pos: BlockPos, steps: &StepHeights) -> Option<Body> {
        let at = [pos.x, pos.y, pos.z];
        match target {
            Target::Player => {
                let p = self.player.as_ref()?;
                if p.spectator {
                    return None;
                }
                Some(Body {
                    bounds: p.bounds(),
                    position: p.position,
                    on_ground: p.ground.on_ground,
                    step_height: steps.player,
                    supported: p.ground.supporting_block == Some(pos),
                })
            }
            Target::Guest(conn) => {
                let host = self.lan.as_ref()?;
                let g = host.hub.guests.iter().find(|g| g.conn == conn)?;
                if g.dead || g.game_mode == 3 {
                    return None;
                }
                let p = g.position;
                Some(Body {
                    bounds: Aabb::new(
                        Vec3::new(p[0] - 0.3, p[1], p[2] - 0.3),
                        Vec3::new(p[0] + 0.3, p[1] + 1.8, p[2] + 0.3),
                    ),
                    position: Vec3::new(p[0], p[1], p[2]),
                    on_ground: g.on_ground,
                    step_height: steps.guests.get(&conn).copied().unwrap_or(0.6),
                    supported: g
                        .motion
                        .as_ref()
                        .is_some_and(|m| m.ground.supporting_block == Some(pos)),
                })
            }
            Target::Mob(i) => {
                let m = &self.mobs[i];
                if m.removed || !m.alive() {
                    return None;
                }
                Some(Body {
                    bounds: m.bounds(),
                    position: m.position,
                    on_ground: m.on_ground,
                    step_height: m.kind.step_height as f32,
                    supported: m.supporting == Some(at),
                })
            }
            Target::Item(i) => {
                let entity = &self.dropped[i];
                Some(Body {
                    bounds: entity.bounds(),
                    position: entity.position,
                    on_ground: entity.on_ground,
                    step_height: 0.0,
                    supported: false,
                })
            }
            Target::Boat(i) => {
                let entity = &self.boats[i];
                Some(Body {
                    bounds: entity.bounds(),
                    position: entity.position,
                    on_ground: entity.on_ground,
                    step_height: 0.0,
                    supported: false,
                })
            }
            Target::Cart(i) => {
                let entity = &self.carts[i];
                Some(Body {
                    bounds: entity.bounds(),
                    position: entity.position,
                    on_ground: entity.on_ground,
                    step_height: 0.0,
                    supported: false,
                })
            }
            Target::ArmorStand(i) => {
                let entity = &self.armor_stands[i];
                if entity.marker {
                    return None;
                }
                Some(Body {
                    bounds: entity.bounds(),
                    position: entity.position,
                    on_ground: entity.on_ground,
                    step_height: 0.0,
                    supported: false,
                })
            }
        }
    }

    fn move_pushed(
        &mut self,
        target: Target,
        before: Aabb,
        at: [f64; 2],
        step_height: f32,
        on_ground: bool,
        movement: Vec3,
        piston_direction: Direction,
        accumulators: &mut std::collections::HashMap<Target, PistonMovement>,
    ) {
        let Some(limited) = self.limit_piston(target, movement, accumulators) else {
            return;
        };
        if limited == Vec3::ZERO {
            return;
        }
        let resolved = with_piston_noclip(piston_direction, || {
            pushed_movement(
                SceneCollisions {
                    world: &self.world,
                    binding: self.collision,
                    subject: None,
                },
                self.border.geometry(),
                at,
                before,
                limited,
                step_height,
                on_ground,
            )
            .unwrap_or(Vec3::ZERO)
        });
        if resolved.length_squared() <= 1.0e-7
            && limited.length_squared() - resolved.length_squared() >= 1.0e-7
        {
            return;
        }
        self.apply_push(target, resolved);
    }

    fn limit_piston(
        &mut self,
        target: Target,
        movement: Vec3,
        accumulators: &mut std::collections::HashMap<Target, PistonMovement>,
    ) -> Option<Vec3> {
        let time = self.world.game_time;
        let limit = |state: &mut PistonMovement| state.limit(movement, || Ok::<i64, ()>(time)).ok();
        match target {
            Target::Player => {
                let p = self.player.as_mut()?;
                limit(&mut p.piston)
            }
            Target::Guest(conn) => {
                let host = self.lan.as_mut()?;
                let g = host.hub.guests.iter_mut().find(|g| g.conn == conn)?;
                limit(&mut g.motion.as_deref_mut()?.piston)
            }
            Target::Mob(i) => limit(&mut self.mobs[i].piston),
            _ => limit(accumulators.entry(target).or_default()),
        }
    }

    fn apply_push(&mut self, target: Target, movement: Vec3) {
        match target {
            Target::Player => {
                if let Some(p) = self.player.as_mut() {
                    p.position = p.position.add_vector(movement);
                }
            }
            Target::Guest(conn) => {
                if let Some(host) = self.lan.as_mut()
                    && let Some(g) = host.hub.guests.iter_mut().find(|g| g.conn == conn)
                {
                    for (slot, delta) in g.position.iter_mut().zip(movement.components()) {
                        *slot += delta;
                    }
                    if let Some(m) = g.motion.as_deref_mut() {
                        m.position = m.position.add_vector(movement);
                    }
                }
            }
            Target::Mob(i) => {
                let m = &mut self.mobs[i];
                m.position = m.position.add_vector(movement);
            }
            Target::Item(i) => {
                self.dropped[i].position = self.dropped[i].position.add_vector(movement)
            }
            Target::Boat(i) => self.boats[i].position = self.boats[i].position.add_vector(movement),
            Target::Cart(i) => self.carts[i].position = self.carts[i].position.add_vector(movement),
            Target::ArmorStand(i) => {
                self.armor_stands[i].position = self.armor_stands[i].position.add_vector(movement)
            }
        }
    }

    fn bounce(&mut self, target: Target, movement: Direction) {
        match target {
            Target::Player => {
                if let Some(p) = self.player.as_mut() {
                    p.velocity = piston_push::bounce_velocity(movement, p.velocity);
                }
            }
            Target::Mob(i) => {
                let m = &mut self.mobs[i];
                m.velocity = piston_push::bounce_velocity(movement, m.velocity);
            }
            Target::Guest(conn) => {
                if let Some(host) = self.lan.as_mut()
                    && let Some(guest) = host.hub.guests.iter_mut().find(|g| g.conn == conn)
                    && let Some(motion) = guest.motion.as_deref_mut()
                {
                    motion.velocity = piston_push::bounce_velocity(movement, motion.velocity);
                }
            }
            Target::Item(i) => {
                self.dropped[i].velocity =
                    piston_push::bounce_velocity(movement, self.dropped[i].velocity)
            }
            Target::Boat(i) => {
                self.boats[i].velocity =
                    piston_push::bounce_velocity(movement, self.boats[i].velocity)
            }
            Target::Cart(i) => {
                self.carts[i].velocity =
                    piston_push::bounce_velocity(movement, self.carts[i].velocity)
            }
            Target::ArmorStand(i) => {
                self.armor_stands[i].velocity =
                    piston_push::bounce_velocity(movement, self.armor_stands[i].velocity)
            }
        }
    }
}

fn pushed_movement<W>(
    blocks: W,
    border: &BorderGeometry,
    at: [f64; 2],
    before: Aabb,
    movement: Vec3,
    step_height: f32,
    on_ground: bool,
) -> vise_client::Result<Vec3>
where
    W: BlockCollisionWorld<Error = std::io::Error> + EntityMovementWorld<Error = std::io::Error>,
{
    EntityCollisions::with_border(blocks, border, at).resolve(
        before,
        movement,
        step_height,
        on_ground,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use vise_client::collisions::EmptyCollisions;

    fn border() -> BorderGeometry {
        BorderGeometry::new([0.0, 0.0], 10.0, 29_999_984).expect("a border")
    }

    #[test]
    fn a_pushed_body_stops_at_the_world_border() {
        let before = Aabb::new(Vec3::new(4.0, 0.0, 0.0), Vec3::new(4.6, 1.8, 0.6));
        let border = border();
        let moved = pushed_movement(
            EmptyCollisions,
            &border,
            [4.3, 0.3],
            before,
            Vec3::new(0.51, 0.0, 0.0),
            0.0,
            false,
        )
        .expect("the push resolves");
        assert!(
            (moved.x - 0.4).abs() < 1.0e-9,
            "the body was not stopped by the border: {moved:?}"
        );
    }

    #[test]
    fn a_pushed_body_moves_whole_away_from_the_border() {
        let before = Aabb::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.6, 1.8, 0.6));
        let border = border();
        let moved = pushed_movement(
            EmptyCollisions,
            &border,
            [0.3, 0.3],
            before,
            Vec3::new(0.51, 0.0, 0.0),
            0.0,
            false,
        )
        .expect("the push resolves");
        assert!(
            (moved.x - 0.51).abs() < 1.0e-9,
            "the body did not move the whole push: {moved:?}"
        );
    }
}
