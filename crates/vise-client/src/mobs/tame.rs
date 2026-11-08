use super::{
    Damage, Mob, MobEvent, MobWorld, PlayerView, Species, Who,
    anim::{Poses, State},
    goals::{
        Breed, Cx, Float, Goal, HurtByTarget, JUMP, LOOK, LeapAtTarget, LookAtPlayer, MOVE,
        MeleeAttack, NearestTarget, Panic, Prey, RandomLookAround, Stroll, TARGET, goal_clone,
        reduced,
    },
};
use vise_core::geometry::trig::Mth;
use vise_core::{
    geometry::{Aabb, Axis, Vec3},
    random::RandomSource,
};
use std::f32::consts::PI;

pub const WOLF_VARIANTS: [(&str, &str); 9] = [
    ("pale", "wolf"),
    ("spotted", "wolf_spotted"),
    ("snowy", "wolf_snowy"),
    ("black", "wolf_black"),
    ("ashen", "wolf_ashen"),
    ("rusty", "wolf_rusty"),
    ("woods", "wolf_woods"),
    ("chestnut", "wolf_chestnut"),
    ("striped", "wolf_striped"),
];
pub fn wolf_variant_for(biome: &str, has_tag: &dyn Fn(&str) -> bool) -> u8 {
    if has_tag("minecraft:is_savanna") {
        1
    } else if biome == "minecraft:grove" {
        2
    } else if biome == "minecraft:old_growth_pine_taiga" {
        3
    } else if biome == "minecraft:snowy_taiga" {
        4
    } else if has_tag("minecraft:is_jungle") {
        5
    } else if biome == "minecraft:forest" {
        6
    } else if biome == "minecraft:old_growth_spruce_taiga" {
        7
    } else if has_tag("minecraft:is_badlands") {
        8
    } else {
        0
    }
}
pub const RED: u8 = 14;

pub const INTEREST: usize = 0;
pub const SHAKE: usize = 1;

pub fn register(m: &mut Mob) {
    m.extra.color = RED;
    let food = m.kind.food.clone();
    let g = &mut m.ai.goals;
    g.add(1, Float);
    g.add(1, TamePanic(Panic::new(1.5)));
    g.add(2, SitWhenOrdered);
    g.add(3, AvoidLlamas::new());
    g.add(4, LeapAtTarget::new(0.4));
    g.add(5, MeleeAttack::new(1.0, true));
    g.add(6, FollowOwner::new(1.0, 10.0, 2.0));
    g.add(7, Breed::new(1.0));
    g.add(8, Stroll::avoiding_water(1.0));
    g.add(9, Beg { food, ticks: 0 });
    g.add(10, LookAtPlayer::new(8.0));
    g.add(10, RandomLookAround::default());
    m.ai.targets.add(1, OwnerHurtBy { timestamp: 0 });
    m.ai.targets.add(2, OwnerHurt { timestamp: 0 });
    m.ai.targets.add(3, HurtByTarget::new().alert());
    m.ai.targets
        .add(4, NearestTarget::player(true).when(angry_at_player));
    m.ai.targets.add(
        5,
        NearestTarget::new(Prey::Filter(wolf_prey), false).when(|m, _| !m.extra.tame),
    );
    m.ai.targets.add(
        6,
        NearestTarget::new(Prey::Filter(baby_turtle_on_land), false).when(|m, _| !m.extra.tame),
    );
    m.ai.targets.add(
        7,
        NearestTarget::species(
            &[
                Species::Skeleton,
                Species::Stray,
                Species::Bogged,
                Species::WitherSkeleton,
            ],
            false,
        ),
    );
}
pub fn register_parrot(m: &mut Mob) {
    let g = &mut m.ai.goals;
    g.add(0, TamePanic(Panic::new(1.25)));
    g.add(0, Float);
    g.add(1, LookAtPlayer::new(8.0));
    g.add(2, SitWhenOrdered);
    g.add(2, FollowOwner::new(1.0, 5.0, 1.0));
    g.add(2, Stroll::parrot(1.0));
    g.add(3, LandOnOwnersShoulder);
}
#[derive(Clone, Debug)]
struct LandOnOwnersShoulder;
impl Goal for LandOnOwnersShoulder {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        is_owner(m, cx.w)
            && owner(m, cx.w).is_some_and(|(_, _, spectator)| !spectator)
            && !m.extra.sitting
            && m.age > 100
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if !m.extra.sitting
            && m.extra.leash.is_none()
            && is_owner(m, cx.w)
            && cx
                .w
                .player
                .as_ref()
                .is_some_and(|p| m.bounds().intersects(p.bounds))
        {
            cx.events.push(MobEvent::ParrotToShoulder { mob: m.id });
        }
    }
}
fn wolf_prey(s: &super::Seen) -> bool {
    matches!(s.species, Species::Sheep | Species::Rabbit | Species::Fox)
}
fn baby_turtle_on_land(s: &super::Seen) -> bool {
    s.species == Species::Turtle && s.baby
}
fn angry_at_player(m: &Mob, _: &MobWorld) -> bool {
    m.extra.angry_at == Some(Who::Player)
}
pub fn angry(m: &Mob) -> bool {
    m.extra.anger > 0
}
pub fn set_tame(m: &mut Mob, tame: bool) {
    m.extra.tame = tame;
    if tame {
        m.attrs.max_health = 40.0;
        m.health = 40.0;
    } else {
        m.attrs.max_health = 8.0;
    }
}
fn owner_view<'a>(m: &Mob, w: &'a MobWorld) -> Option<&'a PlayerView> {
    if !m.extra.tame {
        return None;
    }
    let p = w.player.as_ref()?;
    (m.extra.owner == Some(p.uuid)).then_some(p)
}
fn owner(m: &Mob, w: &MobWorld) -> Option<(Vec3, f64, bool)> {
    let p = owner_view(m, w)?;
    Some((p.position, p.eye[1], p.spectator))
}
fn is_owner(m: &Mob, w: &MobWorld) -> bool {
    w.player
        .as_ref()
        .is_some_and(|p| owned_by(m.extra.owner, p.uuid))
}
pub fn owned_by(owner: Option<[i32; 4]>, uuid: [i32; 4]) -> bool {
    owner.is_some_and(|o| o == uuid || (o == [0, 0, 0, 1] && uuid == [0; 4]))
}
fn unable_to_move_to_owner(m: &Mob, w: &MobWorld) -> bool {
    m.extra.sitting || owner_view(m, w).is_some_and(|p| p.spectator)
}
fn teleport_to_owner(m: &mut Mob, w: &MobWorld) {
    let Some((at, _, _)) = owner(m, w) else {
        return;
    };
    let base = [
        at.x.floor() as i32,
        at.y.floor() as i32,
        at.z.floor() as i32,
    ];
    for _ in 0..10 {
        let xd = m.random.next_int_bound(7).unwrap_or(0) - 3;
        let zd = m.random.next_int_bound(7).unwrap_or(0) - 3;
        if xd.abs() < 2 && zd.abs() < 2 {
            continue;
        }
        let yd = m.random.next_int_bound(3).unwrap_or(0) - 1;
        let p = [base[0] + xd, base[1] + yd, base[2] + zd];
        if can_teleport_to(m, w, p) {
            m.position = Vec3::new(
                f64::from(p[0]) + 0.5,
                f64::from(p[1]),
                f64::from(p[2]) + 0.5,
            );
            m.previous = m.position;
            m.nav_stop();
            return;
        }
    }
}
fn can_teleport_to(m: &Mob, w: &MobWorld, p: [i32; 3]) -> bool {
    let below = w.facts.get(w.world.block(p[0], p[1] - 1, p[2]));
    let flies = m.species() == Species::Parrot;
    if !below.full_collision || (!flies && below.name.ends_with("_leaves")) {
        return false;
    }
    let here = w.facts.get(w.world.block(p[0], p[1], p[2]));
    if here.fluid != crate::blocks::Fluid::None {
        return false;
    }
    let half = f64::from(m.width()) / 2.0;
    let (x, y, z) = (
        f64::from(p[0]) + 0.5,
        f64::from(p[1]),
        f64::from(p[2]) + 0.5,
    );
    w.no_collision(Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(m.height()), z + half),
    ))
}
fn owner_distance_sqr(m: &Mob, w: &MobWorld) -> Option<f64> {
    owner(m, w).map(|(at, _, _)| m.distance_sqr(at))
}

#[derive(Clone, Debug)]
struct TamePanic(Panic);
impl Goal for TamePanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.fire_ticks > 0 && self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if !unable_to_move_to_owner(m, cx.w)
            && owner_distance_sqr(m, cx.w).is_some_and(|d| d >= 144.0)
        {
            teleport_to_owner(m, cx.w);
        }
        self.0.tick(m, cx);
    }
}
#[derive(Clone, Debug)]
struct SitWhenOrdered;
impl Goal for SitWhenOrdered {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !m.extra.sitting && !m.extra.tame {
            return false;
        }
        if m.fluids.in_water || !m.on_ground {
            return false;
        }
        match owner_distance_sqr(m, cx.w) {
            None => true,
            Some(d) => {
                let attacked = owner_view(m, cx.w).is_some_and(|p| p.hurt_by.is_some());
                !(d < 144.0 && attacked) && m.extra.sitting
            }
        }
    }
    fn can_continue(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        m.extra.sitting
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
        m.extra.flag = false;
    }
}
#[derive(Clone, Debug)]
struct AvoidLlamas(super::goals::AvoidEntity);
impl AvoidLlamas {
    fn new() -> Self {
        Self(super::goals::AvoidEntity::new(
            super::goals::Avoid::Species(vec![Species::Llama, Species::TraderLlama]),
            24.0,
            1.5,
            1.5,
        ))
    }
}
impl Goal for AvoidLlamas {
    goal_clone!();
    fn name(&self) -> &'static str {
        "WolfAvoidEntityGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !self.0.can_use(m, cx) {
            return false;
        }
        let Some(strength) = self
            .0
            .found
            .and_then(|id| cx.w.seen(id))
            .map(|s| s.profession)
        else {
            return false;
        };
        !m.extra.tame && i32::from(strength) >= m.random.next_int_bound(5).unwrap_or(0)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        m.ai.target = None;
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        m.ai.target = None;
        self.0.tick(m, cx)
    }
}
#[derive(Clone, Debug)]
struct FollowOwner {
    speed: f64,
    start: f64,
    stop: f64,
    recalc: i32,
}
impl FollowOwner {
    fn new(speed: f64, start: f64, stop: f64) -> Self {
        Self {
            speed,
            start,
            stop,
            recalc: 0,
        }
    }
}
impl Goal for FollowOwner {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(d) = owner_distance_sqr(m, cx.w) else {
            return false;
        };
        !unable_to_move_to_owner(m, cx.w) && d >= self.start * self.start
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if m.ai.nav.is_done() || unable_to_move_to_owner(m, cx.w) {
            return false;
        }
        owner_distance_sqr(m, cx.w).is_some_and(|d| d > self.stop * self.stop)
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        self.recalc = 0;
        m.set_malus(super::path::PathType::Water, 0.0);
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
        let water = super::path::PathType::Water.malus();
        m.set_malus(super::path::PathType::Water, water);
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let Some((at, eye, _)) = owner(m, cx.w) else {
            return;
        };
        let far = m.distance_sqr(at) >= 144.0;
        if !far {
            let max = m.max_head_x_rot();
            m.look_at_speed([at.x, eye, at.z], 10.0, max);
        }
        self.recalc -= 1;
        if self.recalc <= 0 {
            self.recalc = reduced(10);
            if far {
                teleport_to_owner(m, cx.w);
            } else {
                m.nav_move_to_entity(cx.w, [at.x, at.y, at.z], 0, self.speed);
            }
        }
    }
}
#[derive(Clone, Debug)]
struct OwnerHurtBy {
    timestamp: i64,
}
impl Goal for OwnerHurtBy {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !m.extra.tame || m.extra.sitting {
            return false;
        }
        let Some((who, at)) = owner_view(m, cx.w).and_then(|p| p.hurt_by) else {
            return false;
        };
        at != self.timestamp && wants_to_attack(m, cx.w, who)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.ai.target
            .and_then(|t| cx.w.target_info(t))
            .is_some_and(|t| t.alive)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some((who, at)) = owner_view(m, cx.w).and_then(|p| p.hurt_by) {
            m.ai.target = Some(who);
            self.timestamp = at;
        }
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.ai.target = None;
    }
}
#[derive(Clone, Debug)]
struct OwnerHurt {
    timestamp: i64,
}
impl Goal for OwnerHurt {
    goal_clone!();
    fn flags(&self) -> u8 {
        TARGET
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !m.extra.tame || m.extra.sitting {
            return false;
        }
        let Some((who, at)) = owner_view(m, cx.w).and_then(|p| p.last_hurt) else {
            return false;
        };
        at != self.timestamp && wants_to_attack(m, cx.w, who)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.ai.target
            .and_then(|t| cx.w.target_info(t))
            .is_some_and(|t| t.alive)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some((who, at)) = owner_view(m, cx.w).and_then(|p| p.last_hurt) {
            m.ai.target = Some(who);
            self.timestamp = at;
        }
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.ai.target = None;
    }
}
fn wants_to_attack(m: &Mob, w: &MobWorld, who: Who) -> bool {
    let Who::Mob(id) = who else {
        return false;
    };
    if id == m.id {
        return false;
    }
    let Some(s) = w.seen(id) else {
        return false;
    };
    match s.species {
        Species::Creeper | Species::Ghast => false,
        Species::Wolf => !s.tame || s.owner != m.extra.owner,
        Species::Cat | Species::Parrot => !s.tame,
        Species::Horse | Species::Donkey | Species::Mule | Species::Llama => !s.tame,
        _ => true,
    }
}
#[derive(Clone, Debug)]
struct Beg {
    food: std::sync::Arc<[String]>,
    ticks: i32,
}
impl Beg {
    fn interesting(&self, w: &MobWorld) -> bool {
        let Some(p) = &w.player else {
            return false;
        };
        let held = |i: &Option<std::sync::Arc<str>>| {
            i.as_deref()
                .is_some_and(|i| i == "minecraft:bone" || self.food.iter().any(|f| f == i))
        };
        p.alive && !p.spectator && (held(&p.main_hand) || held(&p.off_hand))
    }
}
impl Goal for Beg {
    goal_clone!();
    fn flags(&self) -> u8 {
        LOOK
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        cx.w.player
            .as_ref()
            .is_some_and(|p| m.distance_sqr(p.position) <= 64.0)
            && self.interesting(cx.w)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        cx.w.player
            .as_ref()
            .is_some_and(|p| p.alive && m.distance_sqr(p.position) <= 64.0)
            && self.ticks > 0
            && self.interesting(cx.w)
    }
    fn start(&mut self, m: &mut Mob, _: &mut Cx) {
        m.extra.flag = true;
        self.ticks = reduced(40 + m.random.next_int_bound(40).unwrap_or(0));
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.extra.flag = false;
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(p) = &cx.w.player {
            let max = m.max_head_x_rot();
            m.look_at_speed([p.position.x, p.eye[1], p.position.z], 10.0, max);
        }
        self.ticks -= 1;
    }
}
fn valid_player_target(w: &MobWorld) -> bool {
    w.player
        .as_ref()
        .is_some_and(|p| !p.creative && !p.spectator)
        && w.difficulty.id != 0
}
fn stop_being_angry(m: &mut Mob) {
    m.ai.last_hurt_by = None;
    m.ai.last_hurt_by_time = m.age as i32;
    m.ai.target = None;
    m.extra.angry_at = None;
    m.extra.anger = 0;
}
pub fn forgive_dead_player(m: &mut Mob) {
    if m.extra.angry_at == Some(Who::Player) {
        stop_being_angry(m);
    }
}
fn update_persistent_anger(m: &mut Mob, w: &MobWorld) {
    let angry_at = m.extra.angry_at;
    if let Some(target @ Who::Mob(id)) = m.ai.target
        && w.seen(id).is_none_or(|s| !s.alive)
        && angry_at == Some(target)
    {
        stop_being_angry(m);
        return;
    }
    let target = m.ai.target.filter(|t| {
        *t != Who::Player
            || w.player
                .as_ref()
                .is_some_and(|p| !p.creative && !p.spectator)
    });
    if let Some(target) = target {
        m.extra.angry_at = Some(target);
        m.extra.anger = 400 + m.random.next_int_bound(381).unwrap_or(0);
    } else if m.extra.anger > 0 {
        m.extra.anger -= 1;
    }
    if angry_at.is_some()
        && m.extra.anger == 0
        && !(target == Some(Who::Player) && valid_player_target(w))
    {
        stop_being_angry(m);
    }
    if angry_at == Some(Who::Player)
        && w.player
            .as_ref()
            .is_some_and(|p| p.creative || p.spectator || w.difficulty.id == 0)
    {
        stop_being_angry(m);
    }
}
pub fn interest_tick(m: &mut Mob) {
    let goal = if m.extra.flag { 1.0 } else { 0.0 };
    m.anim.vo[INTEREST] = m.anim.v[INTEREST];
    m.anim.v[INTEREST] += (goal - m.anim.v[INTEREST]) * 0.4;
}
pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    update_persistent_anger(m, w);
    if !m.alive() {
        return;
    }
    interest_tick(m);
    let wet = m.fluids.in_water || super::ai::in_rain(m, w);
    server_shake(m, wet, events);
}
pub fn server_shake(m: &mut Mob, wet: bool, events: &mut Vec<MobEvent>) {
    if m.extra.flag2 && m.extra.counter2 == 0 && m.ai.nav.is_done() && m.on_ground {
        start_shaking(m);
        broadcast(m, events, 8);
    }
    shake_tick(m, wet, true, events);
}
pub fn client_tick(m: &mut Mob, rain: bool, events: &mut Vec<MobEvent>) {
    if m.alive() {
        shake_tick(m, m.fluids.in_water || rain, false, events);
    }
}
pub fn in_rain(m: &Mob, w: &MobWorld) -> bool {
    super::ai::in_rain(m, w)
}
pub fn entity_event(m: &mut Mob, event: u8) {
    match event {
        8 => start_shaking(m),
        56 => cancel_shake(m),
        _ => {}
    }
}
fn start_shaking(m: &mut Mob) {
    m.extra.counter2 = 1;
    m.anim.v[SHAKE] = 0.0;
    m.anim.vo[SHAKE] = 0.0;
}
fn cancel_shake(m: &mut Mob) {
    m.extra.counter2 = 0;
    m.anim.v[SHAKE] = 0.0;
    m.anim.vo[SHAKE] = 0.0;
}
fn broadcast(m: &Mob, events: &mut Vec<MobEvent>, event: i8) {
    events.push(MobEvent::EntityEvent {
        mob: m.id,
        event,
        at: [m.position.x, m.position.y, m.position.z],
    });
}
fn shake_tick(m: &mut Mob, wet: bool, server: bool, events: &mut Vec<MobEvent>) {
    let shaking = m.extra.counter2 > 0;
    if wet {
        m.extra.flag2 = true;
        if shaking && server {
            broadcast(m, events, 56);
            cancel_shake(m);
        }
    } else if shaking {
        if m.anim.v[SHAKE] == 0.0 {
            let pitch = (m.random.next_float() - m.random.next_float()) * 0.2 + 1.0;
            if server {
                events.push(MobEvent::Sound("entity.wolf.shake", m.center(), 0.4, pitch));
            }
        }
        m.anim.vo[SHAKE] = m.anim.v[SHAKE];
        m.anim.v[SHAKE] += 0.05;
        if m.anim.vo[SHAKE] >= 2.0 {
            m.extra.flag2 = false;
            cancel_shake(m);
        }
        if m.anim.v[SHAKE] > 0.4 {
            let y = m.position.y as f32 + 0.8;
            let count = (((m.anim.v[SHAKE] - 0.4) * PI).msin() * 7.0) as i32;
            let half = m.width() * 0.5;
            let v = m.velocity;
            for _ in 0..count {
                let dx = (m.random.next_float() * 2.0 - 1.0) * half;
                let dz = (m.random.next_float() * 2.0 - 1.0) * half;
                events.push(MobEvent::ParticleAt {
                    kind: "splash",
                    at: [
                        m.position.x + f64::from(dx),
                        f64::from(y),
                        m.position.z + f64::from(dz),
                    ],
                    velocity: [v.x, v.y, v.z],
                });
            }
        }
    }
}
pub fn after_hurt(m: &mut Mob, _source: &Damage) {
    m.extra.sitting = false;
}
fn tail_angle(m: &Mob) -> f32 {
    if angry(m) {
        1.539_380_4
    } else if m.extra.tame {
        let max = m.attrs.max_health;
        let ratio = (max - m.health) / max;
        (0.55 - ratio * 0.4) * PI
    } else {
        PI / 5.0
    }
}
fn body_roll(shake: f32, offset: f32) -> f32 {
    let p = ((shake + offset) / 1.8).clamp(0.0, 1.0);
    (p * PI).msin() * (p * PI * 11.0).msin() * 0.15 * PI
}
pub fn anim(m: &Mob, s: &State, p: &mut Poses) {
    let (pos, speed) = (s.pos, s.speed);
    let baby = m.is_baby();
    if angry(m) {
        p.yrot("tail", 0.0);
    } else {
        p.yrot("tail", (pos * 0.6662).mcos() * 1.4 * speed);
    }
    if m.extra.sitting {
        let age = if baby { 0.5 } else { 1.0 };
        let add = |p: &mut Poses, part: &str, x: f32, y: f32, z: f32| {
            let v = p.get(part);
            v[0] += x;
            v[1] += y;
            v[2] += z;
        };
        add(p, "body", 0.0, 4.0 * age, -2.0 * age);
        p.xrot("body", PI / 4.0);
        add(p, "tail", 0.0, 9.0 * age, -2.0 * age);
        add(p, "right_hind_leg", 0.0, 6.7 * age, -5.0 * age);
        p.xrot("right_hind_leg", PI * 3.0 / 2.0);
        add(p, "left_hind_leg", 0.0, 6.7 * age, -5.0 * age);
        p.xrot("left_hind_leg", PI * 3.0 / 2.0);
        p.xrot("right_front_leg", 5.811_947);
        add(p, "right_front_leg", 0.01 * age, 1.0 * age, 0.0);
        p.xrot("left_front_leg", 5.811_947);
        add(p, "left_front_leg", -0.01 * age, 1.0 * age, 0.0);
        if baby {
            let x = p.get("body")[3] - PI / 2.0;
            p.xrot("body", x);
        } else if p.has("upper_body") {
            p.get("upper_body")[1] += 2.0;
            p.xrot("upper_body", PI * 2.0 / 5.0);
            p.yrot("upper_body", 0.0);
        }
    } else {
        p.xrot("right_hind_leg", (pos * 0.6662).mcos() * 1.4 * speed);
        p.xrot("left_hind_leg", (pos * 0.6662 + PI).mcos() * 1.4 * speed);
        p.xrot("right_front_leg", (pos * 0.6662 + PI).mcos() * 1.4 * speed);
        p.xrot("left_front_leg", (pos * 0.6662).mcos() * 1.4 * speed);
    }
    let shake = m.anim.lerp(SHAKE, s.partial);
    let roll = m.anim.lerp(INTEREST, s.partial) * 0.15 * PI;
    p.zrot("body", body_roll(shake, -0.16));
    if baby {
        p.zrot("head", roll + body_roll(shake, 0.0));
        p.zrot("tail", body_roll(shake, -0.2));
    } else {
        if p.has("real_head") {
            p.zrot("real_head", roll + body_roll(shake, 0.0));
        }
        if p.has("upper_body") {
            p.zrot("upper_body", body_roll(shake, -0.08));
        }
        if p.has("real_tail") {
            p.zrot("real_tail", body_roll(shake, -0.2));
        }
    }
    p.xrot("head", s.x_rot.to_radians());
    p.yrot("head", s.y_rot.to_radians());
    p.xrot("tail", tail_angle(m));
}
pub fn tint(m: &Mob, partial: f32) -> u32 {
    if !m.extra.flag2 {
        return 0xFFFF_FFFF;
    }
    let shade = (0.75 + m.anim.lerp(SHAKE, partial) / 2.0 * 0.25).min(1.0);
    let c = (shade * 255.0) as u32;
    0xFF00_0000 | c << 16 | c << 8 | c
}
pub fn texture(m: &Mob) -> String {
    let file = WOLF_VARIANTS[(m.extra.variant as usize).min(8)].1;
    let state = if m.extra.tame {
        "_tame"
    } else if angry(m) {
        "_angry"
    } else {
        ""
    };
    let baby = if m.is_baby() { "_baby" } else { "" };
    format!("entity/wolf/{file}{state}{baby}")
}

pub const CAT_VARIANTS: [&str; 11] = [
    "tabby",
    "black",
    "red",
    "siamese",
    "british_shorthair",
    "calico",
    "persian",
    "ragdoll",
    "white",
    "jellie",
    "all_black",
];
const LIE: usize = 0;
const LIE_TAIL: usize = 1;
const RELAX: usize = 2;

pub fn register_cat(m: &mut Mob) {
    m.extra.color = RED;
    let food = m.kind.food.clone();
    let g = &mut m.ai.goals;
    g.add(1, Float);
    g.add(1, TamePanic(Panic::new(1.5)));
    g.add(2, SitWhenOrdered);
    g.add(3, CatRelaxOnOwner::default());
    g.add(4, CatTempt(super::goals::Tempt::new(0.6, food, true), None));
    g.add(
        5,
        CatLieOnBlock(super::goals::MoveToBlock {
            vertical_start: -2,
            ..super::goals::MoveToBlock::new(1.1, 8, 6)
        }),
    );
    g.add(6, FollowOwner::new(1.0, 10.0, 5.0));
    g.add(7, CatSitOnBlock(super::goals::MoveToBlock::new(0.8, 8, 1)));
    g.add(8, LeapAtTarget::new(0.3));
    g.add(9, OcelotAttack { attack_time: 0 });
    g.add(10, Breed::new(0.8));
    g.add(11, Stroll::avoiding_water(0.8));
    g.add(12, LookAtPlayer::new(10.0));
    g.add(
        4,
        super::goals::AvoidEntity::new(super::goals::Avoid::Player, 16.0, 0.8, 1.33)
            .when(|m| !m.extra.tame),
    );
    m.ai.targets.add(
        1,
        NearestTarget::species(&[Species::Rabbit], false).when(|m, _| !m.extra.tame),
    );
    m.ai.targets.add(
        1,
        NearestTarget::new(Prey::Filter(baby_turtle_on_land), false).when(|m, _| !m.extra.tame),
    );
}
pub fn register_ocelot(m: &mut Mob) {
    let food = m.kind.food.clone();
    let mut stroll = Stroll::avoiding_water(0.8);
    stroll.water_avoiding = Some(1.000_000_1e-5);
    let g = &mut m.ai.goals;
    g.add(1, Float);
    g.add(3, OcelotTempt(super::goals::Tempt::new(0.6, food, true)));
    g.add(7, LeapAtTarget::new(0.3));
    g.add(8, OcelotAttack { attack_time: 0 });
    g.add(9, Breed::new(0.8));
    g.add(10, stroll);
    g.add(11, LookAtPlayer::new(10.0));
    g.add(4, OcelotAvoid::new());
    m.ai.targets
        .add(1, NearestTarget::species(&[Species::Chicken], false));
    m.ai.targets.add(
        1,
        NearestTarget::new(Prey::Filter(baby_turtle_on_land), false),
    );
}
#[derive(Clone, Debug)]
struct OcelotTempt(super::goals::Tempt);
impl Goal for OcelotTempt {
    goal_clone!();
    fn name(&self) -> &'static str {
        "OcelotTemptGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.tick(m, cx)
    }
}
#[derive(Clone, Debug)]
struct OcelotAvoid(super::goals::AvoidEntity);
impl OcelotAvoid {
    fn new() -> Self {
        Self(super::goals::AvoidEntity::new(
            super::goals::Avoid::Player,
            16.0,
            0.8,
            1.33,
        ))
    }
}
impl Goal for OcelotAvoid {
    goal_clone!();
    fn name(&self) -> &'static str {
        "OcelotAvoidEntityGoal"
    }
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.tick(m, cx)
    }
}
pub fn cat_sitting(m: &Mob) -> bool {
    m.extra.sitting || m.extra.flag2
}
#[derive(Clone, Debug)]
struct CatTempt(super::goals::Tempt, Option<[i32; 4]>);
impl Goal for CatTempt {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        !m.extra.tame && self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let picked = self.1.is_some() && self.1 == cx.w.player.as_ref().map(|p| p.uuid);
        let scare = self.0.can_scare;
        self.0.can_scare = scare && !picked;
        let go = self.0.can_continue(m, cx);
        self.0.can_scare = scare;
        go
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if m.age.is_multiple_of(100) {
            cx.events.push(MobEvent::Sound(
                super::finalize::variant_sound(m, "beg_for_food")
                    .unwrap_or("entity.cat.beg_for_food"),
                m.center(),
                1.0,
                1.0,
            ));
        }
        self.0.tick(m, cx);
        if self.1.is_none() && m.random.next_int_bound(reduced(600)).unwrap_or(1) == 0 {
            self.1 = cx.w.player.as_ref().map(|p| p.uuid);
        } else if m.random.next_int_bound(reduced(500)).unwrap_or(1) == 0 {
            self.1 = None;
        }
    }
}
fn has_tag(w: &MobWorld, p: [i32; 3], tag: &str) -> bool {
    let name = w.facts.get(w.world.block(p[0], p[1], p[2])).name.clone();
    w.block_tags.get(tag).is_some_and(|t| t.contains(&*name))
}
fn empty_above(w: &MobWorld, p: [i32; 3]) -> bool {
    w.facts
        .get(w.world.block(p[0], p[1] + 1, p[2]))
        .name
        .as_ref()
        == "minecraft:air"
}
pub fn is_owner_of(m: &Mob, p: &super::PlayerView) -> bool {
    m.extra.owner.is_none_or(|o| o == p.uuid)
}
#[derive(Clone, Debug, Default)]
struct CatRelaxOnOwner {
    goal: Option<[i32; 3]>,
    on_bed_ticks: i32,
}
impl CatRelaxOnOwner {
    fn occupied(&self, m: &Mob, w: &MobWorld) -> bool {
        let Some(g) = self.goal else { return true };
        let area = Aabb::new(
            Vec3::new(
                f64::from(g[0]) - 2.0,
                f64::from(g[1]) - 2.0,
                f64::from(g[2]) - 2.0,
            ),
            Vec3::new(
                f64::from(g[0]) + 3.0,
                f64::from(g[1]) + 3.0,
                f64::from(g[2]) + 3.0,
            ),
        );
        w.others.iter().any(|o| {
            o.species == Species::Cat && o.id != m.id && o.lying && o.bounds.intersects(area)
        })
    }
    fn sleeping_owner<'w>(m: &Mob, w: &'w MobWorld) -> Option<&'w super::PlayerView> {
        w.player
            .as_ref()
            .filter(|p| m.extra.tame && !m.extra.sitting && p.sleeping && is_owner_of(m, p))
    }
}
impl Goal for CatRelaxOnOwner {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let w = cx.w;
        let Some(p) = Self::sleeping_owner(m, w) else {
            return false;
        };
        if m.distance_sqr(p.position) > 100.0 {
            return false;
        }
        let at = [
            p.position.x.floor() as i32,
            p.position.y.floor() as i32,
            p.position.z.floor() as i32,
        ];
        if !has_tag(w, at, "minecraft:cats_can_lie_on") {
            return false;
        }
        let back = match w.prop(at[0], at[1], at[2], "facing") {
            Some("north") => [0, 0, 1],
            Some("south") => [0, 0, -1],
            Some("east") => [-1, 0, 0],
            Some("west") => [1, 0, 0],
            Some("up") => [0, -1, 0],
            Some("down") => [0, 1, 0],
            _ => [0, 0, 0],
        };
        self.goal = Some([at[0] + back[0], at[1] + back[1], at[2] + back[2]]);
        !self.occupied(m, w)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        Self::sleeping_owner(m, cx.w).is_some() && self.goal.is_some() && !self.occupied(m, cx.w)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(g) = self.goal {
            m.extra.flag2 = false;
            m.nav_move_to(
                cx.w,
                [f64::from(g[0]), f64::from(g[1]), f64::from(g[2])],
                1.1,
            );
        }
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        m.extra.flag = false;
        let day = cx.w.day_time.rem_euclid(24000);
        let chance = if !cx.w.nether && !cx.w.end && !(362..23667).contains(&day) {
            0.7
        } else {
            0.0
        };
        if cx
            .w
            .player
            .as_ref()
            .is_some_and(|p| is_owner_of(m, p) && p.sleep_timer >= 100)
            && m.random.next_float() < chance
        {
            give_morning_gift(m, cx);
        }
        self.on_bed_ticks = 0;
        m.extra.relax_one = false;
        m.nav_stop();
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let (Some(p), Some(g)) = (cx.w.player.as_ref(), self.goal) else {
            return;
        };
        m.extra.flag2 = false;
        m.nav_move_to(
            cx.w,
            [f64::from(g[0]), f64::from(g[1]), f64::from(g[2])],
            1.1,
        );
        if m.distance_sqr(p.position) < 2.5 {
            self.on_bed_ticks += 1;
            if self.on_bed_ticks > reduced(16) {
                m.extra.flag = true;
                m.extra.relax_one = false;
            } else {
                m.look_at_speed(p.eye, 45.0, 45.0);
                m.extra.relax_one = true;
            }
        } else {
            m.extra.flag = false;
        }
    }
}
fn give_morning_gift(m: &mut Mob, cx: &mut Cx) {
    let w = cx.w;
    let at = [
        m.position.x.floor() as i32,
        m.position.y.floor() as i32,
        m.position.z.floor() as i32,
    ];
    let x = at[0] + m.random.next_int_bound(11).unwrap_or(5) - 5;
    let y = at[1] + m.random.next_int_bound(5).unwrap_or(2) - 2;
    let z = at[2] + m.random.next_int_bound(11).unwrap_or(5) - 5;
    let tagged = |p: [i32; 3], tag| has_tag(w, p, tag);
    let mut ground = y - 1;
    while ground > w.world.bottom_y()
        && !tagged([x, ground, z], "minecraft:entities_can_teleport_to")
    {
        ground -= 1;
    }
    if w.world.loaded(x, z) && ground > w.world.bottom_y() {
        let feet = f64::from(ground + 1);
        let half = f64::from(m.width()) / 2.0;
        let b = Aabb::new(
            Vec3::new(f64::from(x) - half, feet, f64::from(z) - half),
            Vec3::new(
                f64::from(x) + half,
                feet + f64::from(m.height()),
                f64::from(z) + half,
            ),
        );
        let dangerous = |p| tagged(p, "minecraft:dangerous_for_teleportation");
        let in_box = |f: &dyn Fn([i32; 3]) -> bool| {
            (b.min(Axis::X).floor() as i32..=b.max(Axis::X).floor() as i32).any(|bx| {
                (b.min(Axis::Y).floor() as i32..=b.max(Axis::Y).floor() as i32).any(|by| {
                    (b.min(Axis::Z).floor() as i32..=b.max(Axis::Z).floor() as i32)
                        .any(|bz| f([bx, by, bz]))
                })
            })
        };
        let fluids = w.fluids(b, feet);
        if !dangerous([x, ground, z])
            && w.no_collision(b)
            && !(fluids.in_water || fluids.in_lava)
            && !in_box(&dangerous)
        {
            m.position = Vec3::new(f64::from(x), feet, f64::from(z));
            m.previous = m.position;
            m.velocity = Vec3::ZERO;
        }
    }
    let yaw = m.body_yaw * (PI / 180.0);
    let here = [
        m.position.x.floor() - f64::from(yaw.msin()),
        m.position.y.floor(),
        m.position.z.floor() + f64::from(yaw.mcos()),
    ];
    cx.events.push(MobEvent::DropGift {
        table: "minecraft:gameplay/cat_morning_gift",
        at: here,
    });
}
#[derive(Clone, Debug)]
struct CatLieOnBlock(super::goals::MoveToBlock);
fn can_lie_on(w: &MobWorld, p: [i32; 3]) -> bool {
    empty_above(w, p) && has_tag(w, p, "minecraft:cats_can_lie_on")
}
impl Goal for CatLieOnBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.extra.tame
            && !m.extra.sitting
            && !m.extra.flag
            && self.0.can_use(m, cx.w, reduced(40), can_lie_on)
    }
    fn can_continue(&mut self, _: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(cx.w, can_lie_on)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx.w);
        m.extra.flag2 = false;
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.extra.flag = false;
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.tick(m, cx.w);
        m.extra.flag2 = false;
        m.extra.flag = self.0.reached;
    }
}
#[derive(Clone, Debug)]
struct CatSitOnBlock(super::goals::MoveToBlock);
fn can_sit_on(w: &MobWorld, p: [i32; 3]) -> bool {
    if !empty_above(w, p) || !has_tag(w, p, "minecraft:cats_can_sit_on") {
        return false;
    }
    let f = w.facts.get(w.world.block(p[0], p[1], p[2]));
    match f.name.as_ref() {
        "minecraft:furnace" => f.emission > 0,
        "minecraft:chest" => {
            let key = (p[0], p[1], p[2]);
            !w.lid_openers.contains_key(&key)
                && w.world.chest_openers.get(&key).is_none_or(|o| o.0 < 1)
        }
        _ => w.prop(p[0], p[1], p[2], "part") != Some("head"),
    }
}
impl Goal for CatSitOnBlock {
    goal_clone!();
    fn flags(&self) -> u8 {
        JUMP | MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if !m.extra.tame || m.extra.sitting {
            return false;
        }
        if self.0.next_start > 0 {
            self.0.next_start -= 1;
            return false;
        }
        let next = reduced(200 + m.random.next_int_bound(200).unwrap_or(0));
        self.0.can_use(m, cx.w, next, can_sit_on)
    }
    fn can_continue(&mut self, _: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(cx.w, can_sit_on)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx.w);
        m.extra.flag2 = false;
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.extra.flag2 = false;
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.tick(m, cx.w);
        m.extra.flag2 = self.0.reached;
    }
}
#[derive(Clone, Debug)]
struct OcelotAttack {
    attack_time: i32,
}
impl Goal for OcelotAttack {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        m.ai.target.is_some()
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        let Some(t) = m.ai.target.and_then(|t| cx.w.target_info(t)) else {
            return false;
        };
        t.alive
            && m.distance_sqr(t.position) <= 225.0
            && (!m.ai.nav.is_done() || m.ai.target.is_some())
    }
    fn stop(&mut self, m: &mut Mob, _: &mut Cx) {
        m.nav_stop();
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let Some(who) = m.ai.target else {
            return;
        };
        let Some(t) = cx.w.target_info(who) else {
            return;
        };
        m.look_at_speed([t.position.x, t.eye_y, t.position.z], 30.0, 30.0);
        let reach = f64::from(m.width() * 2.0);
        let melee = reach * reach;
        let d2 = m.distance_sqr(t.position);
        let speed = if d2 > melee && d2 < 16.0 {
            1.33
        } else if d2 < 225.0 {
            0.6
        } else {
            0.8
        };
        m.nav_move_to_entity(cx.w, [t.position.x, t.position.y, t.position.z], 0, speed);
        self.attack_time = (self.attack_time - 1).max(0);
        if d2 <= melee && self.attack_time <= 0 {
            self.attack_time = 20;
            super::ai::do_hurt_target(m, who, cx);
        }
    }
}
pub fn cat_ai_step(m: &mut Mob, events: &mut Vec<MobEvent>) {
    cat_amounts(m);
    if (m.extra.flag || m.extra.relax_one) && m.age.is_multiple_of(5) {
        let volume = 0.6 + 0.4 * (m.random.next_float() - m.random.next_float());
        let sound = super::finalize::variant_sound(m, "purr").unwrap_or("entity.cat.purr");
        events.push(MobEvent::Sound(sound, m.center(), volume, 1.0));
    }
}
pub fn cat_amounts(m: &mut Mob) {
    let lying = m.extra.flag;
    let relaxing = m.extra.relax_one;
    let v = &mut m.anim.v;
    if lying {
        v[LIE] = (v[LIE] + 0.15).min(1.0);
        v[LIE_TAIL] = (v[LIE_TAIL] + 0.08).min(1.0);
    } else {
        v[LIE] = (v[LIE] - 0.22).max(0.0);
        v[LIE_TAIL] = (v[LIE_TAIL] - 0.13).max(0.0);
    }
    v[RELAX] = if relaxing {
        (v[RELAX] + 0.1).min(1.0)
    } else {
        (v[RELAX] - 0.13).max(0.0)
    };
}
pub fn cat_variant(m: &mut Mob, w_moon_full: bool) -> u8 {
    let n = if w_moon_full { 11 } else { 10 };
    m.random.next_int_bound(n).unwrap_or(0) as u8
}
pub fn cat_texture(m: &Mob) -> String {
    let v = CAT_VARIANTS[(m.extra.variant as usize).min(10)];
    let baby = if m.is_baby() { "_baby" } else { "" };
    format!("entity/cat/cat_{v}{baby}")
}
pub fn cat_rotations(m: &Mob, partial: f32) -> glam::Mat4 {
    let lie = m.anim.lerp(LIE, partial);
    if lie <= 0.0 {
        return glam::Mat4::IDENTITY;
    }
    let on_top = if m.extra.on_sleeper { 0.15 * lie } else { 0.0 };
    glam::Mat4::from_translation(glam::Vec3::new(0.4 * lie, 0.15 * lie, 0.1 * lie))
        * glam::Mat4::from_rotation_z((90.0 * lie).to_radians())
        * glam::Mat4::from_translation(glam::Vec3::new(on_top, 0.0, 0.0))
}
pub fn cat_lying_on_sleeper(m: &mut Mob, sleepers: &[vise_core::geometry::Aabb]) {
    m.extra.on_sleeper = false;
    if !m.extra.flag {
        return;
    }
    let (x, y, z) = (
        m.position.x.floor(),
        m.position.y.floor(),
        m.position.z.floor(),
    );
    let area = vise_core::geometry::Aabb::new(
        Vec3::new(x - 2.0, y - 2.0, z - 2.0),
        Vec3::new(x + 3.0, y + 3.0, z + 3.0),
    );
    m.extra.on_sleeper = sleepers.iter().any(|b| b.intersects(area));
}
fn rot_lerp(t: f32, from: f32, to: f32) -> f32 {
    from + t * super::living::wrap(to - from)
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Feline {
    pub crouching: bool,
    pub sprinting: bool,
    pub sitting: bool,
    pub lie_down: f32,
    pub lie_down_tail: f32,
    pub relax_one: f32,
}
pub fn feline_server_ai_step(m: &mut Mob) {
    let speed = m.ai.mov.has_wanted().then_some(m.ai.mov.speed_modifier);
    m.extra.pose = if speed == Some(0.6) {
        super::sync::CROUCHING
    } else {
        super::sync::STANDING
    };
    m.extra.sprinting = speed == Some(1.33);
}
pub fn cat_anim(m: &Mob, s: &State, p: &mut Poses) {
    let st = Feline {
        crouching: m.extra.pose == super::sync::CROUCHING,
        sprinting: m.extra.sprinting,
        sitting: cat_sitting(m),
        lie_down: m.anim.lerp(LIE, s.partial),
        lie_down_tail: m.anim.lerp(LIE_TAIL, s.partial),
        relax_one: m.anim.lerp(RELAX, s.partial),
    };
    feline(p, s, &st);
}
pub fn feline(p: &mut Poses, s: &State, st: &Feline) {
    let baby = s.baby;
    let age = if baby { 0.5 } else { 1.0 };
    let (crouching, sprinting, sitting) = (st.crouching, st.sprinting, st.sitting);
    let add = |p: &mut Poses, part: &str, x: f32, y: f32, z: f32| {
        let v = p.get(part);
        v[0] += x;
        v[1] += y;
        v[2] += z;
    };
    if crouching {
        add(p, "body", 0.0, age, 0.0);
        add(p, "head", 0.0, 2.0 * age, 0.0);
        add(p, "tail1", 0.0, age, 0.0);
        add(p, "tail2", 0.0, -4.0 * age, 2.0 * age);
        p.xrot("tail1", PI / 2.0);
        p.xrot("tail2", PI / 2.0);
    } else if sprinting {
        let y1 = p.get("tail1")[1];
        p.get("tail2")[1] = y1;
        add(p, "tail2", 0.0, 0.0, 2.0 * age);
        p.xrot("tail1", PI / 2.0);
        p.xrot("tail2", PI / 2.0);
    }
    p.xrot("head", s.x_rot.to_radians());
    p.yrot("head", s.y_rot.to_radians());
    let (pos, sp) = (s.pos, s.speed);
    if !sitting {
        if !baby {
            p.xrot("body", PI / 2.0);
        }
        if sprinting {
            p.xrot("left_hind_leg", (pos * 0.6662).mcos() * sp);
            p.xrot("right_hind_leg", (pos * 0.6662 + 0.3).mcos() * sp);
            p.xrot("left_front_leg", (pos * 0.6662 + PI + 0.3).mcos() * sp);
            p.xrot("right_front_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("tail2", 1.727_876_1 + PI / 10.0 * pos.mcos() * sp);
        } else {
            p.xrot("left_hind_leg", (pos * 0.6662).mcos() * sp);
            p.xrot("right_hind_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("left_front_leg", (pos * 0.6662 + PI).mcos() * sp);
            p.xrot("right_front_leg", (pos * 0.6662).mcos() * sp);
            let swing = if crouching { 0.471_238_94 } else { PI / 4.0 };
            p.xrot("tail2", 1.727_876_1 + swing * pos.mcos() * sp);
        }
    } else if baby {
        let x = p.get("body")[3] - 0.436_332_32;
        p.xrot("body", x);
        add(p, "body", 0.0, 1.25, 0.0);
        add(p, "head", 0.0, 0.0, 0.75);
        let t = p.get("tail1")[3] + 0.545_415_4;
        p.xrot("tail1", t);
        add(p, "tail1", 0.0, 4.0, -0.9);
        add(p, "left_hind_leg", 0.0, 0.0, -0.9);
        add(p, "right_hind_leg", 0.0, 0.0, -0.9);
    } else {
        p.xrot("body", PI / 4.0);
        add(p, "body", 0.0, -4.0 * age, 5.0 * age);
        add(p, "head", 0.0, -3.3 * age, age);
        add(p, "tail1", 0.0, 8.0 * age, -2.0 * age);
        add(p, "tail2", 0.0, 2.0 * age, -0.8 * age);
        p.xrot("tail1", 1.727_876_1);
        p.xrot("tail2", 2.670_354);
        p.xrot("left_front_leg", -PI / 20.0);
        add(p, "left_front_leg", 0.0, 2.0 * age, -2.0 * age);
        p.xrot("right_front_leg", -PI / 20.0);
        add(p, "right_front_leg", 0.0, 2.0 * age, -2.0 * age);
        p.xrot("left_hind_leg", -PI / 2.0);
        add(p, "left_hind_leg", 0.0, 3.0 * age, -4.0 * age);
        p.xrot("right_hind_leg", -PI / 2.0);
        add(p, "right_hind_leg", 0.0, 3.0 * age, -4.0 * age);
    }
    let lie = st.lie_down;
    let lie_tail = st.lie_down_tail;
    if lie > 0.0 && baby {
        add(p, "body", 1.0, 0.0, 0.0);
        let hx = p.get("head")[3];
        p.xrot("head", rot_lerp(lie, hx, PI / 18.0));
        let hz = p.get("head")[5];
        p.zrot("head", rot_lerp(lie, hz, -PI * 5.0 / 12.0));
        add(p, "head", 1.5, 0.75, -0.5);
        p.xrot("right_front_leg", -PI / 4.0);
        add(p, "right_front_leg", 3.5, -0.5, 0.0);
        p.xrot("left_front_leg", -PI / 2.0);
        add(p, "left_front_leg", 1.5, -1.0, -2.0);
        p.xrot("right_hind_leg", PI * 2.0 / 9.0);
        p.yrot("right_hind_leg", PI / 9.0);
        p.zrot("right_hind_leg", -PI / 9.0);
        add(p, "right_hind_leg", 2.5, -0.25, 0.5);
        add(p, "left_hind_leg", 1.5, 0.0, -1.0);
        let t = p.get("tail1");
        let (tx, ty, tz) = (t[3], t[4], t[5]);
        p.xrot("tail1", tx + rot_lerp(lie_tail, tx, -PI / 6.0));
        p.yrot("tail1", ty + rot_lerp(lie_tail, ty, 0.0));
        p.zrot("tail1", tz + rot_lerp(lie_tail, tz, -PI / 18.0));
        add(p, "tail1", 1.0, 0.5, -0.25);
    } else if lie > 0.0 {
        let (hz, hy) = (p.get("head")[5], p.get("head")[4]);
        p.zrot("head", rot_lerp(lie, hz, -1.270_796_3));
        p.yrot("head", rot_lerp(lie, hy, 1.270_796_3));
        p.xrot("left_front_leg", -1.270_796_3);
        p.xrot("right_front_leg", -0.470_796_35);
        p.zrot("right_front_leg", -0.2);
        add(p, "right_front_leg", age, 0.0, 0.0);
        p.xrot("left_hind_leg", -0.4);
        p.xrot("right_hind_leg", 0.5);
        p.zrot("right_hind_leg", -0.5);
        add(p, "right_hind_leg", 0.8 * age, 2.0 * age, 0.0);
        let (t1, t2) = (p.get("tail1")[3], p.get("tail2")[3]);
        p.xrot("tail1", rot_lerp(lie_tail, t1, 0.8));
        p.xrot("tail2", rot_lerp(lie_tail, t2, -0.4));
    }
    let relax = st.relax_one;
    if relax > 0.0 {
        let hx = p.get("head")[3];
        p.xrot("head", rot_lerp(relax, hx, -0.581_776_44));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    const WHITE: u32 = 0xFFFF_FFFF;

    fn cat() -> Mob {
        let mut kind = kind();
        let k = Arc::make_mut(&mut kind);
        k.id = "minecraft:cat".into();
        k.species = Species::Cat;
        Mob::new(1, kind, Vec3::new(0.5, 64.0, 0.5), 0.0)
    }
    #[test]
    fn a_cat_lying_near_a_sleeper_lies_on_top_of_them() {
        use vise_core::geometry::Aabb;
        let sleeper = Aabb::new(Vec3::new(2.4, 64.0, 0.4), Vec3::new(2.6, 64.2, 0.6));
        let far = Aabb::new(Vec3::new(5.4, 64.0, 0.4), Vec3::new(5.6, 64.2, 0.6));
        let mut m = cat();
        m.anim.v[LIE] = 1.0;
        m.extra.flag = true;
        cat_lying_on_sleeper(&mut m, &[far]);
        assert!(!m.extra.on_sleeper);
        cat_lying_on_sleeper(&mut m, &[far, sleeper]);
        assert!(m.extra.on_sleeper);
        let on_top = cat_rotations(&m, 1.0);
        m.extra.on_sleeper = false;
        let plain = cat_rotations(&m, 1.0);
        assert_ne!(on_top, plain);
        assert!((on_top.w_axis.y - plain.w_axis.y - 0.15).abs() < 1.0e-4);
        m.extra.flag = false;
        cat_lying_on_sleeper(&mut m, &[sleeper]);
        assert!(!m.extra.on_sleeper);
    }
    fn wolf() -> Mob {
        let mut kind = kind();
        let k = Arc::make_mut(&mut kind);
        k.id = "minecraft:wolf".into();
        k.species = Species::Wolf;
        k.width = 0.6;
        k.height = 0.85;
        let mut m = Mob::new(1, kind, Vec3::ZERO, 0.0);
        m.on_ground = true;
        m
    }
    fn entity_events(events: &[MobEvent]) -> Vec<i8> {
        events
            .iter()
            .filter_map(|e| match e {
                MobEvent::EntityEvent { event, .. } => Some(*event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_wolf_of_a_server_darkens_in_water_and_shakes_dry_on_its_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "wet in water");
        m.fluids.in_water = false;
        client_tick(&mut m, false, &mut events);
        assert_eq!(tint(&m, 1.0), 0xFFBF_BFBF, "still wet before it shakes");
        assert_eq!(m.anim.v[SHAKE], 0.0);
        let mut dry = wolf();
        client_tick(&mut dry, true, &mut events);
        assert_ne!(tint(&dry, 1.0), WHITE, "wet in the rain");

        crate::mobs::sync::entity_event(&mut m, 8);
        let mut ticks = 0;
        while m.extra.flag2 && ticks < 100 {
            client_tick(&mut m, false, &mut events);
            ticks += 1;
            if ticks == 1 {
                assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
                assert_eq!(m.anim.vo[SHAKE], 0.0);
            }
        }
        assert!((41..=42).contains(&ticks), "shook for {ticks} ticks");
        assert_eq!(tint(&m, 1.0), WHITE, "dry after the shake");
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::ParticleAt { kind: "splash", .. })),
            "the client draws the splashes"
        );
        assert!(
            !events.iter().any(|e| matches!(e, MobEvent::Sound(..))),
            "the shake sound is the server's"
        );
    }

    #[test]
    fn a_wolf_of_a_server_stops_shaking_on_its_cancel_event() {
        let mut m = wolf();
        let mut events = Vec::new();
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        m.fluids.in_water = false;
        crate::mobs::sync::entity_event(&mut m, 8);
        for _ in 0..10 {
            client_tick(&mut m, false, &mut events);
        }
        assert!(m.anim.v[SHAKE] > 0.4);
        m.fluids.in_water = true;
        client_tick(&mut m, false, &mut events);
        assert!(m.extra.counter2 > 0, "only the server cancels");
        crate::mobs::sync::entity_event(&mut m, 56);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert_ne!(tint(&m, 1.0), WHITE, "still wet");
    }

    #[test]
    fn a_server_tells_the_clients_when_a_wolf_shakes() {
        let mut m = wolf();
        let mut events = Vec::new();
        server_shake(&mut m, true, &mut events);
        assert!(m.extra.flag2);
        server_shake(&mut m, false, &mut events);
        assert_eq!(entity_events(&events), [8]);
        assert!(m.extra.counter2 > 0);
        assert!((m.anim.v[SHAKE] - 0.05).abs() < 1e-6);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, MobEvent::Sound("entity.wolf.shake", ..)))
        );
        server_shake(&mut m, true, &mut events);
        assert_eq!(entity_events(&events), [8, 56]);
        assert_eq!((m.extra.counter2, m.anim.v[SHAKE]), (0, 0.0));
        assert!(m.extra.flag2, "still wet");
    }
}
