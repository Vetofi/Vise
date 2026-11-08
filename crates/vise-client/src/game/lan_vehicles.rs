use super::{Game, Keys, lan::LanHost, minecarts::SEAT};
use vise_client::collisions::SceneCollisions;
use vise_client::mobs::equine;
use vise_core::geometry::{Aabb, Vec3};
use vise_protocol::{
    packets::play::{self, Clientbound as C},
    text::translatable,
};

pub(super) fn input_keys(flags: u8) -> Keys {
    Keys {
        forward: flags & 1 != 0,
        backward: flags & 2 != 0,
        left: flags & 4 != 0,
        right: flags & 8 != 0,
        jump: flags & 16 != 0,
        shift: flags & 32 != 0,
        sprint: flags & 64 != 0,
        ..Keys::default()
    }
}

fn corners(b: Aabb) -> ([f64; 3], [f64; 3]) {
    let (lo, hi) = (b.min_position(), b.max_position());
    ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
}

impl Game<'_> {
    pub(super) fn object_target(&self, id: i32) -> Option<([f64; 3], [f64; 3], [f64; 3])> {
        let id = u32::try_from(id).ok()?;
        let at = |v: Vec3| [v.x, v.y, v.z];
        let centre = |p: [i32; 3]| p.map(|v| f64::from(v) + 0.5);
        if let Some(b) = self.boats.iter().find(|b| b.id == id) {
            let (lo, hi) = corners(b.bounds());
            return Some((lo, hi, at(b.position)));
        }
        if let Some(c) = self.carts.iter().find(|c| c.id == id) {
            let (lo, hi) = corners(c.bounds());
            return Some((lo, hi, at(c.position)));
        }
        if let Some(a) = self.armor_stands.iter().find(|a| a.id == id) {
            let (lo, hi) = corners(a.bounds());
            return Some((lo, hi, at(a.position)));
        }
        if let Some(c) = self.cushions.iter().find(|c| c.id == id) {
            let (lo, hi) = corners(Aabb::new(
                Vec3::new(c.position.x - 0.5, c.position.y, c.position.z - 0.5),
                Vec3::new(c.position.x + 0.5, c.position.y + 0.25, c.position.z + 0.5),
            ));
            return Some((lo, hi, at(c.position)));
        }
        if let Some(f) = self.item_frames.iter().find(|f| f.id == id) {
            let (lo, hi) = f.bounds();
            return Some((lo, hi, centre(f.pos)));
        }
        let p = self.paintings.iter().find(|p| p.id == id)?;
        let (lo, hi) = p.bounds();
        Some((lo, hi, centre(p.pos)))
    }

    pub(super) fn guest_vehicles(&self) -> Vec<u32> {
        self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub.guests.iter().filter_map(|g| g.vehicle).collect()
        })
    }

    pub(super) fn ridden_by_guest(&self, vehicle: u32) -> bool {
        self.lan.as_ref().is_some_and(|h| {
            h.hub
                .guests
                .iter()
                .any(|g| Some(g.conn) != self.lan_acting && g.vehicle == Some(vehicle))
        })
    }
    pub(super) fn guest_rides(&self, vehicle: u32) -> bool {
        self.lan
            .as_ref()
            .is_some_and(|h| h.hub.guests.iter().any(|g| g.vehicle == Some(vehicle)))
    }

    pub(super) fn guest_vehicle_tick(&mut self, host: &mut LanHost, conn: u32) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let g = &host.hub.guests[gi];
        let Some(vehicle) = g.vehicle else {
            return;
        };
        let (keys, dead) = (input_keys(g.input), g.dead || g.health <= 0.0);
        let boat = self.boats.iter().position(|b| b.id == vehicle);
        let cart = self.carts.iter().any(|c| c.id == vehicle);
        let mount = self.mobs.iter().any(|m| m.id == vehicle);
        let cushion = self.cushions.iter().any(|c| c.id == vehicle);
        if dead || (boat.is_none() && !cart && !mount && !cushion) {
            let g = &mut host.hub.guests[gi];
            (g.vehicle, g.vehicle_good) = (None, None);
            return;
        }
        if let Some(i) = boat {
            let b = &mut self.boats[i];
            b.hurt_time = (b.hurt_time - 1).max(0);
            b.damage = (b.damage - 1.0).max(0.0);
        }
        let seat = self.as_guest(host, conn, |game| {
            if game.cushions.iter().any(|c| Some(c.id) == game.riding) {
                if keys.shift {
                    game.tick_cushion_riding(keys);
                    return None;
                }
                let c = game.cushions.iter().find(|c| Some(c.id) == game.riding)?;
                let p = c.seat();
                return Some([p.x, p.y, p.z]);
            }
            if let Some(i) = game.boats.iter().position(|b| Some(b.id) == game.riding) {
                if keys.shift {
                    game.dismount(i);
                    return None;
                }
                let p = game.player_boat_seat(i);
                return Some([p.x, p.y, p.z]);
            }
            if !game.tick_cart_riding(keys) && game.tick_mount_riding(keys) {
                game.riding?;
                game.position_rider();
            }
            game.riding?;
            let p = game.player.as_ref()?.position;
            Some([p.x, p.y, p.z])
        });
        if let Some(Some(at)) = seat {
            let g = &mut host.hub.guests[gi];
            g.previous_position = g.position;
            g.position = at;
        }
    }

    pub(super) fn guest_boarded(&mut self, host: &mut LanHost, conn: u32, before: Option<u32>) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let Some(vehicle) = host.hub.guests[gi].vehicle else {
            return;
        };
        if before == Some(vehicle) {
            return;
        }
        if self.riding == Some(vehicle) {
            host.hub.guests[gi].vehicle = before;
            return;
        }
        let at = self
            .boats
            .iter()
            .position(|b| b.id == vehicle)
            .map(|i| (self.boats[i].position, self.player_boat_seat(i)))
            .or_else(|| {
                self.carts
                    .iter()
                    .find(|c| c.id == vehicle)
                    .map(|c| (c.position, c.position.add_vector(Vec3::new(0.0, SEAT, 0.0))))
            })
            .or_else(|| {
                self.cushions
                    .iter()
                    .find(|c| c.id == vehicle)
                    .map(|c| (c.position, c.seat()))
            });
        if let Some((p, seat)) = at {
            let g = &mut host.hub.guests[gi];
            g.vehicle_good = Some([p.x, p.y, p.z]);
            g.previous_position = g.position;
            g.position = [seat.x, seat.y, seat.z];
            g.on_ground = false;
            return;
        }
        let seat = self.as_guest(host, conn, |game| {
            game.position_rider();
            game.player
                .as_ref()
                .map(|p| [p.position.x, p.position.y, p.position.z])
        });
        if let Some(Some(at)) = seat {
            let g = &mut host.hub.guests[gi];
            g.previous_position = g.position;
            g.position = at;
            g.on_ground = false;
        }
    }

    pub(super) fn guest_move_vehicle(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        p: &play::MoveVehicleRequest,
    ) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let [x, y, z] = p.position;
        if !x.is_finite()
            || !y.is_finite()
            || !z.is_finite()
            || !p.yaw.is_finite()
            || !p.pitch.is_finite()
        {
            host.hub.kick(
                conn,
                translatable("multiplayer.disconnect.invalid_vehicle_movement", &[]),
            );
            return;
        }
        let g = &host.hub.guests[gi];
        if g.awaiting_teleport.is_some() || !g.loaded {
            return;
        }
        let owner = host.hub.singleplayer_owner == Some(g.uuid);
        let (vehicle, good) = (g.vehicle, g.vehicle_good);
        let back = |host: &mut LanHost, position: Vec3, yaw: f32| {
            let packet = play::MoveVehicle {
                position: [position.x, position.y, position.z],
                yaw,
                pitch: 0.0,
            };
            host.hub.send_to(conn, &C::MoveVehicle(packet));
        };
        let boat = vehicle.and_then(|v| self.boats.iter().position(|b| b.id == v));
        let mount = vehicle.and_then(|v| {
            self.mobs
                .iter()
                .position(|m| m.id == v && equine::moved_by_client(m))
        });
        let (id, old, yaw, velocity, bounds, step, on_ground) = if let Some(i) = boat {
            let b = &self.boats[i];
            (
                b.id,
                b.position,
                b.yaw,
                b.velocity,
                b.bounds(),
                0.0,
                b.on_ground,
            )
        } else if let Some(i) = mount {
            let m = &self.mobs[i];
            let step = m.kind.step_height as f32;
            (
                m.id,
                m.position,
                m.yaw,
                m.velocity,
                m.bounds(),
                step,
                m.on_ground,
            )
        } else {
            if let Some(c) = vehicle.and_then(|v| self.carts.iter().find(|c| c.id == v))
                && c.position.distance_squared(Vec3::new(x, y, z)) > 1.0
            {
                back(host, c.position, c.yaw);
            }
            return;
        };
        let good = good.map_or(old, |g| Vec3::new(g[0], g[1], g[2]));
        let target = Vec3::new(
            x.clamp(-3.0E7, 3.0E7),
            y.clamp(-2.0E7, 2.0E7),
            z.clamp(-3.0E7, 3.0E7),
        );
        let moved = target.distance_squared(old) - velocity.length_squared();
        if moved > 100.0 && !owner {
            log::warn!("vehicle {id} moved too quickly! {moved}");
            back(host, old, yaw);
            return;
        }
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let wanted = target.subtract(good);
        let resolved = scene
            .resolve(bounds, wanted, step, on_ground)
            .unwrap_or(wanted);
        let reached = old.add_vector(resolved);
        let wrongly = (target.x - reached.x).powi(2) + (target.z - reached.z).powi(2) > 0.0625;
        let free_before = scene.unobstructed(bounds).unwrap_or(true);
        let moved_box = bounds.move_by(target.subtract(old));
        let free_after = scene
            .unobstructed(moved_box.deflate(Vec3::new(1.0E-5, 1.0E-5, 1.0E-5)))
            .unwrap_or(true);
        if free_before && (wrongly || !free_after) {
            log::warn!("vehicle {id} moved wrongly");
            back(host, old, yaw);
            return;
        }
        host.hub.guests[gi].vehicle_good = Some([target.x, target.y, target.z]);
        if let Some(i) = mount {
            let m = &mut self.mobs[i];
            m.position = target;
            m.yaw = p.yaw;
            m.head_pitch = p.pitch;
            m.on_ground = p.on_ground;
            return;
        }
        let Some(i) = boat else {
            return;
        };
        let b = &mut self.boats[i];
        b.previous = old;
        b.velocity = target.subtract(old);
        b.position = target;
        b.previous_yaw = b.yaw;
        b.yaw = p.yaw;
        b.on_ground = p.on_ground;
        let seat = self.player_boat_seat(i);
        let g = &mut host.hub.guests[gi];
        g.previous_position = g.position;
        g.position = [seat.x, seat.y, seat.z];
    }

    pub(super) fn guest_paddle_boat(&mut self, host: &LanHost, conn: u32, paddles: [bool; 2]) {
        let vehicle = host
            .hub
            .guests
            .iter()
            .find(|g| g.conn == conn)
            .and_then(|g| g.vehicle);
        if let Some(b) = self.boats.iter_mut().find(|b| Some(b.id) == vehicle) {
            b.paddling = paddles;
        }
    }
}
