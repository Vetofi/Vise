use super::Game;
use vise_client::{Result, blocks::Fluid, collisions::SceneCollisions, items::ItemStack};
use vise_core::{
    geometry::{Vec3, VoxelShape},
    position::BlockPos,
    random::{LegacyRandom, RandomSource},
};
use vise_world::{
    collision_context::{CollisionContext, CollisionEntity, CollisionItem},
    entity_obstruction::EntityIdentity,
    java_hash_map::{JavaHashMap, JavaKey},
    registry::BlockStateId,
    world_ray::{RayBlockMode, RayFluidMode, RayWorld, clip_world},
};
use std::{cmp::Ordering, io};

struct ColliderRays<'a, 'r> {
    scene: SceneCollisions<'a, 'r>,
    context: CollisionContext<'static>,
}
impl RayWorld for ColliderRays<'_, '_> {
    type Error = io::Error;
    type Block = BlockStateId;
    type Fluid = ();
    fn block_state(&mut self, p: BlockPos) -> io::Result<BlockStateId> {
        Ok(self.scene.world.block(p.x, p.y, p.z))
    }
    fn fluid_state(&mut self, _: BlockPos) -> io::Result<()> {
        Ok(())
    }
    fn block_shape(
        &mut self,
        state: BlockStateId,
        p: BlockPos,
        _: RayBlockMode,
    ) -> io::Result<VoxelShape> {
        let binding = self.scene.binding;
        binding
            .collision_shape_with_context(&mut self.scene, state, p, &self.context)
            .map_err(|e| io::Error::other(format!("{e:?}")))
    }
    fn interaction_shape(&mut self, _: BlockStateId, _: BlockPos) -> io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
    fn fluid_is_source(&mut self, _: ()) -> io::Result<bool> {
        Ok(false)
    }
    fn fluid_is_empty(&mut self, _: ()) -> io::Result<bool> {
        Ok(true)
    }
    fn fluid_is_water(&mut self, _: ()) -> io::Result<bool> {
        Ok(false)
    }
    fn fluid_shape(&mut self, _: (), _: BlockPos) -> io::Result<VoxelShape> {
        Ok(VoxelShape::empty())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Key([i32; 3]);
impl JavaKey for Key {
    fn java_hash(&self) -> i32 {
        let [x, y, z] = self.0;
        y.wrapping_add(z.wrapping_mul(31))
            .wrapping_mul(31)
            .wrapping_add(x)
    }
    fn comparable_compare(&self, other: &Self) -> Option<Ordering> {
        let ([x, y, z], [ox, oy, oz]) = (self.0, other.0);
        Some(y.cmp(&oy).then(z.cmp(&oz)).then(x.cmp(&ox)))
    }
    fn tie_break(&self, _other: &Self) -> Ordering {
        Ordering::Less
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BlockInteraction {
    Keep,
    Destroy,
    DestroyWithDecay,
}

pub(super) fn blast_reaches(center: [f64; 3], diameter: f64, at: [f64; 3]) -> bool {
    let d =
        ((at[0] - center[0]).powi(2) + (at[1] - center[1]).powi(2) + (at[2] - center[2]).powi(2))
            .sqrt();
    d / diameter <= 1.0
}

pub(super) fn blast_spares_decorations(
    mob_griefing: bool,
    interaction: BlockInteraction,
    by_mob: bool,
) -> bool {
    !mob_griefing && (interaction == BlockInteraction::Keep || by_mob)
}

thread_local! {
    static CLIENT_RANDOM: std::cell::RefCell<LegacyRandom> = std::cell::RefCell::new(LegacyRandom::new(0x5eed));
}
pub(super) fn client_float() -> f32 {
    CLIENT_RANDOM.with(|r| r.borrow_mut().next_float())
}

impl Game<'_> {
    pub(super) fn seen_percent(&self, center: [f64; 3], min: [f64; 3], max: [f64; 3]) -> f32 {
        let xs = 1.0 / ((max[0] - min[0]) * 2.0 + 1.0);
        let ys = 1.0 / ((max[1] - min[1]) * 2.0 + 1.0);
        let zs = 1.0 / ((max[2] - min[2]) * 2.0 + 1.0);
        let x_offset = (1.0 - (1.0 / xs).floor() * xs) / 2.0;
        let z_offset = (1.0 - (1.0 / zs).floor() * zs) / 2.0;
        if xs < 0.0 || ys < 0.0 || zs < 0.0 {
            return 0.0;
        }
        let mut rays = ColliderRays {
            scene: SceneCollisions {
                world: &self.world,
                binding: self.collision,
                subject: None,
            },
            context: CollisionContext::for_entity(
                CollisionEntity {
                    identity: EntityIdentity(0),
                    y: min[1],
                    descending: false,
                    main_hand: Some(CollisionItem::EMPTY),
                },
                false,
            ),
        };
        let (mut hits, mut count) = (0, 0);
        let mut xx = 0.0;
        while xx <= 1.0 {
            let mut yy = 0.0;
            while yy <= 1.0 {
                let mut zz = 0.0;
                while zz <= 1.0 {
                    let x = min[0] + (max[0] - min[0]) * xx;
                    let y = min[1] + (max[1] - min[1]) * yy;
                    let z = min[2] + (max[2] - min[2]) * zz;
                    let from = Vec3::new(x + x_offset, y, z + z_offset);
                    let to = Vec3::new(center[0], center[1], center[2]);
                    let missed = clip_world(
                        &mut rays,
                        from,
                        to,
                        RayBlockMode::Collider,
                        RayFluidMode::None,
                        4096,
                    )
                    .is_ok_and(|r| r.miss);
                    if missed {
                        hits += 1;
                    }
                    count += 1;
                    zz += zs;
                }
                yy += ys;
            }
            xx += xs;
        }
        hits as f32 / count as f32
    }
    fn exploded_positions(
        &mut self,
        center: [f64; 3],
        radius: f32,
        resists: Option<([i32; 3], f32)>,
    ) -> Vec<[i32; 3]> {
        let (min_y, max_y) = self.with_level(|_, level, _| {
            use vise_world::level::LevelAccess;
            (level.min_y(), level.max_y())
        });
        let mut set: JavaHashMap<Key, ()> = JavaHashMap::default();
        for xx in 0..16 {
            for yy in 0..16 {
                for zz in 0..16 {
                    if !(xx == 0 || xx == 15 || yy == 0 || yy == 15 || zz == 0 || zz == 15) {
                        continue;
                    }
                    let mut xd = f64::from(xx as f32 / 15.0 * 2.0 - 1.0);
                    let mut yd = f64::from(yy as f32 / 15.0 * 2.0 - 1.0);
                    let mut zd = f64::from(zz as f32 / 15.0 * 2.0 - 1.0);
                    let d = (xd * xd + yd * yd + zd * zd).sqrt();
                    xd /= d;
                    yd /= d;
                    zd /= d;
                    let mut power = radius * (0.7 + self.world.random.next_float() * 0.6);
                    let (mut xp, mut yp, mut zp) = (center[0], center[1], center[2]);
                    while power > 0.0 {
                        let pos = [xp.floor() as i32, yp.floor() as i32, zp.floor() as i32];
                        if pos[1] < min_y
                            || pos[1] >= max_y
                            || !(-30_000_000..30_000_000).contains(&pos[0])
                            || !(-30_000_000..30_000_000).contains(&pos[2])
                        {
                            break;
                        }
                        let state = self.world.block(pos[0], pos[1], pos[2]);
                        let f = self.res.facts().get(state);
                        let fluid = if f.fluid != Fluid::None {
                            100.0f32
                        } else {
                            0.0
                        };
                        match resists {
                            Some((at, resistance)) if at == pos => {
                                power -= (resistance + 0.3) * 0.3;
                            }
                            _ if !self.res.facts().is_air(state) || f.fluid != Fluid::None => {
                                power -= (f.explosion_resistance.max(fluid) + 0.3) * 0.3;
                            }
                            _ => {}
                        }
                        if power > 0.0 {
                            set.put(Key(pos), ());
                        }
                        xp += xd * f64::from(0.3f32);
                        yp += yd * f64::from(0.3f32);
                        zp += zd * f64::from(0.3f32);
                        power -= 0.225_000_01;
                    }
                }
            }
        }
        set.entries().map(|(k, _)| k.0).collect()
    }
    pub(super) fn explosion_hurt_player(
        &mut self,
        center: [f64; 3],
        radius: f32,
        damage_type: &str,
        exploding: Option<u32>,
    ) -> Option<[f64; 3]> {
        let diameter = f64::from(radius * 2.0);
        let resistance = self
            .player
            .as_mut()
            .and_then(|p| p.attribute("minecraft:explosion_knockback_resistance").ok())
            .unwrap_or(0.0);
        let p = self.player.as_ref()?;
        let feet = [p.position.x, p.position.y, p.position.z];
        let dist = ((feet[0] - center[0]).powi(2)
            + (feet[1] - center[1]).powi(2)
            + (feet[2] - center[2]).powi(2))
        .sqrt()
            / diameter;
        if dist > 1.0 || p.health.dead_or_dying() {
            return None;
        }
        self.adv.marks.blast = Some((feet, false));
        let w = f64::from(p.dimensions.width) / 2.0;
        let h = f64::from(p.dimensions.height);
        let eye = [
            feet[0],
            feet[1] + f64::from(p.dimensions.eye_height),
            feet[2],
        ];
        let exposure = self.seen_percent(
            center,
            [feet[0] - w, feet[1], feet[2] - w],
            [feet[0] + w, feet[1] + h, feet[2] + w],
        );
        let pow = (1.0 - dist) * f64::from(exposure);
        let amount = ((pow * pow + pow) / 2.0 * 7.0 * diameter + 1.0) as f32;
        let d = [eye[0] - center[0], eye[1] - center[1], eye[2] - center[2]];
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let dir = if l < 1.0e-4 {
            [0.0; 3]
        } else {
            [d[0] / l, d[1] / l, d[2] / l]
        };
        let power = pow * (1.0 - resistance);
        let hurtable = exploding.is_none_or(|id| {
            vise_client::scoreboard::teams_allow_damage(self.mob_team(id), self.player_team())
        });
        let blocking = self.player_blocking(Some(center), damage_type, false);
        let raised = blocking.is_some();
        let (creative, spectator) = (self.creative, self.spectator());
        let flying = self.player.as_ref().is_some_and(|p| p.flying);
        let mut outcome = vise_client::health::BlockOutcome::default();
        let mut pushed = None;
        if let Some(p) = self.player.as_mut()
            && let Some(id) = self.motion.health.damage.id(damage_type)
        {
            if !creative
                && !spectator
                && hurtable
                && let Ok(o) = p.hurt_caused_by(
                    &self.motion.health,
                    self.world.health_rules,
                    id,
                    amount,
                    blocking,
                    exploding.is_some(),
                )
            {
                outcome = o;
                if o.knockback && o.blocked <= 0.0 {
                    p.health
                        .indicate_damage(center[0] - feet[0], center[2] - feet[2], p.yaw);
                }
            }
            if !spectator && !(creative && flying) {
                let push = [dir[0] * power, dir[1] * power, dir[2] * power];
                p.velocity = Vec3::new(
                    p.velocity.x + push[0],
                    p.velocity.y + push[1],
                    p.velocity.z + push[2],
                );
                pushed = (!p.health.sync_velocity).then_some(push);
            }
        }
        if raised {
            self.after_block(outcome, amount, None, false);
        }
        pushed
    }

    fn explosion_hurt_entities(
        &mut self,
        center: [f64; 3],
        radius: f32,
        damage_type: &str,
        interaction: BlockInteraction,
    ) -> Vec<(u32, [f64; 3])> {
        if radius < 1.0e-5 {
            return Vec::new();
        }
        self.explosion_hits_special(center, radius);
        let diameter = f64::from(radius * 2.0);
        let distance = |p: [f64; 3]| {
            ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2))
                .sqrt()
        };
        let damage = |dist: f64, exposure: f32| {
            let pow = (1.0 - dist) * f64::from(exposure);
            ((pow * pow + pow) / 2.0 * 7.0 * diameter + 1.0) as f32
        };
        let direction = |origin: [f64; 3]| {
            let d = [
                origin[0] - center[0],
                origin[1] - center[1],
                origin[2] - center[2],
            ];
            let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if l < 1.0e-4 {
                [0.0; 3]
            } else {
                [d[0] / l, d[1] / l, d[2] / l]
            }
        };
        let exploding = self.combat.explosion_source;
        let teams_allow = |g: &Self, victim: Option<&vise_client::scoreboard::Team>| {
            exploding.is_none_or(|id| {
                vise_client::scoreboard::teams_allow_damage(g.mob_team(id), victim)
            })
        };
        let own = self.explosion_hurt_player(center, radius, damage_type, exploding);
        let mut pushed: Vec<_> = self.lan_acting.zip(own).into_iter().collect();
        if let Some(mut host) = self.lan.take() {
            let guests: Vec<u32> = host
                .hub
                .guests
                .iter()
                .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
                .map(|g| g.conn)
                .collect();
            for conn in guests {
                let push = self.as_guest(&mut host, conn, |g| {
                    g.explosion_hurt_player(center, radius, damage_type, exploding)
                });
                if let Some(Some(push)) = push {
                    pushed.push((conn, push));
                }
            }
            self.lan = Some(host);
        }
        let cause = self
            .combat
            .explosion_owner
            .or(exploding)
            .and_then(|id| self.mobs.iter().find(|m| m.id == id))
            .map(|m| m.species());
        let spared = exploding.filter(|_| self.combat.explosion_source_named);
        let mut mobs = std::mem::take(&mut self.mobs);
        for m in mobs
            .iter_mut()
            .filter(|m| m.alive() && spared != Some(m.id))
        {
            let hurtable = teams_allow(self, self.mob_team(m.id));
            let feet = [m.position.x, m.position.y, m.position.z];
            let dist = distance(feet) / diameter;
            if dist > 1.0 {
                continue;
            }
            let (w, h) = (m.kind.width / 2.0, m.kind.height);
            let eye = [feet[0], feet[1] + m.kind.eye_height, feet[2]];
            let exposure = self.seen_percent(
                center,
                [feet[0] - w, feet[1], feet[2] - w],
                [feet[0] + w, feet[1] + h, feet[2] + w],
            );
            let source = vise_client::mobs::Damage {
                kind: vise_client::mobs::DamageKind::Explosion,
                attacker: self
                    .combat
                    .explosion_owner
                    .or(exploding)
                    .filter(|&o| o != m.id)
                    .map(vise_client::mobs::Who::Mob),
                from: Some(center),
            };
            let shielded =
                cause.is_some_and(|c| vise_client::mobs::wither::shielded_from(m.species(), c));
            if hurtable && (shielded || !m.damage(damage(dist, exposure), source)) {
                continue;
            }
            let dir = direction(eye);
            let power = (1.0 - dist) * f64::from(exposure);
            m.velocity = Vec3::new(
                m.velocity.x + dir[0] * power,
                m.velocity.y + dir[1] * power,
                m.velocity.z + dir[2] * power,
            );
        }
        self.mobs = mobs;
        let mut items = std::mem::take(&mut self.dropped);
        items.retain_mut(|item| {
            let feet = [item.position.x, item.position.y, item.position.z];
            let dist = distance(feet) / diameter;
            if dist > 1.0 {
                return true;
            }
            let half = 0.125;
            let exposure = self.seen_percent(
                center,
                [feet[0] - half, feet[1], feet[2] - half],
                [feet[0] + half, feet[1] + 0.25, feet[2] + half],
            );
            if &*item.stack.item != "minecraft:nether_star" && damage(dist, exposure) >= 5.0 {
                return false;
            }
            let dir = direction([feet[0], feet[1] + 0.2125, feet[2]]);
            let power = (1.0 - dist) * f64::from(exposure);
            item.velocity = Vec3::new(
                item.velocity.x + dir[0] * power,
                item.velocity.y + dir[1] * power,
                item.velocity.z + dir[2] * power,
            );
            true
        });
        self.dropped = items;
        let mut tnts = std::mem::take(&mut self.tnts);
        for t in &mut tnts {
            let feet = [t.position.x, t.position.y, t.position.z];
            let dist = distance(feet) / diameter;
            if dist > 1.0 {
                continue;
            }
            let half = 0.49;
            let exposure = self.seen_percent(
                center,
                [feet[0] - half, feet[1], feet[2] - half],
                [feet[0] + half, feet[1] + 0.98, feet[2] + half],
            );
            let dir = direction(feet);
            let power = (1.0 - dist) * f64::from(exposure);
            t.velocity = Vec3::new(
                t.velocity.x + dir[0] * power,
                t.velocity.y + dir[1] * power,
                t.velocity.z + dir[2] * power,
            );
        }
        self.tnts = tnts;
        let ids: Vec<u32> = self.carts.iter().map(|c| c.id).collect();
        for id in ids {
            let Some(i) = self.carts.iter().position(|c| c.id == id) else {
                continue;
            };
            let bounds = self.carts[i].bounds();
            let feet = self.carts[i].position;
            let dist = distance([feet.x, feet.y, feet.z]) / diameter;
            if dist > 1.0 {
                continue;
            }
            let (lo, hi) = (bounds.min_position(), bounds.max_position());
            let exposure = self.seen_percent(center, [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
            self.hurt_cart(i, damage(dist, exposure), false, true);
            let Some(c) = self.carts.iter_mut().find(|c| c.id == id) else {
                continue;
            };
            let eye_height = f64::from((hi.y - lo.y) as f32 * 0.85f32);
            let dir = direction([feet.x, feet.y + eye_height, feet.z]);
            let power = (1.0 - dist) * f64::from(exposure);
            c.velocity = Vec3::new(
                c.velocity.x + dir[0] * power,
                c.velocity.y + dir[1] * power,
                c.velocity.z + dir[2] * power,
            );
        }
        self.explosion_breaks_frames(center, diameter, interaction);
        self.explosion_breaks_decorations(center, diameter, interaction);
        pushed
    }
    pub(super) fn blast_spares_decorations(&self, interaction: BlockInteraction) -> bool {
        let by_mob = self
            .combat
            .explosion_owner
            .or(self.combat.explosion_source)
            .is_some();
        blast_spares_decorations(self.ui.rules.bool("mob_griefing"), interaction, by_mob)
    }
    fn explosion_breaks_decorations(
        &mut self,
        center: [f64; 3],
        diameter: f64,
        interaction: BlockInteraction,
    ) {
        if self.blast_spares_decorations(interaction) {
            return;
        }
        let mut i = 0;
        while i < self.paintings.len() {
            let (lo, hi) = self.paintings[i].bounds();
            let at = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
            if blast_reaches(center, diameter, at) {
                self.break_painting(i, false);
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.cushions.len() {
            let p = self.cushions[i].position;
            if blast_reaches(center, diameter, [p.x, p.y, p.z]) {
                self.break_cushion(i, false);
            } else {
                i += 1;
            }
        }
        let mut i = 0;
        while i < self.armor_stands.len() {
            let a = &self.armor_stands[i];
            let p = a.position;
            if !super::armor_stands::spared_by_blast(a)
                && blast_reaches(center, diameter, [p.x, p.y, p.z])
            {
                self.break_armor_stand_by_anything(i);
            } else {
                i += 1;
            }
        }
    }
    pub(super) fn destroy_type(&self, rule: &str) -> BlockInteraction {
        if self.ui.rules.bool(rule) {
            BlockInteraction::DestroyWithDecay
        } else {
            BlockInteraction::Destroy
        }
    }
    pub(super) fn with_hive_emptying_blast<R>(&mut self, blast: impl FnOnce(&mut Self) -> R) -> R {
        self.combat.hive_emptying_blast = true;
        let result = blast(self);
        self.combat.hive_emptying_blast = false;
        result
    }
    pub(super) fn server_explode(
        &mut self,
        center: [f64; 3],
        radius: f32,
        interaction: BlockInteraction,
        fire: bool,
    ) -> Result<()> {
        self.server_explode_as(
            center,
            radius,
            interaction,
            fire,
            "minecraft:explosion",
            None,
        )
    }
    pub(super) fn server_explode_as(
        &mut self,
        center: [f64; 3],
        radius: f32,
        interaction: BlockInteraction,
        fire: bool,
        damage_type: &str,
        resists: Option<([i32; 3], f32)>,
    ) -> Result<()> {
        if let Some(p) = &self.player {
            let d = [
                p.position.x - center[0],
                p.position.y - center[1],
                p.position.z - center[2],
            ];
            let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            self.pad_rumble(vise_client::gamepad::Rumble::Explosion { distance, radius });
        }
        let ctx = vise_client::sculk::Context::default();
        self.post_game_event("minecraft:explode", center, ctx);
        let mut targets = self.exploded_positions(center, radius, resists);
        let pushed = self.explosion_hurt_entities(center, radius, damage_type, interaction);
        if interaction != BlockInteraction::Keep {
            for i in (2..=targets.len()).rev() {
                let swap = self.world.random.next_int_bound(i as i32).unwrap_or(0) as usize;
                targets.swap(i - 1, swap);
            }
            let mut stacks: Vec<([i32; 3], ItemStack)> = Vec::new();
            for &pos in &targets {
                self.explosion_hit_block(pos, radius, interaction, &mut stacks)?;
            }
            for (pos, stack) in stacks {
                self.pop_resource(pos, stack);
            }
        }
        if fire {
            for &pos in &targets {
                if self.world.random.next_int_bound(3).unwrap_or(1) == 0
                    && self
                        .res
                        .facts()
                        .is_air(self.world.block(pos[0], pos[1], pos[2]))
                    && self
                        .res
                        .facts()
                        .get(self.world.block(pos[0], pos[1] - 1, pos[2]))
                        .solid_render
                {
                    let at = BlockPos::new(pos[0], pos[1], pos[2]);
                    let fire =
                        self.with_level(|b, level, ticks| b.fire_state_for(level, ticks, at));
                    self.level_set(pos, fire)?;
                }
            }
        }
        let pitch = (1.0 + (client_float() - client_float()) * 0.2) * 0.7;
        self.audio
            .play("minecraft:entity.generic.explode", Some(center), 4.0, pitch);
        let block_count = targets.len();
        let small = radius < 2.0 || interaction == BlockInteraction::Keep;
        let (world, facts) = (&self.world, self.res.facts());
        self.particles
            .explosion(center, small, block_count, radius, &|b| {
                facts.is_air(world.block(b[0], b[1], b[2]))
            });
        self.lan_explode(center, small, radius, block_count, pushed);
        Ok(())
    }
    fn explosion_hit_block(
        &mut self,
        pos: [i32; 3],
        radius: f32,
        interaction: BlockInteraction,
        stacks: &mut Vec<([i32; 3], ItemStack)>,
    ) -> Result<()> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if self.res.facts().is_air(state) {
            return Ok(());
        }
        let name = self.res.facts().get(state).name.clone();
        let is_tnt = &*name == "minecraft:tnt";
        if &*name == "minecraft:creaking_heart" {
            self.creaking_heart_broken(pos, false);
        }
        let key = (pos[0], pos[1], pos[2]);
        let is_hive = matches!(&*name, "minecraft:beehive" | "minecraft:bee_nest");
        if is_hive && self.combat.hive_emptying_blast {
            let facing = self
                .pack
                .assets
                .registry
                .state(state)
                .and_then(|s| s.properties.get("facing").cloned());
            self.empty_hive(
                pos,
                vise_client::beehive::facing_step(facing.as_deref()),
                vise_client::beehive::Release::Emergency,
            );
        }
        if let Some(entity) = self.world.block_entities.get(&key).cloned() {
            let custom = self.world.block_names.get(&key).cloned();
            self.pending.record(pos, &entity, &name, custom);
        } else if let Some(tag) = super::spill::saved_entity(&self.world, pos) {
            self.pending.record_saved(pos, &tag);
        }
        if !is_tnt {
            let dynamic = self.pending.dynamic(pos);
            let decay = (interaction == BlockInteraction::DestroyWithDecay).then_some(radius);
            let drops = world_items!(self).block_drops_exploded(
                self.pack.assets.registry,
                state,
                None,
                &dynamic,
                &mut self.world.random,
                Some((&mut self.rand_sequences, self.level.seed)),
                decay,
                false,
            );
            for mut stack in drops {
                if &*stack.item == "minecraft:air" || stack.count == 0 {
                    continue;
                }
                self.pending.attach(pos, &mut stack);
                let items = self.res.items();
                add_or_append_stack(stacks, stack, pos, &|s| {
                    items.get(&s.item).map_or(64, |i| i.max_stack)
                });
            }
        }
        self.pending.clear(pos);
        self.level_set(pos, self.world.air)?;
        if is_hive {
            self.anger_nearby_bees(pos);
        }
        if is_tnt && self.ui.rules.bool("tnt_explodes") {
            let r = &mut self.world.random;
            let rot = r.next_double() * f64::from(std::f32::consts::TAU);
            let fuse = r.next_int_bound(20).unwrap_or(0) + 10;
            self.add_primed_tnt(BlockPos::new(pos[0], pos[1], pos[2]), rot, fuse);
        }
        Ok(())
    }
}

fn add_or_append_stack(
    stacks: &mut Vec<([i32; 3], ItemStack)>,
    mut stack: ItemStack,
    pos: [i32; 3],
    max_stack: &dyn Fn(&ItemStack) -> u32,
) {
    let max = i64::from(max_stack(&stack));
    for (_, existing) in stacks.iter_mut() {
        if existing.same_item_same_components(&stack)
            && i64::from(existing.count) + i64::from(stack.count) <= max
        {
            let moved = (max.min(16) - i64::from(existing.count))
                .min(i64::from(stack.count))
                .max(0) as u32;
            existing.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return;
            }
        }
    }
    stacks.push((pos, stack));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blasts_reach_entities_within_their_diameter() {
        let center = [0.5, 64.0, 0.5];
        assert!(blast_reaches(center, 8.0, [8.5, 64.0, 0.5]));
        assert!(blast_reaches(center, 8.0, [-3.5, 68.0, 4.5]));
        assert!(!blast_reaches(center, 8.0, [8.6, 64.0, 0.5]));
    }

    #[test]
    fn mob_griefing_off_spares_decorations_from_kept_and_mob_blasts() {
        use BlockInteraction::{Destroy, DestroyWithDecay, Keep};
        assert!(!blast_spares_decorations(true, DestroyWithDecay, true));
        assert!(!blast_spares_decorations(true, Keep, true));
        assert!(!blast_spares_decorations(false, Destroy, false));
        assert!(!blast_spares_decorations(false, DestroyWithDecay, false));
        assert!(blast_spares_decorations(false, Keep, false));
        assert!(blast_spares_decorations(false, Destroy, true));
    }

    #[test]
    fn block_pos_hash_matches_vec3i() {
        assert_eq!(Key([1, 2, 3]).java_hash(), (2 + 3 * 31) * 31 + 1);
        assert_eq!(Key([-5, 64, -7]).java_hash(), (64 - 7 * 31) * 31 - 5);
        assert_eq!(
            Key([0, 1, 0]).comparable_compare(&Key([5, 0, 9])),
            Some(Ordering::Greater)
        );
        assert_eq!(
            Key([1, 0, 2]).comparable_compare(&Key([0, 0, 3])),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn drops_merge_up_to_sixteen() {
        let stack = |item: &str| ItemStack {
            item: item.into(),
            count: 1,
            damage: 0,
            extra: None,
        };
        let max = |s: &ItemStack| {
            if &*s.item == "minecraft:diamond_sword" {
                1
            } else {
                64
            }
        };
        let mut stacks = Vec::new();
        for _ in 0..20 {
            add_or_append_stack(&mut stacks, stack("minecraft:dirt"), [0, 0, 0], &max);
        }
        add_or_append_stack(&mut stacks, stack("minecraft:stone"), [1, 0, 0], &max);
        let counts: Vec<u32> = stacks.iter().map(|(_, s)| s.count).collect();
        assert_eq!(counts, vec![16, 4, 1]);
    }

    #[test]
    fn unstackable_drops_stay_separate() {
        let sword = ItemStack {
            item: "minecraft:diamond_sword".into(),
            count: 1,
            damage: 0,
            extra: None,
        };
        let max = |_: &ItemStack| 1;
        let mut stacks = Vec::new();
        add_or_append_stack(&mut stacks, sword.clone(), [0, 0, 0], &max);
        add_or_append_stack(&mut stacks, sword, [0, 0, 0], &max);
        let counts: Vec<u32> = stacks.iter().map(|(_, s)| s.count).collect();
        assert_eq!(counts, vec![1, 1]);
    }

    #[test]
    fn drops_with_different_damage_stay_separate() {
        let pick = |damage| ItemStack {
            item: "minecraft:iron_pickaxe".into(),
            count: 1,
            damage,
            extra: None,
        };
        let max = |_: &ItemStack| 64;
        let mut stacks = Vec::new();
        add_or_append_stack(&mut stacks, pick(0), [0, 0, 0], &max);
        add_or_append_stack(&mut stacks, pick(5), [0, 0, 0], &max);
        assert_eq!(stacks.len(), 2);
    }
}
