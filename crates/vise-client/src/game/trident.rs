use super::{Game, view_vector};
use vise_client::{
    arrows::{back_off, clip_block_stop},
    collisions::SceneCollisions,
    enchant::EffectContext,
    items::ItemStack,
    mesher::Vertex,
    mobs::{Damage, DamageKind, Who, anim::Poses},
    projectiles::ProjectileWorld,
};
use vise_core::{
    geometry::{Aabb, Vec3, traverse_blocks},
    position::BlockPos,
    random::RandomSource,
};

pub(super) struct ThrownTrident {
    pub id: u32,
    pub(super) stack: ItemStack,
    pub(super) position: Vec3,
    pub(super) previous: Vec3,
    pub(super) velocity: Vec3,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) in_ground: bool,
    in_ground_time: u32,
    shake: u32,
    dealt_damage: bool,
    pub(super) loyalty: u32,
    pub(super) no_physics: bool,
    returning: u32,
    creative_only: bool,
    pub(super) owner: u32,
    pub(super) guest: Option<u32>,
}
struct Catcher {
    guest: Option<u32>,
    id: i32,
    feet: Vec3,
    eye: Vec3,
    touches: bool,
}
impl ThrownTrident {
    pub(super) fn remote(id: u32, position: Vec3, yaw: f32, pitch: f32) -> Self {
        Self {
            id,
            stack: ItemStack::new("minecraft:trident", 1),
            position,
            previous: position,
            velocity: Vec3::ZERO,
            yaw,
            pitch,
            in_ground: false,
            in_ground_time: 0,
            shake: 0,
            dealt_damage: false,
            loyalty: 0,
            no_physics: false,
            returning: 0,
            creative_only: false,
            owner: 0,
            guest: None,
        }
    }
    fn loyalty_pull(&mut self, owner_eye: Vec3) {
        let v = Vec3::new(
            owner_eye.x - self.position.x,
            owner_eye.y - self.position.y,
            owner_eye.z - self.position.z,
        );
        self.position.y += v.y * 0.015 * f64::from(self.loyalty);
        let len = (v.x * v.x + v.y * v.y + v.z * v.z).sqrt().max(1e-9);
        let accel = 0.05 * f64::from(self.loyalty);
        self.velocity = Vec3::new(
            self.velocity.x * 0.95 + v.x / len * accel,
            self.velocity.y * 0.95 + v.y / len * accel,
            self.velocity.z * 0.95 + v.z / len * accel,
        );
    }
    pub(super) fn remote_tick(&mut self, w: &impl ProjectileWorld, owner_eye: Option<Vec3>) {
        self.previous = self.position;
        self.shake = self.shake.saturating_sub(1);
        if self.in_ground_time > 4 {
            self.dealt_damage = true;
        }
        if self.loyalty > 0
            && (self.dealt_damage || self.no_physics)
            && let Some(eye) = owner_eye
        {
            self.no_physics = true;
            self.in_ground = false;
            self.loyalty_pull(eye);
        }
        if self.in_ground && !self.no_physics {
            self.in_ground_time += 1;
            return;
        }
        self.in_ground_time = 0;
        let m = self.velocity;
        let (x, z) = if self.no_physics {
            (-m.x, -m.z)
        } else {
            (m.x, m.z)
        };
        self.yaw = x.atan2(z).to_degrees() as f32;
        self.pitch = m.y.atan2(m.x.hypot(m.z)).to_degrees() as f32;
        let to = self.position.add_vector(m);
        if !self.no_physics
            && let Some((at, _, _)) = w.clip(self.position, to)
        {
            self.position = back_off(at, m);
            self.velocity = Vec3::ZERO;
            self.in_ground = true;
            self.shake = 7;
            return;
        }
        self.position = to;
        self.velocity = self.velocity.scale(0.99);
        if !self.no_physics {
            self.velocity.y -= 0.05;
        }
    }
}
const HALF: f64 = 0.25;
fn segment_box(from: Vec3, d: Vec3, b: Aabb) -> Option<f64> {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (o, d, lo, hi) = (
        [from.x, from.y, from.z],
        [d.x, d.y, d.z],
        [lo.x, lo.y, lo.z],
        [hi.x, hi.y, hi.z],
    );
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for a in 0..3 {
        if d[a].abs() < 1e-12 {
            if o[a] < lo[a] || o[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[a] - o[a]) / d[a], (hi[a] - o[a]) / d[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
    }
    (t0 <= t1).then_some(t0)
}
impl Game<'_> {
    pub(super) fn use_trident(&mut self, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:trident" {
            return false;
        }
        let max = self
            .res
            .items()
            .get(&stack.item)
            .map_or(250, |i| i.max_damage);
        let riptide = stack.enchantment_level("minecraft:riptide") > 0;
        if stack.damage + 1 >= max || (riptide && !self.in_water_or_rain()) {
            return true;
        }
        if self.using.is_none() {
            self.using = Some((self.held_slot(), 0));
        }
        true
    }
    fn in_water_or_rain(&mut self) -> bool {
        let Some(p) = &self.player else { return false };
        let (x, y, z) = (
            p.position.x.floor() as i32,
            p.position.y.floor() as i32,
            p.position.z.floor() as i32,
        );
        let water = self.res.facts().get(self.world.block(x, y, z)).fluid
            == vise_client::blocks::Fluid::Water;
        let (at, top) = (p.position, p.bounds().max_position().y);
        water || self.in_rain(at, top)
    }
    pub(super) fn spin_targets(&self) -> Vec<(u32, Aabb)> {
        let Some(p) = self
            .player
            .as_ref()
            .filter(|p| p.auto_spin_attack_ticks > 0)
        else {
            return Vec::new();
        };
        let near = p.bounds().inflate(Vec3::new(16.0, 16.0, 16.0));
        self.mobs
            .iter()
            .filter(|m| m.alive() && m.bounds().intersects(near))
            .map(|m| (m.id, m.bounds()))
            .collect()
    }
    pub(super) fn end_spin_attack(&mut self, hit: Option<u32>) {
        if let Some(id) = hit
            && let Some(index) = self.mobs.iter().position(|m| m.id == id)
        {
            self.attack_mob(index);
        }
        if let Some(p) = self.player.as_mut()
            && p.auto_spin_attack_ticks <= 0
        {
            p.auto_spin_attack_dmg = 0.0;
        }
    }
    fn channel_lightning(&mut self, at: [f64; 3], guest: Option<u32>) {
        if self.dim != super::Dim::Overworld || self.world.weather.levels(1.0).1 <= 0.9 {
            return;
        }
        let (x, y, z) = (
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        );
        if self.world.light(x, y, z) >> 4 != 15 {
            return;
        }
        self.spawn_bolt(at, false);
        if let Some(bolt) = self.bolts.last_mut() {
            bolt.cause = Some(guest);
        }
        self.audio
            .play("minecraft:item.trident.thunder", Some(at), 5.0, 1.0);
    }
    fn channel_rod_hit(&mut self, from: Vec3, velocity: Vec3, guest: Option<u32>) {
        let to = Vec3::new(
            from.x + velocity.x,
            from.y + velocity.y,
            from.z + velocity.z,
        );
        let mut scene = SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let hit = traverse_blocks(from, to, 1024, |p: BlockPos| {
            let state = self.world.block(p.x, p.y, p.z);
            if self.res.facts().is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p)
        });
        let Some(hit) = hit.ok().flatten() else {
            return;
        };
        let state = self.world.block(hit.block.x, hit.block.y, hit.block.z);
        if !self.res.facts().get(state).name.ends_with("lightning_rod") {
            return;
        }
        let edge = f64::from(1.0e-5f32);
        let clamp =
            |v: f64, origin: i32| v.clamp(f64::from(origin) + edge, f64::from(origin) + 1.0 - edge);
        self.channel_lightning(
            [
                clamp(hit.location.x, hit.block.x),
                clamp(hit.location.y, hit.block.y),
                clamp(hit.location.z, hit.block.z),
            ],
            guest,
        );
    }
    pub(super) fn release_trident(&mut self, ticks: u32) {
        if ticks < 10 {
            return;
        }
        let Some(stack) = self.held().cloned() else {
            return;
        };
        let Some(eye) = self.eye_position() else {
            return;
        };
        let riptide = stack.enchantment_level("minecraft:riptide");
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        if riptide > 0 {
            if !self.in_water_or_rain() || self.riding.is_some() {
                return;
            }
            let server_client = self.remote.is_some();
            if !server_client {
                self.hurt_held_without_breaking(1);
            }
            let strength = 1.5 + 0.75 * (riptide as f32 - 1.0);
            let d = view_vector(self.yaw, self.pitch);
            if let Some(p) = self.player.as_mut() {
                p.velocity = Vec3::new(
                    p.velocity.x + d[0] * f64::from(strength),
                    p.velocity.y + d[1] * f64::from(strength),
                    p.velocity.z + d[2] * f64::from(strength),
                );
                p.auto_spin_attack_ticks = 20;
                p.auto_spin_attack_dmg = 8.0;
            }
            let lift = self
                .player
                .as_ref()
                .filter(|p| p.ground.on_ground)
                .map(|p| p.bounds());
            if let Some(bounds) = lift {
                let mut scene = SceneCollisions {
                    world: &self.world,
                    binding: self.collision,
                    subject: None,
                };
                let up = Vec3::new(0.0, f64::from(1.1999999f32), 0.0);
                let moved = scene.resolve(bounds, up, 0.0, true).unwrap_or(Vec3::ZERO);
                if let Some(p) = self.player.as_mut() {
                    p.position.y += moved.y;
                }
            }
            if !server_client {
                let sound = format!("minecraft:item.trident.riptide_{}", riptide.min(3));
                self.audio.play(&sound, at, 1.0, 1.0);
            }
            return;
        }
        if self.remote.is_some() {
            return;
        }
        self.hurt_held_without_breaking(1);
        let thrown = self.held().cloned().unwrap_or(stack);
        if !self.creative {
            *self.held_mut() = None;
        }
        let start = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let mut projectile = vise_client::projectiles::Projectile::new(
            vise_client::projectiles::ProjectileKind::Snowball,
            start,
            Some(Who::Player),
        );
        let (v, on_ground) = self.player.as_ref().map_or((Vec3::ZERO, true), |p| {
            (self.known_player_movement(), p.ground.on_ground)
        });
        projectile.shoot_from_rotation(
            &mut self.world.random,
            self.pitch,
            self.yaw,
            0.0,
            2.5,
            1.0,
            v,
            on_ground,
        );
        self.tridents.push(ThrownTrident {
            id: 0,
            loyalty: thrown.enchantment_level("minecraft:loyalty"),
            stack: thrown,
            position: start,
            previous: start,
            velocity: projectile.velocity,
            yaw: projectile.yaw,
            pitch: projectile.pitch,
            in_ground: false,
            in_ground_time: 0,
            shake: 0,
            dealt_damage: false,
            no_physics: false,
            returning: 0,
            creative_only: self.creative,
            owner: 0,
            guest: self.lan_acting,
        });
        self.player_projectile_shot();
        self.audio
            .play("minecraft:item.trident.throw", at, 1.0, 1.0);
    }
    fn thrower_hit(
        &mut self,
        guest: Option<u32>,
        victim: &vise_client::advancements::EntityView,
        direct: vise_client::advancements::EntityView,
        damage: f32,
    ) {
        let Some(conn) = guest.filter(|c| self.lan_acting != Some(*c)) else {
            self.player_hit(victim, "minecraft:trident", Some(direct), damage);
            return;
        };
        let Some(mut host) = self.lan.take() else {
            return;
        };
        self.as_guest(&mut host, conn, |g| {
            g.player_hit(victim, "minecraft:trident", Some(direct), damage)
        });
        self.lan = Some(host);
    }
    fn trident_catchers(&self) -> Vec<Catcher> {
        let mut out = Vec::new();
        if let Some(eye) = self.eye_position() {
            let feet = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
            out.push(Catcher {
                guest: None,
                id: 0,
                feet,
                eye: Vec3::new(eye[0], eye[1], eye[2]),
                touches: true,
            });
        }
        if let Some(host) = self.lan.as_ref() {
            for g in host.hub.guests.iter().filter(|g| host.hub.here(g)) {
                let [x, y, z] = g.position;
                out.push(Catcher {
                    guest: Some(g.conn),
                    id: g.entity_id,
                    feet: Vec3::new(x, y, z),
                    eye: Vec3::new(x, y + g.eye_height(), z),
                    touches: !g.dead && !g.spectator(),
                });
            }
        }
        out
    }
    fn trident_taken(&mut self, t: &ThrownTrident, by: &Catcher) -> bool {
        if !t.creative_only {
            let rest = match by.guest {
                None => self.give(t.stack.clone()),
                Some(conn) => {
                    let Some(mut host) = self.lan.take() else {
                        return false;
                    };
                    let rest = self.as_guest(&mut host, conn, |g| g.give(t.stack.clone()));
                    if rest.is_some() {
                        let at = [t.position.x, t.position.y, t.position.z];
                        host.hub.take_item(t.id as i32, by.id, 1, at);
                    }
                    self.lan = Some(host);
                    let Some(rest) = rest else {
                        return false;
                    };
                    rest
                }
            };
            if let Some(rest) = rest {
                self.spawn_item(rest, [by.feet.x, by.feet.y, by.feet.z], [0.0; 3], 0);
            }
        }
        if by.guest.is_none() {
            let pitch = ((self.world.random.next_float() - self.world.random.next_float()) * 0.7
                + 1.0)
                * 2.0;
            self.audio.play(
                "minecraft:entity.item.pickup",
                Some([by.feet.x, by.feet.y, by.feet.z]),
                0.2,
                pitch,
            );
        }
        true
    }
    pub(super) fn tick_tridents(&mut self) {
        if self.tridents.is_empty() {
            return;
        }
        let catchers = self.trident_catchers();
        let mut i = 0;
        while i < self.tridents.len() {
            let mut t = self.tridents.remove(i);
            t.previous = t.position;
            t.shake = t.shake.saturating_sub(1);
            if t.in_ground_time > 4 {
                t.dealt_damage = true;
            }
            let owner_eye = catchers.iter().find(|c| c.guest == t.guest).map(|c| c.eye);
            if t.loyalty > 0
                && (t.dealt_damage || t.no_physics)
                && let Some(owner_eye) = owner_eye
            {
                t.no_physics = true;
                t.in_ground = false;
                t.loyalty_pull(owner_eye);
                if t.returning == 0 {
                    self.audio.play(
                        "minecraft:item.trident.return",
                        Some([t.position.x, t.position.y, t.position.z]),
                        10.0,
                        1.0,
                    );
                }
                t.returning += 1;
            }
            if t.in_ground {
                t.in_ground_time += 1;
            } else if t.no_physics {
                t.position = Vec3::new(
                    t.position.x + t.velocity.x,
                    t.position.y + t.velocity.y,
                    t.position.z + t.velocity.z,
                );
            } else {
                let mut hit_mob = None;
                if !t.dealt_damage {
                    for m in self.mobs.iter().filter(|m| m.alive()) {
                        if let Some(d) = segment_box(t.position, t.velocity, m.bounds())
                            && hit_mob.is_none_or(|(_, best)| d < best)
                        {
                            hit_mob = Some((m.id, d));
                        }
                    }
                }
                if let Some((id, _)) = hit_mob {
                    let victim = self
                        .mobs
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.kind.id.clone());
                    let ctx = EffectContext {
                        this: victim.as_deref(),
                        attacker: Some("minecraft:player"),
                        direct_attacker: Some("minecraft:trident"),
                        is_direct: true,
                        damage_type: Some("minecraft:trident"),
                        items: Some(world_items!(self)),
                        ..Default::default()
                    };
                    let damage = world_stations!(self).enchantments.modify_value(
                        Some(&t.stack),
                        "minecraft:damage",
                        &ctx,
                        &mut self.world.random,
                        8.0,
                    );
                    t.dealt_damage = true;
                    let mut hurt_at = None;
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                        let from = Some([t.position.x, t.position.y, t.position.z]);
                        let hurt = m.damage(
                            damage,
                            Damage {
                                kind: DamageKind::Arrow,
                                attacker: Some(Who::Player),
                                from,
                            },
                        );
                        if hurt {
                            hurt_at = Some([m.position.x, m.position.y, m.position.z]);
                        }
                    }
                    if let Some(at) = hurt_at
                        && t.stack.enchantment_level("minecraft:channeling") > 0
                    {
                        self.channel_lightning(at, t.guest);
                    }
                    if hurt_at.is_some()
                        && let Some(m) = self.mobs.iter().find(|m| m.id == id)
                    {
                        let victim = self.mob_entity(m);
                        let direct = vise_client::advancements::EntityView {
                            kind: "minecraft:trident".into(),
                            position: [t.position.x, t.position.y, t.position.z],
                            ..Default::default()
                        };
                        self.thrower_hit(t.guest, &victim, direct, damage);
                    }
                    self.audio.play(
                        "minecraft:item.trident.hit",
                        Some([t.position.x, t.position.y, t.position.z]),
                        1.0,
                        1.0,
                    );
                    t.velocity = Vec3::new(
                        -t.velocity.x * 0.02,
                        -t.velocity.y * 0.2,
                        -t.velocity.z * 0.02,
                    );
                } else {
                    let from = t.position;
                    let flight = t.velocity;
                    if let Some((stop, _)) =
                        clip_block_stop(&self.world, self.res.facts(), self.collision, from, flight)
                    {
                        if t.stack.enchantment_level("minecraft:channeling") > 0 {
                            self.channel_rod_hit(from, flight, t.guest);
                        }
                        t.position = stop;
                        t.velocity = Vec3::ZERO;
                        t.in_ground = true;
                        t.shake = 7;
                        self.audio.play(
                            "minecraft:item.trident.hit_ground",
                            Some([stop.x, stop.y, stop.z]),
                            1.0,
                            1.0,
                        );
                    } else {
                        t.position =
                            Vec3::new(from.x + flight.x, from.y + flight.y, from.z + flight.z);
                    }
                }
                if !t.in_ground {
                    let drag = 0.99;
                    t.velocity = Vec3::new(
                        t.velocity.x * drag,
                        t.velocity.y * drag - 0.05,
                        t.velocity.z * drag,
                    );
                    let h = (t.velocity.x * t.velocity.x + t.velocity.z * t.velocity.z).sqrt();
                    t.yaw = t.velocity.x.atan2(t.velocity.z).to_degrees() as f32;
                    t.pitch = t.velocity.y.atan2(h).to_degrees() as f32;
                }
            }
            if (t.in_ground || t.no_physics) && t.shake == 0 {
                let near = |c: &&Catcher| {
                    c.touches
                        && (owner_eye.is_none() || c.guest == t.guest)
                        && (t.position.x - c.feet.x).abs() < 1.0
                        && (t.position.z - c.feet.z).abs() < 1.0
                        && t.position.y > c.feet.y - 1.0
                        && t.position.y < c.feet.y + 2.5
                };
                if let Some(by) = catchers.iter().find(near)
                    && self.trident_taken(&t, by)
                {
                    continue;
                }
            }
            self.tridents.insert(i, t);
            i += 1;
        }
    }
    pub(super) fn trident_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        let Some(model) = &self.trident_model else {
            return;
        };
        let Some(uv) = self
            .gui
            .atlas
            .sprites
            .get("entity/trident/trident")
            .map(|r| [r.u0, r.v0, r.u1, r.v1])
        else {
            return;
        };
        let range = 10.0
            * super::entity_culling::default_range(
                glam::DVec3::splat(2.0 * HALF),
                self.view_scale(),
            );
        for t in &self.tridents {
            if !self.standing_entity_in_view(
                camera,
                super::entity_culling::at(t.previous),
                super::entity_culling::at(t.position),
                alpha,
                2.0 * HALF,
                2.0 * HALF,
                range,
                1.5,
            ) {
                continue;
            }
            let p = glam::DVec3::new(
                t.previous.x + (t.position.x - t.previous.x) * alpha,
                t.previous.y + (t.position.y - t.previous.y) * alpha,
                t.previous.z + (t.position.z - t.previous.z) * alpha,
            ) - camera;
            let base = glam::Mat4::from_translation(p.as_vec3())
                * glam::Mat4::from_rotation_y((t.yaw - 90.0).to_radians())
                * glam::Mat4::from_rotation_z((t.pitch + 90.0).to_radians());
            let l = self.world.light(
                t.position.x.floor() as i32,
                t.position.y.floor() as i32,
                t.position.z.floor() as i32,
            );
            let packed = (u32::from(l >> 4) * 16) | ((u32::from(l & 15) * 16) << 8);
            let poses = Poses::new(model);
            vise_client::mobs::render::draw(
                model,
                base,
                &poses,
                uv,
                0xFFFF_FFFF,
                packed,
                0,
                out,
            );
        }
    }
}
