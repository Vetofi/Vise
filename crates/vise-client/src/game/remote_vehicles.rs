use super::{Game, Keys, ray_box};
use vise_client::mobs::{Mob, equine};
use vise_core::geometry::{Aabb, Vec3};
use vise_protocol::packets::play::{self, Serverbound as S};

pub(super) struct ObjectHit {
    pub id: i32,
    pub distance: f64,
    pub location: [f64; 3],
}

impl Game<'_> {
    pub(super) fn remote_object_hit(&self, attacking: bool) -> Option<ObjectHit> {
        let (eye, dir) = self.aim()?;
        let at = |d: f64| {
            [
                eye[0] + dir[0] * d,
                eye[1] + dir[1] * d,
                eye[2] + dir[2] * d,
            ]
        };
        let block_centre = |p: [i32; 3]| {
            [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            ]
        };
        let mut hits: Vec<(i32, f64, [f64; 3])> = Vec::new();
        if let Some((i, d)) = self.painting_hit() {
            let p = &self.paintings[i];
            hits.push((p.id as i32, d, block_centre(p.pos)));
        }
        if let Some((i, d)) = self.frame_hit() {
            let f = &self.item_frames[i];
            hits.push((f.id as i32, d, block_centre(f.pos)));
        }
        if let Some((i, d, _)) = self.armor_stand_hit() {
            let a = &self.armor_stands[i];
            let p = a.position;
            hits.push((a.id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d, _)) = self.cushion_hit() {
            let c = &self.cushions[i];
            let p = c.position;
            hits.push((c.id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d)) = self.boat_hit() {
            let p = self.boats[i].position;
            hits.push((self.boats[i].id as i32, d, [p.x, p.y, p.z]));
        }
        if let Some((i, d)) = self.cart_hit() {
            let p = self.carts[i].position;
            hits.push((self.carts[i].id as i32, d, [p.x, p.y, p.z]));
        }
        if attacking {
            let reach = if self.creative { 5.0 } else { 3.0 };
            for c in self.crystals.iter().filter(|c| c.id != 0) {
                let p = c.position;
                let bounds = Aabb::new(
                    Vec3::new(p.x - 1.0, p.y, p.z - 1.0),
                    Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0),
                );
                if let Some(d) = ray_box(eye, dir, reach, bounds) {
                    hits.push((c.id as i32, d, [p.x, p.y, p.z]));
                }
            }
            hits.extend(self.remote_dragon_part_hit(eye, dir, reach));
        }
        let (id, distance, origin) = hits.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
        let point = at(distance);
        Some(ObjectHit {
            id,
            distance,
            location: [
                point[0] - origin[0],
                point[1] - origin[1],
                point[2] - origin[2],
            ],
        })
    }

    pub(super) fn remote_set_passengers(&mut self, vehicle: i32, passengers: &[i32]) {
        let Some(me) = self.remote.as_ref().map(|r| r.entity_id) else {
            return;
        };
        let vehicle_id = vehicle as u32;
        for m in &mut self.mobs {
            if m.id == vehicle_id {
                m.extra.rider = passengers.first().map(|&p| p as u32);
            }
            if m.extra.mount == Some(vehicle_id) && !passengers.contains(&(m.id as i32)) {
                m.extra.mount = None;
                m.extra.passenger = false;
            } else if passengers.contains(&(m.id as i32)) {
                m.extra.mount = Some(vehicle_id);
                m.extra.passenger = true;
            }
        }
        for (id, display) in &mut self.text_displays {
            if passengers.contains(id) {
                display.vehicle = Some(vehicle_id);
            } else if display.vehicle == Some(vehicle_id) {
                display.vehicle = None;
            }
        }
        if passengers.contains(&me)
            && let Some(r) = self.remote.as_mut()
        {
            r.first_passenger = passengers.first() == Some(&me);
        }
        if passengers.contains(&me) {
            if self.riding != Some(vehicle_id) {
                self.riding = Some(vehicle_id);
                if let Some(p) = self.player.as_mut() {
                    p.velocity = Vec3::ZERO;
                    p.fall_distance = 0.0;
                }
            }
        } else if self.riding == Some(vehicle_id) {
            self.riding = None;
            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == vehicle_id) {
                m.rider = None;
            }
            if let Some(m) = self.mobs.iter_mut().find(|m| {
                m.id == vehicle_id && m.species() == vise_client::mobs::Species::HappyGhast
            }) {
                m.extra.counter = 10;
                if let Some(r) = self.remote.as_mut() {
                    r.ghast_dismount = Some(vehicle_id);
                }
            }
        }
    }

    pub(super) fn remote_ride(&mut self, keys: Keys) -> bool {
        let Some(id) = self.riding else {
            return false;
        };
        if let Some(i) = self.mobs.iter().position(|m| m.id == id) {
            let jump = self.charge_mount_jump(i, keys);
            if self.drives(&self.mobs[i]) {
                self.drive_mount(i, keys, jump);
            }
            self.position_rider();
            return true;
        }
        let seat = if let Some(c) = self.cushions.iter().find(|c| c.id == id) {
            Some(c.seat())
        } else if let Some(i) = self.boats.iter().position(|b| b.id == id) {
            let mut b = self.tick_boat(self.boats[i].clone(), true);
            b.input = [keys.left, keys.right, keys.forward, keys.backward];
            self.yaw += b.delta_rotation;
            let delta = (self.yaw - b.yaw + 180.0).rem_euclid(360.0) - 180.0;
            self.yaw += delta.clamp(-105.0, 105.0) - delta;
            self.boats[i] = b;
            Some(self.player_boat_seat(i))
        } else {
            self.carts.iter().find(|c| c.id == id).map(|c| {
                c.position
                    .add_vector(Vec3::new(0.0, super::minecarts::SEAT, 0.0))
            })
        };
        if let Some(p) = self.player.as_mut() {
            self.previous_position = p.position;
            if let Some(v) = seat {
                p.position = v;
            }
            p.velocity = Vec3::ZERO;
            p.fall_distance = 0.0;
        }
        true
    }

    pub(super) fn remote_vehicle_packets(&self) -> Vec<S> {
        if let Some(m) = self.driven_mount() {
            return vec![mount_move(m)];
        }
        let Some(b) = self
            .riding
            .and_then(|id| self.boats.iter().find(|b| b.id == id))
        else {
            return Vec::new();
        };
        vec![
            S::MoveVehicle(play::MoveVehicleRequest {
                position: [b.position.x, b.position.y, b.position.z],
                yaw: b.yaw,
                pitch: 0.0,
                on_ground: b.on_ground,
            }),
            S::PaddleBoat(play::PaddleBoat {
                left: b.paddling[0],
                right: b.paddling[1],
            }),
        ]
    }

    pub(super) fn remote_move_vehicle(&mut self, position: [f64; 3], yaw: f32) {
        let at = Vec3::new(position[0], position[1], position[2]);
        if let Some(id) = self.driven_mount().map(|m| m.id)
            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
        {
            m.previous = at;
            m.position = at;
            m.yaw = yaw;
            m.previous_yaw = yaw;
            m.velocity = Vec3::ZERO;
            return;
        }
        let Some(b) = self
            .riding
            .and_then(|id| self.boats.iter_mut().find(|b| b.id == id))
        else {
            return;
        };
        b.previous = at;
        b.position = at;
        b.yaw = yaw;
        b.previous_yaw = yaw;
        b.velocity = Vec3::ZERO;
    }

    fn drives(&self, m: &Mob) -> bool {
        self.riding == Some(m.id)
            && client_moves(m, self.remote.as_ref().is_some_and(|r| r.first_passenger))
    }

    pub(super) fn driven_mount(&self) -> Option<&Mob> {
        let id = self.riding?;
        self.mobs.iter().find(|m| m.id == id && self.drives(m))
    }

    fn drive_mount(&mut self, i: usize, keys: Keys, jump: f32) {
        if jump > 0.0
            && let Some(id) = self.remote.as_ref().map(|r| r.entity_id)
        {
            self.send(riding_jump(id, self.mount.jump_scale));
        }
        let strafe = f32::from(u8::from(keys.left)) - f32::from(u8::from(keys.right));
        let forward = f32::from(u8::from(keys.forward)) - f32::from(u8::from(keys.backward));
        let m = &mut self.mobs[i];
        if jump > 0.0 && equine::is_equine(m.species()) {
            equine::on_player_jump(m);
        }
        let pending = m.rider.map_or(0.0, |r| r.jump);
        m.rider = Some(equine::Rider {
            strafe,
            forward,
            yaw: self.yaw,
            pitch: self.pitch,
            jump: if jump > 0.0 { jump } else { pending },
            steer: false,
            sprint: keys.sprint,
            jumping: keys.jump,
            client: false,
        });
        let mut mob = self.mobs.remove(i);
        let mut random = self.world.random.clone();
        let mut events = Vec::new();
        {
            let world = self.mob_world_base(&[]);
            mob.tick(&world, &mut random, &mut events);
        }
        mob.removed = false;
        self.world.random = random;
        self.mobs.insert(i, mob);
    }
}

fn client_moves(m: &Mob, first_passenger: bool) -> bool {
    first_passenger && equine::moved_by_rider(m.species()) && m.extra.saddled && m.alive()
}

fn riding_jump(player: i32, scale: f32) -> S {
    S::PlayerCommand(play::PlayerCommand {
        id: player,
        action: 3,
        data: (scale * 100.0).floor() as i32,
    })
}

fn mount_move(m: &Mob) -> S {
    S::MoveVehicle(play::MoveVehicleRequest {
        position: [m.position.x, m.position.y, m.position.z],
        yaw: m.yaw,
        pitch: m.head_pitch,
        on_ground: m.on_ground,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vise_client::mobs::MobTypes;

    fn mob(kind: &str) -> Option<Mob> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        let types = MobTypes::load(&root).ok()?;
        let t = std::sync::Arc::new(types.types.get(kind)?.clone());
        Some(Mob::new(7, t, Vec3::new(1.5, 64.0, -2.5), 0.0))
    }

    #[test]
    fn the_rider_moves_a_saddled_horse_on_its_client() {
        for kind in ["horse", "donkey", "mule", "camel", "pig", "llama"] {
            let Some(mut m) = mob(&format!("minecraft:{kind}")) else {
                return;
            };
            assert!(!client_moves(&m, true), "{kind} without a saddle");
            m.set_saddle(Some(vise_client::items::ItemStack::new(
                "minecraft:saddle",
                1,
            )));
            let horse = matches!(kind, "horse" | "donkey" | "mule" | "camel");
            assert_eq!(client_moves(&m, true), horse, "{kind}");
            assert!(!client_moves(&m, false), "{kind} behind another rider");
        }
    }

    #[test]
    fn the_rider_sends_its_mounts_moves_and_jumps() {
        let Some(mut m) = mob("minecraft:horse") else {
            return;
        };
        (m.yaw, m.head_pitch, m.on_ground) = (90.0, 10.0, true);
        match mount_move(&m) {
            S::MoveVehicle(p) => {
                assert_eq!(p.position, [1.5, 64.0, -2.5]);
                assert_eq!((p.yaw, p.pitch, p.on_ground), (90.0, 10.0, true));
            }
            other => panic!("{other:?}"),
        }
        match riding_jump(42, 0.75) {
            S::PlayerCommand(p) => assert_eq!((p.id, p.action, p.data), (42, 3, 75)),
            other => panic!("{other:?}"),
        }
    }
}
