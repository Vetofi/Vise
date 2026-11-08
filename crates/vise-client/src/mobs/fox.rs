use super::{
    Mob, MobEvent, MobWorld, Seen, Species,
    goals::{
        Avoid, AvoidEntity, Breed, ClimbOnTopOfPowderSnow, Cx, Float, FollowParent, Goal, JUMP,
        LOOK, LeapAtTarget, LookAtPlayer, MOVE, MeleeAttack, MoveToBlock, NearestTarget, Panic,
        Prey, Stroll, goal_clone, hide_pos, reduced,
    },
    living::wrap,
};
use vise_core::{
    geometry::{Axis, Vec3},
    jmath,
    random::RandomSource,
};
use vise_world::registry::BlockStateId;

pub const SITTING: i32 = 1;
pub const CROUCHING: i32 = 4;
pub const INTERESTED: i32 = 8;
pub const POUNCING: i32 = 16;
pub const SLEEPING: i32 = 32;
pub const FACEPLANTED: i32 = 64;
pub const DEFENDING: i32 = 128;
const TICKS_SINCE_EATEN: usize = 1;
const INTERESTED_ANGLE: usize = 0;
const CROUCH_AMOUNT: usize = 1;
const MAX_CROUCH_AMOUNT: f32 = 5.0;

pub fn flag(m: &Mob, f: i32) -> bool {
    m.extra.ints[0] & f != 0
}
fn set_flag(m: &mut Mob, f: i32, on: bool) {
    if on {
        m.extra.ints[0] |= f;
    } else {
        m.extra.ints[0] &= !f;
    }
}
fn clear_states(m: &mut Mob) {
    m.extra.ints[0] = 0;
}
pub fn can_move(m: &Mob) -> bool {
    !flag(m, SLEEPING | SITTING | FACEPLANTED)
}
pub fn resets_x_rot(m: &Mob) -> bool {
    !flag(m, POUNCING | CROUCHING | INTERESTED | FACEPLANTED)
}

pub fn populate_default_equipment(m: &mut Mob) {
    if m.random.next_float() < 0.2 {
        let odds = m.random.next_float();
        let held = if odds < 0.05 {
            "minecraft:emerald"
        } else if odds < 0.2 {
            "minecraft:egg"
        } else if odds < 0.4 {
            if m.random.next_boolean() {
                "minecraft:rabbit_foot"
            } else {
                "minecraft:rabbit_hide"
            }
        } else if odds < 0.6 {
            "minecraft:wheat"
        } else if odds < 0.8 {
            "minecraft:leather"
        } else {
            "minecraft:feather"
        };
        m.extra.main_hand = Some(held.into());
    }
}
pub fn register(m: &mut Mob) {
    let countdown = m.random.next_int_bound(reduced(140)).unwrap_or(0);
    let g = &mut m.ai.goals;
    g.add(0, Wrap::new(Float).on_start(clear_states));
    g.add(0, ClimbOnTopOfPowderSnow);
    g.add(1, Faceplant { countdown: 0 });
    g.add(2, Panic::new(2.2));
    g.add(3, Wrap::new(Breed::new(1.0)).on_start(clear_states));
    g.add(
        4,
        AvoidEntity::new(Avoid::Player, 16.0, 1.6, 1.4)
            .player_when(|p| !p.sneaking && !p.spectator),
    );
    g.add(
        4,
        AvoidEntity::new(Avoid::Species(vec![Species::Wolf]), 8.0, 1.6, 1.4).seen_when(|s| !s.tame),
    );
    g.add(
        4,
        AvoidEntity::new(Avoid::Species(vec![Species::PolarBear]), 8.0, 1.6, 1.4),
    );
    g.add(5, StalkPrey);
    g.add(6, Pounce);
    g.add(6, SeekShelter::new(1.25));
    g.add(
        7,
        Wrap::new(MeleeAttack::new(1.2, true).bite("entity.fox.bite"))
            .when(
                |m| !flag(m, SITTING | SLEEPING | CROUCHING | FACEPLANTED),
                false,
            )
            .on_start(|m| set_flag(m, INTERESTED, false)),
    );
    g.add(7, Sleep { countdown });
    g.add(8, Wrap::new(FollowParent::new(1.25)).on_start(clear_states));
    g.add(10, EatBerries::new());
    g.add(10, LeapAtTarget::new(0.4));
    g.add(11, Stroll::avoiding_water(1.0));
    g.add(
        12,
        Wrap::new(LookAtPlayer::new(24.0)).when(|m| !flag(m, FACEPLANTED | INTERESTED), true),
    );
    g.add(13, Perch::default());
    let red = |m: &Mob, _: &MobWorld| m.extra.variant == 0;
    let snow = |m: &Mob, _: &MobWorld| m.extra.variant != 0;
    let land = || NearestTarget::new(Prey::Filter(chicken_or_rabbit), false).interval(10);
    let turtle = || NearestTarget::new(Prey::Filter(baby_turtle_on_land), false).interval(10);
    let fish = || NearestTarget::new(Prey::Filter(schooling_fish), false).interval(20);
    let t = &mut m.ai.targets;
    t.add(4, land().gate(red));
    t.add(4, turtle().gate(red));
    t.add(6, fish().gate(red));
    t.add(4, fish().gate(snow));
    t.add(6, land().gate(snow));
    t.add(6, turtle().gate(snow));
}

fn chicken_or_rabbit(s: &Seen) -> bool {
    matches!(s.species, Species::Chicken | Species::Rabbit)
}
fn baby_turtle_on_land(s: &Seen) -> bool {
    s.species == Species::Turtle && s.baby && !s.in_water
}
fn schooling_fish(s: &Seen) -> bool {
    matches!(
        s.species,
        Species::Cod | Species::Salmon | Species::TropicalFish
    )
}

pub fn client_tick(m: &mut Mob) {
    let (interested, crouching) = (flag(m, INTERESTED), flag(m, CROUCHING));
    let v = &mut m.anim.v;
    m.anim.vo[INTERESTED_ANGLE] = v[INTERESTED_ANGLE];
    v[INTERESTED_ANGLE] += (if interested { 1.0 } else { 0.0 } - v[INTERESTED_ANGLE]) * 0.4;
    m.anim.vo[CROUCH_AMOUNT] = v[CROUCH_AMOUNT];
    v[CROUCH_AMOUNT] = if crouching {
        (v[CROUCH_AMOUNT] + 0.2).min(MAX_CROUCH_AMOUNT)
    } else {
        0.0
    };
}
fn fully_crouched(m: &Mob) -> bool {
    m.anim.v[CROUCH_AMOUNT] == MAX_CROUCH_AMOUNT
}

pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    client_tick(m);
    if !m.alive() {
        return;
    }
    m.extra.ints[TICKS_SINCE_EATEN] += 1;
    if let Some(item) = m.extra.main_hand.clone()
        && eats(w, &item)
        && m.ai.target.is_none()
        && m.on_ground
        && !flag(m, SLEEPING)
    {
        let eaten = m.extra.ints[TICKS_SINCE_EATEN];
        if eaten > 600 {
            m.extra.set_main_hand(
                w.items
                    .and_then(|i| i.items.get(&item))
                    .and_then(|i| i.use_remainder.as_deref())
                    .map(Into::into),
            );
            m.extra.ints[TICKS_SINCE_EATEN] = 0;
        } else if eaten > 560 && m.random.next_float() < 0.1 {
            let at = [m.position.x, m.position.y, m.position.z];
            events.push(MobEvent::Sound("entity.fox.eat", at, 1.0, 1.0));
            events.push(MobEvent::EntityEvent {
                mob: m.id,
                event: 45,
                at,
            });
        }
    }
    if !m.target_info(w).is_some_and(|t| t.alive) {
        set_flag(m, CROUCHING | INTERESTED, false);
    }
    let in_water = m.fluids.in_water;
    if in_water || m.ai.target.is_some() || w.thundering {
        set_flag(m, SLEEPING, false);
    }
    if in_water || flag(m, SLEEPING) {
        set_flag(m, SITTING, false);
    }
    if flag(m, FACEPLANTED) && m.random.next_float() < 0.2 {
        let at = [
            m.position.x.floor() as i32,
            m.position.y.floor() as i32,
            m.position.z.floor() as i32,
        ];
        events.push(MobEvent::LevelEvent {
            id: 2001,
            pos: at,
            data: w.world.block(at[0], at[1], at[2]).0 as i32,
        });
    }
}
pub fn pre_ai_step(m: &mut Mob) {
    if flag(m, SLEEPING) {
        m.ai.jumping = false;
        m.ai.xxa = 0.0;
        m.ai.zza = 0.0;
    }
}
fn eats(w: &MobWorld, item: &str) -> bool {
    w.items
        .and_then(|i| i.items.get(item))
        .is_some_and(|i| i.food.is_some() && i.consumable.is_some())
}

fn has_shelter(m: &Mob, w: &MobWorld) -> bool {
    let at = [
        m.position.x.floor() as i32,
        m.bounds().max(Axis::Y).floor() as i32,
        m.position.z.floor() as i32,
    ];
    !w.can_see_sky(at[0], at[1], at[2]) && m.walk_target_value(w, at) >= 0.0
}
fn alertable(m: &Mob, w: &MobWorld) -> bool {
    let (from, id, y) = (m.position, m.id, m.position.y);
    let mob = w
        .nearby(from, 12.0, |s: &Seen| {
            s.id != id
                && (s.position.y - y).abs() <= 6.0
                && m.distance_sqr(s.position) <= 144.0
                && match s.species {
                    Species::Fox => false,
                    Species::Chicken | Species::Rabbit => true,
                    Species::Wolf | Species::Cat | Species::Parrot | Species::Nautilus => !s.tame,
                    _ => true,
                }
        })
        .next()
        .is_some();
    mob || w.player.as_ref().is_some_and(|p| {
        p.alive
            && !p.creative
            && !p.spectator
            && !p.sneaking
            && (p.position.y - y).abs() <= 6.0
            && m.distance_sqr(p.position) <= 144.0
    })
}

#[derive(Clone, Debug)]
struct Sleep {
    countdown: i32,
}
impl Sleep {
    fn can_sleep(&mut self, m: &Mob, w: &MobWorld) -> bool {
        if self.countdown > 0 {
            self.countdown -= 1;
            return false;
        }
        let (x, y, z) = (
            m.position.x.floor() as i32,
            m.position.y.floor() as i32,
            m.position.z.floor() as i32,
        );
        w.daylight
            && has_shelter(m, w)
            && !alertable(m, w)
            && w.block_name(x, y, z) != "minecraft:powder_snow"
    }
}
impl Goal for Sleep {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.ai.nav.is_done() && (self.can_sleep(m, cx.w) || flag(m, SLEEPING))
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.can_sleep(m, cx.w)
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        set_flag(m, SITTING | CROUCHING | INTERESTED, false);
        m.ai.jumping = false;
        set_flag(m, SLEEPING, true);
        m.nav_stop();
        let p = m.position;
        m.ai.mov.set_wanted(p.x, p.y, p.z, 0.0);
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        self.countdown = m.random.next_int_bound(reduced(140)).unwrap_or(0);
        clear_states(m);
    }
}

#[derive(Clone, Debug)]
struct Wrap<G> {
    goal: G,
    when: Option<fn(&Mob) -> bool>,
    both: bool,
    on_start: Option<fn(&mut Mob)>,
}
impl<G: Goal + Clone + 'static> Wrap<G> {
    fn new(goal: G) -> Self {
        Self {
            goal,
            when: None,
            both: false,
            on_start: None,
        }
    }
    fn when(mut self, f: fn(&Mob) -> bool, both: bool) -> Self {
        (self.when, self.both) = (Some(f), both);
        self
    }
    fn on_start(mut self, f: fn(&mut Mob)) -> Self {
        self.on_start = Some(f);
        self
    }
}
impl<G: Goal + Clone + 'static> Goal for Wrap<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.goal.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.when.is_none_or(|f| f(m)) && self.goal.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        (!self.both || self.when.is_none_or(|f| f(m))) && self.goal.can_continue(m, cx)
    }
    fn interruptable(&self) -> bool {
        self.goal.interruptable()
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(f) = self.on_start {
            f(m);
        }
        self.goal.start(m, cx);
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.goal.stop(m, cx);
    }
    fn every_tick(&self) -> bool {
        self.goal.every_tick()
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.goal.tick(m, cx);
    }
    fn name(&self) -> &'static str {
        self.goal.name()
    }
}

fn path_clear(m: &Mob, w: &MobWorld, to: Vec3) -> bool {
    let (zdiff, xdiff) = (to.z - m.position.z, to.x - m.position.x);
    let slope = zdiff / xdiff;
    for i in 0..6 {
        let step = f64::from(i as f32 / 6.0);
        let z = if slope == 0.0 { 0.0 } else { zdiff * step };
        let x = if slope == 0.0 {
            xdiff * step
        } else {
            z / slope
        };
        for j in 1..4 {
            let at = [
                (m.position.x + x).floor() as i32,
                (m.position.y + f64::from(j)).floor() as i32,
                (m.position.z + z).floor() as i32,
            ];
            if !w.facts.get(w.world.block(at[0], at[1], at[2])).replaceable {
                return false;
            }
        }
    }
    true
}
fn signum(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

#[derive(Clone, Debug)]
struct Faceplant {
    countdown: i32,
}
impl Goal for Faceplant {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK | JUMP
    }
    fn can_use(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        flag(m, FACEPLANTED)
    }
    fn can_continue(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        flag(m, FACEPLANTED) && self.countdown > 0
    }
    fn start(&mut self, _: &mut Mob, _: &mut Cx) {
        self.countdown = reduced(40);
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        set_flag(m, FACEPLANTED, false);
    }
    fn tick(&mut self, _: &mut Mob, _: &mut Cx) {
        self.countdown -= 1;
    }
}

#[derive(Clone, Debug)]
struct StalkPrey;
impl StalkPrey {
    fn look(m: &mut Mob, eye: [f64; 3]) {
        let (y, x) = (m.kind.max_head_y_rot, m.kind.max_head_x_rot);
        m.look_at_speed(eye, y, x);
    }
}
impl Goal for StalkPrey {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if flag(m, SLEEPING) {
            return false;
        }
        let Some(t) = m.target_info(cx.w) else {
            return false;
        };
        let stalkable = match m.ai.target {
            Some(super::Who::Mob(id)) => {
                cx.w.seen(id)
                    .is_some_and(|s| matches!(s.species, Species::Chicken | Species::Rabbit))
            }
            _ => false,
        };
        t.alive
            && stalkable
            && m.distance_sqr(t.position) > 36.0
            && !flag(m, CROUCHING | INTERESTED)
            && !m.ai.jumping
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        set_flag(m, SITTING | FACEPLANTED, false);
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        match m.target_info(cx.w) {
            Some(t) if path_clear(m, cx.w, t.position) => {
                set_flag(m, INTERESTED | CROUCHING, true);
                m.nav_stop();
                Self::look(m, [t.position.x, t.eye_y, t.position.z]);
            }
            _ => set_flag(m, INTERESTED | CROUCHING, false),
        }
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let Some(t) = m.target_info(cx.w) else { return };
        Self::look(m, [t.position.x, t.eye_y, t.position.z]);
        if m.distance_sqr(t.position) <= 36.0 {
            set_flag(m, INTERESTED | CROUCHING, true);
            m.nav_stop();
        } else {
            let at = [t.position.x, t.position.y, t.position.z];
            m.nav_move_to_entity(cx.w, at, 0, 1.5);
        }
    }
}

#[derive(Clone, Debug)]
struct Pounce;
impl Goal for Pounce {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !fully_crouched(m) {
            return false;
        }
        let Some(t) = m.target_info(cx.w).filter(|t| t.alive) else {
            return false;
        };
        let clear = path_clear(m, cx.w, t.position);
        if !clear {
            set_flag(m, CROUCHING | INTERESTED, false);
        }
        clear
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !m.target_info(cx.w).is_some_and(|t| t.alive) {
            return false;
        }
        let yd = m.velocity.y;
        (yd * yd >= f64::from(0.05f32) || m.head_pitch.abs() >= 15.0 || !m.on_ground)
            && !flag(m, FACEPLANTED)
    }
    fn interruptable(&self) -> bool {
        false
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        m.ai.jumping = true;
        set_flag(m, POUNCING, true);
        set_flag(m, INTERESTED, false);
        if let Some(t) = m.target_info(cx.w) {
            m.look_at_speed([t.position.x, t.eye_y, t.position.z], 60.0, 30.0);
            let u = m.position.vector_to(t.position).normalize();
            m.velocity = m.velocity.add_vector(Vec3::new(u.x * 0.8, 0.9, u.z * 0.8));
        }
        m.nav_stop();
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        set_flag(m, CROUCHING | INTERESTED | POUNCING, false);
        m.anim.v[CROUCH_AMOUNT] = 0.0;
        m.anim.vo[CROUCH_AMOUNT] = 0.0;
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let target = m.target_info(cx.w);
        if let Some(t) = &target {
            m.look_at_speed([t.position.x, t.eye_y, t.position.z], 60.0, 30.0);
        }
        if !flag(m, FACEPLANTED) {
            let v = m.velocity;
            if v.y * v.y < f64::from(0.03f32) && m.head_pitch != 0.0 {
                m.head_pitch += 0.2 * wrap(-m.head_pitch);
            } else {
                let horizontal = v.horizontal_distance();
                let bias = if m.ai.jumping && v.y > 0.0 { 6.5 } else { 1.0 };
                let y = v.y * bias;
                let len = (horizontal * horizontal + y * y).sqrt();
                if len > f64::from(1.0e-5f32) {
                    let pitch = signum(-y) * jmath::acos(horizontal / len) * 180.0
                        / f64::from(std::f32::consts::PI);
                    m.head_pitch = (pitch as f32 % 360.0).clamp(-90.0, 90.0);
                }
            }
        }
        if let (Some(t), Some(who)) = (&target, m.ai.target)
            && m.distance_sqr(t.position) <= 4.0
        {
            super::ai::do_hurt_target(m, who, cx);
        } else if m.head_pitch > 0.0
            && m.on_ground
            && m.velocity.y as f32 != 0.0
            && cx.w.block_name(
                m.position.x.floor() as i32,
                m.position.y.floor() as i32,
                m.position.z.floor() as i32,
            ) == "minecraft:snow"
        {
            m.head_pitch = 60.0;
            m.ai.target = None;
            set_flag(m, FACEPLANTED, true);
        }
    }
}

#[derive(Clone, Debug)]
struct SeekShelter {
    speed: f64,
    interval: i32,
    wanted: [f64; 3],
}
impl SeekShelter {
    fn new(speed: f64) -> Self {
        Self {
            speed,
            interval: reduced(100),
            wanted: [0.0; 3],
        }
    }
    fn set_wanted_pos(&mut self, m: &mut Mob, w: &MobWorld) -> bool {
        hide_pos(m, w).is_some_and(|at| {
            self.wanted = at;
            true
        })
    }
}
impl Goal for SeekShelter {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if flag(m, SLEEPING) || m.ai.target.is_some() {
            return false;
        }
        let at = [
            m.position.x.floor() as i32,
            m.position.y.floor() as i32,
            m.position.z.floor() as i32,
        ];
        let sees_sky = cx.w.can_see_sky(at[0], at[1], at[2]);
        if cx.w.thundering && sees_sky {
            self.set_wanted_pos(m, cx.w)
        } else if self.interval > 0 {
            self.interval -= 1;
            false
        } else {
            self.interval = 100;
            cx.w.daylight && sees_sky && self.set_wanted_pos(m, cx.w)
        }
    }
    fn can_continue(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        !m.ai.nav.is_done()
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        clear_states(m);
        m.nav_move_to(cx.w, self.wanted, self.speed);
    }
}

#[derive(Clone, Debug, Default)]
struct Perch {
    rel: [f64; 2],
    look_time: i32,
    looks_remaining: i32,
}
impl Perch {
    fn reset_look(&mut self, m: &mut Mob) {
        let angle = std::f64::consts::TAU * m.random.next_double();
        self.rel = [angle.cos(), angle.sin()];
        self.look_time = reduced(80 + m.random.next_int_bound(20).unwrap_or(0));
    }
}
impl Goal for Perch {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.ai.last_hurt_by.is_none()
            && m.random.next_float() < 0.02
            && !flag(m, SLEEPING)
            && m.ai.target.is_none()
            && m.ai.nav.is_done()
            && !alertable(m, cx.w)
            && !flag(m, POUNCING | CROUCHING)
    }
    fn can_continue(&mut self, _: &mut Mob, _: &mut Cx) -> bool {
        self.looks_remaining > 0
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        self.reset_look(m);
        self.looks_remaining = 2 + m.random.next_int_bound(3).unwrap_or(0);
        set_flag(m, SITTING, true);
        m.nav_stop();
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        set_flag(m, SITTING, false);
    }
    fn tick(&mut self, m: &mut Mob, _: &mut Cx) {
        self.look_time -= 1;
        if self.look_time <= 0 {
            self.looks_remaining -= 1;
            self.reset_look(m);
        }
        let at = [
            m.position.x + self.rel[0],
            m.eye_y(),
            m.position.z + self.rel[1],
        ];
        let (y, x) = (m.kind.max_head_y_rot, m.kind.max_head_x_rot);
        m.look_at_speed(at, y, x);
    }
}

fn with_prop(w: &MobWorld, p: [i32; 3], key: &str, value: &str) -> Option<BlockStateId> {
    let mut s = w.registry.state(w.world.block(p[0], p[1], p[2]))?.clone();
    s.properties.insert(key.into(), value.into());
    w.registry.id(&s)
}
fn has_berries(w: &MobWorld, p: [i32; 3]) -> bool {
    match w.block_name(p[0], p[1], p[2]) {
        "minecraft:sweet_berry_bush" => w
            .prop(p[0], p[1], p[2], "age")
            .and_then(|a| a.parse::<i32>().ok())
            .is_some_and(|a| a >= 2),
        "minecraft:cave_vines" | "minecraft:cave_vines_plant" => {
            w.prop(p[0], p[1], p[2], "berries") == Some("true")
        }
        _ => false,
    }
}

#[derive(Clone, Debug)]
struct EatBerries {
    block: MoveToBlock,
    waited: i32,
}
impl EatBerries {
    fn new() -> Self {
        Self {
            block: MoveToBlock::new(1.2, 12, 1),
            waited: 0,
        }
    }
    fn pick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let p = self.block.block;
        let at = [
            f64::from(p[0]) + 0.5,
            f64::from(p[1]) + 0.5,
            f64::from(p[2]) + 0.5,
        ];
        if cx.w.block_name(p[0], p[1], p[2]) == "minecraft:sweet_berry_bush" {
            let age: i32 =
                cx.w.prop(p[0], p[1], p[2], "age")
                    .and_then(|a| a.parse().ok())
                    .unwrap_or(0);
            let mut count = 1 + m.random.next_int_bound(2).unwrap_or(0) + i32::from(age == 3);
            if m.extra.main_hand.is_none() {
                m.extra
                    .set_main_hand(Some("minecraft:sweet_berries".into()));
                count -= 1;
            }
            if count > 0 {
                cx.events.push(MobEvent::DropItem {
                    item: "minecraft:sweet_berries".into(),
                    count: count as u32,
                    at,
                });
            }
            if let Some(state) = with_prop(cx.w, p, "age", "1") {
                cx.events.push(MobEvent::SetBlock { pos: p, state });
            }
            cx.events.push(MobEvent::Sound(
                "block.sweet_berry_bush.pick_berries",
                at,
                1.0,
                1.0,
            ));
        } else {
            cx.events.push(MobEvent::DropItem {
                item: "minecraft:glow_berries".into(),
                count: 1,
                at,
            });
            if let Some(state) = with_prop(cx.w, p, "berries", "false") {
                cx.events.push(MobEvent::SetBlock { pos: p, state });
            }
            let pitch = 0.8 + m.random.next_float() * 0.4;
            cx.events.push(MobEvent::Sound(
                "block.cave_vines.pick_berries",
                at,
                1.0,
                pitch,
            ));
        }
    }
}
impl Goal for EatBerries {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if flag(m, SLEEPING) {
            return false;
        }
        if self.block.next_start > 0 {
            self.block.next_start -= 1;
            return false;
        }
        let next = reduced(200 + m.random.next_int_bound(200).unwrap_or(0));
        self.block.can_use(m, cx.w, next, has_berries)
    }
    fn can_continue(&mut self, _: &mut Mob, cx: &mut Cx) -> bool {
        self.block.can_continue(cx.w, has_berries)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.waited = 0;
        set_flag(m, SITTING, false);
        self.block.start(m, cx.w);
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if self.block.reached {
            if self.waited >= 40 {
                if cx.w.mob_griefing {
                    self.pick(m, cx);
                }
            } else {
                self.waited += 1;
            }
        } else if m.random.next_float() < 0.05 {
            let at = [m.position.x, m.position.y, m.position.z];
            cx.events
                .push(MobEvent::Sound("entity.fox.sniff", at, 1.0, 1.0));
        }
        self.block.tick_within(m, cx.w, 2.0);
    }
}

pub fn pose(m: &Mob, partial: f32) -> super::body_anim::Fox {
    super::body_anim::Fox {
        crouching: flag(m, CROUCHING),
        sleeping: flag(m, SLEEPING),
        sitting: flag(m, SITTING),
        pouncing: flag(m, POUNCING),
        faceplanted: flag(m, FACEPLANTED),
        head_roll_angle: m.anim.lerp(INTERESTED_ANGLE, partial) * 0.11 * std::f32::consts::PI,
        crouch_amount: m.anim.lerp(CROUCH_AMOUNT, partial),
        leg_motion_pos: (m.age as f32 + partial) * 0.67 * 3.0,
    }
}
