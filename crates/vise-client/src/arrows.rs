use crate::{
    blocks::BlockFacts,
    collisions::SceneCollisions,
    mobs::Who,
    motion::{FluidResources, LAVA, WATER},
    projectiles::{Hit, Projectile, ProjectileKind},
    world::World,
};
use vise_core::{
    geometry::{Aabb, Direction, Vec3, sweep_intersected_blocks},
    position::BlockPos,
    random::{LegacyRandom, RandomSource},
};
use vise_world::{
    block_collision::BlockCollisionRegistry,
    entity_fluid::{
        CurrentTarget, EntityFluidInteraction, FluidInteractionWorld, FluidMembership,
        FluidSections, FluidTagId,
    },
    entity_fluid_tick::{self, EntityFluidTickWorld},
    fluid::{FluidHeightWorld, FluidKind, FluidState},
    fluid_flow::{FluidFlowWorld, flow_at},
    registry::BlockStateId,
    water_splash::{self, SplashParticle, WaterSplashWorld},
};
use std::{collections::HashSet, fmt};

const WIDTH: f32 = 0.5;
const HEIGHT: f32 = 0.5;
const EYE_HEIGHT: f32 = 0.13;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pickup {
    #[default]
    Disallowed,
    Allowed,
    CreativeOnly,
}
impl Pickup {
    pub fn from_byte(b: i64) -> Self {
        match b {
            1 => Self::Allowed,
            2 => Self::CreativeOnly,
            _ => Self::Disallowed,
        }
    }
    pub fn byte(self) -> i8 {
        self as i8
    }
}
pub const INTANGIBLE: &str = "minecraft:intangible_projectile";
pub fn drawn_ammo(ammo: &crate::items::ItemStack, intangible: bool) -> crate::items::ItemStack {
    let mut drawn = crate::items::ItemStack {
        count: 1,
        ..ammo.clone()
    };
    if intangible {
        drawn.data_mut().set_other(
            INTANGIBLE,
            Some(vise_world::nbt::Tag::Compound(Default::default())),
        );
    }
    drawn
}
#[derive(Clone, Debug, PartialEq)]
pub enum Touch {
    Ignore,
    Give(crate::items::ItemStack),
    Take,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ArrowEvent {
    Particle {
        kind: ArrowParticle,
        position: Vec3,
        velocity: Vec3,
    },
    PotionPuff {
        id: u32,
        color: u32,
    },
    Sound {
        sound: ArrowSound,
        volume: f32,
        pitch: f32,
    },
    BubbleColumnParticles {
        pos: BlockPos,
    },
    Block {
        pos: BlockPos,
        reaction: BlockReaction,
    },
    SplashEvent,
    ShootEvent,
    Drop,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowParticle {
    Bubble,
    Crit,
    Splash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowSound {
    Splash,
    HitGround,
    CrossbowHit,
    Extinguish,
    TridentHitGround,
    TridentHit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReaction {
    PrimeTnt,
    Light(BlockStateId),
}
#[derive(Clone)]
pub struct ArrowState {
    pub in_ground: bool,
    pub in_ground_time: i32,
    pub life: i32,
    pub shake: i32,
    pub last_state: Option<BlockStateId>,
    pub pickup: Pickup,
    pub pickup_item: crate::items::ItemStack,
    pub fired_from_weapon: Option<crate::items::ItemStack>,
    pub no_physics: bool,
    pub effect_color: i32,
    pub pierce: u8,
    pub pierced: Vec<Who>,
    pub fire_ticks: i32,
    pub fall_distance: f64,
    pub fluids: EntityFluidInteraction,
    pub was_touching_water: bool,
    pub was_eye_in_water: bool,
    pub first_tick: bool,
    pub has_been_shot: bool,
    pub dealt_damage: bool,
    pub removed: bool,
    pub random: LegacyRandom,
    pub events: Vec<ArrowEvent>,
    pub hit_speed: f64,
}
impl fmt::Debug for ArrowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrowState")
            .field("in_ground", &self.in_ground)
            .field("life", &self.life)
            .field("fire_ticks", &self.fire_ticks)
            .finish_non_exhaustive()
    }
}
impl PartialEq for ArrowState {
    fn eq(&self, other: &Self) -> bool {
        self.in_ground == other.in_ground
            && self.life == other.life
            && self.last_state == other.last_state
            && self.fire_ticks == other.fire_ticks
    }
}
impl ArrowState {
    pub fn new(random: LegacyRandom) -> Self {
        Self {
            in_ground: false,
            in_ground_time: 0,
            life: 0,
            shake: 0,
            last_state: None,
            pickup: Pickup::Disallowed,
            pickup_item: crate::items::ItemStack::new("minecraft:arrow", 1),
            fired_from_weapon: None,
            no_physics: false,
            effect_color: -1,
            pierce: 0,
            pierced: Vec::new(),
            fire_ticks: 0,
            fall_distance: 0.0,
            fluids: EntityFluidInteraction::new([WATER, LAVA]),
            was_touching_water: false,
            was_eye_in_water: false,
            first_tick: true,
            has_been_shot: false,
            dealt_damage: false,
            removed: false,
            random,
            events: Vec::new(),
            hit_speed: 0.0,
        }
    }
    pub fn potion_color(&self) -> Option<u32> {
        let potion = self.pickup_item.potion()?;
        Some(crate::brewing::potion_color(Some(potion)))
    }
    pub fn set_item(&mut self, item: &crate::items::ItemStack) {
        self.pickup_item = item.clone();
        if item.data().and_then(|d| d.other(INTANGIBLE)).is_some() {
            self.pickup = Pickup::CreativeOnly;
        }
    }
    pub fn set_owner(&mut self, owner: Option<Who>) {
        if owner == Some(Who::Player) && self.pickup == Pickup::Disallowed {
            self.pickup = Pickup::Allowed;
        }
    }
    pub fn player_touch(&self, creative: bool) -> Touch {
        if !(self.in_ground || self.no_physics) || self.shake > 0 {
            return Touch::Ignore;
        }
        match self.pickup {
            Pickup::Disallowed => Touch::Ignore,
            Pickup::Allowed => Touch::Give(self.pickup_item.clone()),
            Pickup::CreativeOnly if creative => Touch::Take,
            Pickup::CreativeOnly => Touch::Ignore,
        }
    }
    pub fn on_fire(&self) -> bool {
        self.fire_ticks > 0
    }
    pub fn clear_fire(&mut self) {
        self.fire_ticks = self.fire_ticks.min(0);
    }
    pub fn ignite_for_ticks(&mut self, ticks: i32) {
        if self.fire_ticks < ticks {
            self.fire_ticks = ticks;
        }
    }
    pub fn ignite_for_seconds(&mut self, seconds: f32) {
        self.ignite_for_ticks((seconds * 20.0).floor() as i32);
    }
    fn fire_ignite(&mut self) {
        if self.fire_ticks < 0 {
            self.fire_ticks += 1;
        }
        if self.fire_ticks >= 0 {
            self.ignite_for_ticks(160);
        }
    }
}
pub struct ArrowScene<'a, 'r> {
    pub world: &'a World,
    pub registry: &'a vise_world::registry::BlockRegistry,
    pub facts: &'a BlockFacts,
    pub collision: &'a BlockCollisionRegistry<'r>,
    pub fluids: &'a FluidResources,
    pub raining: &'a dyn Fn(BlockPos) -> bool,
    pub mob_griefing: bool,
    pub tnt_explodes: bool,
}
impl ArrowScene<'_, '_> {
    fn block(&self, p: BlockPos) -> BlockStateId {
        self.world.block(p.x, p.y, p.z)
    }
    fn is_air(&self, state: BlockStateId) -> bool {
        self.facts.is_air(state)
    }
    fn collision_boxes(&self, state: BlockStateId, p: BlockPos) -> Vec<Aabb> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        self.collision
            .collision_shape(&mut scene, state, [p.x, p.y, p.z])
            .map(|s| s.boxes().to_vec())
            .unwrap_or_default()
    }
    fn property(&self, state: BlockStateId, name: &str) -> Option<&str> {
        self.registry
            .state(state)?
            .properties
            .get(name)
            .map(String::as_str)
    }
    fn clip(&self, from: Vec3, to: Vec3) -> Option<(Vec3, BlockPos, [i32; 3])> {
        let mut scene = SceneCollisions {
            world: self.world,
            binding: self.collision,
            subject: None,
        };
        let hit = vise_core::geometry::traverse_blocks(from, to, 1024, |p: BlockPos| {
            let state = self.block(p);
            if self.is_air(state) {
                return None;
            }
            let shape = self
                .collision
                .collision_shape(&mut scene, state, [p.x, p.y, p.z])
                .ok()?;
            shape.clip(from, to, p)
        });
        let hit = hit.ok().flatten()?;
        Some((hit.location, hit.block, hit.direction.normal()))
    }
    fn no_block_collision(&self, b: Aabb) -> bool {
        let (lo, hi) = (b.min_position(), b.max_position());
        for x in (lo.x.floor() as i32 - 1)..=(hi.x.floor() as i32 + 1) {
            for y in (lo.y.floor() as i32 - 1)..=(hi.y.floor() as i32 + 1) {
                for z in (lo.z.floor() as i32 - 1)..=(hi.z.floor() as i32 + 1) {
                    let p = BlockPos::new(x, y, z);
                    let state = self.block(p);
                    if self.is_air(state) {
                        continue;
                    }
                    let origin = Vec3::new(f64::from(x), f64::from(y), f64::from(z));
                    if self
                        .collision_boxes(state, p)
                        .iter()
                        .any(|c| c.move_by(origin).intersects(b))
                    {
                        return false;
                    }
                }
            }
        }
        true
    }
}
pub fn clip_block_stop(
    world: &World,
    facts: &BlockFacts,
    collision: &BlockCollisionRegistry<'_>,
    from: Vec3,
    movement: Vec3,
) -> Option<(Vec3, BlockPos)> {
    let to = Vec3::new(
        from.x + movement.x,
        from.y + movement.y,
        from.z + movement.z,
    );
    let mut scene = SceneCollisions {
        world,
        binding: collision,
        subject: None,
    };
    let hit = vise_core::geometry::traverse_blocks(from, to, 1024, |p: BlockPos| {
        let state = world.block(p.x, p.y, p.z);
        if facts.is_air(state) {
            return None;
        }
        let shape = collision
            .collision_shape(&mut scene, state, [p.x, p.y, p.z])
            .ok()?;
        shape.clip(from, to, p)
    })
    .ok()
    .flatten()?;
    Some((back_off(hit.location, movement), hit.block))
}
pub fn back_off(location: Vec3, movement: Vec3) -> Vec3 {
    fn signum(v: f64) -> f64 {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    }
    let offset = f64::from(0.05f32);
    Vec3::new(
        location.x - signum(movement.x) * offset,
        location.y - signum(movement.y) * offset,
        location.z - signum(movement.z) * offset,
    )
}
fn bounds(at: Vec3) -> Aabb {
    let w = f64::from(WIDTH / 2.0);
    Aabb::new(
        Vec3::new(at.x - w, at.y, at.z - w),
        Vec3::new(at.x + w, at.y + f64::from(HEIGHT), at.z + w),
    )
}
pub fn arrow_bounds(at: Vec3) -> Aabb {
    bounds(at)
}
pub fn blocks_inside(from: Vec3, to: Vec3) -> Vec<BlockPos> {
    let deflated = bounds(to).deflate(Vec3::new(
        f64::from(1.0e-5f32),
        f64::from(1.0e-5f32),
        f64::from(1.0e-5f32),
    ));
    let mut inside = Vec::new();
    let _ = sweep_intersected_blocks(from, to, deflated, 1_000_000, |p, iteration| {
        if iteration >= 16 {
            return false;
        }
        inside.push(p);
        true
    });
    inside
}
fn lerp_rotation(mut old: f32, new: f32) -> f32 {
    while new - old < -180.0 {
        old -= 360.0;
    }
    while new - old >= 180.0 {
        old += 360.0;
    }
    old + 0.2 * (new - old)
}
fn degrees(y: f64, x: f64) -> f32 {
    vise_core::geometry::trig::atan2_degrees(y, x)
}
fn signum(v: f64) -> f64 {
    if v > 0.0 {
        1.0
    } else if v < 0.0 {
        -1.0
    } else {
        v
    }
}
pub fn tick(p: &mut Projectile, scene: &ArrowScene, targets: &[(Who, Aabb)]) -> (bool, Hit) {
    let Some(mut st) = p.arrow.take() else {
        return (false, Hit::None);
    };
    p.previous = p.position;
    p.age += 1;
    let hit = Ticker {
        p,
        st: &mut st,
        scene,
    }
    .tick(targets);
    p.stuck = st.in_ground;
    let keep = !st.removed && !matches!(hit, Hit::Entity { .. });
    p.arrow = Some(st);
    (keep, hit)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityHit {
    Hurt,
    Refused,
    Deflected,
    Dodged,
}
pub fn finish_entity_hit(p: &mut Projectile, scene: &ArrowScene, outcome: EntityHit) -> bool {
    let Some(mut st) = p.arrow.take() else {
        return false;
    };
    let trident = matches!(p.kind, ProjectileKind::Trident);
    let mut ticker = Ticker {
        p,
        st: &mut st,
        scene,
    };
    match outcome {
        EntityHit::Hurt | EntityHit::Refused if trident => ticker.trident_bounce(),
        EntityHit::Hurt => {
            ticker.hit_sound();
            if ticker.st.pierce == 0 {
                ticker.st.removed = true;
            }
        }
        EntityHit::Refused => ticker.turn_back(),
        EntityHit::Dodged => {}
        EntityHit::Deflected => {
            if ticker.st.pierce > 0 {
                ticker.st.pierced.pop();
            }
        }
    }
    ticker.finish();
    p.stuck = st.in_ground;
    let keep = !st.removed;
    p.arrow = Some(st);
    keep
}
struct Ticker<'p, 's, 'a, 'r> {
    p: &'p mut Projectile,
    st: &'s mut ArrowState,
    scene: &'s ArrowScene<'a, 'r>,
}
impl Ticker<'_, '_, '_, '_> {
    fn position(&self) -> Vec3 {
        self.p.position
    }
    fn block_position(&self) -> BlockPos {
        BlockPos::containing(self.p.position)
    }
    fn in_rain(&self) -> bool {
        let p = self.block_position();
        (self.scene.raining)(p)
            || (self.scene.raining)(BlockPos::new(
                p.x,
                bounds(self.p.position).max_position().y.floor() as i32,
                p.z,
            ))
    }
    fn trident(&self) -> bool {
        matches!(self.p.kind, ProjectileKind::Trident)
    }
    fn tick(&mut self, targets: &[(Who, Aabb)]) -> Hit {
        if self.trident() && self.st.in_ground_time > 4 {
            self.st.dealt_damage = true;
        }
        let physics = !self.st.no_physics;
        let movement = self.p.velocity;
        let block_pos = self.block_position();
        let state = self.scene.block(block_pos);
        if !self.scene.is_air(state) && physics {
            let origin = Vec3::new(
                f64::from(block_pos.x),
                f64::from(block_pos.y),
                f64::from(block_pos.z),
            );
            let at = self.position();
            if self
                .scene
                .collision_boxes(state, block_pos)
                .iter()
                .any(|b| b.move_by(origin).contains(at))
            {
                self.p.velocity = Vec3::ZERO;
                self.st.in_ground = true;
            }
        }
        if self.st.shake > 0 {
            self.st.shake -= 1;
        }
        if self.st.was_touching_water || self.in_rain() {
            self.st.clear_fire();
        }
        if self.st.in_ground && physics {
            if self.st.last_state != Some(state) && self.should_fall() {
                self.start_falling();
            } else {
                self.st.life += 1;
                if self.st.life >= 1200 {
                    self.st.removed = true;
                }
            }
            self.st.in_ground_time += 1;
            if !self.st.removed {
                let at = self.position();
                self.apply_effects_from_blocks(at, at);
            }
            if self.st.in_ground_time >= 600
                && let Some(color) = self.st.potion_color()
            {
                self.st.events.push(ArrowEvent::PotionPuff {
                    id: self.p.id,
                    color,
                });
                self.st.pickup_item = crate::items::ItemStack::new("minecraft:arrow", 1);
            }
            return Hit::None;
        }
        self.st.in_ground_time = 0;
        let original = self.position();
        if self.st.was_touching_water {
            let inertia = if self.trident() { 0.99f32 } else { 0.6f32 };
            self.p.velocity = self.p.velocity.scale(f64::from(inertia));
            let v = self.p.velocity;
            for _ in 0..4 {
                self.st.events.push(ArrowEvent::Particle {
                    kind: ArrowParticle::Bubble,
                    position: Vec3::new(
                        original.x - v.x * 0.25,
                        original.y - v.y * 0.25,
                        original.z - v.z * 0.25,
                    ),
                    velocity: v,
                });
            }
        }
        if self.p.critical() {
            for i in 0..4 {
                let f = f64::from(i) / 4.0;
                self.st.events.push(ArrowEvent::Particle {
                    kind: ArrowParticle::Crit,
                    position: Vec3::new(
                        original.x + movement.x * f,
                        original.y + movement.y * f,
                        original.z + movement.z * f,
                    ),
                    velocity: Vec3::new(-movement.x, -movement.y + 0.2, -movement.z),
                });
            }
        }
        let y_rot = if physics {
            degrees(movement.x, movement.z)
        } else {
            degrees(-movement.x, -movement.z)
        };
        let x_rot = degrees(movement.y, movement.horizontal_distance());
        self.p.pitch = lerp_rotation(self.p.pitch, x_rot);
        self.p.yaw = lerp_rotation(self.p.yaw, y_rot);
        self.check_left_owner(targets);
        let mut hit = Hit::None;
        let end = Vec3::new(
            original.x + movement.x,
            original.y + movement.y,
            original.z + movement.z,
        );
        if physics {
            hit = self.step_move_and_hit(original, end, targets);
            if matches!(hit, Hit::Entity { .. }) {
                return hit;
            }
        } else {
            self.p.position = end;
            self.apply_effects_from_blocks(original, end);
        }
        self.finish();
        hit
    }
    fn finish(&mut self) {
        let physics = !self.st.no_physics;
        if !self.st.was_touching_water {
            self.p.velocity = self.p.velocity.scale(f64::from(0.99f32));
        }
        if physics && !self.st.in_ground {
            self.add_velocity(-0.05);
        }
        if !self.st.has_been_shot {
            self.st.events.push(ArrowEvent::ShootEvent);
            self.st.has_been_shot = true;
        }
        self.st.was_eye_in_water = self.st.fluids.is_eye_in_fluid(&Tags, WATER);
        let mut fluids = std::mem::take(&mut self.st.fluids);
        let _ = entity_fluid_tick::update(&mut fluids, self, WATER, LAVA, 1_000_000);
        self.st.fluids = fluids;
        if self.st.fire_ticks > 0 {
            self.st.fire_ticks -= 1;
        }
        if !self.st.first_tick && self.st.fluids.is_in_fluid(&Tags, LAVA) {
            self.st.fall_distance *= 0.5;
        }
        if self.position().y < f64::from(self.scene.world.bottom_y() - 64) {
            self.st.removed = true;
        }
        self.st.first_tick = false;
    }
    fn add_velocity(&mut self, y: f64) {
        let v = self.p.velocity;
        self.p.velocity = Vec3::new(v.x + 0.0, v.y + y, v.z + 0.0);
    }
    fn check_left_owner(&mut self, targets: &[(Who, Aabb)]) {
        if self.p.left_owner {
            return;
        }
        let owner = targets
            .iter()
            .find(|(who, _)| Some(*who) == self.p.owner)
            .map(|(_, b)| *b);
        let swept = bounds(self.p.position)
            .expand_towards(self.p.velocity)
            .inflate(Vec3::new(1.0, 1.0, 1.0));
        self.p.left_owner = owner.is_none_or(|b| !swept.intersects(b));
    }
    fn should_fall(&self) -> bool {
        let at = self.position();
        self.st.in_ground
            && self
                .scene
                .no_block_collision(Aabb::new(at, at).inflate(Vec3::new(0.06, 0.06, 0.06)))
    }
    fn start_falling(&mut self) {
        self.st.in_ground = false;
        let v = self.p.velocity;
        let x = f64::from(self.st.random.next_float() * 0.2);
        let y = f64::from(self.st.random.next_float() * 0.2);
        let z = f64::from(self.st.random.next_float() * 0.2);
        self.p.velocity = Vec3::new(v.x * x, v.y * y, v.z * z);
        self.st.life = 0;
    }
    fn step_move_and_hit(&mut self, from: Vec3, to: Vec3, targets: &[(Who, Aabb)]) -> Hit {
        let block = self.scene.clip(from, to);
        let end = block.map_or(to, |b| b.0);
        let margin = f64::from(((self.p.age as i32 - 2) as f32 / 20.0).clamp(0.0, 0.3));
        let area = bounds(from)
            .expand_towards(self.p.velocity)
            .inflate(Vec3::new(1.0, 1.0, 1.0));
        let mut best: Option<(Who, Vec3, f64)> = None;
        let targets = if self.st.dealt_damage { &[] } else { targets };
        for (who, b) in targets {
            if Some(*who) == self.p.owner && !self.p.left_owner || !b.intersects(area) {
                continue;
            }
            if self.st.pierced.contains(who) {
                continue;
            }
            if let Some(at) = self.entity_hit(b, from, end, margin) {
                let c = b.center();
                let d = from.distance_squared(Vec3::new(c.x, b.min_position().y, c.z));
                if best.is_none_or(|bb| d < bb.2) {
                    best = Some((*who, at, d));
                }
            }
        }
        let next = best.map_or(end, |b| b.1);
        self.p.position = next;
        self.apply_effects_from_blocks(from, next);
        if let Some((who, at, _)) = best {
            if self.st.pierce > 0 {
                if self.st.pierced.len() > usize::from(self.st.pierce) {
                    self.st.removed = true;
                    return Hit::None;
                }
                self.st.pierced.push(who);
            }
            return Hit::Entity { who, at };
        }
        let Some((at, pos, face)) = block else {
            return Hit::None;
        };
        if self.st.removed {
            return Hit::None;
        }
        let state = self.scene.block(pos);
        if self.scene.is_air(state) {
            return Hit::None;
        }
        self.on_hit_block(pos, state);
        Hit::Block {
            pos: [pos.x, pos.y, pos.z],
            at,
            face,
        }
    }
    fn entity_hit(&self, b: &Aabb, from: Vec3, to: Vec3, margin: f64) -> Option<Vec3> {
        if let Some(at) = b.clip(from, to) {
            return Some(at);
        }
        if margin <= 0.0 {
            return None;
        }
        let outside = b
            .inflate(Vec3::new(margin, margin, margin))
            .clip(from, to)?;
        let centre = b.center();
        let towards = self.scene.clip(outside, centre).map_or(centre, |h| h.0);
        b.clip(outside, towards).map(|_| outside)
    }
    fn hit_sound(&mut self) {
        let pitch = 1.2 / (self.st.random.next_float() * 0.2 + 0.9);
        let crossbow = self
            .st
            .fired_from_weapon
            .as_ref()
            .is_some_and(|s| &*s.item == "minecraft:crossbow");
        let sound = if self.trident() {
            ArrowSound::TridentHitGround
        } else if crossbow {
            ArrowSound::CrossbowHit
        } else {
            ArrowSound::HitGround
        };
        self.st.events.push(ArrowEvent::Sound {
            sound,
            volume: 1.0,
            pitch,
        });
    }
    fn trident_bounce(&mut self) {
        self.st.dealt_damage = true;
        self.st.events.push(ArrowEvent::Sound {
            sound: ArrowSound::TridentHit,
            volume: 1.0,
            pitch: 1.0,
        });
        let rotation = 170.0 + self.st.random.next_float() * 20.0;
        let v = self.p.velocity;
        self.p.velocity = Vec3::new(v.x * (-0.02 * 0.5), v.y * (-0.2 * 0.5), v.z * (-0.02 * 0.5));
        self.p.yaw += rotation;
    }
    fn turn_back(&mut self) {
        let power = 0.2;
        let rotation = 170.0 + self.st.random.next_float() * 20.0;
        let v = self.p.velocity;
        let k = -power * 0.5;
        self.p.velocity = Vec3::new(v.x * k, v.y * k, v.z * k);
        self.p.yaw += rotation;
        if self.p.velocity.length_squared() < 1.0e-7 {
            if self.st.pickup == Pickup::Allowed {
                self.st.events.push(ArrowEvent::Drop);
            }
            self.st.removed = true;
        }
    }
    fn on_hit_block(&mut self, pos: BlockPos, state: BlockStateId) {
        self.st.last_state = Some(state);
        self.st.hit_speed = self.p.velocity.length();
        if self.st.on_fire()
            && let Some(reaction) = self.block_reaction(state)
        {
            self.st.events.push(ArrowEvent::Block { pos, reaction });
        }
        let v = self.p.velocity;
        let offset = f64::from(0.05f32);
        self.p.position = Vec3::new(
            self.p.position.x - signum(v.x) * offset,
            self.p.position.y - signum(v.y) * offset,
            self.p.position.z - signum(v.z) * offset,
        );
        self.p.velocity = Vec3::ZERO;
        self.hit_sound();
        self.st.in_ground = true;
        self.st.shake = 7;
        self.p.set_critical(false);
        self.st.pierce = 0;
        self.st.pierced.clear();
    }
    fn may_interact(&self) -> bool {
        !matches!(self.p.owner, Some(Who::Mob(_))) || self.scene.mob_griefing
    }
    fn block_reaction(&self, state: BlockStateId) -> Option<BlockReaction> {
        let name = self.scene.registry.state(state)?.name.as_str();
        let flag = |key: &str| self.scene.property(state, key) == Some("true");
        let lit = || {
            let mut definition = self.scene.registry.state(state)?.clone();
            definition.properties.insert("lit".into(), "true".into());
            self.scene.registry.id(&definition)
        };
        match name {
            "minecraft:tnt" if self.may_interact() && self.scene.tnt_explodes => {
                Some(BlockReaction::PrimeTnt)
            }
            "minecraft:campfire" | "minecraft:soul_campfire"
                if self.may_interact() && !flag("lit") && !flag("waterlogged") =>
            {
                lit().map(BlockReaction::Light)
            }
            _ if name.ends_with("candle_cake") && !flag("lit") => lit().map(BlockReaction::Light),
            _ if name.ends_with("candle") && !flag("lit") && !flag("waterlogged") => {
                lit().map(BlockReaction::Light)
            }
            _ => None,
        }
    }
    fn apply_effects_from_blocks(&mut self, from: Vec3, to: Vec3) {
        if self.st.removed || self.st.no_physics {
            return;
        }
        let was_on_fire = self.st.on_fire();
        let previous_fire = self.st.fire_ticks;
        let mut visited = HashSet::new();
        let mut effects = Collector::default();
        let used = self.check_inside_blocks(from, to, &mut visited, &mut effects, 16);
        if 16 - used <= 0 {
            self.check_inside_blocks(to, to, &mut visited, &mut effects, 1);
        }
        for effect in effects.finish() {
            if self.st.removed {
                break;
            }
            match effect {
                Effect::ClearFreeze => {}
                Effect::FireIgnite => self.st.fire_ignite(),
                Effect::LavaIgnite => self.st.ignite_for_ticks(300),
                Effect::Extinguish => self.st.clear_fire(),
            }
        }
        if self.in_rain() {
            self.st.clear_fire();
        }
        if was_on_fire && !self.st.on_fire() {
            let pitch = 1.6 + (self.st.random.next_float() - self.st.random.next_float()) * 0.4;
            self.st.events.push(ArrowEvent::Sound {
                sound: ArrowSound::Extinguish,
                volume: 0.7,
                pitch,
            });
        }
        let ignited = self.st.fire_ticks > previous_fire;
        if !self.st.on_fire() && !ignited {
            self.st.fire_ticks = 0;
        }
    }
    fn check_inside_blocks(
        &mut self,
        from: Vec3,
        to: Vec3,
        visited: &mut HashSet<BlockPos>,
        effects: &mut Collector,
        max_iterations: i32,
    ) -> i32 {
        let deflated = bounds(to).deflate(Vec3::new(
            f64::from(1.0e-5f32),
            f64::from(1.0e-5f32),
            f64::from(1.0e-5f32),
        ));
        let far = from.distance_squared(to) > 0.999_990_000_000_252_6f64 * 0.999_990_000_000_252_6;
        let mut hits = Vec::new();
        let mut iterations = 0;
        let alive = !self.st.removed;
        let _ = sweep_intersected_blocks(from, to, deflated, 1_000_000, |p, iteration| {
            if !alive || iteration >= max_iterations {
                return false;
            }
            iterations = iteration;
            hits.push((p, iteration));
            true
        });
        for (p, step) in hits {
            let state = self.scene.block(p);
            if self.scene.is_air(state) {
                continue;
            }
            let inside_block = true;
            let fluid = self.scene.fluids.fluid(state);
            let inside_fluid = self.collided_with_fluid(fluid, p, from, to);
            if !(inside_block || inside_fluid) || !visited.insert(p) {
                continue;
            }
            effects.advance(step);
            let name = &*self.scene.facts.get(state).name;
            if matches!(name, "minecraft:fire" | "minecraft:soul_fire") {
                effects.apply(Effect::ClearFreeze);
                effects.apply(Effect::FireIgnite);
            }
            if name == "minecraft:bubble_column" {
                let precise = far
                    || deflated.intersects(Aabb::new(
                        Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z)),
                        Vec3::new(f64::from(p.x + 1), f64::from(p.y + 1), f64::from(p.z + 1)),
                    ));
                if precise {
                    self.bubble_column(state, p);
                }
            }
            if inside_fluid {
                effects.advance(step);
                match fluid.kind() {
                    FluidKind::Water | FluidKind::FlowingWater => effects.apply(Effect::Extinguish),
                    FluidKind::Lava | FluidKind::FlowingLava => {
                        effects.apply(Effect::ClearFreeze);
                        effects.apply(Effect::LavaIgnite);
                    }
                    FluidKind::Empty => {}
                }
            }
        }
        iterations + 1
    }
    fn collided_with_fluid(
        &mut self,
        fluid: FluidState,
        p: BlockPos,
        from: Vec3,
        to: Vec3,
    ) -> bool {
        if fluid.kind() == FluidKind::Empty {
            return false;
        }
        let Ok(height) = fluid.height_at(self, [p.x, p.y, p.z]) else {
            return false;
        };
        let b = Aabb::new(
            Vec3::new(f64::from(p.x), f64::from(p.y), f64::from(p.z)),
            Vec3::new(
                f64::from(p.x) + 1.0,
                f64::from(p.y) + f64::from(height),
                f64::from(p.z) + 1.0,
            ),
        );
        bounds(from).collided_along_vector(to.subtract(from), &[b])
    }
    fn bubble_column(&mut self, state: BlockStateId, p: BlockPos) {
        let drag = self.scene.property(state, "drag") == Some("true");
        let above = self.scene.block(BlockPos::new(p.x, p.y + 1, p.z));
        let nothing_above = self.scene.collision_boxes(above, p).is_empty()
            && self.scene.fluids.fluid(above).kind() == FluidKind::Empty;
        if self.st.in_ground {
            return;
        }
        if nothing_above {
            self.add_velocity(if drag { -0.03 } else { 0.1 });
            self.st
                .events
                .push(ArrowEvent::BubbleColumnParticles { pos: p });
        } else {
            self.add_velocity(if drag { -0.03 } else { 0.06 });
            self.st.fall_distance = 0.0;
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    ClearFreeze,
    FireIgnite,
    LavaIgnite,
    Extinguish,
}
const EFFECT_ORDER: [Effect; 4] = [
    Effect::ClearFreeze,
    Effect::FireIgnite,
    Effect::LavaIgnite,
    Effect::Extinguish,
];
struct Collector {
    last_step: i32,
    step: [bool; 4],
    effects: Vec<Effect>,
}
impl Default for Collector {
    fn default() -> Self {
        Self {
            last_step: -1,
            step: [false; 4],
            effects: Vec::new(),
        }
    }
}
impl Collector {
    fn advance(&mut self, step: i32) {
        if self.last_step != step {
            self.last_step = step;
            self.flush();
        }
    }
    fn apply(&mut self, effect: Effect) {
        self.step[effect as usize] = true;
    }
    fn flush(&mut self) {
        for effect in EFFECT_ORDER {
            if std::mem::take(&mut self.step[effect as usize]) {
                self.effects.push(effect);
            }
        }
    }
    fn finish(mut self) -> Vec<Effect> {
        self.flush();
        self.effects
    }
}
struct Tags;
impl FluidMembership for Tags {
    fn fluid_in_tag(&self, kind: FluidKind, tag: FluidTagId) -> bool {
        if tag == WATER {
            matches!(kind, FluidKind::Water | FluidKind::FlowingWater)
        } else {
            matches!(kind, FluidKind::Lava | FluidKind::FlowingLava)
        }
    }
}
type Error = Box<dyn std::error::Error>;
impl FluidMembership for Ticker<'_, '_, '_, '_> {
    fn fluid_in_tag(&self, kind: FluidKind, tag: FluidTagId) -> bool {
        Tags.fluid_in_tag(kind, tag)
    }
}
impl CurrentTarget for Ticker<'_, '_, '_, '_> {
    type Error = Error;
    fn is_player_class(&self) -> bool {
        false
    }
    fn velocity(&mut self) -> Result<Vec3, Error> {
        Ok(self.p.velocity)
    }
    fn add_delta_movement(&mut self, delta: Vec3) -> Result<(), Error> {
        let v = self.p.velocity;
        let sum = Vec3::new(v.x + delta.x, v.y + delta.y, v.z + delta.z);
        if delta.is_finite() && sum.is_finite() {
            self.p.velocity = sum;
        }
        Ok(())
    }
}
impl FluidInteractionWorld for Ticker<'_, '_, '_, '_> {
    type Error = Error;
    type Chunk = ();
    fn interaction_box(&mut self) -> Result<Option<Aabb>, Error> {
        Ok(Some(
            bounds(self.p.position).deflate(Vec3::new(0.001, 0.001, 0.001)),
        ))
    }
    fn loaded_chunk(&mut self, x: i32, z: i32) -> Result<Option<()>, Error> {
        Ok(self.scene.world.chunks.contains_key(&(x, z)).then_some(()))
    }
    fn chunk_sections(&mut self, _: ()) -> Result<FluidSections<'_>, Error> {
        let layout = self.scene.world.layout();
        Ok(FluidSections {
            minimum_section_y: layout.min_y() >> 4,
            has_fluid: crate::chunk::every_section(layout),
        })
    }
    fn bounding_box_min_y(&self) -> f64 {
        self.p.position.y
    }
    fn block_x(&self) -> i32 {
        self.p.position.x.floor() as i32
    }
    fn eye_y(&mut self) -> Result<f64, Error> {
        Ok(self.p.position.y + f64::from(EYE_HEIGHT))
    }
    fn block_z(&self) -> i32 {
        self.p.position.z.floor() as i32
    }
    fn fluid_state(&mut self, p: BlockPos) -> Result<FluidState, Error> {
        Ok(self.scene.fluids.fluid(self.scene.block(p)))
    }
    fn fluid_height(&mut self, f: FluidState, p: BlockPos) -> Result<f32, Error> {
        f.height_at(self, [p.x, p.y, p.z])
    }
    fn camera_height(&mut self, f: FluidState, p: BlockPos) -> Result<f32, Error> {
        f.camera_height_at(self, [p.x, p.y, p.z])
    }
    fn flow(&mut self, f: FluidState, p: BlockPos) -> Result<Vec3, Error> {
        flow_at(self, f, p)
    }
}
impl EntityFluidTickWorld for Ticker<'_, '_, '_, '_> {
    fn is_pushed_by_fluid(&mut self) -> Result<bool, Error> {
        Ok(!self.st.in_ground)
    }
    fn reset_fall_distance(&mut self) -> Result<(), Error> {
        self.st.fall_distance = 0.0;
        Ok(())
    }
    fn was_touching_water(&self) -> bool {
        self.st.was_touching_water
    }
    fn first_tick(&self) -> bool {
        self.st.first_tick
    }
    fn water_splash(&mut self) -> Result<(), Error> {
        water_splash::water_splash(self, 1_000_000).map_err(|e| e.to_string())?;
        Ok(())
    }
    fn set_was_touching_water(&mut self, value: bool) {
        self.st.was_touching_water = value;
    }
    fn fast_lava(&mut self) -> Result<bool, Error> {
        Ok(false)
    }
}
impl FluidHeightWorld for Ticker<'_, '_, '_, '_> {
    type Error = Error;
    fn fluid(&mut self, p: [i32; 3]) -> Result<FluidState, Error> {
        Ok(self
            .scene
            .fluids
            .fluid(self.scene.block(BlockPos::new(p[0], p[1], p[2]))))
    }
    fn sturdy_down(&mut self, p: [i32; 3]) -> Result<bool, Error> {
        let state = self.scene.block(BlockPos::new(p[0], p[1], p[2]));
        Ok(self.scene.fluids.sturdy(state, Direction::Down))
    }
}
impl FluidFlowWorld for Ticker<'_, '_, '_, '_> {
    type Error = Error;
    type Block = BlockStateId;
    fn fluid(&mut self, p: BlockPos) -> Result<FluidState, Error> {
        Ok(self.scene.fluids.fluid(self.scene.block(p)))
    }
    fn block(&mut self, p: BlockPos) -> Result<BlockStateId, Error> {
        Ok(self.scene.block(p))
    }
    fn blocks_fluid_flow(&self, s: BlockStateId) -> bool {
        self.scene.fluids.blocks_flow(s)
    }
    fn is_ice_class(&self, s: BlockStateId) -> bool {
        self.scene.fluids.ice(s)
    }
    fn is_face_sturdy(
        &mut self,
        s: BlockStateId,
        _: BlockPos,
        d: Direction,
    ) -> Result<bool, Error> {
        Ok(self.scene.fluids.sturdy(s, d))
    }
}
impl WaterSplashWorld for Ticker<'_, '_, '_, '_> {
    type Error = Error;
    type Entity = ();
    type Sound = ();
    fn this_entity(&self) {}
    fn controlling_passenger(&mut self) -> Result<Option<()>, Error> {
        Ok(None)
    }
    fn velocity(&mut self, _: ()) -> Result<Vec3, Error> {
        Ok(self.p.velocity)
    }
    fn splash_sound(&mut self, _: bool) -> Result<(), Error> {
        Ok(())
    }
    fn random_float(&mut self) -> Result<f32, Error> {
        Ok(self.st.random.next_float())
    }
    fn random_double(&mut self) -> Result<f64, Error> {
        Ok(self.st.random.next_double())
    }
    fn play_sound(&mut self, _: (), volume: f32, pitch: f32) -> Result<(), Error> {
        self.st.events.push(ArrowEvent::Sound {
            sound: ArrowSound::Splash,
            volume,
            pitch,
        });
        Ok(())
    }
    fn position(&self) -> Vec3 {
        self.p.position
    }
    fn width(&self) -> f32 {
        WIDTH
    }
    fn particle(
        &mut self,
        kind: SplashParticle,
        position: Vec3,
        velocity: Vec3,
    ) -> Result<(), Error> {
        self.st.events.push(ArrowEvent::Particle {
            kind: match kind {
                SplashParticle::Bubble => ArrowParticle::Bubble,
                SplashParticle::Splash => ArrowParticle::Splash,
            },
            position,
            velocity,
        });
        Ok(())
    }
    fn splash_event(&mut self) -> Result<(), Error> {
        self.st.events.push(ArrowEvent::SplashEvent);
        Ok(())
    }
}
