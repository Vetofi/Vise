use super::{Dim, Game, dispenser, view_vector};
use vise_assets::item_model::DisplayContext;
use vise_client::{
    Result,
    arrows::EntityHit,
    blocks::Fluid,
    collisions::SceneCollisions,
    items::ItemStack,
    mesher::Vertex,
    mobs::{
        Damage, DamageKind, Mob, MobEvent, MobWorld, PlayerView, Seen, SpawnSetup, Species, Who,
        ai::{eating_sound, feed_animal, takes_food},
        farm::Climate,
        spawning::{self, BiomeFacts},
    },
    projectiles::{Hit, Projectile, ProjectileKind, ProjectileWorld},
    text::Text,
};
use vise_core::{
    geometry::{Aabb, Direction, Vec3, traverse_blocks, trig::Mth},
    position::BlockPos,
    random::RandomSource,
};
use std::sync::Arc;

fn arrow_damage(arrow: &mut Projectile, fallback: &mut impl RandomSource) -> f32 {
    let mut state = arrow.arrow.take();
    let damage = match state.as_mut() {
        Some(a) => arrow.arrow_damage(&mut a.random),
        None => arrow.arrow_damage(fallback),
    };
    arrow.arrow = state;
    damage
}

#[derive(Default)]
pub(super) struct Creatures {
    pub projectiles: Vec<Projectile>,
    pub wind_bursts: Vec<super::wind::Burst>,
    pub hook_targets: Vec<super::fishing::HookTarget>,
    pub hook_pulls: Vec<(u32, vise_core::geometry::Vec3)>,
    pub smash_pushes: Vec<super::mace::SmashPush>,
    pub sweeps: Vec<super::lan_pvp::Sweep>,
    pub lan_explosions: Vec<([f64; 3], bool, f32, usize, Vec<(u32, [f64; 3])>)>,
    pub elder_apparition: i32,
    pub trader: super::wandering_trader::Spawner,
    pub shoulder_time: std::collections::HashMap<[i32; 4], i64>,
    pub cat_tick: i32,
    pub lan_particles: Vec<(
        &'static str,
        vise_client::particles::ParticleOptions,
        Option<vise_world::registry::BlockStateId>,
        [f64; 3],
        [f64; 3],
        f64,
        i32,
    )>,
}
pub(super) struct ProjWorld<'a, 'w> {
    pub w: &'a MobWorld<'w>,
    pub targets: Vec<(Who, Aabb)>,
}
impl ProjectileWorld for ProjWorld<'_, '_> {
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, [i32; 3], [i32; 3])> {
        let mut scene = SceneCollisions {
            world: self.w.world,
            binding: self.w.collision,
            subject: None,
        };
        let r = traverse_blocks(from, to, 256, |p: BlockPos| {
            let state = self.w.world.block(p.x, p.y, p.z);
            if self.w.facts.is_air(state) {
                return None;
            }
            let shape = self
                .w
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p)
        });
        let hit = r.ok().flatten()?;
        Some((
            hit.location,
            [hit.block.x, hit.block.y, hit.block.z],
            hit.direction.normal(),
        ))
    }
    fn in_water(&self, bounds: Aabb) -> bool {
        self.w.fluids(bounds, bounds.max_position().y).in_water
    }
    fn targets(&self) -> &[(Who, Aabb)] {
        &self.targets
    }
    fn empty(&self, p: [i32; 3]) -> bool {
        self.w.facts.is_air(self.w.world.block(p[0], p[1], p[2]))
    }
    fn solid(&self, p: [i32; 3]) -> bool {
        self.w
            .facts
            .get(self.w.world.block(p[0], p[1], p[2]))
            .full_collision
    }
}

const WIND_CHARGE_IDS: u32 = super::dragon::SPECIAL - 0x10000;

fn wind_charge_hit(id: u32) -> Option<usize> {
    (WIND_CHARGE_IDS..super::dragon::SPECIAL)
        .contains(&id)
        .then(|| (id - WIND_CHARGE_IDS) as usize)
}

fn wind_charge_hitboxes(
    projectiles: &[Projectile],
    alive: &[bool],
    skip: usize,
) -> Vec<(Who, Aabb)> {
    projectiles
        .iter()
        .enumerate()
        .filter(|(j, q)| {
            *j != skip
                && alive[*j]
                && !q.stuck
                && matches!(
                    q.kind,
                    ProjectileKind::WindCharge | ProjectileKind::BreezeWindCharge
                )
        })
        .map(|(j, q)| (Who::Mob(WIND_CHARGE_IDS + j as u32), q.aabb()))
        .collect()
}

fn flame_geometry(
    atlas: &vise_client::hud::HudAtlas,
    at: glam::Vec3,
    width: f32,
    height: f32,
    camera_yaw: f32,
    light: u8,
    out: &mut Vec<Vertex>,
) {
    let (Some(fire0), Some(fire1)) = (
        atlas.sprites.get("block/fire_0").copied(),
        atlas.sprites.get("block/fire_1").copied(),
    ) else {
        return;
    };
    let s = width * 1.4;
    let mut h = height / s;
    let turn = glam::Mat3::from_rotation_y(std::f32::consts::PI - camera_yaw.to_radians());
    let lift = 0.3 - (h as i32) as f32 * 0.02;
    let packed = (u32::from(light >> 4) * 16) | ((15 * 16) << 8);
    let (mut r, mut yo, mut zo, mut ss) = (0.5f32, 0.0f32, 0.0f32, 0);
    while h > 0.0 {
        let tex = if ss % 2 == 0 { fire0 } else { fire1 };
        let (mut u0, v0, mut u1, v1) = (tex.u0, tex.v0, tex.u1, tex.v1);
        if ss / 2 % 2 == 0 {
            std::mem::swap(&mut u0, &mut u1);
        }
        for (x, y, uv) in [
            (-r, -yo, [u1, v1]),
            (r, -yo, [u0, v1]),
            (r, 1.4 - yo, [u0, v0]),
            (-r, 1.4 - yo, [u1, v0]),
        ] {
            let p = at + turn * glam::Vec3::new(x, y, zo + lift) * s;
            out.push(Vertex {
                normal: [0; 4],
                pos: p.to_array(),
                uv,
                layer: 0,
                color: 0xFFFF_FFFF,
                light: packed,

                glint_uv: [0.0, 0.0],
            });
        }
        h -= 0.45;
        yo -= 0.45;
        r *= 0.9;
        zo -= 0.03;
        ss += 1;
    }
}
pub(super) fn seen(m: &Mob, player: Option<[i32; 4]>) -> Seen {
    m.seen(player)
}
impl Game<'_> {
    pub(super) fn player_view(&self) -> Option<PlayerView> {
        let p = self.player.as_ref()?;
        let w = f64::from(p.dimensions.width) / 2.0;
        let bounds = Aabb::new(
            Vec3::new(p.position.x - w, p.position.y, p.position.z - w),
            Vec3::new(
                p.position.x + w,
                p.position.y + f64::from(p.dimensions.height),
                p.position.z + w,
            ),
        );
        Some(PlayerView {
            position: p.position,
            bounds,
            eye: [
                p.position.x,
                p.position.y + f64::from(p.dimensions.eye_height),
                p.position.z,
            ],
            alive: !p.health.dead_or_dying(),
            creative: self.creative || self.spectator(),
            sneaking: self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft),
            swimming: p.swimming,
            look: view_vector(self.yaw, self.pitch),
            main_hand: self.held().map(|s| s.item.clone()),
            off_hand: None,
            head: self.armor[0].as_ref().map(|s| s.item.clone()),
            armor_cover: self.armor.iter().filter(|a| a.is_some()).count() as f32 / 4.0,
            invisible: self.potions.effects.get("invisibility").is_some(),
            spectator: self.spectator(),
            gold_armor: self.wearing_gold(),
            hurt_by: self.player_hurt_by(),
            last_hurt: self.combat.last_hurt_mob.map(|(id, at)| (Who::Mob(id), at)),
            velocity: p.velocity,
            yaw: self.yaw,
            sprinting: p.sprinting,
            riding: self.riding.is_some(),
            hero: self.potions.effects.get("hero_of_the_village").is_some(),
            effects: self.potions.effects.list.iter().map(|e| e.id).collect(),
            uuid: self.level.player_uuid.unwrap_or_default(),
            allies: std::sync::Arc::new(self.player_allies()),
            remote: false,
            sleeping: self.sleeping.is_some(),
            sleep_timer: self.sleep_timer,
            passenger: self.riding.is_some(),
        })
    }
    pub(super) fn biome_facts(&self, x: i32, y: i32, z: i32) -> BiomeFacts {
        let Some(chunk) = self.world.chunks.get(&(x >> 4, z >> 4)) else {
            return BiomeFacts::default();
        };
        let b = self.res.biomes().get(chunk.biome(x, y, z));
        let tags = self.res.mob_data();
        let has = |t: &str| tags.biome_has(t, &b.name);
        let climate = if has("minecraft:spawns_warm_variant_farm_animals") {
            Climate::Warm
        } else if has("minecraft:spawns_cold_variant_farm_animals") {
            Climate::Cold
        } else {
            Climate::Temperate
        };
        BiomeFacts {
            coral_nautilus: has("minecraft:spawns_coral_variant_zombie_nautilus"),
            climate,
            temperature: b.temperature,
            snowy: has("minecraft:spawns_snow_foxes")
                || b.name.contains("snow")
                || b.name.contains("frozen"),
            snow_fox: has("minecraft:spawns_snow_foxes"),
            swamp: b.name.contains("swamp"),
            ocean: has("minecraft:is_ocean"),
            mushroom: b.name == "minecraft:mushroom_fields",
            desert: b.name == "minecraft:desert",
            jungle: has("minecraft:is_jungle"),
            wolf: vise_client::mobs::tame::wolf_variant_for(&b.name, &has),
            full_moon: (self.world.day_time / 24000).rem_euclid(8) == 0,
            frog: if has("minecraft:spawns_warm_variant_frogs") {
                1
            } else if has("minecraft:spawns_cold_variant_frogs") {
                2
            } else {
                0
            },
        }
    }
    pub(super) fn spawn_mob(&mut self, kind: &str, at: [f64; 3], persistent: bool) -> Option<u32> {
        self.spawn_mob_for(kind, at, persistent, false)
    }
    pub(super) fn spawn_mob_spawned(&mut self, kind: &str, at: [f64; 3]) -> Option<u32> {
        let id = self.spawn_mob_for(kind, at, true, false)?;
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.persistent = false;
        }
        Some(id)
    }
    pub(super) fn spawn_mob_for(
        &mut self,
        kind: &str,
        at: [f64; 3],
        persistent: bool,
        scripted: bool,
    ) -> Option<u32> {
        let kind = if kind.contains(':') {
            kind.to_owned()
        } else {
            format!("minecraft:{kind}")
        };
        let t = self.res.mob_types().get(&kind).cloned()?;
        self.next_entity += 1;
        let yaw = self.world.random.next_float() * 360.0;
        let mut mob = Mob::new(self.next_entity, t, Vec3::new(at[0], at[1], at[2]), yaw);
        mob.persistent = persistent;
        let facts = self.biome_facts(
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        );
        let difficulty = self.local_difficulty(at);
        spawning::finalize(&mut mob, &facts, &difficulty);
        if !scripted {
            let reason = if persistent {
                vise_client::mobs::raider::Reason::Other
            } else {
                vise_client::mobs::raider::Reason::Natural
            };
            vise_client::mobs::raider::finalize(&mut mob, reason);
        }
        let id = mob.id;
        self.mobs.push(mob);
        self.enchant_spawn_equipment(id);
        if !persistent && !scripted {
            self.spawn_jockeys(id, true);
        }
        Some(id)
    }
    pub(super) fn convert_to(
        &mut self,
        from: &Mob,
        kind: &str,
        params: vise_client::mobs::conversion::Params,
    ) -> Option<u32> {
        let t = self.res.mob_types().get(kind).cloned()?;
        self.next_entity += 1;
        let mut mob = Mob::new(self.next_entity, t, from.position, from.yaw);
        vise_client::mobs::conversion::single(from, &mut mob, params);
        if matches!(from.species(), Species::Zombie | Species::Husk) {
            let at = [from.position.x, from.position.y, from.position.z];
            let special = self.local_difficulty(at).special();
            vise_client::mobs::finalize::zombie_attributes(
                &mut mob,
                special,
                vise_client::mobs::finalize::Reason::Conversion,
            );
        }
        let id = mob.id;
        for rider in self.mobs.iter_mut() {
            if rider.extra.mount == Some(from.id) {
                rider.extra.mount = Some(id);
            }
        }
        self.mobs.push(mob);
        Some(id)
    }
    pub(super) fn summon(&mut self, kind: &str, at: Option<[f64; 3]>, baby: bool) -> Result<u32> {
        let at = match at {
            Some(a) => a,
            None => {
                let p = self.player.as_ref().ok_or("no player yet")?;
                let d = view_vector(self.yaw, 0.0);
                [
                    p.position.x + d[0] * 3.0,
                    p.position.y,
                    p.position.z + d[2] * 3.0,
                ]
            }
        };
        let id = self
            .spawn_mob_spawned(kind, at)
            .ok_or_else(|| format!("unknown or unsupported mob {kind}"))?;
        if baby && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
            m.extra.age = if m.species() == Species::Sniffer {
                vise_client::mobs::sniffer::BABY_START_AGE
            } else {
                -24000
            };
        }
        self.spawn_jockeys(id, false);
        Ok(id)
    }
    pub(super) fn control_mobs(&self, radius: f64) -> serde_json::Value {
        let center = self.player.as_ref().map_or(Vec3::ZERO, |p| p.position);
        self.control_mobs_at(radius, center)
    }
    pub(super) fn control_mobs_at(&self, radius: f64, center: Vec3) -> serde_json::Value {
        let list: Vec<serde_json::Value> = self
            .mobs
            .iter()
            .filter(|m| m.distance_sqr(center) <= radius * radius)
            .map(|m| {
                let mut state = serde_json::json!({
                    "id": m.id,
                    "kind": m.kind.id,
                    "pos": [m.position.x, m.position.y, m.position.z],
                    "health": m.health,
                    "baby": m.is_baby(),
                    "target": m.ai.target.map(|t| format!("{t:?}")),
                    "goals": m.ai.goals.running_names(),
                    "held": m.extra.main_hand.as_deref(),
                    "mount": m.extra.mount,
                    "rider": m.extra.rider,
                    "ridden": m.rider.map(|r| r.steer),
                    "controlled": vise_client::mobs::equine::controlled(m),
                    "saddled": m.extra.saddled,
                    "enchants": vise_client::mobs::gear::gear(m).map(|g| g.slots.iter().map(|s| s.iter().map(|(e, l)| format!("{e}:{l}")).collect::<Vec<_>>()).collect::<Vec<_>>()),
                    "ints": m.extra.ints,
                    "pois": [m.extra.home, m.extra.bed, m.extra.meeting],
                    "profession": m.extra.variant2,
                    "targets": m.ai.targets.running_names(),
                    "path": m.ai.nav.path.as_ref().map(|p| p.nodes.len() - p.next.min(p.nodes.len())),
                    "on_ground": m.on_ground,
                    "variant": m.extra.variant,
                    "color": m.extra.color,
                    "sheared": m.extra.sheared,
                    "burning": m.fire_ticks > 0,
                    "size": m.extra.size,
                    "leash": m.extra.leash.map(|h| format!("{h:?}")),
                    "tame": m.extra.tame,
                    "sitting": m.extra.sitting,
                    "temper": m.extra.variant3,
                    "chested": m.extra.chested,
                    "head": m.extra.head.as_deref(),
                    "warden": vise_client::mobs::warden::describe(m),
                    "villager": vise_client::mobs::villager::ext(m).map(|e| serde_json::json!({
                        "food_level": e.food_level,
                        "inventory": e.inventory.iter().map(|s| format!("{}x{}", s.count, s.item)).collect::<Vec<_>>(),
                        "gossips": e.gossips.len(),
                        "last_slept": e.last_slept,
                        "raid": e.raid,
                    })),
                    "raider": vise_client::mobs::raider::state(m).map(|s| serde_json::json!({
                        "raid": s.raid_id,
                        "wave": s.wave,
                        "leader": s.patrol_leader,
                        "patrolling": s.patrolling,
                        "patrol_target": s.patrol_target,
                        "vehicle": m.extra.mount,
                    })),
                });
                state["passenger"] = m.extra.passenger.into();
                state["guardian_beam"] = serde_json::json!(vise_client::mobs::guardian::beam(m, 0.0));
                state
            })
            .collect();
        serde_json::json!({ "count": self.mobs.len(), "near": list, "projectiles": self.creatures.projectiles.len() })
    }
    pub(super) fn motion_top(&self, x: i32, z: i32) -> i32 {
        let top = if self.dim == Dim::Nether { 127 } else { 319 };
        for y in (self.world.bottom_y()..=top).rev() {
            let f = self.res.facts().get(self.world.block(x, y, z));
            if (f.full_collision || f.fluid != Fluid::None) && !f.name.ends_with("leaves") {
                return y + 1;
            }
        }
        self.world.bottom_y()
    }
    pub(super) fn mob_world_base<'s>(&'s self, others: &'s [Seen]) -> MobWorld<'s> {
        let (rain, thunder) = self.world.weather.levels(1.0);
        let weather = self.dim == Dim::Overworld;
        let player = self.player_view();
        MobWorld {
            world: &self.world,
            registry: self.pack.assets.registry,
            facts: self.res.facts(),
            fluid_tables: &self.motion.fluids,
            movement: &self.motion.blocks,
            collision: self.collision,
            data: self.res.mob_data(),
            nearest_player: player.as_ref().filter(|p| !p.spectator).map(|p| p.position),
            player,
            others,
            others_grid: None,
            others_index: None,
            daylight: self.dim != Dim::Nether && self.sky_darken() < 4,
            monsters_burn: self.dim == Dim::Overworld
                && !(12542..23460).contains(&self.world.day_time.rem_euclid(24000)),
            game_time: self.world.game_time,
            day_time: self.world.day_time,
            raining: weather && rain > 0.2,
            thundering: weather && thunder > 0.9,
            nether: self.dim == Dim::Nether,
            block_tags: &self.items().block_tags,
            biomes: self.res.biomes(),
            mob_griefing: self.ui.rules.bool("mob_griefing"),
            mob_drops: self.ui.rules.bool("mob_drops"),
            max_entity_cramming: self.ui.rules.int("max_entity_cramming"),
            end: self.dim == Dim::End,
            world_seed: self.level.seed,
            sea_level: if self.dim == Dim::Nether { 32 } else { 63 },
            teams: std::sync::Arc::new(self.mob_teams()),
            items: Some(self.items()),
            lid_openers: &self.lid_openers,
            difficulty: self.local_difficulty(
                self.level_players()
                    .first()
                    .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]),
            ),
        }
    }
    pub(super) fn share_spawn_conditions(&self) {
        if let Some(terrain) = self.end.locator() {
            terrain.set_spawn_conditions(self.difficulty_id() as u8, self.world.day_time);
        }
    }
    pub(super) fn local_difficulty(&self, at: [f64; 3]) -> spawning::LocalDifficulty {
        use vise_world::food::Difficulty;
        let base = match self.world.health_rules.difficulty {
            Difficulty::Peaceful => 0,
            Difficulty::Easy => 1,
            Difficulty::Normal => 2,
            Difficulty::Hard => 3,
        };
        let key = ((at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4);
        let (inhabited, moon) = if self.world.chunks.contains_key(&key) {
            let phase = self.world.day_time.div_euclid(24000).rem_euclid(8) as usize;
            (
                self.world.inhabited.get(&key).copied().unwrap_or(0),
                spawning::MOON_BRIGHTNESS[phase],
            )
        } else {
            (0, 0.0)
        };
        spawning::LocalDifficulty::new(base, self.world.day_time, inhabited, moon)
    }
    pub(super) fn spawn_fresh_chunks(&mut self) {
        let fresh: Vec<(i32, i32)> = self
            .world
            .chunks
            .iter()
            .filter(|(_, c)| c.fresh && c.lit)
            .map(|(k, _)| *k)
            .collect();
        for key in fresh {
            if let Some(c) = self.world.chunks.get_mut(&key) {
                Arc::make_mut(c).fresh = false;
            }
            self.spawn_generation_creatures(key.0, key.1);
        }
    }
    pub(super) fn tick_creatures(&mut self) -> Result<()> {
        self.attack_ticks = self.attack_ticks.saturating_add(1);
        self.tick_leads();
        self.spawn_natural();
        if self.ui.rules.bool("spawn_mobs") {
            self.tick_wandering_trader();
            self.tick_patrol_spawner();
            self.tick_cat_spawner();
            self.tick_village_siege();
        }
        self.tick_raids();
        self.tick_creature_entities()
    }
    pub(super) fn entity_ticking_area(&self) -> impl Fn(i32, i32) -> bool + use<> {
        let levels = self.tickets.levels();
        move |x, z| levels.entity_ticking_at(x, z)
    }
    pub(super) fn tick_creature_entities(&mut self) -> Result<()> {
        let killed: Vec<Mob> = self
            .mobs
            .iter()
            .filter(|m| !m.alive() && !m.removed && !self.died.contains(&m.id))
            .cloned()
            .collect();
        for m in killed {
            self.died.insert(m.id);
            self.mob_died(&m);
        }
        self.combat.charged_exploders.clear();
        self.combat.exploded.clear();
        let uuid = self.level.player_uuid;
        let mut snapshot: Vec<Seen> = self.mobs.iter().map(|m| seen(m, uuid)).collect();
        let mut events = Vec::new();
        let mut mobs = std::mem::take(&mut self.mobs);
        let mut dead = Vec::new();
        let mut corpses = Vec::new();
        let mut died = std::mem::take(&mut self.died);
        let ticking = self.entity_ticking_area();
        let guests = self.lan_guest_views();
        let players = self.level_players();
        let mut bindings = match self.lan.as_mut() {
            Some(host) if !guests.is_empty() => std::mem::take(&mut host.mob_players),
            _ => Default::default(),
        };
        let guest_ids: Vec<(u32, u32)> = match self.lan.as_ref() {
            Some(host) if !guests.is_empty() => host
                .hub
                .guests
                .iter()
                .map(|g| (g.conn, g.entity_id as u32))
                .collect(),
            _ => Vec::new(),
        };
        let mut guest_events = Vec::new();
        let mut gone = Vec::new();
        let level_random;
        {
            let mut scratch = self.world.random.clone();
            let mut pushes = vise_client::mobs::PendingPushes::default();
            let mut keep = Vec::with_capacity(mobs.len());
            let mut grid =
                vise_client::mobs::seen_grid::SeenGrid::new(snapshot.iter().map(|s| s.bounds));
            let base = self.mob_world_base(&[]);
            let mut index = vise_client::fast_hash::FxHashMap::<u32, u32>::default();
            for (i, s) in snapshot.iter().enumerate() {
                index.entry(s.id).or_insert(i as u32);
            }
            for i in 0..mobs.len() {
                let mut w = MobWorld {
                    others: &snapshot,
                    others_grid: Some(&grid),
                    others_index: Some(&index),
                    ..base.clone()
                };
                let m = &mut mobs[i];
                w.difficulty = self.local_difficulty([m.position.x, m.position.y, m.position.z]);
                w.nearest_player = super::natural_spawning::nearest_player(
                    &players,
                    m.position.x,
                    m.position.y,
                    m.position.z,
                )
                .map(|p| p.position);
                let seen_guest = if guests.is_empty() {
                    None
                } else {
                    let bound = bindings.get(&m.id).copied();
                    let conn = super::lan_mobs::player_for(m, w.player.as_ref(), &guests, bound);
                    match conn {
                        Some(conn) => {
                            bindings.insert(m.id, conn);
                            w.player = guests.iter().find(|g| g.0 == conn).map(|g| g.1.clone());
                        }
                        None => {
                            bindings.remove(&m.id);
                        }
                    }
                    conn
                };
                if !w
                    .world
                    .loaded(m.position.x.floor() as i32, m.position.z.floor() as i32)
                {
                    keep.push(!m.hostile() || m.persistent);
                    continue;
                }
                if !ticking(m.position.x.floor() as i32, m.position.z.floor() as i32) {
                    let kept = vise_client::mobs::ai::despawn_check(m, &w);
                    if !kept {
                        died.remove(&m.id);
                        snapshot[i].alive = false;
                    }
                    keep.push(kept);
                    continue;
                }
                vise_client::mobs::apply_mob_pushes(m, &mut pushes);
                let from = events.len();
                let kept = m.tick(&w, &mut scratch, &mut events);
                if !kept && !m.alive() && !m.removed && !m.extra.effects.is_empty() {
                    corpses.push(m.clone());
                }
                vise_client::mobs::take_mob_pushes(&mut events, from, &mut pushes);
                if !kept && !m.removed && !m.alive() && m.death_time >= 20 {
                    let (width, height) = (f64::from(m.width()), f64::from(m.height()));
                    gone.push((m.id, m.position, width, height));
                }
                if let Some(conn) = seen_guest {
                    if let Some(&(_, id)) = guest_ids.iter().find(|g| g.0 == conn) {
                        for e in &mut events[from..] {
                            if let MobEvent::Projectile(p) = e
                                && let ProjectileKind::ShulkerBullet { target, .. } = &mut p.kind
                                && *target == Who::Player
                            {
                                *target = Who::Mob(id);
                            }
                        }
                    }
                    let mut k = from;
                    while k < events.len() {
                        if super::lan_mobs::acts_on_player(&events[k]) {
                            guest_events.push((conn, events.remove(k)));
                        } else {
                            k += 1;
                        }
                    }
                }
                if !m.alive() && !m.removed && died.insert(m.id) {
                    dead.push(m.clone());
                }
                if !kept {
                    died.remove(&m.id);
                }
                keep.push(kept);
                snapshot[i] = seen(m, uuid);
                snapshot[i].alive &= kept;
                grid.update(i, snapshot[i].bounds);
                vise_client::mobs::apply_brain_events(&mut mobs, &mut events, from);
            }
            let mut flags = keep.into_iter();
            mobs.retain(|_| flags.next().unwrap_or(true));
            for m in &mut mobs {
                vise_client::mobs::apply_mob_pushes(m, &mut pushes);
            }
            level_random = scratch;
        }
        self.world.random = level_random;
        if let Some(host) = self.lan.as_mut()
            && !guests.is_empty()
        {
            let live: vise_client::fast_hash::FxHashSet<u32> =
                mobs.iter().map(|m| m.id).collect();
            bindings.retain(|id, _| live.contains(id));
            host.mob_players = bindings;
        }
        self.mobs.append(&mut mobs);
        self.died = died;
        for (id, at, width, height) in gone {
            let at = [at.x, at.y, at.z];
            self.lan_entity_event(id as i32, 60, at);
            self.death_poof(at, width, height);
        }
        self.sync_riders();
        self.tick_skeleton_traps();
        self.tick_apparition();
        self.guardian_bubbles();
        self.villager_reputation_tick();
        self.merchant_tick();
        self.villager_item_sense();
        self.villager_pickups();
        for m in dead {
            self.mob_died(&m);
        }
        for mut m in corpses {
            let names: Vec<&'static str> = m
                .extra
                .effects
                .iter()
                .map(|e| e.0.trim_start_matches("minecraft:"))
                .collect();
            let bounds = m.bounds();
            self.effects_on_death(&names, bounds, false, Some(m.id), Some(&mut m.random));
        }
        self.handle_mob_events(events)?;
        self.lan_guest_mob_events(guest_events);
        self.tick_projectiles()?;
        self.tick_fangs();
        self.tick_allays();
        self.tick_pandas();
        self.tick_breeze_effects();
        self.tick_sniffer_effects();
        self.piglin_pickups();
        self.sulfur_cube_pickups();
        self.sulfur_cube_power_priming();
        let mut queued = Vec::new();
        for m in &mut self.mobs {
            queued.append(&mut m.outbox);
        }
        self.handle_mob_events(queued)?;
        Ok(())
    }
    pub(super) fn death_poof(&mut self, at: [f64; 3], width: f64, height: f64) {
        let options = vise_client::particles::ParticleOptions::simple(
            vise_client::particles::ParticleType::Poof,
        );
        for _ in 0..20 {
            let r = self.particles.level_random();
            let xa = r.next_gaussian() * 0.02;
            let ya = r.next_gaussian() * 0.02;
            let za = r.next_gaussian() * 0.02;
            let x = at[0] + width * (2.0 * r.next_double() - 1.0) - xa * 10.0;
            let y = at[1] + height * r.next_double() - ya * 10.0;
            let z = at[2] + width * (2.0 * r.next_double() - 1.0) - za * 10.0;
            self.particles.add(&options, [x, y, z], [xa, ya, za]);
        }
    }
    fn mob_died(&mut self, m: &Mob) {
        if let Some(bed) = vise_client::mobs::villager::sleeping_pos(m) {
            self.wake_villager(m.id);
            self.set_bed_occupied(bed, false);
        }
        let killer = self.last_hurt_by_player.remove(&m.id);
        self.tell_owner_pet_died(m, killer);
        self.love_cause.remove(&m.id);
        let by_player = killer.is_some() || m.ai.last_hurt_by_player > 0;
        let mut dying = m.clone();
        self.sculk_mob_died(m);
        let mut catalyst_xp = 0;
        let consumed =
            self.catalyst_takes_experience([m.position.x, m.position.y, m.position.z], |_| {
                catalyst_xp = if vise_client::mobs::drops::drops_experience(&dying) {
                    vise_client::mobs::drops::base_experience(&mut dying).max(0) as u32
                } else {
                    0
                };
                catalyst_xp
            });
        if consumed && catalyst_xp > 0 {
            let who = match killer {
                Some(Some(conn)) => Some(super::commands::Target::Guest(conn)),
                Some(None) if self.lan_acting.is_none() => Some(super::commands::Target::Player),
                _ => None,
            };
            if let Some(who) = who {
                self.for_player(who, |g| {
                    g.trigger_player("minecraft:kill_mob_near_sculk_catalyst")
                });
            }
        }
        if by_player {
            let guest = killer.flatten().filter(|_| self.lan_acting.is_none());
            let credited = guest.is_some_and(|conn| {
                let Some(mut host) = self.lan.take() else {
                    return false;
                };
                let done = self
                    .as_guest(&mut host, conn, |g| g.credit_kill(m))
                    .is_some();
                self.lan = Some(host);
                done
            });
            if !credited {
                self.credit_kill(m);
            }
        }
        self.score_entity_removed(m.id);
        let slain_by_player = by_player && !killed_by_mob(m);
        self.raider_died(m, slain_by_player);
        self.villager_murdered(m, slain_by_player, killer.flatten());
        let pos = [m.position.x, m.position.y + 0.5, m.position.z];
        self.drop_death_loot(&mut dying, by_player, consumed, pos);
        self.create_wither_rose(m);
        if m.species() == Species::CopperGolem {
            self.golem_drop_equipment(&mut dying);
        }
        if m.species() == Species::SulfurCube {
            self.split_sulfur_cube(m);
        }
        if m.species() == Species::Slime || m.species() == Species::MagmaCube {
            let size = m.extra.size;
            if size > 1 {
                let n = 2 + self.world.random.next_int_bound(3).unwrap_or(0);
                for k in 0..n {
                    let off = |kk: i32| (f64::from(kk % 2) - 0.5) * f64::from(size) / 4.0;
                    let at = [
                        m.position.x + off(k),
                        m.position.y + 0.5,
                        m.position.z + off(k / 2),
                    ];
                    if let Some(id) = self.spawn_mob(&m.kind.id, at, m.persistent)
                        && let Some(c) = self.mobs.iter_mut().find(|c| c.id == id)
                    {
                        vise_client::mobs::cube::set_size(c, size / 2);
                    }
                }
            }
        }
    }
    fn credit_kill(&mut self, m: &Mob) {
        if !killed_by_mob(m) {
            self.stats.add("killed", &m.kind.id, 1);
        }
        self.stats.custom("mob_kills", 1);
        self.score_player_killed(super::commands::Target::Mob(m.id));
        let view = self.mob_entity(m);
        let blow = m.last_damage.map(|(d, _)| self.damage_view(d));
        self.trigger_killed(&view, blow.as_ref());
    }
    fn damage_view(&self, d: Damage) -> vise_client::advancements::DamageView {
        let source = match d.attacker {
            Some(Who::Player) => Some(self.player_entity()),
            Some(Who::Mob(id)) => self
                .mobs
                .iter()
                .find(|o| o.id == id)
                .map(|o| self.mob_entity(o)),
            None => None,
        };
        let projectile = match d.kind {
            DamageKind::Arrow => Some("minecraft:arrow"),
            DamageKind::Fireball => Some("minecraft:fireball"),
            _ => None,
        };
        let direct = match projectile {
            Some(kind) => Some(vise_client::advancements::EntityView {
                kind: kind.into(),
                position: d.from.unwrap_or_default(),
                ..Default::default()
            }),
            None => source.clone(),
        };
        vise_client::advancements::DamageView {
            kind: damage_type(d.kind, matches!(d.attacker, Some(Who::Player))).to_owned(),
            is_direct: projectile.is_none(),
            direct,
            source,
            ..Default::default()
        }
    }
    fn drop_death_loot(&mut self, m: &mut Mob, by_player: bool, consumed: bool, pos: [f64; 3]) {
        use vise_client::mobs::{DamageKind, Who, drops};
        use vise_world::loot::{LootDamage, LootEntity};
        let Some(loot) = vise_client::structure_blocks::loot_data(
            &self.options.resources,
            &self.pack_files(),
        ) else {
            return;
        };
        let player = || {
            let mut e = LootEntity::new("minecraft:player");
            if let Some(held) = self.held() {
                e.equipment.push(("mainhand".into(), to_loot(held)));
            }
            e
        };
        let damage = m.last_damage.map(|(d, _)| {
            let source = match d.attacker {
                Some(Who::Player) => Some(player()),
                Some(Who::Mob(id)) => self
                    .mobs
                    .iter()
                    .find(|o| o.id == id)
                    .map(|o| drops::loot_entity(o, None))
                    .or_else(|| {
                        self.combat
                            .charged_exploders
                            .contains(&id)
                            .then(|| LootEntity {
                                powered: true,
                                ..LootEntity::new("minecraft:creeper")
                            })
                    }),
                None => None,
            };
            let direct = match d.kind {
                DamageKind::Arrow => Some(LootEntity {
                    kind: "minecraft:arrow".into(),
                    ..LootEntity::default()
                }),
                DamageKind::Fireball => Some(LootEntity {
                    kind: "minecraft:fireball".into(),
                    ..LootEntity::default()
                }),
                _ => source.clone(),
            };
            let player_source = matches!(d.attacker, Some(Who::Player));
            LootDamage {
                kind: damage_type(d.kind, player_source).into(),
                source,
                direct,
            }
        });
        let vehicle = m
            .extra
            .mount
            .and_then(|id| self.mobs.iter().find(|o| o.id == id))
            .map(|o| LootEntity::new(&o.kind.id));
        let death = drops::Death {
            loot: &loot,
            damage,
            player: by_player.then(player),
            luck: 0.0,
            vehicle,
            mob_drops: self.ui.rules.bool("mob_drops"),
            world_seed: self.level.seed,
        };
        let (items, skulls) = drops::die(m, &death, &mut self.rand_sequences);
        if skulls && let Some(Who::Mob(id)) = m.last_damage.and_then(|(d, _)| d.attacker) {
            self.combat.charged_exploders.retain(|c| *c != id);
            if let Some(creeper) = self.mobs.iter_mut().find(|o| o.id == id) {
                creeper.extra.charged = false;
            }
        }
        let raider = vise_client::mobs::raider::is_raider(m.species());
        for d in items {
            match d {
                drops::Drop::Item(stack) => {
                    let item = if raider && stack.item == vise_client::mobs::raider::BANNER {
                        vise_client::mobs::raider::ominous_banner()
                    } else {
                        from_loot(&stack)
                    };
                    self.spawn_item(item, pos, dispenser::new_item_motion(), 10);
                }
                drops::Drop::CarriedBlock(state) => {
                    let stacks = vise_client::mobs::enderman::carried_block_drops(
                        world_items!(self),
                        self.pack.assets.registry,
                        state,
                        &mut self.world.random,
                        Some((&mut self.rand_sequences, self.level.seed)),
                    );
                    for item in stacks {
                        self.spawn_item(item, pos, dispenser::new_item_motion(), 10);
                    }
                }
                drops::Drop::Orb(value) if !consumed => self.spawn_orbs(pos, value as u32),
                drops::Drop::Orb(_) => {}
            }
        }
    }
    fn mob_hits_mob(&mut self, target: u32, amount: f32, source: Damage) -> bool {
        let mut killed = false;
        let mut hurt = false;
        let mut infested = false;
        if let Some(t) = self.mobs.iter_mut().find(|m| m.id == target) {
            let hit = t.damage(amount, source);
            hurt = hit;
            if hit {
                killed = !t.alive();
                let center = t.center();
                for event in std::mem::take(&mut t.outbox) {
                    match event {
                        MobEvent::Sound(s, _, volume, pitch) => {
                            self.audio
                                .play(&format!("minecraft:{s}"), Some(center), volume, pitch);
                        }
                        MobEvent::Infested { .. } => infested = true,
                        other => {
                            t.outbox.push(other);
                        }
                    }
                }
            }
        }
        if hurt {
            self.resolve_player_responsible(target, source);
        }
        if infested {
            self.mob_infested(target);
        }
        if hurt
            && let Some(Who::Mob(attacker)) = source.attacker
            && let Some(m) = self
                .mobs
                .iter()
                .find(|m| m.id == attacker && m.species() == Species::Axolotl)
        {
            self.audio.play(
                "minecraft:entity.axolotl.attack",
                Some([m.position.x, m.position.y, m.position.z]),
                1.0,
                1.0,
            );
        }
        if killed && let Some(Who::Mob(killer)) = source.attacker {
            self.zombie_killed(killer, target);
        }
        hurt
    }
    pub(super) fn resolve_player_responsible(&mut self, victim: u32, source: Damage) {
        let Some(Who::Mob(attacker)) = source.attacker else {
            return;
        };
        let Some(owner) = self
            .mobs
            .iter()
            .find(|m| m.id == attacker && m.species() == Species::Wolf && m.extra.tame)
            .map(|m| m.extra.owner)
        else {
            return;
        };
        let player = owner.and_then(|uuid| self.player_with_uuid(uuid));
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == victim) else {
            return;
        };
        m.ai.last_hurt_by_player = if owner.is_some() { 100 } else { 0 };
        match player {
            Some(who) => self.last_hurt_by_player.insert(victim, who),
            None => self.last_hurt_by_player.remove(&victim),
        };
    }
    fn player_with_uuid(&self, uuid: [i32; 4]) -> Option<Option<u32>> {
        if self.player.is_some()
            && vise_client::mobs::tame::owned_by(Some(uuid), self.acting_uuid())
        {
            return Some(self.lan_acting);
        }
        let uuid = vise_client::lan::playerdata::ints_uuid(uuid);
        self.lan
            .iter()
            .flat_map(|h| h.hub.guests.iter())
            .find(|g| g.placed && g.uuid == uuid)
            .map(|g| Some(g.conn))
    }
    pub(super) fn mob_infested(&mut self, mob: u32) {
        if let Some(m) = self.mobs.iter().find(|m| m.id == mob) {
            let at = [
                m.position.x,
                m.position.y + f64::from(m.height()) / 2.0,
                m.position.z,
            ];
            let look = m.view_vector();
            self.infested_hurt(at, look, Some(mob));
        }
    }
    fn drop_gift_loot(&mut self, mob: u32, table: &'static str, at: [f64; 3]) {
        use vise_client::mobs::drops;
        use vise_world::loot::EntityLootParams;
        let loot = vise_client::structure_blocks::loot_data(
            &self.options.resources,
            &self.pack_files(),
        );
        let this = self
            .mobs
            .iter()
            .find(|m| m.id == mob)
            .map(|m| drops::loot_entity(m, None));
        let stacks = match (loot, this) {
            (Some(loot), Some(this)) if loot.has_table(table) => {
                let params = EntityLootParams {
                    this,
                    ..EntityLootParams::default()
                };
                let seed = self.level.seed;
                match loot.random_sequence(table) {
                    Some(id) => {
                        let random = self.rand_sequences.get(id, seed);
                        loot.roll_entity(table, random, &params)
                            .iter()
                            .filter(|s| !s.is_empty())
                            .map(from_loot)
                            .collect()
                    }
                    None => Vec::new(),
                }
            }
            _ => self
                .res
                .items()
                .loot_table(table, false, None, &mut self.world.random),
        };
        for d in stacks {
            self.spawn_item(d, at, dispenser::new_item_motion(), 10);
        }
    }
    pub(super) fn add_projectile(&mut self, mut p: vise_client::projectiles::Projectile) {
        if p.id == 0 {
            self.next_entity += 1;
            p.id = self.next_entity;
        }
        self.creatures.projectiles.push(p);
    }
    pub(super) fn handle_mob_events(&mut self, events: Vec<MobEvent>) -> Result<()> {
        for e in events {
            match e {
                MobEvent::SquidCycle { mob, at } => self.lan_entity_event(mob as i32, 19, at),
                MobEvent::RabbitHop { mob, at } => self.lan_entity_event(mob as i32, 1, at),
                MobEvent::Drown { mob, at } => {
                    self.lan_entity_event(mob as i32, super::combat::DROWN_EVENT, at);
                    let v = self
                        .mobs
                        .iter()
                        .find(|m| m.id == mob)
                        .map_or([0.0; 3], |m| [m.velocity.x, m.velocity.y, m.velocity.z]);
                    self.drown_particles(at, v);
                }
                MobEvent::EntityEvent { mob, event, at } => {
                    self.lan_entity_event(mob as i32, event, at)
                }
                MobEvent::LevelParticles {
                    kind,
                    at,
                    spread,
                    speed,
                    count,
                } => {
                    if let Some(ty) = vise_client::particles::ParticleType::from_name(kind) {
                        let options = vise_client::particles::ParticleOptions::simple(ty);
                        self.particles
                            .send_particles(&options, at, spread, speed, count);
                        self.lan_level_particles(&options, at, spread, speed, count, None);
                    }
                }
                MobEvent::Sound(name, at, volume, pitch)
                | MobEvent::LocalSound(name, at, volume, pitch) => {
                    self.audio
                        .play(&format!("minecraft:{name}"), Some(at), volume, pitch)
                }
                MobEvent::LevelEventSound {
                    sound,
                    at,
                    volume,
                    spread,
                } => {
                    if sound == "entity.wither.spawn" {
                        let pos = at.map(|v| v.floor() as i32);
                        self.global_level_event(1023, pos, 0);
                    } else {
                        let mut r = vise_core::random::LegacyRandom::new(
                            self.world.game_time.wrapping_mul(341_873_128_712),
                        );
                        let pitch = (r.next_float() - r.next_float()) * spread + 1.0;
                        self.audio
                            .play(&format!("minecraft:{sound}"), Some(at), volume, pitch)
                    }
                }
                MobEvent::SoundId(name, at, volume, pitch) => {
                    self.audio.play(&name, Some(at), volume, pitch)
                }
                MobEvent::GameEvent { .. } => {}
                MobEvent::AttackPlayer(amount, from) => {
                    self.mob_hits_player(amount, from, "minecraft:mob_attack");
                }
                MobEvent::AttackPlayerWithEffect {
                    amount,
                    from,
                    effect,
                    ticks,
                    amplifier,
                } => {
                    if self.mob_hits_player(amount, from, "minecraft:mob_attack") {
                        self.give_player_effect(effect, ticks, amplifier);
                    }
                }
                MobEvent::DamagePlayer { amount, from, kind } => {
                    self.mob_hits_player(amount, from, kind);
                }
                MobEvent::ElderCurse { at } => self.elder_curse(at),
                MobEvent::Push { target, velocity } => {
                    let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
                    match target {
                        Who::Player => {
                            if let Some(p) = self.player.as_mut() {
                                p.velocity = p.velocity.add_vector(v);
                            }
                        }
                        Who::Mob(id) => {
                            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                                m.velocity = m.velocity.add_vector(v);
                            }
                        }
                    }
                }
                MobEvent::Knockback {
                    target,
                    power,
                    x,
                    z,
                } => match target {
                    Who::Player => self.knock_back_player(power, x, z),
                    Who::Mob(id) => {
                        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                            m.knockback(power, x, z);
                        }
                    }
                },
                MobEvent::Shift { target, delta } => {
                    let d = Vec3::new(delta[0], delta[1], delta[2]);
                    match target {
                        Who::Player => {
                            if let Some(p) = self.player.as_mut() {
                                p.position = p.position.add_vector(d);
                            }
                        }
                        Who::Mob(id) => {
                            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                                m.position = m.position.add_vector(d);
                            }
                        }
                    }
                }
                MobEvent::Wololo { mob } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.extra.color = 14;
                    }
                }
                MobEvent::Fangs {
                    at,
                    yaw,
                    delay,
                    owner,
                } => self.add_fangs(at, yaw, delay, owner),
                MobEvent::Shoot(..) => {}
                MobEvent::ExplosionSource { mob, charged, name } => {
                    self.combat.explosion_source = Some(mob);
                    self.combat.exploded.push((mob, name.clone()));
                    self.note_named_attacker("minecraft:explosion", mob, name);
                    if charged {
                        self.combat.charged_exploders.push(mob);
                    }
                }
                MobEvent::Explode(at, radius, species) => {
                    let noted = self.combat.explosion_source.is_some();
                    let source_mob = self.combat.explosion_source.or_else(|| {
                        self.mobs
                            .iter()
                            .filter(|m| {
                                (m.position.x - at[0]).powi(2) + (m.position.z - at[2]).powi(2)
                                    < 1.0
                            })
                            .map(|m| m.id)
                            .next()
                    });
                    if !noted && let Some(id) = source_mob {
                        self.note_attacker("minecraft:explosion", id);
                    }
                    self.combat.explosion_source = source_mob;
                    self.combat.explosion_source_named = noted;
                    let result = if matches!(species, Species::Creeper | Species::Wither) {
                        self.with_hive_emptying_blast(|g| g.explode(at, radius))
                    } else {
                        self.explode(at, radius)
                    };
                    self.combat.explosion_source = None;
                    self.combat.explosion_source_named = false;
                    result?
                }
                MobEvent::AttackMob {
                    target,
                    amount,
                    source,
                } => {
                    self.mob_hits_mob(target, amount, source);
                }
                MobEvent::CubeHurtMob {
                    cube,
                    target,
                    amount,
                    source,
                } => {
                    if self.mob_hits_mob(target, amount, source)
                        && let Some(m) = self.mobs.iter_mut().find(|m| m.id == cube)
                    {
                        let pitch = (m.random.next_float() - m.random.next_float()) * 0.2 + 1.0;
                        let at = m.center();
                        if !m.silent {
                            self.handle_mob_events(vec![MobEvent::Sound(
                                "entity.slime.attack",
                                at,
                                1.0,
                                pitch,
                            )])?;
                        }
                    }
                }
                MobEvent::SonicBoom {
                    target,
                    attacker,
                    from,
                    direction,
                } => {
                    let (hurt, resistance) = match target {
                        Who::Player => {
                            let hurt = self.mob_hits_player(10.0, from, "minecraft:sonic_boom");
                            let resistance = self.player.as_mut().map_or(0.0, |p| {
                                p.attribute("minecraft:knockback_resistance").unwrap_or(0.0)
                            });
                            (hurt, resistance)
                        }
                        Who::Mob(id) => {
                            let source = Damage {
                                kind: DamageKind::SonicBoom,
                                attacker: Some(Who::Mob(attacker)),
                                from: Some(from),
                            };
                            let hurt = self.mob_hits_mob(id, 10.0, source);
                            let resistance = self
                                .mobs
                                .iter()
                                .find(|m| m.id == id)
                                .map_or(0.0, |m| self.mob_knockback_resistance(m));
                            (hurt, resistance)
                        }
                    };
                    if hurt {
                        let horizontal = 2.5 * (1.0 - resistance);
                        let vertical = 0.5 * (1.0 - resistance);
                        let velocity = [
                            direction[0] * horizontal,
                            direction[1] * vertical,
                            direction[2] * horizontal,
                        ];
                        self.handle_mob_events(vec![MobEvent::Push { target, velocity }])?;
                    }
                }
                MobEvent::FinishAdmiring { mob } => self.finish_admiring(mob),
                MobEvent::Hiss { mob } => {
                    if let Some(m) = self.mobs.iter().find(|m| m.id == mob) {
                        let at = m.center();
                        let sound = vise_client::mobs::finalize::variant_sound(m, "hiss")
                            .unwrap_or("entity.cat.hiss");
                        self.audio
                            .play(&format!("minecraft:{sound}"), Some(at), 1.0, 1.0);
                    }
                }
                MobEvent::Tamed { .. } | MobEvent::Buck { .. } | MobEvent::StabDismount { .. } => {
                    self.mount_event(&e)
                }
                MobEvent::Launch { target, up } => match target {
                    Who::Player => {
                        if let Some(p) = self.player.as_mut() {
                            p.velocity.y += up;
                        }
                    }
                    Who::Mob(id) => {
                        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                            m.velocity.y += up;
                        }
                    }
                },
                MobEvent::Bonemeal { pos } => {
                    let at = BlockPos::new(pos[0], pos[1], pos[2]);
                    if self.with_level(|b, level, ticks| b.grow_crop(level, ticks, at)) {
                        self.level_events
                            .push(vise_world::level::LevelEvent::LevelEvent {
                                id: 1505,
                                pos: at,
                                data: 15,
                            });
                        let _ = self.process_level_events();
                    }
                }
                MobEvent::RingBell { bell } => {
                    let pos = BlockPos::new(bell[0], bell[1], bell[2]);
                    let rang = self
                        .with_level(|b, level, ticks| b.bell_attempt_to_ring(level, ticks, pos));
                    if rang {
                        let _ = self.process_level_events();
                    }
                }
                MobEvent::ChestOpen { chest, open } => self.golem_chest_open(chest, open),
                MobEvent::ChestSlot { chest, slot, stack } => {
                    self.golem_chest_slot(chest, slot, stack)
                }
                MobEvent::BecomeStatue { mob } => self.golem_become_statue(mob),
                MobEvent::EnterHive { mob, hive } => self.bee_enter_hive(mob, hive),
                MobEvent::ParrotToShoulder { mob } => self.parrot_to_shoulder(mob),
                MobEvent::Infested { mob } => self.mob_infested(mob),
                MobEvent::Gift { target, item } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == target)
                        && m.saddle_item().is_none()
                    {
                        m.set_saddle(Some(ItemStack::new(item, 1)));
                    }
                }
                MobEvent::HurtSelf { mob, kind, amount } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.damage(amount, vise_client::mobs::Damage::of(kind));
                    }
                }
                MobEvent::SetTarget { mob, target } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.ai.target = Some(target);
                    }
                }
                MobEvent::HuntHoglin { mob, target, ticks } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.extra.ints[0] = ticks;
                        if m.ai.target.is_none() {
                            m.ai.target = Some(Who::Mob(target));
                        }
                    }
                }
                MobEvent::Projectile(mut p) => {
                    self.next_entity += 1;
                    p.id = self.next_entity;
                    let (owner, at) = (p.owner, [p.position.x, p.position.y, p.position.z]);
                    if p.kind == ProjectileKind::LlamaSpit {
                        let v = p.velocity;
                        self.particles.llama_spit_burst(at, [v.x, v.y, v.z]);
                    }
                    self.add_projectile(p);
                    self.projectile_event("minecraft:projectile_shoot", owner, at);
                }
                MobEvent::Spawn(mut m) => {
                    self.next_entity += 1;
                    m.id = self.next_entity;
                    self.mobs.push(*m);
                }
                MobEvent::SpawnKind { kind, at, setup } => {
                    if let Some(id) = self.spawn_mob(&kind, at, setup.persistent) {
                        self.apply_setup(id, &setup);
                    }
                }
                MobEvent::Convert { from, kind, params } => {
                    self.convert_to(&from, kind, params);
                }
                MobEvent::DropItem { item, count, at } => self.spawn_item(
                    ItemStack {
                        item,
                        count,
                        damage: 0,
                        extra: None,
                    },
                    at,
                    dispenser::new_item_motion(),
                    10,
                ),
                MobEvent::DropGift { table, at } => {
                    let stacks =
                        self.res
                            .items()
                            .loot_table(table, false, None, &mut self.world.random);
                    for stack in stacks {
                        self.spawn_item(stack, at, dispenser::new_item_motion(), 10);
                    }
                }
                MobEvent::SetBlock { pos, state } => self.level_set(pos, state)?,
                MobEvent::BedOccupied { pos, occupied } => self.set_bed_occupied(pos, occupied),
                MobEvent::SetDoorOpen { pos, open } => {
                    let pos = BlockPos::new(pos[0], pos[1], pos[2]);
                    self.with_level(|b, level, ticks| b.set_door_open(level, ticks, pos, open));
                    self.process_level_events()?;
                }
                MobEvent::BreakDoor { pos } => {
                    let air = self.world.air;
                    let at = BlockPos::new(pos[0], pos[1], pos[2]);
                    self.with_level(|b, level, ticks| b.set_block(level, ticks, at, air, 3));
                    self.process_level_events()?;
                    self.level_event_at(1021, pos, 0);
                    let left = self.world.block(pos[0], pos[1], pos[2]);
                    self.level_event_at(2001, pos, left.0 as i32);
                }
                MobEvent::DestroyProgress { mob, pos, progress } => {
                    self.destroy_block_progress(mob as i32, pos, progress);
                    if let Some(host) = self.lan.as_ref() {
                        host.hub.broadcast_near(
                            [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])],
                            64.0,
                            &vise_protocol::packets::play::Clientbound::BlockDestruction(
                                vise_protocol::packets::play::BlockDestruction {
                                    id: mob as i32,
                                    pos,
                                    progress: if (0..10).contains(&progress) {
                                        progress as u8
                                    } else {
                                        255
                                    },
                                },
                            ),
                            None,
                        );
                    }
                }
                MobEvent::LevelEvent { id, pos, data } => self.level_event_at(id, pos, data),
                MobEvent::SetBlockName { pos, name, effect } => {
                    if effect {
                        let old = self.world.block(pos[0], pos[1], pos[2]);
                        self.destroy_particles(pos, old);
                        let s = self.res.facts().get(old).sounds.break_sound.clone();
                        self.audio.play(
                            &s,
                            Some([
                                f64::from(pos[0]) + 0.5,
                                f64::from(pos[1]) + 0.5,
                                f64::from(pos[2]) + 0.5,
                            ]),
                            1.0,
                            0.8,
                        );
                    }
                    if let Some(state) = self.pack.assets.registry.default_state(name) {
                        self.world.set(pos[0], pos[1], pos[2], state)?;
                    }
                }
                MobEvent::Infest {
                    pos,
                    infested,
                    poof,
                } => {
                    let registry = &self.pack.assets.registry;
                    let old = self.world.block(pos[0], pos[1], pos[2]);
                    if let Some(mut state) = registry.state(old).cloned() {
                        let (from, to) = if infested {
                            ("minecraft:", "minecraft:infested_")
                        } else {
                            ("minecraft:infested_", "minecraft:")
                        };
                        state.name = state.name.replacen(from, to, 1);
                        if let Ok(id) = registry.resolve(&state) {
                            self.world.set(pos[0], pos[1], pos[2], id)?;
                        }
                    }
                    if let Some(mob) = poof {
                        self.mob_poof(mob);
                    }
                }
                MobEvent::BreakBlock { pos, drop } => {
                    let old = self.world.block(pos[0], pos[1], pos[2]);
                    if drop
                        && self
                            .res
                            .facts()
                            .get(old)
                            .name
                            .starts_with("minecraft:infested_")
                        && self.ui.rules.bool("block_drops")
                    {
                        let at = [
                            f64::from(pos[0]) + 0.5,
                            f64::from(pos[1]),
                            f64::from(pos[2]) + 0.5,
                        ];
                        if let Some(id) = self.spawn_mob("minecraft:silverfish", at, false) {
                            self.mob_poof(id);
                        }
                    }
                    if !self.res.facts().get(old).has_class("BaseFireBlock") {
                        self.destroy_particles(pos, old);
                    }
                    if drop {
                        let drops = world_items!(self).block_drops_in(
                            self.pack.assets.registry,
                            old,
                            None,
                            &mut self.world.random,
                            Some((&mut self.rand_sequences, self.level.seed)),
                        );
                        for d in drops {
                            self.pop_resource(pos, d);
                        }
                    }
                    let b = self.res.behaviours();
                    let left = b.legacy_block(b.fluid(old));
                    if left != old {
                        self.level_set(pos, left)?;
                    }
                }
                MobEvent::Particles { kind, at, count } => {
                    if let Some(ty) = vise_client::particles::ParticleType::from_name(kind) {
                        if kind.starts_with("item_") {
                            self.ensure_item_particle_sprites();
                        }
                        let options = vise_client::particles::ParticleOptions::simple(ty);
                        if ty == vise_client::particles::ParticleType::SonicBoom {
                            self.lan_sonic_boom(&options, at);
                        }
                        if count == 0 {
                            self.particles.add(&options, at, [0.0; 3]);
                        }
                        for _ in 0..count {
                            let r = &mut self.world.random;
                            let p = [
                                at[0] + (f64::from(r.next_float()) - 0.5),
                                at[1] + (f64::from(r.next_float()) - 0.5) * 0.5,
                                at[2] + (f64::from(r.next_float()) - 0.5),
                            ];
                            let v = [
                                r.next_gaussian() * 0.02,
                                r.next_gaussian() * 0.02,
                                r.next_gaussian() * 0.02,
                            ];
                            self.particles.add(&options, p, v);
                        }
                    }
                }
                MobEvent::AroundSelf {
                    mob,
                    event,
                    kind,
                    feet,
                    width,
                    height,
                    burst,
                } => {
                    self.lan_entity_event(mob as i32, event, feet);
                    self.particles_around(kind, feet, [width, height], burst)
                }
                MobEvent::ParticleAt { kind, at, velocity } => {
                    if let Some(ty) = vise_client::particles::ParticleType::from_name(kind) {
                        let options = vise_client::particles::ParticleOptions::simple(ty);
                        self.particles.add(&options, at, velocity);
                    }
                }
                MobEvent::Experience { at, amount } => self.spawn_breeding_orb(at, amount),
                MobEvent::AxolotlSupportPlayer { mob } => {
                    let previous = self
                        .potions
                        .effects
                        .get("regeneration")
                        .map_or(0, |e| e.duration);
                    if (0..=2399).contains(&previous) {
                        self.potions
                            .effects
                            .add(vise_client::effects::EffectInstance::new(
                                "regeneration",
                                (previous + 100).min(2400),
                                0,
                            ));
                    }
                    self.potions
                        .effects
                        .list
                        .retain(|e| e.id != "mining_fatigue");
                    let source = self
                        .mobs
                        .iter()
                        .find(|m| m.id == mob)
                        .map(|m| self.mob_entity(m));
                    self.trigger_effects_changed(source);
                }
                MobEvent::DolphinTreasure { mob } => {
                    let Some(at) = self.mobs.iter().find(|m| m.id == mob).map(|m| {
                        [
                            m.position.x.floor() as i32,
                            m.position.y.floor() as i32,
                            m.position.z.floor() as i32,
                        ]
                    }) else {
                        continue;
                    };
                    let names = self.structure_tag("minecraft:dolphin_located");
                    let found = self
                        .end
                        .locator()
                        .filter(|_| !names.is_empty())
                        .and_then(|t| t.locate(&names, at, 50));
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.extra.home = found.map(|(_, p)| p);
                    }
                }
                MobEvent::EffectPlayer {
                    effect,
                    ticks,
                    amplifier,
                } => {
                    self.give_player_effect(effect, ticks, amplifier);
                }
                MobEvent::Ignite { target, seconds } => match target {
                    Who::Player => {
                        if !self.creative
                            && let Some(p) = self.player.as_mut()
                        {
                            p.ignite_for_ticks(seconds * 20)?;
                        }
                    }
                    Who::Mob(id) => {
                        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                            m.ignite_for_ticks(seconds * 20);
                        }
                    }
                },
                MobEvent::Breed {
                    parent,
                    partner,
                    llama,
                } => self.breed(parent, partner, llama),
                MobEvent::LockGaze { mob, partner } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        vise_client::mobs::animal_brain::lock_gaze(m, partner);
                    }
                }
                MobEvent::BreedTarget { mob, partner } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        vise_client::mobs::animal_brain::set_breed_partner(m, partner);
                    }
                }
                MobEvent::Retreat { mob, from, at } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        vise_client::mobs::piglin::receive_retreat(m, from, at);
                    }
                }
                MobEvent::StartRiding { rider, vehicle } => {
                    vise_client::mobs::riders::start_riding(&mut self.mobs, rider, vehicle);
                }
                MobEvent::StartFollowing { .. } | MobEvent::SchoolSize { .. } => {}
                MobEvent::LeashBreak { mob, drop, sound } => self.break_leash(mob, drop, sound),
                MobEvent::Vibration { from, to, ticks } => {
                    self.mob_vibration_particle(from, to, ticks)
                }
                MobEvent::DigParticles { at } => self.warden_dig_particles(at),
                MobEvent::BlockBreakEffect { pos, block } => {
                    let state = self.world.block(block[0], block[1], block[2]);
                    self.destroy_particles(pos, state);
                }
                MobEvent::Darkness { at, radius } => self.apply_darkness(at, radius),
                MobEvent::CreakingHurt {
                    home,
                    by_player,
                    id,
                } => self.creaking_hurt(home, id, by_player),
                MobEvent::CreakingTearDown { center, spread } => {
                    self.creaking_tear_down(center, spread)
                }
                MobEvent::CureZombieVillager { mob } => self.cure_zombie_villager(mob),
                MobEvent::Reinforcements { mob, target } => self.call_reinforcements(mob, target),
                MobEvent::VillagerMeet { mob, other } => self.villager_meet(mob, other),
                MobEvent::VillagerBirth {
                    parent,
                    partner,
                    child,
                } => self.villager_birth(parent, partner, child),
                MobEvent::WorkAtComposter { mob, pos } => self.villager_work_at_composter(mob, pos),
                MobEvent::GolemCheck { mob, needed } => self.villager_golem_check(mob, needed),
                MobEvent::Firework { at, color, flight } => {
                    self.villager_firework(at, color, flight)
                }
                MobEvent::ThrowGift { table, from, to } => self.throw_gift(&table, from, to),
                MobEvent::PatrolTarget { mob, target } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        vise_client::mobs::raider::set_patrol_target(m, target);
                    }
                }
                MobEvent::RaiderTarget {
                    mob,
                    target,
                    aggressive,
                } => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob) {
                        m.ai.target = Some(target);
                        m.ai.aggressive |= aggressive;
                    }
                }
                MobEvent::PandaSneezed { mob } => self.panda_sneezed(mob),
                MobEvent::FallToPassengers {
                    mob,
                    distance,
                    modifier,
                } => self.fall_to_passengers(mob, distance, modifier),
                MobEvent::ShedScute { mob, at, pitch } => {
                    let table = "minecraft:gameplay/armadillo_shed";
                    let drops = self.sequence_loot(table, table, None);
                    for d in drops {
                        self.spawn_at_location(Vec3::new(at[0], at[1], at[2]), 0.0, d);
                    }
                    self.audio.play(
                        "minecraft:entity.armadillo.scute_drop",
                        Some(at),
                        1.0,
                        pitch,
                    );
                    let ctx = self.who_context(Some(Who::Mob(mob)), None);
                    self.post_game_event("minecraft:entity_place", at, ctx);
                }
                MobEvent::GiftLoot { mob, table, at } => {
                    if matches!(
                        table,
                        "minecraft:gameplay/turtle_grow" | "minecraft:gameplay/panda_sneeze"
                    ) && !self.ui.rules.bool("mob_drops")
                    {
                        continue;
                    }
                    self.drop_gift_loot(mob, table, at);
                }
            }
        }
        Ok(())
    }
    pub(super) fn hatch_turtles(
        &mut self,
        pos: [i32; 3],
        state: vise_world::registry::BlockStateId,
    ) {
        let eggs = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("eggs"))
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(1);
        for i in 0..eggs {
            let at = [
                f64::from(pos[0]) + 0.3 + f64::from(i) * 0.2,
                f64::from(pos[1]),
                f64::from(pos[2]) + 0.3,
            ];
            if let Some(id) = self.spawn_mob("minecraft:turtle", at, true)
                && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
            {
                m.extra.age = -24000;
                m.extra.home = Some(pos);
                m.yaw = 0.0;
                m.body_yaw = 0.0;
                m.head_yaw = 0.0;
            }
        }
    }
    fn apply_setup(&mut self, id: u32, setup: &SpawnSetup) {
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) else {
            return;
        };
        if setup.baby {
            m.extra.age = -24000;
        }
        if setup.nausea {
            m.add_effect("minecraft:nausea", 200, 0);
        }
        if setup.size > 0 {
            m.extra.size = setup.size;
        }
        m.yaw = setup.yaw;
        m.body_yaw = setup.yaw;
        m.head_yaw = setup.yaw;
        m.velocity = Vec3::new(setup.velocity[0], setup.velocity[1], setup.velocity[2]);
        if let Some(h) = setup.health {
            m.health = h;
        }
        if let Some(v) = setup.variant {
            m.extra.variant = v;
        }
        if let Some(owner) = setup.owner_mob {
            m.extra.counter2 = owner as i32;
        }
        if let Some(home) = setup.home {
            m.extra.home = Some(home);
        }
        if let Some(life) = setup.life {
            m.extra.egg_time = life;
        }
    }
    fn spawn_breeding_orb(&mut self, at: [f64; 3], amount: u32) {
        if self.ui.rules.bool("mob_drops") {
            self.spawn_orbs(at, amount);
        }
    }
    fn dye_mix(&mut self, a: u8, b: u8) -> u8 {
        if let Some(mixed) = self.dye_color_mix_in_recipes(a, b) {
            return mixed;
        }
        if self.world.random.next_boolean() {
            a
        } else {
            b
        }
    }
    fn dye_color_mix_in_recipes(&self, a: u8, b: u8) -> Option<u8> {
        let dye = |c: u8| {
            let name = vise_client::mobs::render::DYE_NAMES[usize::from(c) & 15];
            Some(ItemStack::new(&format!("minecraft:{name}_dye"), 1))
        };
        let grid = [dye(a), dye(b), None, None];
        let (_, result) = self.res.recipes().find(&grid, 2)?;
        let name = result
            .item
            .strip_prefix("minecraft:")?
            .strip_suffix("_dye")?;
        let index = vise_client::mobs::render::DYE_NAMES
            .iter()
            .position(|n| *n == name)?;
        u8::try_from(index).ok()
    }
    pub(super) fn tell_neutral_mobs_player_died(
        &mut self,
        feet: [f64; 3],
        conn: Option<u32>,
        bindings: &std::collections::HashMap<u32, u32>,
    ) {
        if self.remote.is_some() || !self.ui.rules.bool("forgive_dead_players") {
            return;
        }
        let block = feet.map(f64::floor);
        let area = Aabb::new(
            Vec3::new(block[0], block[1], block[2]),
            Vec3::new(block[0] + 1.0, block[1] + 1.0, block[2] + 1.0),
        )
        .inflate(Vec3::new(32.0, 10.0, 32.0));
        for m in self.mobs.iter_mut() {
            if bindings.get(&m.id).copied() == conn && m.bounds().intersects(area) {
                vise_client::mobs::ai::forgive_dead_player(m);
            }
        }
    }
    fn breed(
        &mut self,
        parent: u32,
        partner: u32,
        llama: Option<Box<vise_client::mobs::llama::Offspring>>,
    ) {
        let Some(p) = self.mobs.iter().find(|m| m.id == parent).cloned() else {
            return;
        };
        if let Some(cause) = breeding_credit(&mut self.love_cause, parent, partner) {
            let parent = self.mob_entity(&p);
            let partner_view = self
                .mobs
                .iter()
                .find(|m| m.id == partner)
                .map(|m| self.mob_entity(m))
                .unwrap_or_else(|| parent.clone());
            let mut child = parent.clone();
            child.baby = true;
            let turtle = p.species() == Species::Turtle;
            if matches!(
                (
                    p.species(),
                    self.mobs
                        .iter()
                        .find(|m| m.id == partner)
                        .map(|m| m.species())
                ),
                (Species::Horse, Some(Species::Donkey)) | (Species::Donkey, Some(Species::Horse))
            ) {
                child.kind = "minecraft:mule".into();
            }
            let who = cause.map_or(
                super::commands::Target::Player,
                super::commands::Target::Guest,
            );
            self.for_player(who, |g| {
                g.stats.custom("animals_bred", 1);
                g.trigger_bred(&parent, &partner_view, (!turtle).then_some(&child));
            });
        }
        if let Some(q) = self.mobs.iter_mut().find(|m| m.id == partner) {
            q.extra.age = 6000;
            q.extra.in_love = 0;
        }
        let q = self.mobs.iter().find(|m| m.id == partner).cloned();
        self.mob_event_particles(parent, 18);
        if p.species() == Species::Sniffer {
            self.lay_sniffer_egg(&p);
            return;
        }
        if p.species() == Species::Turtle {
            if self.ui.rules.bool("mob_drops")
                && let Some(m) = self.mobs.iter_mut().find(|m| m.id == parent)
            {
                let xp = 1 + m.random.next_int_bound(7).unwrap_or(0) as u32;
                self.add_orb([p.position.x, p.position.y, p.position.z], xp);
            }
            return;
        }
        let coin = p.species() != Species::Mooshroom
            && self.world.random.next_int_bound(2).unwrap_or(0) == 0;
        let axolotl = if p.species() == Species::Axolotl {
            let other = q.as_ref().map_or(p.extra.variant, |q| q.extra.variant);
            self.mobs.iter_mut().find(|m| m.id == parent).map(|m| {
                if m.random.next_int_bound(1200).unwrap_or(1) == 0 {
                    let _ = m.random.next_int_bound(1);
                    4
                } else if m.random.next_boolean() {
                    p.extra.variant
                } else {
                    other
                }
            })
        } else {
            None
        };
        let mooshroom = if p.species() == Species::Mooshroom {
            let mate = q.as_ref().map_or(p.extra.variant, |q| q.extra.variant);
            let own = p.extra.variant;
            self.mobs.iter_mut().find(|m| m.id == parent).map(|m| {
                if own == mate && m.random.next_int_bound(1024).unwrap_or(1) == 0 {
                    if own == 1 { 0 } else { 1 }
                } else if m.random.next_boolean() {
                    own
                } else {
                    mate
                }
            })
        } else {
            None
        };
        let kind = match (p.species(), q.as_ref().map(|q| q.species())) {
            (Species::Horse, Some(Species::Donkey)) | (Species::Donkey, Some(Species::Horse)) => {
                "minecraft:mule".to_owned()
            }
            _ => p.kind.id.clone(),
        };
        if let Some(id) = self.spawn_mob(&kind, [p.position.x, p.position.y, p.position.z], true)
            && let Some(child) = self.mobs.iter_mut().find(|m| m.id == id)
        {
            child.extra.age = -24000;
            spawning::bred_offspring(child);
            child.extra.sound_variant = None;
            if p.species() == Species::Fox {
                child.extra.main_hand = None;
            }
            if p.species() == Species::Goat {
                child.extra.lost_horns = [false; 2];
                vise_client::mobs::finalize::goat_init_memories(child, &mut self.world.random);
                let pick_parent = self.world.random.next_boolean();
                let parent_screams = if pick_parent {
                    p.extra.screaming
                } else {
                    q.as_ref().unwrap_or(&p).extra.screaming
                };
                child.extra.screaming = parent_screams || self.world.random.next_double() < 0.02;
            }
            if let Some(o) = llama {
                vise_client::mobs::llama::apply_offspring(child, &o);
                return;
            }
            if vise_client::mobs::equine::is_equine(p.species()) {
                let other = q.clone().unwrap_or_else(|| p.clone());
                let mut r = self.world.random.clone();
                vise_client::mobs::equine::inherit(child, &p, &other, &mut r);
                if vise_client::mobs::llama::is_llama(p.species()) {
                    vise_client::mobs::llama::offspring(child, &p, &other, &mut r);
                }
                if p.species() == Species::Horse && other.species() == Species::Horse {
                    let c = r.next_int_bound(9).unwrap_or(0);
                    child.extra.variant = if c < 4 {
                        p.extra.variant
                    } else if c < 8 {
                        other.extra.variant
                    } else {
                        r.next_int_bound(7).unwrap_or(0) as u8
                    };
                    let mk = r.next_int_bound(5).unwrap_or(0);
                    child.extra.variant2 = if mk < 2 {
                        p.extra.variant2
                    } else if mk < 4 {
                        other.extra.variant2
                    } else {
                        r.next_int_bound(5).unwrap_or(0) as u8
                    };
                }
                self.world.random = r;
                let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
                self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
                return;
            }
            if p.species() == Species::Cat {
                let other = q.as_ref().unwrap_or(&p);
                child.extra.variant = if coin {
                    p.extra.variant
                } else {
                    other.extra.variant
                };
                if p.extra.tame {
                    child.extra.tame = true;
                    child.extra.owner = p.extra.owner;
                    let mixed = self.dye_mix(p.extra.color, other.extra.color);
                    if let Some(c) = self.mobs.iter_mut().find(|m| m.id == id) {
                        c.extra.color = mixed;
                    }
                }
                let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
                self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
                return;
            }
            if p.species() == Species::Wolf {
                let other = q.as_ref().unwrap_or(&p);
                child.extra.variant = if coin {
                    p.extra.variant
                } else {
                    other.extra.variant
                };
                if p.extra.tame {
                    vise_client::mobs::tame::set_tame(child, true);
                    child.extra.owner = p.extra.owner;
                    let mixed = self.dye_mix(p.extra.color, other.extra.color);
                    if let Some(c) = self.mobs.iter_mut().find(|m| m.id == id) {
                        c.extra.color = mixed;
                    }
                }
                let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
                self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
                return;
            }
            if p.species() == Species::Panda {
                vise_client::mobs::panda::inherit(child, &p, q.as_ref().unwrap_or(&p));
                let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
                self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
                return;
            }
            if let Some(variant) = axolotl {
                child.extra.variant = variant;
                child.persistent = true;
                let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
                self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
                return;
            }
            if p.species() == Species::Nautilus && p.extra.tame {
                child.extra.tame = true;
                child.extra.owner = p.extra.owner;
                child.persistent = true;
            }
            child.extra.variant = p.extra.variant;
            if matches!(
                p.species(),
                Species::Fox | Species::Pig | Species::Cow | Species::Chicken
            ) && !coin
            {
                child.extra.variant = q.as_ref().map_or(p.extra.variant, |q| q.extra.variant);
            }
            if let Some(variant) = mooshroom {
                child.extra.variant = variant;
            }
            if p.species() == Species::Sheep {
                let other = q.as_ref().unwrap_or(&p);
                let mixed = self.dye_mix(p.extra.color, other.extra.color);
                if let Some(c) = self.mobs.iter_mut().find(|m| m.id == id) {
                    c.extra.color = mixed;
                }
            }
        }
        let xp = 1 + self.world.random.next_int_bound(7).unwrap_or(0) as u32;
        self.spawn_breeding_orb([p.position.x, p.position.y, p.position.z], xp);
    }
    fn elder_curse(&mut self, at: [f64; 3]) {
        let mut cursed = Vec::new();
        self.for_each_player(|g| {
            if g.elder_curse_player(at) {
                cursed.push(g.lan_acting);
            }
        });
        if let Some(host) = self.lan.as_ref() {
            for conn in cursed.into_iter().flatten() {
                host.hub.send_to(
                    conn,
                    &vise_protocol::packets::play::Clientbound::GameEvent(
                        vise_protocol::packets::play::GameEvent {
                            event: vise_client::lan::hub::game_event::GUARDIAN_ELDER_EFFECT,
                            param: 1.0,
                        },
                    ),
                );
            }
        }
    }
    fn elder_curse_player(&mut self, at: [f64; 3]) -> bool {
        if self.creative || self.spectator() {
            return false;
        }
        let Some(p) = self.player.as_ref() else {
            return false;
        };
        let d = p.position.subtract(Vec3::new(at[0], at[1], at[2]));
        if d.x * d.x + d.y * d.y + d.z * d.z >= 2500.0 || p.health.dead_or_dying() {
            return false;
        }
        let active = self.potions.effects.get("mining_fatigue");
        if active.is_some_and(|e| e.amplifier >= 2 && !e.ends_within(1199)) {
            return false;
        }
        let at = [p.position.x, p.position.y, p.position.z];
        self.give_player_effect("minecraft:mining_fatigue", 6000, 2);
        if self.lan_acting.is_none() {
            self.audio
                .play("minecraft:entity.elder_guardian.curse", Some(at), 1.0, 1.0);
            self.creatures.elder_apparition = super::mob_blocks::APPARITION_TICKS;
        }
        true
    }
    pub(super) fn mob_hits_player_kind(&mut self, amount: f32, from: [f64; 3], kind: &str) {
        self.mob_hits_player(amount, from, kind);
    }
    pub(super) fn mob_hits_player_caused(
        &mut self,
        amount: f32,
        from: [f64; 3],
        kind: &str,
        cause: Option<u32>,
    ) {
        self.hurt_player_from(amount, Some(from), kind, Some(cause));
    }
    pub(super) fn mob_hits_player(
        &mut self,
        amount: f32,
        from: [f64; 3],
        damage_type: &str,
    ) -> bool {
        self.hurt_player_from(amount, Some(from), damage_type, None)
    }
    pub(super) fn hurt_player_from(
        &mut self,
        amount: f32,
        from: Option<[f64; 3]>,
        damage_type: &str,
        known: Option<Option<u32>>,
    ) -> bool {
        if self.creative || self.combat.refuse_player_hit {
            return false;
        }
        let projectile = self
            .res
            .stations()
            .enchantments
            .damage_tags
            .get("minecraft:is_projectile")
            .is_some_and(|t| t.contains(damage_type));
        let blocking = self.player_blocking(from, damage_type, false);
        let raised = blocking.is_some();
        let attacker = match known {
            Some(known) => known,
            None => from
                .filter(|_| !projectile && damage_type != "minecraft:player_attack")
                .and_then(|from| {
                    self.mobs
                        .iter()
                        .filter(|m| m.alive())
                        .min_by(|a, b| {
                            let d = |m: &vise_client::mobs::Mob| {
                                (m.position.x - from[0]).powi(2) + (m.position.z - from[2]).powi(2)
                            };
                            d(a).total_cmp(&d(b))
                        })
                        .map(|m| m.id)
                }),
        };
        let owner = self.combat.projectile_owner.take();
        if let Some(a) = attacker.or(owner) {
            self.note_attacker(damage_type, a);
        }
        if let Some(name) = self.combat.projectile_direct.take() {
            self.note_direct_entity(damage_type, name);
        }
        let living_cause = attacker.or(owner).is_some();
        let mut outcome = vise_client::health::BlockOutcome::default();
        if let Some(player) = self.player.as_mut()
            && let Some(id) = self.motion.health.damage.id(damage_type)
            && let Ok(o) = player.hurt_caused_by(
                &self.motion.health,
                self.world.health_rules,
                id,
                amount,
                blocking,
                living_cause,
            )
            && {
                outcome = o;
                o.hurt
            }
        {
            if o.knockback {
                let (xd, zd) = from.map_or((0.0, 0.0), |from| {
                    (from[0] - player.position.x, from[2] - player.position.z)
                });
                if o.blocked <= 0.0 {
                    player.health.indicate_damage(xd, zd, player.yaw);
                }
                self.knock_back_player(f64::from(0.4_f32), xd, zd);
                if let Some(p) = self.player.as_mut()
                    && let Some(r) = p.hurt_log.last_mut()
                {
                    r.knockback = Some([p.velocity.x, p.velocity.y, p.velocity.z]);
                }
            }
            if let Some(a) = attacker.or(owner) {
                self.thorns(a);
            }
        }
        if outcome.hurt || outcome.sound {
            let source = attacker
                .or(owner)
                .and_then(|id| self.mobs.iter().find(|m| m.id == id))
                .map(|m| self.mob_entity(m));
            self.trigger_entity_hurt_player(&vise_client::advancements::DamageView {
                kind: damage_type.to_owned(),
                direct: source.clone().filter(|_| !projectile),
                is_direct: !projectile,
                source,
                original: amount,
                actual: amount - outcome.blocked,
                blocked: outcome.blocked > 0.0,
            });
        }
        if raised {
            self.after_block(outcome, amount, attacker, projectile);
        }
        outcome.hurt
    }
    pub(super) fn knock_back_player(&mut self, power: f64, mut xd: f64, mut zd: f64) {
        let Some(p) = self.player.as_mut() else {
            return;
        };
        let resistance = p.attribute("minecraft:knockback_resistance").unwrap_or(0.0);
        let power = power * (1.0 - resistance);
        if power <= 0.0 {
            return;
        }
        while xd * xd + zd * zd < f64::from(1.0e-5_f32) {
            let r = &mut self.world.random;
            xd = (r.next_double() - r.next_double()) * 0.01;
            zd = (r.next_double() - r.next_double()) * 0.01;
        }
        let l = (xd * xd + zd * zd).sqrt();
        let v = p.velocity;
        p.velocity = Vec3::new(
            v.x / 2.0 - xd / l * power,
            if p.ground.on_ground {
                (v.y / 2.0 + power).min(0.4)
            } else {
                v.y
            },
            v.z / 2.0 - zd / l * power,
        );
    }
    pub(super) fn give_player_effect(&mut self, effect: &'static str, ticks: i32, amplifier: i32) {
        if self.creative || self.spectator() {
            return;
        }
        let id = effect.strip_prefix("minecraft:").unwrap_or(effect);
        self.potions
            .effects
            .add(vise_client::effects::EffectInstance::new(
                id, ticks, amplifier,
            ));
    }
    pub(super) fn wither_rose_contact(&mut self) {
        if self.remote.is_some() || self.ui.difficulty == 0 {
            return;
        }
        self.give_player_effect("minecraft:wither", 40, 0);
    }
    pub(super) fn hurt_player_by(&mut self, damage_type: &str, amount: f32) {
        if self.creative {
            return;
        }
        if let Some(player) = self.player.as_mut()
            && let Some(id) = self.motion.health.damage.id(damage_type)
        {
            let _ = player.hurt(&self.motion.health, self.world.health_rules, id, amount);
        }
    }
    pub(super) fn touch_arrows(&mut self) {
        use vise_client::arrows::Touch;
        let Some(p) = self.player.as_ref() else {
            return;
        };
        if p.health.dead_or_dying() || self.spectator() {
            return;
        }
        let w = f64::from(p.dimensions.width) / 2.0;
        let reach = Aabb::new(
            Vec3::new(p.position.x - w, p.position.y, p.position.z - w),
            Vec3::new(
                p.position.x + w,
                p.position.y + f64::from(p.dimensions.height),
                p.position.z + w,
            ),
        )
        .inflate(Vec3::new(1.0, 0.5, 1.0));
        let creative = self.creative;
        let mut projectiles = std::mem::take(&mut self.creatures.projectiles);
        let mut picked = Vec::new();
        projectiles.retain(|shot| {
            let Some(a) = shot.arrow.as_ref().filter(|a| !a.removed) else {
                return true;
            };
            let at = shot.position;
            let bounds = Aabb::new(
                Vec3::new(at.x - 0.25, at.y, at.z - 0.25),
                Vec3::new(at.x + 0.25, at.y + 0.5, at.z + 0.25),
            );
            if !bounds.intersects(reach) {
                return true;
            }
            let taken = match a.player_touch(creative) {
                Touch::Ignore => false,
                Touch::Take => true,
                Touch::Give(stack) => self.give(stack).is_none() || creative,
            };
            if taken {
                let r = &mut self.world.random;
                let pitch = (r.next_float() - r.next_float()) * 1.4 + 2.0;
                self.audio.play(
                    "minecraft:entity.item.pickup",
                    Some([at.x, at.y, at.z]),
                    0.2,
                    pitch,
                );
                picked.push(shot.clone());
            }
            !taken
        });
        self.creatures.projectiles = projectiles;
        for shot in picked {
            self.add_pickup(
                super::pickups::Taken::Arrow(Box::new(shot)),
                super::pickups::Collector::Player,
            );
        }
    }

    fn tick_projectiles(&mut self) -> Result<()> {
        let mut projectiles = std::mem::take(&mut self.creatures.projectiles);
        let mut hits = Vec::new();
        let mut deflections: Vec<[f64; 3]> = Vec::new();
        let mut arrow_events = Vec::new();
        let mut trails = Vec::new();
        {
            let mut targets: Vec<(Who, Aabb)> = self
                .mobs
                .iter()
                .filter(|m| m.alive())
                .map(|m| (Who::Mob(m.id), m.bounds()))
                .collect();
            if let Some(p) = self.player_view()
                && p.alive
            {
                targets.push((Who::Player, p.bounds));
            }
            targets.extend(
                self.special_targets()
                    .into_iter()
                    .map(|(id, b)| (Who::Mob(id), b)),
            );
            let breezes: Vec<u32> = self
                .mobs
                .iter()
                .filter(|m| m.alive() && m.species() == Species::Breeze)
                .map(|m| m.id)
                .collect();
            let jitter = if breezes.is_empty() || projectiles.is_empty() {
                0.0
            } else {
                self.world.random.next_float()
            };
            let w = self.mob_world_base(&[]);
            let rain = self.arrow_rain();
            let scene = self.arrow_scene(&rain);
            let mut arrow_targets = targets.clone();
            arrow_targets.extend(self.carts.iter().map(|c| (Who::Mob(c.id), c.bounds())));
            let guests = self.lan_projectile_targets();
            let mut arrow_targets_guests = arrow_targets.clone();
            arrow_targets_guests.extend(guests.iter().copied());
            let mut targets_guests = targets.clone();
            targets_guests.extend(guests);
            let pw = ProjWorld { w: &w, targets };
            let pw_guests = ProjWorld {
                w: &w,
                targets: targets_guests,
            };
            let guest_entities = self.lan_guest_entities();
            let vanish = self.ui.rules.bool("ender_pearls_vanish_on_death");
            let host_dead = self.player_view().is_some_and(|p| !p.alive);
            let dead_guests: Vec<u32> = self.lan.as_ref().map_or_else(Vec::new, |h| {
                h.hub
                    .guests
                    .iter()
                    .filter(|g| g.dead)
                    .map(|g| g.conn)
                    .collect()
            });
            let mut keep_flags = vec![true; projectiles.len()];
            for i in 0..projectiles.len() {
                let winds = if matches!(projectiles[i].kind, ProjectileKind::EnderPearl) {
                    wind_charge_hitboxes(&projectiles, &keep_flags, i)
                } else {
                    Vec::new()
                };
                let p = &mut projectiles[i];
                if vanish
                    && matches!(p.kind, ProjectileKind::EnderPearl)
                    && p.owner == Some(Who::Player)
                    && p.guest.map_or(host_dead, |c| dead_guests.contains(&c))
                {
                    keep_flags[i] = false;
                    continue;
                }
                let mob_shot = !matches!(p.owner, Some(Who::Player));
                let pvp = !mob_shot && super::lan_pvp::meets_players(&p.kind);
                let shooter = p
                    .guest
                    .filter(|_| pvp)
                    .and_then(|c| guest_entities.iter().find(|(g, _)| *g == c))
                    .map(|&(_, id)| Who::Mob(id));
                if shooter.is_some() {
                    p.owner = shooter;
                }
                let (keep, hit) = match (p.arrow.is_some(), mob_shot || pvp, winds.is_empty()) {
                    (true, false, _) => vise_client::arrows::tick(p, &scene, &arrow_targets),
                    (true, true, _) => {
                        vise_client::arrows::tick(p, &scene, &arrow_targets_guests)
                    }
                    (false, false, true) => p.tick(&pw),
                    (false, true, true) => p.tick(&pw_guests),
                    (false, pvp_shot, false) => {
                        let mut targets = if pvp_shot {
                            pw_guests.targets.clone()
                        } else {
                            pw.targets.clone()
                        };
                        targets.extend(winds);
                        p.tick(&ProjWorld {
                            w: if pvp_shot { pw_guests.w } else { pw.w },
                            targets,
                        })
                    }
                };
                if shooter.is_some() {
                    p.owner = Some(Who::Player);
                }
                if super::remote_projectiles::has_smoke_trail(&p.kind) {
                    trails.push(p.position);
                }
                if keep && hit == Hit::None && matches!(p.kind, ProjectileKind::EnderPearl) {
                    let (at, h) = (p.position, 0.125);
                    vise_client::motion::bubble_column_push(
                        w.world,
                        w.registry,
                        w.facts,
                        w.collision,
                        Aabb::new(
                            Vec3::new(at.x - h, at.y, at.z - h),
                            Vec3::new(at.x + h, at.y + 2.0 * h, at.z + h),
                        ),
                        &mut p.velocity,
                    );
                }
                if let Some(a) = p.arrow.as_mut() {
                    arrow_events.extend(a.events.drain(..).map(|e| (p.position, e)));
                }
                let wind = matches!(
                    p.kind,
                    ProjectileKind::WindCharge | ProjectileKind::BreezeWindCharge
                );
                if let Hit::Entity {
                    who: Who::Mob(id),
                    at,
                } = hit
                    && !wind
                    && breezes.contains(&id)
                {
                    super::wind::reverse(p, jitter);
                    if p.arrow.is_some() {
                        vise_client::arrows::finish_entity_hit(p, &scene, EntityHit::Deflected);
                        let a = p.arrow.as_mut().expect("an arrow");
                        arrow_events.extend(a.events.drain(..).map(|e| (p.position, e)));
                    }
                    deflections.push([at.x, at.y, at.z]);
                    continue;
                }
                if wind && keep && p.position.y > 350.0 {
                    hits.push((
                        p.clone(),
                        Hit::Block {
                            pos: [0; 3],
                            at: p.position,
                            face: [0; 3],
                        },
                    ));
                    keep_flags[i] = false;
                    continue;
                }
                if hit != Hit::None {
                    hits.push((p.clone(), hit));
                }
                keep_flags[i] = keep;
                if let Hit::Entity {
                    who: Who::Mob(id), ..
                } = hit
                    && let Some(j) = wind_charge_hit(id)
                {
                    let (owner, guest) = (p.owner, p.guest);
                    let look = self.owner_look(owner, guest);
                    super::deflect::aim_deflect(&mut projectiles[j], look, owner, guest);
                }
            }
            let mut idx = 0;
            projectiles.retain(|_| {
                let k = keep_flags[idx];
                idx += 1;
                k
            });
        }
        self.smoke_trails(trails);
        for at in deflections {
            self.audio
                .play("minecraft:entity.breeze.deflect", Some(at), 1.0, 1.0);
        }
        for (at, event) in arrow_events {
            self.arrow_event(at, event);
        }
        self.creatures.projectiles.append(&mut projectiles);
        for (p, hit) in hits {
            if let Some(victim) = self.projectile_pvp_victim(&p, hit) {
                self.projectile_hits_player(&p, hit, victim)?;
                continue;
            }
            match p.guest {
                Some(conn)
                    if !matches!(
                        hit,
                        Hit::Entity {
                            who: Who::Player,
                            ..
                        }
                    ) =>
                {
                    self.guest_projectile_hit(conn, &p, hit)?;
                }
                _ => self.projectile_hit(&p, hit)?,
            }
        }
        self.flush_lan_explosions();
        self.flush_wind_bursts()
    }
    fn guest_projectile_hit(&mut self, conn: u32, p: &Projectile, hit: Hit) -> Result<()> {
        let Some(mut host) = self.lan.take() else {
            return self.projectile_hit(p, hit);
        };
        let result = self.as_guest(&mut host, conn, |g| g.projectile_hit(p, hit));
        if result.is_some()
            && let Hit::Entity {
                who: Who::Mob(id), ..
            } = hit
            && self.mobs.iter().any(|m| m.id == id)
        {
            host.mob_players.insert(id, conn);
        }
        self.lan = Some(host);
        match result {
            Some(r) => r,
            None => self.projectile_hit(p, hit),
        }
    }
    fn shot_damage(&mut self, shot: &mut Projectile, victim: Option<&str>) -> f32 {
        if !matches!(shot.kind, ProjectileKind::Trident) {
            return arrow_damage(shot, &mut self.world.random) + shot.bonus.damage;
        }
        let Some(trident) = shot.arrow.as_ref().map(|a| a.pickup_item.clone()) else {
            return 8.0;
        };
        let ctx = vise_client::enchant::EffectContext {
            this: victim,
            attacker: (shot.owner == Some(Who::Player)).then_some("minecraft:player"),
            direct_attacker: Some("minecraft:trident"),
            damage_type: Some("minecraft:trident"),
            items: Some(world_items!(self)),
            ..Default::default()
        };
        world_stations!(self).enchantments.modify_value(
            Some(&trident),
            "minecraft:damage",
            &ctx,
            &mut self.world.random,
            8.0,
        )
    }
    fn bell_projectile_hit(&mut self, p: &Projectile, pos: [i32; 3], at: Vec3, face: [i32; 3]) {
        let Some(face) = Direction::ALL.into_iter().find(|d| d.normal() == face) else {
            return;
        };
        let block = BlockPos::new(pos[0], pos[1], pos[2]);
        let rang = self
            .with_level(|b, level, ticks| b.bell_projectile_hit(level, ticks, block, face, at.y));
        if rang {
            let _ = self.process_level_events();
            if p.owner == Some(Who::Player) {
                self.stats.custom("bell_ring", 1);
            }
        }
    }
    fn amethyst_projectile_hit(&mut self, pos: [i32; 3]) {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if &*self.res.facts().get(state).name != "minecraft:amethyst_block" {
            return;
        }
        let pitch = 0.5 + self.world.random.next_float() * 1.2;
        self.level_events
            .push(vise_world::level::LevelEvent::Sound {
                pos: BlockPos::new(pos[0], pos[1], pos[2]),
                sound: "minecraft:block.amethyst_block.chime".into(),
                volume: 1.0,
                pitch,
            });
        let _ = self.process_level_events();
    }
    fn projectile_breaks_block(&mut self, p: &Projectile, pos: [i32; 3]) {
        use vise_client::command::args::GameMode;
        let impact = matches!(
            p.kind,
            ProjectileKind::Arrow { .. }
                | ProjectileKind::Trident
                | ProjectileKind::Snowball
                | ProjectileKind::Egg
                | ProjectileKind::SmallFireball
                | ProjectileKind::LargeFireball { .. }
                | ProjectileKind::WitherSkull { .. }
                | ProjectileKind::WindCharge
                | ProjectileKind::BreezeWindCharge
        );
        let shooter_may_break = match p.owner {
            Some(Who::Player) => self.mode != GameMode::Adventure,
            Some(Who::Mob(_)) => false,
            None => true,
        };
        if !impact || !shooter_may_break || !self.ui.rules.bool("projectiles_can_break_blocks") {
            return;
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        match &*self.res.facts().get(state).name {
            "minecraft:chorus_flower" => {}
            "minecraft:pointed_dripstone" | "minecraft:sulfur_spike"
                if matches!(p.kind, ProjectileKind::Trident)
                    && p.arrow.as_ref().is_some_and(|a| a.hit_speed > 0.6) => {}
            "minecraft:decorated_pot" => {
                let registry = &self.pack.assets.registry;
                let Some(mut pot) = registry.state(state).cloned() else {
                    return;
                };
                pot.properties.insert("cracked".into(), "true".into());
                let Ok(cracked) = registry.resolve(&pot) else {
                    return;
                };
                self.with_level(|b, level, ticks| b.set_block(level, ticks, at, cracked, 260));
            }
            _ => return,
        }
        let _ = self.destroy_block_dropping(pos);
    }
    fn arrow_event(&mut self, at: Vec3, event: vise_client::arrows::ArrowEvent) {
        use vise_client::arrows::{ArrowEvent, ArrowParticle, ArrowSound, BlockReaction};
        use vise_client::particles::{ParticleOptions, ParticleType};
        let kind = |k| match k {
            ArrowParticle::Bubble => ParticleType::Bubble,
            ArrowParticle::Crit => ParticleType::Crit,
            ArrowParticle::Splash => ParticleType::Splash,
        };
        match event {
            ArrowEvent::Particle {
                kind: k,
                position,
                velocity,
            } => self.particles.add(
                &ParticleOptions::simple(kind(k)),
                [position.x, position.y, position.z],
                [velocity.x, velocity.y, velocity.z],
            ),
            ArrowEvent::PotionPuff { id, .. } => {
                self.lan_entity_event(id as i32, 0, [at.x, at.y, at.z]);
            }
            ArrowEvent::Sound {
                sound,
                volume,
                pitch,
            } => {
                let name = match sound {
                    ArrowSound::Splash => "minecraft:entity.generic.splash",
                    ArrowSound::HitGround => "minecraft:entity.arrow.hit",
                    ArrowSound::CrossbowHit => "minecraft:item.crossbow.hit",
                    ArrowSound::Extinguish => "minecraft:entity.generic.extinguish_fire",
                    ArrowSound::TridentHitGround => "minecraft:item.trident.hit_ground",
                    ArrowSound::TridentHit => "minecraft:item.trident.hit",
                };
                self.audio
                    .play(name, Some([at.x, at.y, at.z]), volume, pitch);
            }
            ArrowEvent::BubbleColumnParticles { pos } => {
                for _ in 0..2 {
                    let r = &mut self.world.random;
                    let splash = [
                        f64::from(pos.x) + r.next_double(),
                        f64::from(pos.y + 1),
                        f64::from(pos.z) + r.next_double(),
                    ];
                    let bubble = [
                        f64::from(pos.x) + r.next_double(),
                        f64::from(pos.y + 1),
                        f64::from(pos.z) + r.next_double(),
                    ];
                    self.particles.send_particles(
                        &ParticleOptions::simple(ParticleType::Splash),
                        splash,
                        [0.0; 3],
                        1.0,
                        1,
                    );
                    self.particles.send_particles(
                        &ParticleOptions::simple(ParticleType::Bubble),
                        bubble,
                        [0.0, 0.01, 0.0],
                        0.2,
                        1,
                    );
                }
            }
            ArrowEvent::Block { pos, reaction } => match reaction {
                BlockReaction::PrimeTnt => {
                    self.spawn_primed_tnt(pos, false);
                    self.with_level(|b, level, ticks| b.remove_block(level, ticks, pos, false));
                    let _ = self.process_level_events();
                }
                BlockReaction::Light(lit) => {
                    let _ = self.level_set([pos.x, pos.y, pos.z], lit);
                }
            },
            ArrowEvent::SplashEvent => {
                self.projectile_event("minecraft:splash", None, [at.x, at.y, at.z]);
            }
            ArrowEvent::ShootEvent | ArrowEvent::Drop => {}
        }
    }
    fn arrow_rain(&self) -> impl Fn(BlockPos) -> bool + '_ {
        move |p: BlockPos| self.raining_at(p)
    }
    fn arrow_scene<'s>(
        &'s self,
        rain: &'s dyn Fn(BlockPos) -> bool,
    ) -> vise_client::arrows::ArrowScene<'s, 's> {
        vise_client::arrows::ArrowScene {
            world: &self.world,
            registry: self.pack.assets.registry,
            facts: self.res.facts(),
            collision: self.collision,
            fluids: &self.motion.fluids,
            raining: rain,
            mob_griefing: self.ui.rules.bool("mob_griefing"),
            tnt_explodes: self.ui.rules.bool("tnt_explodes"),
        }
    }
    fn finish_arrow_hit(&mut self, mut arrow: Projectile, outcome: EntityHit) {
        let keep = {
            let rain = self.arrow_rain();
            vise_client::arrows::finish_entity_hit(&mut arrow, &self.arrow_scene(&rain), outcome)
        };
        let Some(a) = arrow.arrow.as_mut() else {
            return;
        };
        let (events, item) = (std::mem::take(&mut a.events), a.pickup_item.clone());
        for event in events {
            if event == vise_client::arrows::ArrowEvent::Drop {
                self.spawn_at_location(arrow.position, f64::from(0.1f32), item.clone());
            } else {
                self.arrow_event(arrow.position, event);
            }
        }
        if keep {
            self.add_projectile(arrow);
        }
    }
    fn target_block_hit(&mut self, p: &Projectile, pos: [i32; 3], at: Vec3, face: [i32; 3]) {
        let block = BlockPos::new(pos[0], pos[1], pos[2]);
        let arrow = p.arrow.is_some();
        let location = [at.x, at.y, at.z];
        let hit = self.with_level(|b, level, ticks| {
            b.target_projectile_hit(level, ticks, block, location, face, arrow)
        });
        if let Some(strength) = hit {
            let _ = self.process_level_events();
            if p.owner == Some(Who::Player) && (p.guest.is_none() || p.guest == self.lan_acting) {
                self.stats.custom("target_hit", 1);
                let kind = if p
                    .arrow
                    .as_ref()
                    .is_some_and(|a| &*a.pickup_item.item == "minecraft:spectral_arrow")
                {
                    "minecraft:spectral_arrow"
                } else {
                    p.kind.id()
                };
                let projectile = vise_client::advancements::EntityView {
                    kind: kind.to_owned(),
                    position: [p.position.x, p.position.y, p.position.z],
                    ..Default::default()
                };
                self.trigger_target_hit(&projectile, strength);
            }
        }
    }
    pub(super) fn projectile_hit(&mut self, p: &Projectile, hit: Hit) -> Result<()> {
        if let Hit::Block { pos, at, face } = hit
            && face != [0; 3]
        {
            self.bell_projectile_hit(p, pos, at, face);
            self.amethyst_projectile_hit(pos);
            self.projectile_breaks_block(p, pos);
        }
        self.combat.projectile_owner = match p.owner {
            Some(Who::Mob(id)) => Some(id),
            _ => None,
        };
        self.combat.projectile_direct = match hit {
            Hit::Entity {
                who: Who::Player, ..
            } => {
                let spectral = p
                    .arrow
                    .as_ref()
                    .is_some_and(|a| &*a.pickup_item.item == "minecraft:spectral_arrow");
                let id = if spectral {
                    "minecraft:spectral_arrow"
                } else {
                    p.kind.id()
                };
                Some(Text::translate(
                    &format!("entity.{}", id.replace(':', ".")),
                    &[],
                ))
            }
            _ => None,
        };
        let at = match hit {
            Hit::Entity { at, .. } | Hit::Block { at, .. } => at,
            Hit::None => return Ok(()),
        };
        if let Hit::Block { pos, face, .. } = hit
            && face != [0; 3]
        {
            self.target_block_hit(p, pos, at, face);
        }
        if let Hit::Entity {
            who: Who::Mob(id),
            at,
        } = hit
            && self.lan_projectile_hit(p, id, at)
        {
            return Ok(());
        }
        if matches!(
            p.kind,
            ProjectileKind::WindCharge | ProjectileKind::BreezeWindCharge
        ) {
            return self.wind_charge_hit(p, hit);
        }
        if let Hit::Entity {
            who: Who::Mob(id), ..
        } = hit
            && id >= super::dragon::SPECIAL
        {
            let by_player = p.owner == Some(Who::Player);
            if p.arrow.is_some() {
                let mut arrow = p.clone();
                let amount = self.shot_damage(&mut arrow, None);
                let outcome = match self.hit_special(id, amount, by_player, true) {
                    super::dragon::Hurt::Landed { .. } => EntityHit::Hurt,
                    super::dragon::Hurt::Refused => EntityHit::Refused,
                    super::dragon::Hurt::Shrugged => {
                        if let Some(a) = arrow.arrow.as_mut() {
                            a.ignite_for_seconds(1.0);
                        }
                        EntityHit::Refused
                    }
                };
                self.finish_arrow_hit(arrow, outcome);
                return Ok(());
            }
            let amount = match p.kind {
                ProjectileKind::SmallFireball | ProjectileKind::LargeFireball { .. } => 6.0,
                _ => 0.0,
            };
            self.hit_special(id, amount, by_player, false);
            if !matches!(
                p.kind,
                ProjectileKind::EnderPearl | ProjectileKind::Egg | ProjectileKind::Snowball
            ) {
                return Ok(());
            }
        }
        if p.arrow.is_some()
            && let Hit::Entity {
                who: Who::Mob(id), ..
            } = hit
            && let Some(i) = self.carts.iter().position(|c| c.id == id)
        {
            let mut arrow = p.clone();
            let amount = self.shot_damage(&mut arrow, None);
            let burning = p.arrow.as_ref().is_some_and(|a| a.on_fire());
            let creative = p.owner == Some(Who::Player) && self.creative;
            self.arrow_hits_cart(i, amount, p.velocity, burning, creative);
            self.finish_arrow_hit(arrow, EntityHit::Hurt);
            return Ok(());
        }
        let atp = [at.x, at.y, at.z];
        self.projectile_event("minecraft:projectile_land", p.owner, atp);
        let source_kind = match p.kind {
            ProjectileKind::Arrow { .. } | ProjectileKind::Trident => DamageKind::Arrow,
            ProjectileKind::SmallFireball | ProjectileKind::LargeFireball { .. } => {
                DamageKind::Fireball
            }
            _ => DamageKind::Thrown,
        };
        let source = Damage {
            kind: source_kind,
            attacker: p.owner,
            from: Some([p.previous.x, p.previous.y, p.previous.z]),
        };
        match (&p.kind, hit) {
            (ProjectileKind::Trident, Hit::Entity { who, .. }) => {
                let mut trident = p.clone();
                let victim = match who {
                    Who::Mob(id) => self
                        .mobs
                        .iter()
                        .find(|m| m.id == id)
                        .map(|m| m.kind.id.clone()),
                    Who::Player => Some("minecraft:player".to_owned()),
                };
                let amount = self.shot_damage(&mut trident, victim.as_deref());
                let hurt = match who {
                    Who::Player => self.mob_hits_player(
                        amount,
                        [p.previous.x, p.previous.y, p.previous.z],
                        "minecraft:trident",
                    ),
                    Who::Mob(id) => self
                        .mobs
                        .iter_mut()
                        .find(|m| m.id == id)
                        .is_some_and(|m| m.damage(amount, source)),
                };
                let outcome = if hurt {
                    EntityHit::Hurt
                } else {
                    EntityHit::Refused
                };
                self.finish_arrow_hit(trident, outcome);
            }
            (ProjectileKind::Trident, Hit::Block { .. }) => {}
            (ProjectileKind::Arrow { .. }, Hit::Entity { who, .. }) => {
                let mut arrow = p.clone();
                let amount = arrow_damage(&mut arrow, &mut self.world.random) + p.bonus.damage;
                let player_fire = self.player.as_ref().map(|pl| pl.fire_ticks);
                let mut dodged = false;
                if p.arrow.as_ref().is_some_and(|a| a.on_fire()) {
                    self.handle_mob_events(vec![MobEvent::Ignite {
                        target: who,
                        seconds: 5,
                    }])?;
                }
                let hurt = match who {
                    Who::Player => self.mob_hits_player(
                        amount,
                        [p.previous.x, p.previous.y, p.previous.z],
                        "minecraft:arrow",
                    ),
                    Who::Mob(id) => {
                        let hurt = self
                            .mobs
                            .iter_mut()
                            .find(|m| m.id == id)
                            .is_some_and(|m| m.damage(amount, source));
                        dodged = !hurt
                            && self.mobs.iter().find(|m| m.id == id).is_some_and(|m| {
                                m.species() == Species::Enderman
                                    && vise_client::mobs::enderman::dodges(m, &source)
                            });
                        if p.owner == Some(Who::Player) {
                            self.last_hurt_by_player.insert(id, self.lan_acting);
                            if hurt && let Some(m) = self.mobs.iter().find(|m| m.id == id) {
                                let victim = self.mob_entity(m);
                                let spectral = p.arrow.as_ref().is_some_and(|a| {
                                    &*a.pickup_item.item == "minecraft:spectral_arrow"
                                });
                                let direct = vise_client::advancements::EntityView {
                                    kind: if spectral {
                                        "minecraft:spectral_arrow"
                                    } else {
                                        "minecraft:arrow"
                                    }
                                    .into(),
                                    position: [p.position.x, p.position.y, p.position.z],
                                    ..Default::default()
                                };
                                self.player_hit(&victim, "minecraft:arrow", Some(direct), amount);
                            }
                            if hurt
                                && let Some(m) = self.mobs.iter().find(|m| m.id == id)
                                && !m.alive()
                            {
                                let victim = self.mob_entity(m);
                                let weapon =
                                    p.arrow.as_ref().and_then(|a| a.fired_from_weapon.as_ref());
                                self.trigger_killed_by_arrow(&[victim], weapon);
                            }
                        }
                        hurt
                    }
                };
                let flight = (p.velocity.x * p.velocity.x + p.velocity.z * p.velocity.z).sqrt();
                let resistance = match who {
                    Who::Player => self
                        .player
                        .as_mut()
                        .and_then(|pl| pl.attribute("minecraft:knockback_resistance").ok())
                        .unwrap_or(0.0),
                    Who::Mob(id) => self
                        .mobs
                        .iter()
                        .find(|m| m.id == id)
                        .map_or(0.0, |m| m.attrs.knockback_resistance),
                };
                let knockback = if !hurt {
                    0.0
                } else if let Some(weapon) =
                    p.arrow.as_ref().and_then(|a| a.fired_from_weapon.as_ref())
                {
                    let victim = match who {
                        Who::Player => Some("minecraft:player"),
                        Who::Mob(id) => self
                            .mobs
                            .iter()
                            .find(|m| m.id == id)
                            .map(|m| m.kind.id.as_str()),
                    };
                    let owner = match p.owner {
                        Some(Who::Player) => Some("minecraft:player"),
                        Some(Who::Mob(id)) => self
                            .mobs
                            .iter()
                            .find(|m| m.id == id)
                            .map(|m| m.kind.id.as_str()),
                        None => None,
                    };
                    let arrow_type = if p
                        .arrow
                        .as_ref()
                        .is_some_and(|a| &*a.pickup_item.item == "minecraft:spectral_arrow")
                    {
                        "minecraft:spectral_arrow"
                    } else {
                        "minecraft:arrow"
                    };
                    let ctx = vise_client::enchant::EffectContext {
                        this: victim,
                        attacker: owner,
                        direct_attacker: Some(arrow_type),
                        damage_type: Some("minecraft:arrow"),
                        items: Some(self.res.items()),
                        ..Default::default()
                    };
                    self.res.stations().enchantments.modify_value(
                        Some(weapon),
                        "minecraft:knockback",
                        &ctx,
                        &mut self.world.random,
                        0.0,
                    )
                } else {
                    p.bonus.punch as f32
                };
                let push = f64::from(knockback) * 0.6 * (1.0 - resistance).max(0.0);
                if hurt && push > 0.0 && flight > 1e-6 {
                    let k = push / flight;
                    self.handle_mob_events(vec![MobEvent::Push {
                        target: who,
                        velocity: [p.velocity.x * k, 0.1, p.velocity.z * k],
                    }])?;
                }
                if hurt {
                    self.arrow_post_hurt_effects(p, who);
                }
                match (hurt, who) {
                    (true, _) => self.finish_arrow_hit(arrow, EntityHit::Hurt),
                    (false, Who::Player) => {
                        if let (Some(pl), Some(ticks)) = (self.player.as_mut(), player_fire) {
                            pl.set_remaining_fire_ticks(ticks);
                        }
                        self.finish_arrow_hit(arrow, EntityHit::Refused);
                    }
                    (false, Who::Mob(_)) if dodged => {
                        self.finish_arrow_hit(arrow, EntityHit::Dodged);
                    }
                    (false, Who::Mob(_)) => {}
                }
            }
            (ProjectileKind::Arrow { .. }, Hit::Block { .. }) => {}
            (ProjectileKind::Snowball, hit) => {
                if let Hit::Entity {
                    who: Who::Mob(id), ..
                } = hit
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
                {
                    let amount = if m.species() == Species::Blaze {
                        3.0
                    } else {
                        0.0
                    };
                    m.damage(amount, source);
                }
                self.thrown_break_event(p.id, atp);
                self.item_burst(atp, "minecraft:snowball");
            }
            (ProjectileKind::Egg, hit) => {
                if let Hit::Entity {
                    who: Who::Mob(id), ..
                } = hit
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
                {
                    m.damage(0.0, source);
                }
                if self.world.random.next_int_bound(8).unwrap_or(1) == 0 {
                    let n = if self.world.random.next_int_bound(32).unwrap_or(1) == 0 {
                        4
                    } else {
                        1
                    };
                    for _ in 0..n {
                        if let Some(id) = self.spawn_mob("minecraft:chicken", atp, true)
                            && let Some(c) = self.mobs.iter_mut().find(|m| m.id == id)
                        {
                            c.extra.age = -24000;
                        }
                    }
                }
                self.thrown_break_event(p.id, atp);
                self.item_burst(atp, "minecraft:egg");
            }
            (ProjectileKind::EnderPearl, hit) => {
                if let Hit::Entity {
                    who: Who::Mob(id), ..
                } = hit
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
                {
                    m.damage(0.0, source);
                }
                if p.owner == Some(Who::Player) && self.player.is_some() {
                    if self.world.random.next_float() < 0.05
                        && self.ui.rules.bool("spawn_monsters")
                        && self.ui.difficulty != 0
                        && let Some(pl) = self.player.as_ref()
                    {
                        let from = pl.position;
                        self.spawn_mob("minecraft:endermite", [from.x, from.y, from.z], false);
                    }
                    let to = p.previous;
                    if let Some(pl) = self.player.as_mut() {
                        pl.position = to;
                        pl.velocity = Vec3::ZERO;
                        pl.fall_distance = 0.0;
                        pl.teleported = true;
                    }
                    self.previous_position = to;
                    self.hurt_player_by("minecraft:ender_pearl", 5.0);
                    self.audio.play(
                        "minecraft:entity.player.teleport",
                        Some([to.x, to.y, to.z]),
                        1.0,
                        1.0,
                    );
                }
            }
            (ProjectileKind::SmallFireball, hit) => match hit {
                Hit::Entity { who, .. } => match who {
                    Who::Player => {
                        if !self.creative {
                            let remaining = self.player.as_ref().map(|p| p.fire_ticks);
                            if let Some(p) = self.player.as_mut() {
                                p.ignite_for_ticks(100)?;
                            }
                            let hurt = self.mob_hits_player(
                                5.0,
                                [p.previous.x, p.previous.y, p.previous.z],
                                "minecraft:fireball",
                            );
                            if !hurt
                                && let Some(remaining) = remaining
                                && let Some(pl) = self.player.as_mut()
                            {
                                pl.set_remaining_fire_ticks(remaining);
                            }
                        }
                    }
                    Who::Mob(id) => {
                        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                            let remaining = m.fire_ticks;
                            if !m.kind.flags.fire_immune {
                                m.ignite_for_ticks(100);
                            }
                            if !m.damage(5.0, source) {
                                m.fire_ticks = remaining;
                            }
                        }
                    }
                },
                Hit::Block { pos, face, .. } => {
                    let fire = [pos[0] + face[0], pos[1] + face[1], pos[2] + face[2]];
                    let mob = matches!(p.owner, Some(Who::Mob(_)));
                    if (!mob || self.ui.rules.bool("mob_griefing"))
                        && self
                            .res
                            .facts()
                            .is_air(self.world.block(fire[0], fire[1], fire[2]))
                    {
                        let at = BlockPos::new(fire[0], fire[1], fire[2]);
                        let state =
                            self.with_level(|b, level, ticks| b.fire_state_for(level, ticks, at));
                        self.level_set(fire, state)?;
                    }
                }
                Hit::None => {}
            },
            (ProjectileKind::LargeFireball { power }, hit) => {
                if let Hit::Entity { who, .. } = hit {
                    match who {
                        Who::Player => {
                            self.mob_hits_player(
                                6.0,
                                [p.previous.x, p.previous.y, p.previous.z],
                                "minecraft:fireball",
                            );
                        }
                        Who::Mob(id) => {
                            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                                m.damage(6.0, source);
                            }
                        }
                    }
                }
                if let Some(owner) = self.combat.projectile_owner {
                    self.note_attacker("minecraft:explosion", owner);
                }
                self.combat.explosion_owner = self.combat.projectile_owner;
                let result = self.explode(atp, *power as f32);
                self.combat.explosion_owner = None;
                result?;
            }
            (
                ProjectileKind::Potion {
                    effect,
                    ticks,
                    amplifier,
                    ..
                },
                _,
            ) => {
                if let Some(pv) = self.player_view() {
                    let d2 = (pv.position.x - at.x).powi(2)
                        + (pv.position.y - at.y).powi(2)
                        + (pv.position.z - at.z).powi(2);
                    if d2 < 16.0 && !self.creative {
                        let scale = 1.0 - d2.sqrt() / 4.0;
                        let t = (f64::from(*ticks) * scale + 0.5) as i32;
                        match *effect {
                            "minecraft:instant_damage" => {
                                let owner = self
                                    .combat
                                    .projectile_owner
                                    .filter(|id| self.mob_display_name(*id).is_some());
                                match owner {
                                    Some(owner) => {
                                        self.note_attacker("minecraft:indirect_magic", owner)
                                    }
                                    None => self.note_direct_entity(
                                        "minecraft:indirect_magic",
                                        Text::translate("entity.minecraft.splash_potion", &[]),
                                    ),
                                }
                                self.hurt_player_by(
                                    "minecraft:indirect_magic",
                                    6.0 * (1 << *amplifier) as f32 * scale as f32,
                                )
                            }
                            e if t > 20 => self.give_player_effect(e, t, *amplifier),
                            _ => {}
                        }
                    }
                }
                if matches!(
                    *effect,
                    "minecraft:instant_health" | "minecraft:regeneration"
                ) {
                    let center = at;
                    for m in &mut self.mobs {
                        let d2 = m.position.distance_squared(center);
                        if !m.alive()
                            || d2 >= 16.0
                            || !vise_client::mobs::raider::is_raider(m.species())
                        {
                            continue;
                        }
                        let scale = 1.0 - d2.sqrt() / 4.0;
                        if *effect == "minecraft:instant_health" {
                            let heal = (4 << *amplifier) as f64 * scale;
                            m.health = (m.health + heal as f32).min(m.attrs.max_health);
                        } else {
                            let t = (f64::from(*ticks) * scale + 0.5) as i32;
                            if t > 20 {
                                m.add_effect(effect, t, *amplifier);
                            }
                        }
                    }
                }
                self.audio.play(
                    "minecraft:entity.splash_potion.break",
                    Some(atp),
                    1.0,
                    self.world.random.next_float() * 0.1 + 0.9,
                );
            }
            (ProjectileKind::LlamaSpit, Hit::Entity { who, .. }) => match who {
                Who::Player => {
                    self.mob_hits_player(
                        1.0,
                        [p.previous.x, p.previous.y, p.previous.z],
                        "minecraft:spit",
                    );
                }
                Who::Mob(id) => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                        m.damage(1.0, source);
                    }
                }
            },
            (ProjectileKind::ShulkerBullet { .. }, Hit::Entity { who, .. }) => match who {
                Who::Player => {
                    if !self.creative
                        && self.mob_hits_player(
                            4.0,
                            [p.previous.x, p.previous.y, p.previous.z],
                            "minecraft:mob_projectile",
                        )
                    {
                        self.give_player_effect("minecraft:levitation", 200, 0);
                    }
                }
                Who::Mob(id) => {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id)
                        && m.damage(4.0, source)
                    {
                        m.add_effect("minecraft:levitation", 200, 0);
                        vise_client::mobs::shulker::hit_by_bullet(m);
                    }
                }
            },
            (ProjectileKind::WitherSkull { .. }, hit) => {
                let who = match hit {
                    Hit::Entity { who, .. } => Some(who),
                    _ => None,
                };
                self.wither_skull_hit(p, who, atp)?;
            }
            (ProjectileKind::ShulkerBullet { .. }, Hit::Block { .. }) => {
                self.audio
                    .play("minecraft:entity.shulker_bullet.hit", Some(atp), 1.0, 1.0);
                if let Some(ty) = vise_client::particles::ParticleType::from_name("explosion") {
                    let options = vise_client::particles::ParticleOptions::simple(ty);
                    for _ in 0..2 {
                        let r = &mut self.world.random;
                        let at = [
                            atp[0] + r.next_gaussian() * 0.2,
                            atp[1] + r.next_gaussian() * 0.2,
                            atp[2] + r.next_gaussian() * 0.2,
                        ];
                        self.particles.add(&options, at, [0.0; 3]);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn thrown_break_event(&mut self, id: u32, at: [f64; 3]) {
        if id != 0 {
            self.lan_entity_event(id as i32, 3, at);
        }
    }
    pub(super) fn item_burst(&mut self, at: [f64; 3], item: &str) {
        let options = self.item_particle(&vise_client::items::ItemStack::new(item, 1));
        let egg = item != "minecraft:snowball";
        for _ in 0..8 {
            let v = if egg {
                let r = self.particles.random();
                [
                    (f64::from(r.next_float()) - 0.5) * 0.08,
                    (f64::from(r.next_float()) - 0.5) * 0.08,
                    (f64::from(r.next_float()) - 0.5) * 0.08,
                ]
            } else {
                [0.0; 3]
            };
            self.particles.add(&options, at, v);
        }
    }
    pub(super) fn age_lock_takes(&self, m: &Mob, item: &str) -> bool {
        let species = m.species();
        let baby = if species == Species::Tadpole {
            true
        } else if vise_client::mobs::finalize::ageable(species) {
            m.is_baby()
                && !(species == Species::Panda
                    && (vise_client::mobs::panda::scared(m)
                        || vise_client::mobs::panda::is_on_back(m)))
                && !(species == Species::Armadillo && vise_client::mobs::armadillo::scared(m))
        } else {
            false
        };
        let cannot = self
            .res
            .mob_data()
            .entity_has("minecraft:cannot_be_age_locked", &m.kind.id);
        vise_client::mobs::ai::can_use_golden_dandelion(
            item,
            baby,
            m.extra.age_lock_particle_timer,
            cannot,
        )
    }
    pub(super) fn set_age_locked(&mut self, index: usize) {
        let m = &mut self.mobs[index];
        if m.species() == Species::Tadpole {
            vise_client::mobs::aquatic::set_age_locked_data(m);
        } else {
            vise_client::mobs::ai::set_age_locked_data(m);
        }
        self.consume_held();
        let m = &mut self.mobs[index];
        let locked = m.extra.age_locked;
        if locked {
            m.persistent = true;
        }
        let at = [
            m.position.x.floor() + 0.5,
            m.position.y.floor() + 0.5,
            m.position.z.floor() + 0.5,
        ];
        let chunk = (
            m.position.x.floor() as i32 >> 4,
            m.position.z.floor() as i32 >> 4,
        );
        self.world.modified.insert(chunk);
        let sound = if locked {
            "minecraft:item.golden_dandelion.use"
        } else {
            "minecraft:item.golden_dandelion.unuse"
        };
        self.audio.play(sound, Some(at), 1.0, 1.0);
    }
    pub(super) fn interact_mob(&mut self) -> Result<bool> {
        let Some((index, distance)) = self.mob_hit() else {
            return Ok(false);
        };
        if let Some(hit) = self.pick()
            && let Some(eye) = self.eye_position()
        {
            let d = ((hit.point[0] - eye[0]).powi(2)
                + (hit.point[1] - eye[1]).powi(2)
                + (hit.point[2] - eye[2]).powi(2))
            .sqrt();
            if d < distance {
                return Ok(false);
            }
        }
        if self.name_mob(index) {
            return Ok(true);
        }
        if self.spawn_egg_offspring(index) {
            self.swing_arm();
            return Ok(true);
        }
        {
            let item = self.held().map(|h| h.item.to_string()).unwrap_or_default();
            let used = self.held().cloned();
            if self.interact_leash(index, &item) {
                if let Some(stack) = &used {
                    let target = self.mob_entity(&self.mobs[index]);
                    self.trigger_item_entity(
                        "minecraft:player_interacted_with_entity",
                        stack,
                        &target,
                    );
                }
                self.swing_arm();
                return Ok(true);
            }
        }
        let held = self.held().cloned();
        let item = held
            .as_ref()
            .map(|h| h.item.to_string())
            .unwrap_or_default();
        if item == "minecraft:water_bucket"
            && self.mobs[index].alive()
            && let Some((bucket, sound)) = bucketable(self.mobs[index].species())
        {
            let mut filled = ItemStack::new(bucket, 1);
            filled.extra = Some(Box::new(vise_client::entity_nbt::bucket_components(
                &self.mobs[index],
            )));
            self.audio
                .play(sound, Some(self.mobs[index].center()), 1.0, 1.0);
            self.trigger_item("minecraft:filled_bucket", &filled);
            let id = self.mobs[index].id;
            self.break_leash(id, true, false);
            self.mobs[index].removed = true;
            if self.creative {
                if let Some(rest) = self.give(filled) {
                    self.spawn_item(rest, self.eye_position().unwrap_or([0.0; 3]), [0.0; 3], 40);
                }
            } else {
                *self.held_mut() = Some(filled);
            }
            self.swing_arm();
            return Ok(true);
        }
        if self.mobs[index].alive() && self.age_lock_takes(&self.mobs[index], &item) {
            self.set_age_locked(index);
            if let Some(stack) = &held {
                let target = self.mob_entity(&self.mobs[index]);
                self.trigger_item_entity("minecraft:player_interacted_with_entity", stack, &target);
            }
            self.swing_arm();
            return Ok(true);
        }
        let creative = self.creative;
        let selected = self.held_slot();
        if self.mobs[index].species() == Species::Panda && self.mobs[index].alive() {
            let taken = self.interact_panda(index, &item);
            if taken {
                self.swing_arm();
            }
            return Ok(taken);
        }
        if self.interact_zombie_villager(index, &item) {
            return Ok(true);
        }
        if self.interact_trader(index, &item)? {
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_allay(index, &item) {
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_sulfur_cube(index, &item) {
            if let Some(stack) = &held {
                let target = self.mob_entity(&self.mobs[index]);
                self.trigger_item_entity("minecraft:player_interacted_with_entity", stack, &target);
            }
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_armadillo(index, &item) {
            if let Some(stack) = &held {
                let target = self.mob_entity(&self.mobs[index]);
                self.trigger_item_entity("minecraft:player_interacted_with_entity", stack, &target);
            }
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_copper_golem(index, &item) {
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_shear_mob(index, &item) {
            self.swing_arm();
            return Ok(true);
        }
        if self.interact_mount(index, &item) {
            self.swing_arm();
            return Ok(true);
        }
        if self.mobs[index].alive() && self.give_piglin(index, &item) {
            self.swing_arm();
            if !creative && let Some(stack) = self.stack_in_mut(selected).as_mut() {
                stack.count -= 1;
                if stack.count == 0 {
                    *self.stack_in_mut(selected) = None;
                }
            }
            return Ok(true);
        }
        let creeper_igniter = self
            .res
            .items()
            .item_tags
            .get("minecraft:creeper_igniters")
            .is_some_and(|t| t.contains(&*item));
        let fish = self
            .res
            .items()
            .item_tags
            .get("minecraft:fishes")
            .is_some_and(|t| t.contains(&*item));
        let damageable = self
            .res
            .items()
            .get(&item)
            .is_some_and(|i| i.max_damage > 0);
        let parrot_poisonous = self
            .res
            .items()
            .item_tags
            .get("minecraft:parrot_poisonous_food")
            .is_some_and(|t| t.contains(&*item));
        let player_at = self.eye_position();
        let acting = self.acting_player_uuid();
        let sneaking = self.keys.contains(&winit::keyboard::KeyCode::ShiftLeft)
            || self.keys.contains(&winit::keyboard::KeyCode::ShiftRight);
        let held_damage = held.as_ref().map_or(0, |h| h.damage as i32);
        let held_extra = held.as_ref().and_then(|h| h.extra.clone());
        let m = &mut self.mobs[index];
        if !m.alive() {
            return Ok(false);
        }
        let species = m.species();
        let center = m.center();
        let mut armor_drop: Option<ItemStack> = None;
        let mut consume = false;
        let mut give: Option<&str> = None;
        let mut sound: Option<&str> = None;
        let mut eat: Option<(&'static str, f32, f32)> = None;
        let feet = [m.position.x, m.position.y, m.position.z];
        let mut handled = true;
        let mut drops: Vec<(String, u32)> = Vec::new();
        let is_food = m.kind.food.contains(&item);
        let baby_turtle = species == Species::Turtle && m.is_baby();
        match species {
            Species::Cow | Species::Mooshroom if item == "minecraft:bucket" && !m.is_baby() => {
                give = Some("minecraft:milk_bucket");
                consume = true;
                sound = Some("entity.cow.milk");
            }
            Species::Goat if item == "minecraft:bucket" && !m.is_baby() => {
                give = Some("minecraft:milk_bucket");
                consume = true;
                sound = Some(if m.extra.screaming {
                    "entity.goat.screaming.milk"
                } else {
                    "entity.goat.milk"
                });
            }
            Species::Mooshroom if item == "minecraft:bowl" && !m.is_baby() => {
                give = Some("minecraft:mushroom_stew");
                consume = true;
                sound = Some("entity.mooshroom.milk");
            }
            Species::Mooshroom if item == "minecraft:shears" && !m.is_baby() => {
                let mushroom = if m.extra.variant == 1 {
                    "minecraft:brown_mushroom"
                } else {
                    "minecraft:red_mushroom"
                };
                drops.push((mushroom.into(), 5));
                m.removed = true;
                let setup = SpawnSetup {
                    yaw: m.yaw,
                    persistent: true,
                    ..Default::default()
                };
                let pos = [m.position.x, m.position.y, m.position.z];
                sound = Some("entity.mooshroom.shear");
                self.handle_mob_events(vec![MobEvent::SpawnKind {
                    kind: "minecraft:cow".into(),
                    at: pos,
                    setup,
                }])?;
                self.damage_held_tool();
            }
            Species::Sheep if item == "minecraft:shears" && !m.extra.sheared && !m.is_baby() => {
                m.extra.sheared = true;
                let color = vise_client::mobs::render::DYE_NAMES[m.extra.color as usize & 15];
                let stacks = world_items!(self).loot_table(
                    &format!("minecraft:shearing/sheep/{color}"),
                    false,
                    None,
                    &mut self.world.random,
                );
                drops.extend(stacks.into_iter().map(|s| (s.item.to_string(), s.count)));
                sound = Some("entity.sheep.shear");
                self.damage_held_tool();
            }
            Species::Sheep if item.ends_with("_dye") => {
                let name = item
                    .trim_start_matches("minecraft:")
                    .trim_end_matches("_dye");
                if let Some(c) = vise_client::mobs::render::DYE_NAMES
                    .iter()
                    .position(|d| *d == name)
                    && m.extra.color as usize != c
                    && !m.extra.sheared
                {
                    m.extra.color = c as u8;
                    consume = true;
                    sound = Some("item.dye.use");
                } else {
                    handled = false;
                }
            }
            Species::Wolf
                if item == "minecraft:shears"
                    && m.extra.tame
                    && m.extra.owner == Some(acting)
                    && m.extra.head.is_some()
                    && !sneaking =>
            {
                let mut stack = ItemStack::new("minecraft:wolf_armor", 1);
                stack.damage = m.extra.body_damage.max(0) as u32;
                stack.extra = m.extra.body_extra.take();
                armor_drop = Some(stack);
                m.extra.head = None;
                m.extra.body_damage = 0;
                eat = Some(("item.armor.unequip_wolf", 1.0, 1.0));
                self.damage_held_tool();
            }
            Species::Wolf if m.extra.tame && is_food && m.health < m.attrs.max_health => {
                let heal = self
                    .res
                    .items()
                    .get(&item)
                    .and_then(|i| i.food.as_ref())
                    .map_or(2.0, |f| f.0 as f32 * 2.0);
                m.health = (m.health + heal).min(m.attrs.max_health);
                consume = true;
            }
            Species::Wolf
                if item == "minecraft:wolf_armor"
                    && m.extra.tame
                    && m.extra.owner == Some(acting)
                    && m.extra.head.is_none()
                    && !m.is_baby() =>
            {
                m.extra.head = Some(item.as_str().into());
                m.extra.body_damage = held_damage;
                m.extra.body_extra = held_extra;
                consume = true;
                eat = Some(("item.armor.equip_wolf", 1.0, 1.0));
            }
            Species::Wolf
                if item == "minecraft:armadillo_scute"
                    && m.extra.tame
                    && m.extra.owner == Some(acting)
                    && m.extra.sitting
                    && m.extra.head.is_some()
                    && m.extra.body_damage > 0 =>
            {
                m.extra.body_damage = (m.extra.body_damage
                    - (vise_client::mobs::WOLF_ARMOR_DURABILITY as f32 * 0.125) as i32)
                    .max(0);
                consume = true;
                eat = Some(("item.wolf_armor.repair", 1.0, 1.0));
            }
            Species::Wolf
                if m.extra.tame
                    && m.extra.owner == Some(acting)
                    && item.ends_with("_dye")
                    && vise_client::mobs::render::DYE_NAMES
                        .iter()
                        .position(|d| {
                            item.trim_start_matches("minecraft:")
                                .trim_end_matches("_dye")
                                == *d
                        })
                        .is_some_and(|c| c as u8 != m.extra.color) =>
            {
                let name = item
                    .trim_start_matches("minecraft:")
                    .trim_end_matches("_dye");
                if let Some(c) = vise_client::mobs::render::DYE_NAMES
                    .iter()
                    .position(|d| *d == name)
                {
                    m.extra.color = c as u8;
                }
                consume = true;
            }
            Species::Wolf
                if m.extra.tame && m.extra.owner == Some(acting) && !(is_food && takes_food(m)) =>
            {
                m.extra.sitting = !m.extra.sitting;
                m.ai.jumping = false;
                m.nav_stop();
                m.ai.target = None;
                self.world.modified.insert((
                    m.position.x.floor() as i32 >> 4,
                    m.position.z.floor() as i32 >> 4,
                ));
            }
            Species::Wolf
                if !m.extra.tame
                    && item == "minecraft:bone"
                    && !vise_client::mobs::tame::angry(m) =>
            {
                consume = true;
                let tamed = m.random.next_int_bound(3).unwrap_or(1) == 0;
                if tamed {
                    vise_client::mobs::tame::set_tame(m, true);
                    m.extra.owner = Some(acting);
                    m.persistent = true;
                    m.nav_stop();
                    m.ai.target = None;
                    m.extra.sitting = true;
                }
                self.mob_event_particles(self.mobs[index].id, if tamed { 7 } else { 6 });
                if tamed {
                    let view = self.mob_entity(&self.mobs[index]);
                    self.trigger_entity("minecraft:tame_animal", "entity", &view);
                }
                let m = &self.mobs[index];
                self.world.modified.insert((
                    m.position.x.floor() as i32 >> 4,
                    m.position.z.floor() as i32 >> 4,
                ));
            }
            Species::Cat
                if m.extra.tame
                    && m.extra.owner == Some(acting)
                    && item.ends_with("_dye")
                    && vise_client::mobs::render::DYE_NAMES
                        .iter()
                        .position(|d| {
                            item.trim_start_matches("minecraft:")
                                .trim_end_matches("_dye")
                                == *d
                        })
                        .is_some_and(|c| c as u8 != m.extra.color) =>
            {
                let name = item
                    .trim_start_matches("minecraft:")
                    .trim_end_matches("_dye");
                if let Some(c) = vise_client::mobs::render::DYE_NAMES
                    .iter()
                    .position(|d| *d == name)
                {
                    m.extra.color = c as u8;
                }
                m.persistent = true;
                consume = true;
            }
            Species::Cat
                if m.extra.tame
                    && m.extra.owner == Some(acting)
                    && is_food
                    && m.health < m.attrs.max_health =>
            {
                let heal = self
                    .res
                    .items()
                    .get(&item)
                    .and_then(|i| i.food.as_ref())
                    .map_or(1.0, |f| f.0 as f32);
                m.health = (m.health + heal).min(m.attrs.max_health);
                consume = true;
                eat = eating_sound(m, &mut self.world.random);
            }
            Species::Cat
                if m.extra.tame && m.extra.owner == Some(acting) && !(is_food && takes_food(m)) =>
            {
                m.extra.sitting = !m.extra.sitting;
                self.world.modified.insert((
                    m.position.x.floor() as i32 >> 4,
                    m.position.z.floor() as i32 >> 4,
                ));
            }
            Species::Parrot
                if m.extra.tame
                    && m.extra.owner == Some(acting)
                    && m.on_ground
                    && !parrot_poisonous =>
            {
                m.extra.sitting = !m.extra.sitting;
                self.world.modified.insert((
                    m.position.x.floor() as i32 >> 4,
                    m.position.z.floor() as i32 >> 4,
                ));
            }
            Species::Cat if !m.extra.tame && is_food => {
                consume = true;
                m.persistent = true;
                let tamed = m.random.next_int_bound(3).unwrap_or(1) == 0;
                if tamed {
                    m.extra.tame = true;
                    m.extra.owner = Some(acting);
                    m.extra.sitting = true;
                }
                eat = eating_sound(m, &mut self.world.random);
                self.mob_event_particles(self.mobs[index].id, if tamed { 7 } else { 6 });
                if tamed {
                    let view = self.mob_entity(&self.mobs[index]);
                    self.trigger_entity("minecraft:tame_animal", "entity", &view);
                }
                let m = &self.mobs[index];
                self.world.modified.insert((
                    m.position.x.floor() as i32 >> 4,
                    m.position.z.floor() as i32 >> 4,
                ));
            }
            Species::IronGolem
                if item == "minecraft:iron_ingot" && m.health < m.attrs.max_health =>
            {
                m.health = (m.health + 25.0).min(m.attrs.max_health);
                consume = true;
                sound = Some("entity.iron_golem.repair");
            }
            Species::Creeper if creeper_igniter => {
                m.extra.flag = true;
                let event = if item == "minecraft:fire_charge" {
                    "item.firecharge.use"
                } else {
                    "item.flintandsteel.use"
                };
                eat = Some((event, 1.0, m.random.next_float() * 0.4 + 0.8));
                if damageable {
                    self.damage_held_tool();
                } else {
                    consume = true;
                }
            }
            Species::Parrot
                if self
                    .res
                    .items()
                    .item_tags
                    .get("minecraft:parrot_poisonous_food")
                    .is_some_and(|t| t.contains(&*item)) =>
            {
                consume = true;
                m.add_effect("minecraft:poison", 900, 0);
                let id = m.id;
                let hurt = m.damage(
                    f32::MAX,
                    vise_client::mobs::Damage::player(player_at.unwrap_or(center)),
                );
                if hurt {
                    self.last_hurt_by_player.insert(id, self.lan_acting);
                }
            }
            Species::Parrot if !m.extra.tame && is_food => {
                consume = true;
                if !m.silent {
                    let pitch = 1.0 + (m.random.next_float() - m.random.next_float()) * 0.2;
                    eat = Some(("entity.parrot.eat", 1.0, pitch));
                }
                let tamed = m.random.next_int_bound(10).unwrap_or(1) == 0;
                if tamed {
                    m.extra.tame = true;
                    m.extra.owner = Some(acting);
                }
                self.mob_event_particles(self.mobs[index].id, if tamed { 7 } else { 6 });
                if tamed {
                    let view = self.mob_entity(&self.mobs[index]);
                    self.trigger_entity("minecraft:tame_animal", "entity", &view);
                }
            }
            Species::Dolphin if fish => {
                eat = Some(("entity.dolphin.eat", 1.0, 1.0));
                if vise_client::mobs::ai::can_age_up(m) {
                    let secs = ((-m.extra.age / 20) as f32 * 0.1) as i32;
                    vise_client::mobs::ai::age_up(m, secs, true);
                } else {
                    m.extra.flag = true;
                }
                consume = true;
            }
            Species::Tadpole if is_food && !m.extra.age_locked => {
                consume = true;
                let left = 24_000 - m.extra.age;
                m.extra.age += ((left / 20) as f32 * 0.1) as i32 * 20;
                self.particles_around_mob(
                    self.mobs[index].id,
                    "happy_villager",
                    vise_client::particles::Burst::HORSE_EATING,
                );
            }
            Species::Bee
                if is_food
                    && matches!(
                        &*item,
                        "minecraft:wither_rose" | "minecraft:open_eyeblossom"
                    ) =>
            {
                if &*item == "minecraft:wither_rose" {
                    m.add_effect("minecraft:wither", 40, 0);
                } else {
                    m.add_effect("minecraft:poison", 25, 0);
                }
                consume = true;
            }
            _ if is_food
                && m.kind.flags.animal
                && species != Species::Parrot
                && (species != Species::HappyGhast || m.is_baby()) =>
            {
                match feed_animal(m, &mut self.world.random) {
                    Some(sound) => {
                        consume = true;
                        eat = sound;
                        if species == Species::Axolotl && &*item == "minecraft:tropical_fish_bucket"
                        {
                            give = Some("minecraft:water_bucket");
                        }
                        if self.mobs[index].extra.in_love > 0 {
                            self.love_cause.insert(self.mobs[index].id, self.lan_acting);
                            self.mob_event_particles(self.mobs[index].id, 18);
                        }
                    }
                    None => return Ok(true),
                }
            }
            _ => handled = false,
        }
        if !handled {
            return Ok(false);
        }
        if let Some(stack) = &held {
            let target = self.mob_entity(&self.mobs[index]);
            self.trigger_item_entity("minecraft:player_interacted_with_entity", stack, &target);
        }
        if baby_turtle && !self.mobs[index].is_baby() {
            self.handle_mob_events(vec![MobEvent::GiftLoot {
                mob: self.mobs[index].id,
                table: "minecraft:gameplay/turtle_grow",
                at: feet,
            }])?;
        }
        self.swing_arm();
        if let Some(s) = sound {
            let pitch =
                (self.world.random.next_float() - self.world.random.next_float()) * 0.2 + 1.0;
            self.audio
                .play(&format!("minecraft:{s}"), Some(center), 1.0, pitch);
        }
        if let Some((s, volume, pitch)) = eat {
            self.audio
                .play(&format!("minecraft:{s}"), Some(feet), volume, pitch);
        }
        if consume
            && !creative
            && let Some(stack) = self.stack_in_mut(selected).as_mut()
        {
            stack.count -= 1;
            if stack.count == 0 {
                *self.stack_in_mut(selected) = None;
            }
        }
        if let Some(g) = give {
            let stack = ItemStack {
                item: g.into(),
                count: 1,
                damage: 0,
                extra: None,
            };
            if self.stack_in(selected).is_none() {
                *self.stack_in_mut(selected) = Some(stack);
            } else if let Some(rest) = self.give(stack) {
                let at = self.eye_position().unwrap_or(center);
                self.spawn_item(rest, at, [0.0, 0.0, 0.0], 40);
            }
        }
        if let Some(stack) = armor_drop {
            let target = self.mob_entity(&self.mobs[index]);
            self.trigger_item_entity("minecraft:player_sheared_equipment", &stack, &target);
            self.spawn_item(stack, [center[0], center[1] + 0.5, center[2]], [0.0; 3], 10);
        }
        for (item, count) in drops {
            for _ in 0..count {
                let v = [
                    (self.world.random.next_float() - self.world.random.next_float()) as f64 * 0.1,
                    self.world.random.next_float() as f64 * 0.05,
                    (self.world.random.next_float() - self.world.random.next_float()) as f64 * 0.1,
                ];
                self.spawn_item(
                    ItemStack {
                        item: item.as_str().into(),
                        count: 1,
                        damage: 0,
                        extra: None,
                    },
                    [center[0], center[1] + 0.5, center[2]],
                    v,
                    10,
                );
            }
        }
        Ok(true)
    }
    fn interact_shear_mob(&mut self, index: usize, item: &str) -> bool {
        let m = &self.mobs[index];
        if item != "minecraft:shears" || !m.alive() || m.extra.sheared {
            return false;
        }
        let (sound, table, height) = match m.species() {
            Species::SnowGolem => (
                "minecraft:entity.snow_golem.shear",
                "minecraft:shearing/snow_golem",
                m.eye_height(),
            ),
            Species::Bogged => (
                "minecraft:entity.bogged.shear",
                "minecraft:shearing/bogged",
                f64::from(m.height()),
            ),
            _ => return false,
        };
        let at = m.position;
        let tool = self.held().cloned();
        self.audio.play(sound, Some([at.x, at.y, at.z]), 1.0, 1.0);
        self.mobs[index].extra.sheared = true;
        for stack in self.sequence_loot(table, table, tool.as_ref()) {
            self.spawn_at_location(at, height, stack);
        }
        let ctx = self.who_context(Some(vise_client::mobs::Who::Player), None);
        self.post_game_event("minecraft:shear", [at.x, at.y, at.z], ctx);
        self.damage_held_tool();
        true
    }
    fn interact_armadillo(&mut self, index: usize, item: &str) -> bool {
        let m = &self.mobs[index];
        if item != "minecraft:brush"
            || m.species() != Species::Armadillo
            || !m.alive()
            || m.is_baby()
        {
            return false;
        }
        let (id, at) = (m.id, m.position);
        let feet = [at.x, at.y, at.z];
        let tool = self.held().cloned();
        let table = "minecraft:brush/armadillo";
        for d in self.sequence_loot(table, table, tool.as_ref()) {
            self.spawn_at_location(at, 0.0, d);
        }
        self.audio
            .play("minecraft:entity.armadillo.brush", Some(feet), 1.0, 1.0);
        let ctx = self.who_context(Some(Who::Mob(id)), None);
        self.post_game_event("minecraft:entity_interact", feet, ctx);
        if !self.creative
            && let Some(stack) = tool
        {
            let max = self
                .res
                .items()
                .get(&stack.item)
                .map_or(0, |i| i.max_damage);
            if max > 0 {
                self.damage_held(16);
            }
        }
        true
    }
    pub(super) fn damage_held_tool(&mut self) {
        self.damage_held(1);
    }
    pub(super) fn throw_item(&mut self, stack: &ItemStack) -> Result<bool> {
        let item = &*stack.item;
        let kind = match item {
            "minecraft:snowball" => ProjectileKind::Snowball,
            "minecraft:egg" | "minecraft:blue_egg" | "minecraft:brown_egg" => ProjectileKind::Egg,
            "minecraft:ender_pearl" => ProjectileKind::EnderPearl,
            "minecraft:wind_charge" => {
                let thrown = self.throw_wind_charge()?;
                if thrown {
                    self.apply_use_cooldown(stack);
                }
                return Ok(thrown);
            }
            _ if item.ends_with("_spawn_egg") => return self.use_spawn_egg(stack),
            _ => return Ok(false),
        };
        let Some(eye) = self.eye_position() else {
            return Ok(false);
        };
        let Some(p) = self.player.as_ref() else {
            return Ok(false);
        };
        let on_ground = p.ground.on_ground;
        let velocity = self.known_player_movement();
        let start = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let mut proj = Projectile::new(kind.clone(), start, Some(Who::Player));
        proj.shoot_from_rotation(
            &mut self.world.random,
            self.pitch,
            self.yaw,
            0.0,
            1.5,
            1.0,
            velocity,
            on_ground,
        );
        self.add_projectile(proj);
        self.player_projectile_shot();
        self.apply_use_cooldown(stack);
        let sound = match kind {
            ProjectileKind::Snowball => "minecraft:entity.snowball.throw",
            ProjectileKind::Egg => "minecraft:entity.egg.throw",
            _ => "minecraft:entity.ender_pearl.throw",
        };
        let pitch = 0.4 / (self.world.random.next_float() * 0.4 + 0.8);
        self.audio.play(sound, Some(eye), 0.5, pitch);
        self.swing_arm();
        if !self.creative
            && let Some(s) = self.held_mut().as_mut()
        {
            s.count -= 1;
            if s.count == 0 {
                *self.held_mut() = None;
            }
        }
        Ok(true)
    }
    fn spawn_egg_offspring(&mut self, index: usize) -> bool {
        let Some(stack) = self.held().cloned() else {
            return false;
        };
        let parent = &self.mobs[index];
        if !parent.alive()
            || !vise_client::mobs::finalize::spawn_egg_is_for(&stack.item, &parent.kind.id)
            || !vise_client::mobs::drops::can_be_baby(parent.species())
            || parent.species() == Species::WanderingTrader
        {
            return false;
        }
        let (kind, at) = (
            parent.kind.id.clone(),
            [parent.position.x, parent.position.y, parent.position.z],
        );
        let (species, variant, color) =
            (parent.species(), parent.extra.variant, parent.extra.color);
        let target = self.mob_entity(&self.mobs[index]);
        let Some(id) = self.spawn_mob_spawned(&kind, at) else {
            return false;
        };
        if let Some(child) = self.mobs.iter_mut().find(|m| m.id == id) {
            vise_client::mobs::finalize::spawn_egg_offspring(child, species, variant, color);
        }
        self.trigger_item_entity("minecraft:player_interacted_with_entity", &stack, &target);
        self.consume_held();
        true
    }
    fn use_spawn_egg(&mut self, stack: &ItemStack) -> Result<bool> {
        let Some(hit) = self.pick() else {
            return Ok(false);
        };
        let kind = stack.item.trim_end_matches("_spawn_egg").to_owned();
        let id = format!("minecraft:{}", kind.trim_start_matches("minecraft:"));
        if self.ui.difficulty == 0
            && let Some(t) = self.res.mob_types().get(&id)
            && !vise_client::mobs::spawn_rules::allowed_in_peaceful(t.species)
        {
            return Ok(true);
        }
        let (spawner_id, data) = vise_client::block_entity_item::spawn_egg_on_spawner(
            stack,
            self.acting_player_is_op(),
        );
        if let Some(done) = self.use_spawn_egg_on_spawner(hit.pos, &spawner_id, data.as_ref()) {
            if done {
                self.swing_arm();
                self.stats.add("used", &stack.item, 1);
                self.consume_held();
            }
            return Ok(true);
        }
        let p = [
            hit.pos[0] + hit.face[0],
            hit.pos[1] + hit.face[1],
            hit.pos[2] + hit.face[2],
        ];
        let at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]),
            f64::from(p[2]) + 0.5,
        ];
        let Some(mob_id) = self.spawn_mob_spawned(&kind, at) else {
            return Ok(false);
        };
        if let Some(name) = stack.custom_name()
            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == mob_id)
        {
            m.custom_name = Some(name.to_owned());
        }
        self.swing_arm();
        if !self.creative
            && let Some(s) = self.held_mut().as_mut()
        {
            s.count -= 1;
            if s.count == 0 {
                *self.held_mut() = None;
            }
        }
        Ok(true)
    }
    fn beam_target(&self, who: Option<Who>, alpha: f64) -> Option<glam::DVec3> {
        match who? {
            Who::Player => {
                let p = self.player.as_ref()?;
                let (a, b) = (self.previous_position, p.position);
                Some(glam::DVec3::new(
                    a.x + (b.x - a.x) * alpha,
                    a.y + (b.y - a.y) * alpha + f64::from(p.dimensions.height) * 0.5,
                    a.z + (b.z - a.z) * alpha,
                ))
            }
            Who::Mob(id) => {
                let m = self.mobs.iter().find(|m| m.id == id)?;
                Some(glam::DVec3::new(
                    m.previous.x + (m.position.x - m.previous.x) * alpha,
                    m.previous.y
                        + (m.position.y - m.previous.y) * alpha
                        + f64::from(m.height()) * 0.5,
                    m.previous.z + (m.position.z - m.previous.z) * alpha,
                ))
            }
        }
    }
    pub(super) fn guardian_bubbles(&mut self) {
        use vise_client::mobs::guardian;
        use vise_client::particles::{ParticleOptions, ParticleType};
        let mut swimming = Vec::new();
        let mut beams = Vec::new();
        for m in &self.mobs {
            if !m.alive() || !matches!(m.species(), Species::Guardian | Species::ElderGuardian) {
                continue;
            }
            let at = [m.position.x, m.position.y, m.position.z];
            if m.extra.flag2 && m.fluids.in_water {
                let (width, height, _) = m.dims();
                swimming.push((at, width, height, m.view_vector()));
            }
            if let Some((_, scale)) = guardian::beam(m, 0.0)
                && let Some(t) = self.beam_target(m.ai.target, 1.0)
            {
                beams.push(([at[0], m.eye_y(), at[2]], [t.x, t.y, t.z], f64::from(scale)));
            }
        }
        let bubble = ParticleOptions::simple(ParticleType::Bubble);
        for (at, width, height, view) in swimming {
            for _ in 0..2 {
                let (x, y, z) = {
                    let r = self.particles.random();
                    (r.next_double(), r.next_double(), r.next_double())
                };
                let p = [
                    at[0] + width * (2.0 * x - 1.0) * 0.5 - view[0] * 1.5,
                    at[1] + height * y - view[1] * 1.5,
                    at[2] + width * (2.0 * z - 1.0) * 0.5 - view[2] * 1.5,
                ];
                self.particles.add(&bubble, p, [0.0; 3]);
            }
        }
        for (eye, target, scale) in beams {
            let d = [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]];
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            if length <= 0.0 {
                continue;
            }
            let mut dist = self.particles.random().next_double();
            while dist < length {
                let r = self.particles.random();
                dist += 1.8 - scale + r.next_double() * (1.7 - scale);
                let f = dist / length;
                let p = [eye[0] + d[0] * f, eye[1] + d[1] * f, eye[2] + d[2] * f];
                self.particles.add(&bubble, p, [0.0; 3]);
            }
        }
    }
    pub(super) fn mob_geometry(&self, camera: glam::DVec3, alpha: f64, out: &mut Vec<Vertex>) {
        let atlas = &self.gui.atlas;
        let lookup = |name: &str| atlas.sprites.get(name).map(|r| [r.u0, r.v0, r.u1, r.v1]);
        let scale = self.view_scale();
        for mob in &self.mobs {
            let entity_start = out.len();
            let outline = self.mob_outline(mob);
            if mob.invisible && outline.is_none() {
                continue;
            }
            if self.hides_as_camera(mob.id) {
                continue;
            }
            let d = glam::DVec3::new(mob.position.x, mob.position.y, mob.position.z) - camera;
            let (w, h, _) = mob.dims();
            let edge = (w + h + w) / 3.0
                * if &*mob.kind.id == "minecraft:player" {
                    10.0
                } else {
                    1.0
                };
            let range = edge * 64.0 * scale;
            if d.length_squared() >= range * range {
                continue;
            }
            if let Some(view) = &self.view_frustum
                && vise_client::mobs::guardian::beam(mob, alpha as f32).is_none()
            {
                let lo =
                    d.as_vec3() - glam::Vec3::new(w as f32 / 2.0 + 0.5, 0.5, w as f32 / 2.0 + 0.5);
                let hi = d.as_vec3()
                    + glam::Vec3::new(w as f32 / 2.0 + 0.5, h as f32 + 0.5, w as f32 / 2.0 + 0.5);
                if !view.intersects(lo, hi) {
                    continue;
                }
            }
            self.mark_entity(entity_start, &mob.kind.id);
            let light = self.world.light(
                mob.position.x.floor() as i32,
                (mob.position.y + mob.eye_height()).floor() as i32,
                mob.position.z.floor() as i32,
            );
            let mut packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            if mob.fire_ticks > 0 || matches!(mob.species(), Species::Blaze | Species::MagmaCube) {
                packed = (15 * 16) | ((15 * 16) << 8);
            }
            if mob.species() == Species::GlowSquid {
                let lit = (packed >> 12) & 15;
                let block = vise_client::mobs::aquatic::glow_squid_block_light(mob, lit);
                packed = (packed & 0xFF) | ((block * 16) << 8);
            }
            if mob.species() == Species::SulfurCube
                && !vise_client::mobs::trader::is_invisible(mob)
            {
                self.sulfur_cube_body_geometry(
                    mob,
                    alpha as f32,
                    vise_client::mobs::render::base_transform(mob, alpha as f32, camera),
                    packed,
                    out,
                );
            }
            let armor = if &*mob.kind.id == "minecraft:player" {
                self.worn_armor_layers(mob)
            } else {
                Vec::new()
            };
            vise_client::mobs::render::render_with(
                mob,
                alpha as f32,
                camera,
                &lookup,
                packed,
                self.res.mob_data(),
                &armor,
                out,
            );
            if &*mob.kind.id == "minecraft:player" {
                let chest = mob.extra.chest.as_deref();
                self.player_cape_geometry(mob, chest, camera, alpha as f32, packed, out);
                self.player_elytra_geometry(mob, chest, camera, alpha as f32, packed, out);
                self.spin_attack_geometry(mob, camera, alpha as f32, packed, out);
            }
            self.mob_block_geometry(mob, camera, alpha, packed, out);
            if let Some(rgb) = outline {
                self.mark_outline(&out[entity_start..], rgb);
                if mob.invisible {
                    out.truncate(entity_start);
                    continue;
                }
            }
            if outline.is_none()
                && vise_client::mobs::guardian::beam(mob, alpha as f32).is_none()
            {
                self.cull_entity_vertices(out, entity_start);
            }
            if let Some((time, scale)) = vise_client::mobs::guardian::beam(mob, alpha as f32)
                && let Some(target) = self.beam_target(mob.ai.target, alpha)
                && let Some(uv) = lookup("entity/guardian/guardian_beam")
            {
                let a = alpha;
                let eye = glam::DVec3::new(
                    mob.previous.x + (mob.position.x - mob.previous.x) * a,
                    mob.previous.y + (mob.position.y - mob.previous.y) * a + mob.eye_height(),
                    mob.previous.z + (mob.position.z - mob.previous.z) * a,
                ) - camera;
                vise_client::mobs::guardian::beam_geometry(
                    eye.as_vec3(),
                    (target - camera).as_vec3(),
                    time,
                    scale,
                    uv,
                    out,
                );
            }
        }
        self.elder_apparition_geometry(alpha, out);
        self.spawner_geometry(camera, alpha, out);
        let (right, up) = thrown_item_axes(self.camera_type, self.yaw, self.pitch);
        let live = self.creatures.projectiles.iter().map(|a| {
            let at = [
                a.previous.x + (a.position.x - a.previous.x) * alpha,
                a.previous.y + (a.position.y - a.previous.y) * alpha,
                a.previous.z + (a.position.z - a.previous.z) * alpha,
            ];
            (a, at)
        });
        let taken = self.pickups.live.iter().filter_map(|p| match &p.taken {
            super::pickups::Taken::Arrow(a) => Some((&**a, super::pickups::position(p, alpha))),
            _ => None,
        });
        let mut glowing = None;
        for (a, at) in live.chain(taken) {
            if let Some((start, rgb)) = glowing.take() {
                self.mark_outline(&out[start..], rgb);
            }
            if a.id != 0 && self.entity_glowing(a.id) {
                glowing = Some((out.len(), super::glowing::team_color(self.mob_team(a.id))));
            }
            let p = glam::DVec3::from_array(at) - camera;
            let c = p.as_vec3();
            let light = self.world.light(
                a.position.x.floor() as i32,
                a.position.y.floor() as i32,
                a.position.z.floor() as i32,
            );
            let mut packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            if a.is_fireball() {
                packed = (15 * 16) | ((15 * 16) << 8);
            }
            if a.is_arrow() {
                let Some(model) = self.arrow_model.as_ref() else {
                    continue;
                };
                let texture = if a.is_spectral_arrow() {
                    "entity/projectiles/arrow_spectral"
                } else if a.arrow.as_ref().is_some_and(|s| s.effect_color > 0) {
                    "entity/projectiles/arrow_tipped"
                } else {
                    "entity/projectiles/arrow"
                };
                let Some(r) = atlas.sprites.get(texture).copied() else {
                    continue;
                };
                let m = glam::Mat4::from_translation(c)
                    * glam::Mat4::from_rotation_y((a.yaw - 90.0).to_radians())
                    * glam::Mat4::from_rotation_z(a.pitch.to_radians());
                let shake = a
                    .arrow
                    .as_ref()
                    .map_or(0.0, |s| s.shake as f32 - alpha as f32);
                let roll = if shake > 0.0 {
                    -(shake * 3.0).msin() * shake * (std::f32::consts::PI / 180.0)
                } else {
                    0.0
                };
                let pose_for = |part: &vise_client::mobs::Part| {
                    let mut pose = part.pose;
                    if std::ptr::eq(part, model) {
                        pose[5] += roll;
                    }
                    pose
                };
                vise_client::mobs::render_model(
                    model,
                    m,
                    &pose_for,
                    [r.u0, r.v0, r.u1, r.v1],
                    packed,
                    0,
                    out,
                );
                if a.arrow.as_ref().is_some_and(|s| s.on_fire()) {
                    flame_geometry(atlas, c, 0.5, 0.5, self.yaw, light, out);
                }
                continue;
            }
            if matches!(a.kind, ProjectileKind::Trident) {
                let Some(model) = self.trident_model.as_ref() else {
                    continue;
                };
                let Some(r) = atlas.sprites.get("entity/trident/trident").copied() else {
                    continue;
                };
                let m = glam::Mat4::from_translation(c)
                    * glam::Mat4::from_rotation_y((a.yaw - 90.0).to_radians())
                    * glam::Mat4::from_rotation_z((a.pitch + 90.0).to_radians());
                let poses = vise_client::mobs::anim::Poses::new(model);
                vise_client::mobs::render::draw(
                    model,
                    m,
                    &poses,
                    [r.u0, r.v0, r.u1, r.v1],
                    0xFFFF_FFFF,
                    packed,
                    0,
                    out,
                );
                continue;
            }
            if matches!(
                a.kind,
                ProjectileKind::WindCharge | ProjectileKind::BreezeWindCharge
            ) {
                self.wind_charge_geometry(a, c, packed, out);
                continue;
            }
            if matches!(a.kind, ProjectileKind::ShulkerBullet { .. }) {
                static BULLET: std::sync::OnceLock<Option<vise_client::mobs::Part>> =
                    std::sync::OnceLock::new();
                let model = BULLET.get_or_init(|| {
                    vise_client::mobs::load_model(
                        &self.options.resources,
                        "minecraft:shulker_bullet#main",
                    )
                });
                if let (Some(model), Some(r)) =
                    (model, atlas.sprites.get("entity/shulker/spark").copied())
                {
                    use vise_core::geometry::trig;
                    let tc = a.age as f32 + alpha as f32;
                    let mut poses = vise_client::mobs::anim::Poses::new(model);
                    poses.yrot("main", a.yaw.to_radians());
                    poses.xrot("main", a.pitch.to_radians());
                    let light = (packed & 0xFF) | ((15 * 16) << 8);
                    let base = glam::Mat4::from_translation(c)
                        * glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.15, 0.0))
                        * glam::Mat4::from_rotation_y(
                            (trig::sin(f64::from(tc * 0.1)) * 180.0).to_radians(),
                        )
                        * glam::Mat4::from_rotation_x(
                            (trig::cos(f64::from(tc * 0.1)) * 180.0).to_radians(),
                        )
                        * glam::Mat4::from_rotation_z(
                            (trig::sin(f64::from(tc * 0.15)) * 360.0).to_radians(),
                        )
                        * glam::Mat4::from_scale(glam::Vec3::new(-0.5, -0.5, 0.5));
                    let uv = [r.u0, r.v0, r.u1, r.v1];
                    vise_client::mobs::render::draw(
                        model,
                        base,
                        &poses,
                        uv,
                        0xFFFF_FFFF,
                        light,
                        0,
                        out,
                    );
                    vise_client::mobs::render::draw(
                        model,
                        base * glam::Mat4::from_scale(glam::Vec3::splat(1.5)),
                        &poses,
                        uv,
                        0x26FF_FFFF,
                        light,
                        0,
                        out,
                    );
                }
                continue;
            }
            if let Some(dangerous) = super::wither::skull_kind(a) {
                static SKULL: std::sync::OnceLock<Option<vise_client::mobs::Part>> =
                    std::sync::OnceLock::new();
                let model = SKULL.get_or_init(|| {
                    vise_client::mobs::load_model(
                        &self.options.resources,
                        "minecraft:wither_skull#main",
                    )
                });
                let tex = if dangerous {
                    "entity/wither/wither_invulnerable"
                } else {
                    "entity/wither/wither"
                };
                if let (Some(model), Some(r)) = (model, atlas.sprites.get(tex).copied()) {
                    let mut poses = vise_client::mobs::anim::Poses::new(model);
                    poses.yrot("head", a.yaw.to_radians());
                    poses.xrot("head", a.pitch.to_radians());
                    let base = glam::Mat4::from_translation(c)
                        * glam::Mat4::from_scale(glam::Vec3::new(-1.0, -1.0, 1.0));
                    vise_client::mobs::render::draw(
                        model,
                        base,
                        &poses,
                        [r.u0, r.v0, r.u1, r.v1],
                        0xFFFF_FFFF,
                        packed,
                        0,
                        out,
                    );
                }
                continue;
            }
            if let Some(stack) = a.potion_item() {
                let owner = super::item_draw::owner_at(at, 0.0, "minecraft:splash_potion");
                let state = self.resolve_item(&stack, DisplayContext::Ground, Some(&owner), 0);
                let pose = glam::Mat4::from_translation(c)
                    * glam::Mat4::from_quat(self.camera_orientation());
                self.submit_item_world(&state, pose, packed, out);
                continue;
            }
            let Some(name) = a.item_sprite() else {
                continue;
            };
            let Some(r) = atlas.sprites.get(name).copied() else {
                continue;
            };
            let lift = is_item_billboard(&a.kind);
            let quad = thrown_item_quad(c, right, up, a.sprite_scale(), lift);
            for (v, uv) in quad
                .iter()
                .zip([[r.u0, r.v1], [r.u1, r.v1], [r.u1, r.v0], [r.u0, r.v0]])
            {
                out.push(Vertex {
                    normal: [0; 4],
                    pos: v.to_array(),
                    uv,
                    layer: 0,
                    color: 0xFFFF_FFFF,
                    light: packed,

                    glint_uv: [0.0, 0.0],
                });
            }
        }
        if let Some((start, rgb)) = glowing {
            self.mark_outline(&out[start..], rgb);
        }
    }
}
fn thrown_item_axes(
    camera_type: super::perspective::CameraType,
    yaw: f32,
    pitch: f32,
) -> (glam::Vec3, glam::Vec3) {
    let orientation = super::camera_rotation(camera_type, yaw, pitch);
    (orientation * glam::Vec3::X, orientation * glam::Vec3::Y)
}
fn thrown_item_quad(
    at: glam::Vec3,
    right: glam::Vec3,
    up: glam::Vec3,
    scale: f32,
    lift: bool,
) -> [glam::Vec3; 4] {
    let s = 0.25 * scale;
    let rise = if lift { 0.5 * s } else { 0.0 };
    let centre = at + up * rise;
    [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
        .map(|k| centre + right * k.0 * s + up * k.1 * s)
}
fn is_item_billboard(kind: &ProjectileKind) -> bool {
    matches!(
        kind,
        ProjectileKind::Snowball
            | ProjectileKind::Egg
            | ProjectileKind::EnderPearl
            | ProjectileKind::SmallFireball
            | ProjectileKind::LargeFireball { .. }
    )
}
fn bucketable(species: Species) -> Option<(&'static str, &'static str)> {
    let fish = "minecraft:item.bucket.fill_fish";
    Some(match species {
        Species::Cod => ("minecraft:cod_bucket", fish),
        Species::Salmon => ("minecraft:salmon_bucket", fish),
        Species::Pufferfish => ("minecraft:pufferfish_bucket", fish),
        Species::TropicalFish => ("minecraft:tropical_fish_bucket", fish),
        Species::Tadpole => (
            "minecraft:tadpole_bucket",
            "minecraft:item.bucket.fill_tadpole",
        ),
        Species::Axolotl => (
            "minecraft:axolotl_bucket",
            "minecraft:item.bucket.fill_axolotl",
        ),
        _ => return None,
    })
}
pub(super) fn damage_type(kind: vise_client::mobs::DamageKind, player: bool) -> &'static str {
    use vise_client::mobs::DamageKind as K;
    match kind {
        K::PlayerAttack => "minecraft:player_attack",
        K::MaceSmash => "minecraft:mace_smash",
        K::Spear => "minecraft:spear",
        K::MobAttack => "minecraft:mob_attack",
        K::Arrow => "minecraft:arrow",
        K::Thrown => "minecraft:thrown",
        K::Fireball => "minecraft:fireball",
        K::Explosion if player => "minecraft:player_explosion",
        K::Explosion => "minecraft:explosion",
        K::Fall => "minecraft:fall",
        K::FallingBlock => "minecraft:falling_block",
        K::FallingStalactite => "minecraft:falling_stalactite",
        K::OnFire => "minecraft:on_fire",
        K::InFire => "minecraft:in_fire",
        K::Campfire => "minecraft:campfire",
        K::Lava => "minecraft:lava",
        K::Drown => "minecraft:drown",
        K::DryOut => "minecraft:dry_out",
        K::Magic => "minecraft:magic",
        K::IndirectMagic => "minecraft:indirect_magic",
        K::Sting => "minecraft:sting",
        K::Cactus => "minecraft:cactus",
        K::SweetBerryBush => "minecraft:sweet_berry_bush",
        K::Generic => "minecraft:generic",
        K::InWall => "minecraft:in_wall",
        K::Thorns => "minecraft:thorns",
        K::SonicBoom => "minecraft:sonic_boom",
        K::Freeze => "minecraft:freeze",
        K::OutOfWorld => "minecraft:out_of_world",
        K::Cramming => "minecraft:cramming",
        K::FallingAnvil => "minecraft:falling_anvil",
        K::Fireworks => "minecraft:fireworks",
        K::GenericKill => "minecraft:generic_kill",
        K::LightningBolt => "minecraft:lightning_bolt",
        K::OutsideBorder => "minecraft:outside_border",
        K::Spit => "minecraft:spit",
        K::Stalagmite => "minecraft:stalagmite",
        K::Starve => "minecraft:starve",
        K::HotFloor => "minecraft:hot_floor",
        K::SulfurCubeHot => "minecraft:sulfur_cube_hot",
        K::WindCharge => "minecraft:wind_charge",
        K::Wither => "minecraft:wither",
    }
}
fn to_loot(stack: &ItemStack) -> vise_world::loot::LootStack {
    vise_world::loot::LootStack {
        item: stack.item.to_string(),
        count: stack.count as i32,
        damage: stack.damage as i32,
        enchantments: stack
            .enchantments()
            .iter()
            .map(|(e, l)| (e.to_string(), *l as i32))
            .collect(),
        potion: None,
        map: false,
        exploration: None,
        ominous_amplifier: None,
        stew_effects: Vec::new(),
        names: Vec::new(),
        components: Vec::new(),
    }
}
fn from_loot(stack: &vise_world::loot::LootStack) -> ItemStack {
    let mut item = ItemStack {
        item: Arc::from(stack.item.as_str()),
        count: stack.count.max(1) as u32,
        damage: stack.damage.max(0) as u32,
        extra: None,
    };
    for (id, level) in &stack.enchantments {
        item.enchant(id, (*level).max(0) as u32);
    }
    if let Some(potion) = &stack.potion {
        item = item.with_potion(potion);
    }
    if let Some(amplifier) = stack.ominous_amplifier.filter(|a| *a > 0) {
        item.data_mut().ominous_amplifier = Some(amplifier as u8);
    }
    for (id, text) in &stack.names {
        item.data_mut().set_loot_name(id, text);
    }
    for (id, value) in &stack.components {
        item.data_mut().set_loot_component(id, value);
    }
    item
}
impl Game<'_> {
    pub(super) fn portal_spawn(&mut self, pos: [i32; 3]) {
        let centre = [
            f64::from((pos[0] >> 4) * 16 + 8),
            f64::from((pos[2] >> 4) * 16 + 8),
        ];
        let close = self.level_players().iter().any(|p| {
            let (dx, dz) = (p.position.x - centre[0], p.position.z - centre[1]);
            !p.spectator && dx * dx + dz * dz < 16384.0
        });
        if !close {
            return;
        }
        let mut y = pos[1];
        while &*self
            .res
            .facts()
            .get(self.world.block(pos[0], y, pos[2]))
            .name
            == "minecraft:nether_portal"
        {
            y -= 1;
        }
        let below = self.res.facts().get(self.world.block(pos[0], y, pos[2]));
        if below.sturdy_full & (1 << 1) == 0 {
            return;
        }
        let at = [
            f64::from(pos[0]) + 0.5,
            f64::from(y + 1),
            f64::from(pos[2]) + 0.5,
        ];
        if let Some(id) = self.spawn_mob("minecraft:zombified_piglin", at, false) {
            self.set_portal_cooldown(id);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::super::perspective::CameraType;
    use super::thrown_item_axes;
    use super::thrown_item_quad;
    use super::{ProjectileKind, is_item_billboard};

    fn bounds(corners: &[glam::Vec3; 4]) -> (glam::Vec3, glam::Vec3) {
        (
            corners.iter().copied().reduce(glam::Vec3::min).unwrap(),
            corners.iter().copied().reduce(glam::Vec3::max).unwrap(),
        )
    }

    #[test]
    fn thrown_item_billboard_lifts_by_the_generated_ground_transform() {
        assert!(is_item_billboard(&ProjectileKind::Snowball));
        assert!(is_item_billboard(&ProjectileKind::Egg));
        assert!(is_item_billboard(&ProjectileKind::EnderPearl));
        assert!(is_item_billboard(&ProjectileKind::SmallFireball));
        let large = ProjectileKind::LargeFireball { power: 1 };
        assert!(is_item_billboard(&large));
        let at = glam::Vec3::Y * 100.0;
        let (right, up) = (glam::Vec3::X, glam::Vec3::Y);
        let lifted = is_item_billboard(&ProjectileKind::Snowball);
        let (lo, hi) = bounds(&thrown_item_quad(at, right, up, 1.0, lifted));
        assert_eq!((lo.y, hi.y), (99.875, 100.375));
        assert_eq!((lo.x, hi.x), (-0.25, 0.25));
        let (lo, hi) = bounds(&thrown_item_quad(at, right, up, 0.75, lifted));
        assert_eq!((lo.y, hi.y), (99.90625, 100.28125));
        let (lo, hi) = bounds(&thrown_item_quad(at, right, up, 3.0, lifted));
        assert_eq!((lo.y, hi.y), (99.625, 101.125));
        let lifted = is_item_billboard(&ProjectileKind::LlamaSpit);
        assert!(!lifted);
        let (lo, hi) = bounds(&thrown_item_quad(at, right, up, 1.0, lifted));
        assert_eq!((lo.y, hi.y), (99.75, 100.25));
        let (right, up) = (glam::Vec3::Z, glam::Vec3::X);
        let lifted = is_item_billboard(&ProjectileKind::Egg);
        let (lo, hi) = bounds(&thrown_item_quad(at, right, up, 1.0, lifted));
        assert_eq!((lo.x, hi.x), (-0.125, 0.375));
        assert_eq!((lo.y, hi.y), (100.0, 100.0));
    }

    #[test]
    fn thrown_item_billboard_does_not_flip_at_straight_up_or_down() {
        for yaw in [0.0f32, 37.5, 90.0, 180.0, -135.0] {
            for (edge, near) in [(-90.0f32, -89.9f32), (90.0, 89.9)] {
                let (right_edge, up_edge) = thrown_item_axes(CameraType::FirstPerson, yaw, edge);
                let (right_near, up_near) = thrown_item_axes(CameraType::FirstPerson, yaw, near);
                assert!(
                    right_edge.dot(right_near) > 0.99 && up_edge.dot(up_near) > 0.99,
                    "yaw {yaw}, pitch {edge}: {right_edge}/{up_edge} vs {right_near}/{up_near} at {near}"
                );
            }
        }
    }

    #[test]
    fn thrown_item_billboard_faces_the_front_camera() {
        let (right, up) = thrown_item_axes(CameraType::FirstPerson, 0.0, 0.0);
        assert!(right.abs_diff_eq(glam::Vec3::NEG_X, 1e-5), "{right}");
        assert!(up.abs_diff_eq(glam::Vec3::Y, 1e-5), "{up}");
        let (right, up) = thrown_item_axes(CameraType::ThirdPersonFront, 0.0, 0.0);
        assert!(right.abs_diff_eq(glam::Vec3::X, 1e-5), "{right}");
        assert!(up.abs_diff_eq(glam::Vec3::Y, 1e-5), "{up}");
        let (right, _) = thrown_item_axes(CameraType::ThirdPersonBack, 0.0, 0.0);
        assert!(right.abs_diff_eq(glam::Vec3::NEG_X, 1e-5), "{right}");
    }
}

fn killed_by_mob(m: &Mob) -> bool {
    m.last_damage
        .is_some_and(|(d, _)| matches!(d.attacker, Some(Who::Mob(_))))
}

fn breeding_credit(
    love_cause: &mut std::collections::HashMap<u32, Option<u32>>,
    parent: u32,
    partner: u32,
) -> Option<Option<u32>> {
    let from_parent = love_cause.remove(&parent);
    let from_partner = love_cause.remove(&partner);
    from_parent.or(from_partner)
}

#[cfg(test)]
mod breeding_tests {
    use super::*;

    #[test]
    fn the_player_who_fed_either_animal_is_credited() {
        let mut love = std::collections::HashMap::from([(1, Some(7)), (2, Some(9))]);
        assert_eq!(breeding_credit(&mut love, 1, 2), Some(Some(7)));
        assert!(love.is_empty());
        let mut love = std::collections::HashMap::from([(2, None)]);
        assert_eq!(breeding_credit(&mut love, 1, 2), Some(None));
        assert_eq!(breeding_credit(&mut love, 1, 2), None);
    }
}

#[cfg(test)]
mod pearl_tests {
    use super::*;

    #[test]
    fn a_pearl_feels_the_water_its_box_touches() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !root.join("entity-types.json").exists()
            || !root.join("movement-properties.nbt").exists()
        {
            return;
        }
        let assets = vise_client::assets::Assets::load(&root).unwrap();
        let render = assets.reload(&root).unwrap();
        let registry = assets.registry;
        let facts = vise_client::blocks::BlockFacts::load(&root, registry).unwrap();
        let motion = vise_client::motion::MotionResources::load(&root, &assets).unwrap();
        let data = vise_client::mobs::MobData::load(&root, &facts);
        let biomes = vise_client::biome::Biomes::load(&root).unwrap();
        let items = vise_client::items::Items::load(
            &root,
            registry,
            &facts,
            &vise_world::loot::PackOverlay::NONE,
        )
        .unwrap();
        let collision = vise_client::collisions::load(&root, registry).unwrap();
        let mut world = vise_client::world::World::test_scene(&assets, &render, 3).unwrap();
        let water = registry.default_state("minecraft:water").unwrap();
        for x in 20..=40 {
            for z in 0..=2 {
                world.set(x, 4, z, water).unwrap();
            }
        }
        let lid_openers = std::collections::HashMap::new();
        let w = MobWorld {
            teams: Default::default(),
            world: &world,
            registry,
            facts: &facts,
            fluid_tables: &motion.fluids,
            movement: &motion.blocks,
            collision: &collision,
            data: &data,
            player: None,
            nearest_player: None,
            others: &[],
            others_grid: None,
            others_index: None,
            daylight: true,
            monsters_burn: false,
            game_time: 1000,
            day_time: 1000,
            raining: false,
            thundering: false,
            nether: false,
            block_tags: &items.block_tags,
            biomes: &biomes,
            mob_griefing: true,
            mob_drops: true,
            max_entity_cramming: 24,
            difficulty: spawning::LocalDifficulty::new(2, 1000, 0, 0.0),
            end: false,
            world_seed: 0,
            sea_level: 63,
            items: Some(&items),
            lid_openers: &lid_openers,
        };
        let pw = ProjWorld {
            w: &w,
            targets: Vec::new(),
        };
        let pearl = |x: f64, y: f64, velocity: Vec3| {
            let mut p = Projectile::new(ProjectileKind::EnderPearl, Vec3::new(x, y, 1.5), None);
            p.velocity = velocity;
            p
        };
        let mut p = pearl(25.5, 4.95, Vec3::new(0.3, 0.0, 0.0));
        p.tick(&pw);
        assert!(p.position.y > 4.9, "the pearl dipped: {p:?}");
        p.tick(&pw);
        assert!(
            (p.velocity.x - 0.3 * 0.99 * 0.99).abs() < 1e-9,
            "a pearl above the surface slowed as in water: {p:?}"
        );
        let mut p = pearl(19.95, 4.3, Vec3::ZERO);
        p.tick(&pw);
        assert!(
            (p.velocity.y - (-0.03 * 0.99)).abs() < 1e-9,
            "the first tick has the air drag: {p:?}"
        );
        p.tick(&pw);
        assert!(
            (p.velocity.y - (-0.03 * 0.99 - 0.03) * 0.8).abs() < 1e-9,
            "a pearl whose box is in the water kept the air drag: {p:?}"
        );
        let mut p = pearl(30.5, 4.2, Vec3::ZERO);
        p.tick(&pw);
        assert!(
            (p.velocity.y - (-0.03 * 0.99)).abs() < 1e-9,
            "the first tick under water has the air drag: {p:?}"
        );
        p.tick(&pw);
        assert!(
            (p.velocity.y - (-0.03 * 0.99 - 0.03) * 0.8).abs() < 1e-9,
            "the second tick under water has the water drag: {p:?}"
        );
    }
}
