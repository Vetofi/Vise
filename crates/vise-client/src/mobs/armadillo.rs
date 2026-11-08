use super::{
    Damage, DamageKind, Mob, MobEvent, MobWorld, Species, Who,
    goals::{
        Breed, Cx, Float, FollowParent, Goal, LOOK, LookAtPlayer, MOVE, Panic, RandomLookAround,
        Stroll, Tempt, goal_clone,
    },
};
use vise_core::{geometry::Vec3, random::RandomSource};

pub const IDLE: u8 = 0;
pub const ROLLING: u8 = 1;
pub const SCARED: u8 = 2;
pub const UNROLLING: u8 = 3;

const ROLL_OUT: usize = 0;
const ROLL_UP: usize = 1;
const PEEK: usize = 2;

fn animation_duration(state: u8) -> i32 {
    match state {
        ROLLING => 10,
        SCARED => 50,
        UNROLLING => 30,
        _ => 0,
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub state: u8,
    pub in_state_ticks: i32,
    danger: i32,
    sensor: i32,
    ball_up: bool,
    next_peek: i32,
    danger_was_around: bool,
    panic_activity: bool,
    peek_received: bool,
}

pub fn register(m: &mut Mob) {
    let mut panic = Panic::new(2.0);
    panic.environmental = true;
    let g = &mut m.ai.goals;
    g.add(0, Float);
    g.add(1, ArmadilloPanic(panic));
    g.add(2, Rolled);
    g.add(3, Idle(Breed::new(1.0)));
    let tempt = Tempt::new(1.25, m.kind.food.clone(), false);
    g.add(4, Idle(ArmadilloTempt(tempt)));
    g.add(4, Idle(FollowParent::new(1.25)));
    g.add(5, Stroll::avoiding_water(1.0));
    g.add(6, LookAtPlayer::new(6.0));
    g.add(7, RandomLookAround::default());
}

pub fn scared(m: &Mob) -> bool {
    m.extra.armadillo.state != IDLE
}
pub fn hiding_in_shell(m: &Mob) -> bool {
    let a = &m.extra.armadillo;
    match a.state {
        ROLLING => a.in_state_ticks > 5,
        SCARED => true,
        UNROLLING => a.in_state_ticks < 26,
        _ => false,
    }
}
pub fn state(m: &Mob) -> u8 {
    m.extra.armadillo.state
}
pub fn set_state(m: &mut Mob, state: u8) {
    let a = &mut m.extra.armadillo;
    if a.state != state {
        a.state = state;
        a.in_state_ticks = 0;
    }
}
fn can_stay_rolled_up(m: &Mob) -> bool {
    !m.ai.panicking
        && !m.fluids.in_water
        && !m.fluids.in_lava
        && m.extra.leash.is_none()
        && m.extra.mount.is_none()
        && m.extra.rider.is_none()
}
fn play(m: &Mob, events: &mut Vec<MobEvent>, sound: &'static str) {
    if !m.silent {
        let at = [m.position.x, m.position.y, m.position.z];
        events.push(MobEvent::Sound(sound, at, 1.0, 1.0));
    }
}
fn roll_up(m: &mut Mob, events: &mut Vec<MobEvent>) {
    if !scared(m) {
        super::creaking::stop_in_place(m);
        m.extra.in_love = 0;
        if !m.silent {
            m.voice("entity.armadillo.roll", events);
        }
        set_state(m, ROLLING);
    }
}
fn roll_out(m: &mut Mob, events: &mut Vec<MobEvent>) {
    if scared(m) {
        if !m.silent {
            m.voice("entity.armadillo.unroll_finish", events);
        }
        set_state(m, IDLE);
    }
}

pub fn reduce_damage(m: &Mob, amount: f32) -> f32 {
    if scared(m) {
        ((amount - 1.0) / 2.0).max(0.0)
    } else {
        amount
    }
}
pub fn after_hurt(m: &mut Mob, source: &Damage) {
    if m.no_ai || m.health <= 0.0 {
        return;
    }
    let mut events = Vec::new();
    if source.attacker.is_some() {
        m.extra.armadillo.danger = 80;
        if can_stay_rolled_up(m) {
            roll_up(m, &mut events);
        }
    } else if environmental(source.kind) {
        roll_out(m, &mut events);
    }
    m.outbox.append(&mut events);
}
pub fn environmental(kind: DamageKind) -> bool {
    matches!(
        kind,
        DamageKind::Cactus
            | DamageKind::OnFire
            | DamageKind::InFire
            | DamageKind::Lava
            | DamageKind::LightningBolt
            | DamageKind::HotFloor
    )
}
pub fn sounds(m: &Mob) -> [&'static str; 3] {
    let [ambient, hurt, death] = m.kind.sounds;
    if scared(m) {
        ["", "entity.armadillo.hurt_reduced", death]
    } else {
        [ambient, hurt, death]
    }
}

fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::WitherSkeleton
            | Species::SkeletonHorse
            | Species::Bogged
            | Species::Parched
            | Species::ZombieHorse
            | Species::CamelHusk
            | Species::Zombie
            | Species::ZombieVillager
            | Species::ZombifiedPiglin
            | Species::Zoglin
            | Species::Drowned
            | Species::Husk
            | Species::ZombieNautilus
            | Species::Wither
            | Species::Phantom
    )
}
fn scared_by_someone(m: &Mob, w: &MobWorld) -> bool {
    let area = m.bounds().inflate(Vec3::new(7.0, 2.0, 7.0));
    let attacker =
        m.ai.last_hurt_by
            .filter(|_| m.age as i32 - m.ai.last_hurt_by_time <= 100);
    if let Some(p) = &w.player
        && p.alive
        && !p.spectator
        && p.bounds.intersects(area)
        && (attacker == Some(Who::Player) || p.sprinting || p.riding)
    {
        return true;
    }
    w.others_near(area).any(|s| {
        s.id != m.id
            && s.alive
            && s.bounds.intersects(area)
            && (undead(s.species) || attacker == Some(Who::Mob(s.id)))
    })
}

pub fn server_ai_step(m: &mut Mob, cx: &mut Cx) {
    let events = &mut *cx.events;
    let a = &mut m.extra.armadillo;
    if a.danger > 0 {
        a.danger -= 1;
    }
    a.sensor -= 1;
    if a.sensor <= 0 {
        a.sensor = 5;
        if !can_stay_rolled_up(m) {
            m.extra.armadillo.danger = 0;
        } else if scared_by_someone(m, cx.w) {
            m.extra.armadillo.danger = 80;
        }
    }
    if m.extra.armadillo.danger == 0 && scared(m) {
        roll_out(m, events);
    }
    if m.extra.armadillo.panic_activity && !m.extra.armadillo.ball_up && m.on_ground && !liquid(m) {
        m.extra.armadillo.ball_up = true;
        roll_up(m, events);
    }
    if m.extra.armadillo.ball_up {
        ball_up_tick(m, events);
    }
    let a = &mut m.extra.armadillo;
    a.panic_activity = a.danger > 0 && !m.ai.panicking;
}
fn liquid(m: &Mob) -> bool {
    m.fluids.in_water || m.fluids.in_lava
}
fn ball_up_tick(m: &mut Mob, events: &mut Vec<MobEvent>) {
    if !scared(m) {
        m.extra.armadillo.ball_up = false;
        if !can_stay_rolled_up(m) {
            roll_out(m, events);
        }
        return;
    }
    let a = &mut m.extra.armadillo;
    if a.next_peek > 0 {
        a.next_peek -= 1;
    }
    if a.state == ROLLING && a.in_state_ticks > animation_duration(ROLLING) {
        set_state(m, SCARED);
        if m.on_ground {
            play(m, events, "entity.armadillo.land");
        }
        return;
    }
    let danger = m.extra.armadillo.danger;
    let around = danger > 75;
    if around != m.extra.armadillo.danger_was_around {
        m.extra.armadillo.next_peek = next_peek_timer(m);
    }
    m.extra.armadillo.danger_was_around = around;
    let state = m.extra.armadillo.state;
    if state == SCARED {
        if m.extra.armadillo.next_peek == 0 && m.on_ground && around {
            peek(m, events);
            m.extra.armadillo.next_peek = next_peek_timer(m);
        }
        if danger < animation_duration(UNROLLING) {
            play(m, events, "entity.armadillo.unroll_start");
            set_state(m, UNROLLING);
        }
    } else if state == UNROLLING && danger > animation_duration(UNROLLING) {
        set_state(m, SCARED);
    }
}
fn next_peek_timer(m: &mut Mob) -> i32 {
    animation_duration(SCARED) + m.random.next_int_between_inclusive(100, 400).unwrap_or(100)
}
fn peek(m: &mut Mob, events: &mut Vec<MobEvent>) {
    entity_event(m, 64);
    let at = [m.position.x, m.position.y, m.position.z];
    events.push(MobEvent::EntityEvent {
        mob: m.id,
        event: 64,
        at,
    });
    events.push(MobEvent::Sound("entity.armadillo.peek", at, 1.0, 1.0));
}
pub fn entity_event(m: &mut Mob, event: u8) {
    if event == 64 {
        m.extra.armadillo.peek_received = true;
    }
}

pub fn tick(m: &mut Mob) {
    animate(m);
    if scared(m) {
        m.head_yaw += super::living::wrap(m.body_yaw - m.head_yaw);
    }
    m.extra.armadillo.in_state_ticks += 1;
}
fn animate(m: &mut Mob) {
    let age = m.age as f32;
    let a = &mut m.extra.armadillo;
    let s = &mut m.anim.states;
    match a.state {
        ROLLING => {
            s[ROLL_OUT].stop();
            s[ROLL_UP].start_if_stopped(age);
            s[PEEK].stop();
        }
        SCARED => {
            s[ROLL_OUT].stop();
            s[ROLL_UP].stop();
            if a.peek_received {
                s[PEEK].stop();
                a.peek_received = false;
            }
            if a.in_state_ticks == 0 {
                s[PEEK].start(age);
                s[PEEK].fast_forward(animation_duration(SCARED));
            } else {
                s[PEEK].start_if_stopped(age);
            }
        }
        UNROLLING => {
            s[ROLL_OUT].start_if_stopped(age);
            s[ROLL_UP].stop();
            s[PEEK].stop();
        }
        _ => {
            s[ROLL_OUT].stop();
            s[ROLL_UP].stop();
            s[PEEK].stop();
        }
    }
}

#[derive(Clone, Debug)]
struct ArmadilloPanic(Panic);
impl Goal for ArmadilloPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        "Panic"
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        roll_out(m, cx.events);
        self.0.start(m, cx);
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx);
    }
}
#[derive(Clone, Debug)]
struct Idle<G>(G);
fn idle(m: &Mob) -> bool {
    !scared(m) && !m.extra.armadillo.panic_activity
}
impl<G: Goal + Clone + 'static> Goal for Idle<G> {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        idle(m) && self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        idle(m) && self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx);
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx);
    }
    fn every_tick(&self) -> bool {
        self.0.every_tick()
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.tick(m, cx);
    }
}
#[derive(Clone, Debug)]
struct ArmadilloTempt(Tempt);
impl Goal for ArmadilloTempt {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx);
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx);
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop_distance = if m.is_baby() { 1.0 } else { 2.0 };
        self.0.tick(m, cx);
    }
}
#[derive(Clone, Debug)]
struct Rolled;
impl Goal for Rolled {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, _cx: &mut Cx) -> bool {
        scared(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::sensor_scan_tests::kind;
    use std::sync::Arc;

    fn armadillo() -> Mob {
        let mut kind = kind();
        let k = Arc::make_mut(&mut kind);
        k.id = "minecraft:armadillo".into();
        k.species = Species::Armadillo;
        let mut m = Mob::new(1, kind, Vec3::ZERO, 0.0);
        m.on_ground = true;
        m
    }

    #[test]
    fn a_hit_rolls_it_up_into_a_ball() {
        let mut m = armadillo();
        after_hurt(&mut m, &Damage::player([0.0, 0.0, 0.0]));
        assert_eq!(state(&m), ROLLING);
        assert!(
            m.outbox
                .iter()
                .any(|e| matches!(e, MobEvent::Sound(s, ..) if *s == "entity.armadillo.roll")),
            "the roll plays its sound"
        );
        for _ in 0..6 {
            tick(&mut m);
        }
        assert!(hiding_in_shell(&m), "the model shows the shell");
    }

    fn run_goals(m: &mut Mob, w: &MobWorld, ticks: u32) -> Vec<MobEvent> {
        let mut events = Vec::new();
        let mut goals = std::mem::take(&mut m.ai.goals);
        let mut lr = vise_core::random::LegacyRandom::new(0);
        let mut cx = Cx {
            w,
            lr: &mut lr,
            events: &mut events,
        };
        for _ in 0..ticks {
            goals.tick(m, &mut cx);
        }
        m.ai.goals = goals;
        events
    }
    fn real<const N: usize>(at: [[f64; 3]; N]) -> Option<[Mob; N]> {
        let mut out = None;
        crate::mobs::world::with_test_world(&[], |_, types| {
            let kind = Arc::new(types.types["minecraft:armadillo"].clone());
            let mut id = 0;
            out = Some(at.map(|[x, y, z]| {
                id += 1;
                let mut m = Mob::new(id, kind.clone(), Vec3::new(x, y, z), 0.0);
                m.on_ground = true;
                m
            }));
        });
        out
    }

    #[test]
    fn two_armadillos_in_love_make_a_baby() {
        let Some([mut a, mut b, mut c]) =
            real([[0.5, 64.0, 0.5], [2.5, 64.0, 0.5], [0.5, 64.0, 1.5]])
        else {
            return;
        };
        for m in [&mut a, &mut b, &mut c] {
            m.extra.in_love = 600;
        }
        let partner = [b.seen(None)];
        crate::mobs::world::with_test_world(&partner, |w, _| {
            let events = run_goals(&mut a, w, 80);
            assert!(
                events.iter().any(|e| matches!(
                    e,
                    MobEvent::Breed {
                        parent: 1,
                        partner: 2,
                        ..
                    }
                )),
                "the pair breeds"
            );
            assert_eq!(a.extra.in_love, 0);
            assert_eq!(a.extra.age, 6000, "the parent rests");

            c.extra.armadillo.panic_activity = true;
            let events = run_goals(&mut c, w, 80);
            assert!(!events.iter().any(|e| matches!(e, MobEvent::Breed { .. })));
            assert!(!c.ai.goals.is_running("Breed"));
            c.extra.armadillo.panic_activity = false;
            set_state(&mut c, SCARED);
            run_goals(&mut c, w, 1);
            assert!(!c.ai.goals.is_running("Breed"), "a ball does not breed");
            set_state(&mut c, IDLE);
            run_goals(&mut c, w, 1);
            assert!(c.ai.goals.is_running("Breed"), "it looks for its mate");
        });
    }

    #[test]
    fn a_baby_armadillo_follows_an_adult() {
        let Some([mut baby, adult]) = real([[0.5, 64.0, 0.5], [6.5, 64.0, 0.5]]) else {
            return;
        };
        baby.extra.age = -24000;
        crate::mobs::world::with_test_world(&[adult.seen(None)], |w, _| {
            run_goals(&mut baby, w, 1);
            assert!(baby.ai.goals.is_running("FollowParent"));
            set_state(&mut baby, SCARED);
            run_goals(&mut baby, w, 1);
            assert!(!baby.ai.goals.is_running("FollowParent"), "not as a ball");
        });
    }

    #[test]
    fn a_tempted_baby_comes_closer_than_an_adult() {
        let Some([mut baby, mut adult]) = real([[0.5, 64.0, 0.5], [4.5, 64.0, 0.5]]) else {
            return;
        };
        baby.extra.age = -24000;
        crate::mobs::world::with_test_world(&[], |w, _| {
            let mut events = Vec::new();
            let mut lr = vise_core::random::LegacyRandom::new(0);
            let mut cx = Cx {
                w,
                lr: &mut lr,
                events: &mut events,
            };
            let mut tempt = ArmadilloTempt(Tempt::new(1.25, baby.kind.food.clone(), false));
            tempt.tick(&mut baby, &mut cx);
            assert_eq!(tempt.0.stop_distance, 1.0);
            tempt.tick(&mut adult, &mut cx);
            assert_eq!(tempt.0.stop_distance, 2.0);
        });
    }

    #[test]
    fn campfire_damage_is_fire_and_has_no_knockback_but_does_not_cause_panic() {
        let source = Damage::of(DamageKind::Campfire);
        assert!(source.is_fire());
        assert!(source.no_knockback());
        assert!(!source.causes_panic());
        assert!(!environmental(DamageKind::Campfire));
    }
}
