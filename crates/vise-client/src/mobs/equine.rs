use super::finalize::{Attr, set_base};
use super::{
    Mob, MobEvent, MobWorld, Species,
    anim::{Poses, State},
    goals::{
        Breed, Cx, Float, FollowParent, Goal, LookAtPlayer, MOVE, Panic, RandomLookAround, Stroll,
        Tempt, goal_clone,
    },
};
use vise_core::geometry::trig::Mth;
use vise_core::random::RandomSource;
use std::f32::consts::PI;

pub const COLORS: [&str; 7] = [
    "white",
    "creamy",
    "chestnut",
    "brown",
    "black",
    "gray",
    "darkbrown",
];
pub const MARKINGS: [&str; 5] = ["", "white", "whitefield", "whitedots", "blackdots"];

pub const EAT: usize = 0;
pub const STAND: usize = 1;
pub const MOUTH: usize = 2;
pub const TAIL: usize = 3;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rider {
    pub strafe: f32,
    pub forward: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub jump: f32,
    pub steer: bool,
    pub sprint: bool,
    pub jumping: bool,
    pub client: bool,
}

pub fn is_equine(s: Species) -> bool {
    matches!(
        s,
        Species::Horse
            | Species::Donkey
            | Species::Mule
            | Species::SkeletonHorse
            | Species::ZombieHorse
            | Species::Camel
            | Species::CamelHusk
            | Species::Llama
            | Species::TraderLlama
    )
}
pub fn max_temper(s: Species) -> i32 {
    if super::llama::is_llama(s) {
        super::llama::MAX_TEMPER
    } else {
        100
    }
}
fn can_rear(s: Species) -> bool {
    matches!(
        s,
        Species::Horse
            | Species::Donkey
            | Species::Mule
            | Species::SkeletonHorse
            | Species::ZombieHorse
    )
}
pub fn rear_offset(m: &Mob) -> [f64; 3] {
    if !can_rear(m.species()) {
        return [0.0; 3];
    }
    let stand = f64::from(m.anim.vo[STAND]);
    let scale = f64::from(if m.is_baby() { 0.5_f32 } else { 1.0 });
    [0.0, 0.15 * stand * scale, -0.7 * stand * scale]
}
pub fn immobile(m: &Mob) -> bool {
    can_rear(m.species()) && (m.extra.flag || m.extra.counter > 0) && !controlled(m)
}
pub fn on_elastic_leash_pull(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if grazes(m.species()) {
        m.extra.flag = false;
    }
    if super::camel::is_camel(m.species()) {
        super::camel::on_elastic_leash_pull(m, w, events);
    }
}
pub fn on_player_jump(m: &mut Mob) {
    m.extra.stand_sliding = true;
    stand_if_possible(m);
}
pub fn register(m: &mut Mob) {
    if super::camel::is_camel(m.species()) {
        return super::camel::register(m);
    }
    if super::llama::is_llama(m.species()) {
        return super::llama::register(m);
    }
    let tempt = m.kind.tempt.clone();
    let species = m.species();
    let rears = can_rear(species);
    let stand_interval = m.kind.ambient_interval;
    let g = &mut m.ai.goals;
    g.add(1, RunAroundLikeCrazy);
    g.add(2, Breed::new(1.0));
    g.add(4, FollowParent::new(1.0));
    g.add(6, Stroll::avoiding_water(0.7));
    g.add(7, LookAtPlayer::new(6.0));
    g.add(8, RandomLookAround::default());
    if rears {
        g.add(
            9,
            RandomStand {
                next: -stand_interval,
            },
        );
    }
    if species != Species::SkeletonHorse {
        g.add(0, Float);
        if species != Species::ZombieHorse {
            g.add(1, MountPanic(Panic::new(1.2)));
        }
        g.add(3, Tempt::new(1.25, tempt, false));
    }
}
pub fn finalize(m: &mut Mob) {
    let species = m.species();
    let r = &mut m.random;
    let health = matches!(species, Species::Horse | Species::Donkey | Species::Mule).then(|| {
        15.0 + r.next_int_bound(8).unwrap_or(0) as f32 + r.next_int_bound(9).unwrap_or(0) as f32
    });
    let mut speed = None;
    match species {
        Species::Horse => {
            speed = Some(
                (0.45 + r.next_double() * 0.3 + r.next_double() * 0.3 + r.next_double() * 0.3)
                    * 0.25,
            );
            m.extra.vector[0] =
                0.4 + r.next_double() * 0.2 + r.next_double() * 0.2 + r.next_double() * 0.2;
            m.extra.variant = r.next_int_bound(7).unwrap_or(0) as u8;
            m.extra.variant2 = r.next_int_bound(5).unwrap_or(0) as u8;
        }
        Species::Donkey | Species::Mule => m.extra.vector[0] = 0.5,
        Species::ZombieHorse => {
            let d = 0.066_666_666_666_666_67;
            m.extra.vector[0] =
                0.5 + r.next_double() * d + r.next_double() * d + r.next_double() * d;
            speed = Some(
                (9.0 + r.next_double() + r.next_double() + r.next_double()) / f64::from(42.16f32),
            );
        }
        _ => {
            m.extra.vector[0] =
                0.4 + r.next_double() * 0.2 + r.next_double() * 0.2 + r.next_double() * 0.2
        }
    }
    if let Some(health) = health {
        set_base(m, Attr::MaxHealth, f64::from(health));
    }
    if let Some(speed) = speed {
        set_base(m, Attr::MovementSpeed, speed);
    }
    m.health = m.attrs.max_health;
}
pub fn offspring(a: f64, b: f64, min: f64, max: f64, r: &mut impl RandomSource) -> f64 {
    let (a, b) = (a.clamp(min, max), b.clamp(min, max));
    let margin = 0.15 * (max - min);
    let range = (a - b).abs() + margin * 2.0;
    let quality = (r.next_double() + r.next_double() + r.next_double()) / 3.0 - 0.5;
    let v = (a + b) / 2.0 + range * quality;
    if v > max {
        max - (v - max)
    } else if v < min {
        min + (min - v)
    } else {
        v
    }
}
pub fn inherit(child: &mut Mob, a: &Mob, b: &Mob, r: &mut impl RandomSource) {
    let health = offspring(
        f64::from(a.attrs.max_health),
        f64::from(b.attrs.max_health),
        15.0,
        30.0,
        r,
    );
    set_base(child, Attr::MaxHealth, f64::from(health as f32));
    child.health = child.attrs.max_health;
    child.extra.vector[0] = offspring(a.extra.vector[0], b.extra.vector[0], 0.4, 1.0, r);
    let speed = offspring(a.attrs.speed, b.attrs.speed, 0.1125, 0.3375, r);
    set_base(child, Attr::MovementSpeed, speed);
}
pub fn controlled(m: &Mob) -> bool {
    let Some(r) = m.rider else {
        return false;
    };
    match m.species() {
        Species::Pig | Species::Strider => m.extra.saddled && r.steer,
        Species::HappyGhast => super::happy_ghast::controlled(m),
        Species::Nautilus | Species::ZombieNautilus => super::nautilus::controlled(m),
        _ => m.extra.saddled,
    }
}
pub fn moved_by_rider(s: Species) -> bool {
    is_equine(s) && !super::llama::is_llama(s)
}
pub fn moved_by_client(m: &Mob) -> bool {
    moved_by_rider(m.species()) && m.rider.is_some_and(|r| r.client) && controlled(m)
}
pub fn boost(m: &mut Mob) -> bool {
    if m.extra.counter2 > 0 {
        return false;
    }
    m.extra.counter2 = m.random.next_int_bound(841).unwrap_or(0) + 140;
    m.extra.counter = 0;
    true
}
fn boost_factor(m: &mut Mob) -> f32 {
    if m.extra.counter2 <= 0 {
        return 1.0;
    }
    m.extra.counter += 1;
    if m.extra.counter > m.extra.counter2 {
        m.extra.counter2 = 0;
        return 1.0;
    }
    1.0 + 1.15 * (m.extra.counter as f32 / m.extra.counter2 as f32 * PI).msin()
}
#[derive(Clone, Debug)]
struct MountPanic(Panic);
impl Goal for MountPanic {
    goal_clone!();
    fn flags(&self) -> u8 {
        self.0.flags()
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.rider.is_none() && self.0.can_use(m, cx)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.rider.is_none() && self.0.can_continue(m, cx)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx)
    }
    fn stop(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.stop(m, cx)
    }
}
#[derive(Clone, Debug)]
pub(super) struct RunAroundLikeCrazy;
impl Goal for RunAroundLikeCrazy {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if m.extra.tame || m.rider.is_none() {
            return false;
        }
        let r = &mut m.random;
        let (dx, dz) = (
            r.next_int_bound(11).unwrap_or(5) - 5,
            r.next_int_bound(11).unwrap_or(5) - 5,
        );
        let dy = r.next_int_bound(9).unwrap_or(4) - 4;
        let t = [
            m.position.x + f64::from(dx),
            m.position.y + f64::from(dy),
            m.position.z + f64::from(dz),
        ];
        m.nav_move_to(cx.w, t, 1.2)
    }
    fn can_continue(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        !m.extra.tame && !m.ai.nav.is_done() && m.rider.is_some()
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if m.extra.tame || m.random.next_int_bound(50).unwrap_or(1) != 0 {
            return;
        }
        let temper = i32::from(m.extra.variant3);
        let max = max_temper(m.species());
        if m.random.next_int_bound(max).unwrap_or(0) < temper {
            m.extra.tame = true;
            cx.events.push(MobEvent::Tamed { mob: m.id });
            return;
        }
        m.extra.variant3 = (temper + 5).min(max) as u8;
        cx.events.push(MobEvent::Buck { mob: m.id });
        make_mad(m, cx.events);
    }
}
pub fn stand_if_possible(m: &mut Mob) {
    if can_rear(m.species()) {
        m.extra.counter = 20;
        m.extra.flag = false;
    }
}
pub fn after_hurt_server(m: &mut Mob) {
    if m.random.next_int_bound(3).unwrap_or(1) == 0 {
        stand_if_possible(m);
    }
}
pub fn make_mad(m: &mut Mob, events: &mut Vec<MobEvent>) {
    if m.extra.counter > 0 {
        return;
    }
    stand_if_possible(m);
    let sound = match m.species() {
        Species::SkeletonHorse => return,
        Species::Donkey => "entity.donkey.angry",
        Species::Mule => "entity.mule.angry",
        Species::ZombieHorse => "entity.zombie_horse.angry",
        Species::Llama | Species::TraderLlama => "entity.llama.angry",
        _ if m.is_baby() => "entity.baby_horse.angry",
        _ => "entity.horse.angry",
    };
    m.voice(sound, events);
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeldItem {
    Feed,
    Refused,
}
pub fn held_item(
    m: &mut Mob,
    item: &str,
    sneaking: bool,
    dandelion: bool,
    events: &mut Vec<MobEvent>,
) -> Option<HeldItem> {
    let species = m.species();
    let checks_dandelion = match species {
        Species::Horse | Species::Donkey | Species::Mule => true,
        Species::Llama | Species::TraderLlama => true,
        Species::ZombieHorse => false,
        _ => return None,
    };
    let open_inventory = !m.is_baby() && m.extra.tame && sneaking;
    if m.rider.is_some()
        || open_inventory
        || (checks_dandelion && m.is_baby() && dandelion)
        || item.is_empty()
    {
        return None;
    }
    let food = if species == Species::ZombieHorse {
        item == "minecraft:red_mushroom"
    } else {
        m.kind.food.iter().any(|f| f == item)
    };
    if food {
        return Some(HeldItem::Feed);
    }
    if m.extra.tame {
        return None;
    }
    make_mad(m, events);
    Some(HeldItem::Refused)
}
#[derive(Clone, Debug)]
struct RandomStand {
    next: i32,
}
impl Goal for RandomStand {
    goal_clone!();
    fn flags(&self) -> u8 {
        0
    }
    fn can_use(&mut self, m: &mut Mob, _: &mut Cx) -> bool {
        self.next += 1;
        if self.next > 0 && m.random.next_int_bound(1000).unwrap_or(1000) < self.next {
            self.next = -m.kind.ambient_interval;
            return !immobile(m) && m.random.next_int_bound(10).unwrap_or(1) == 0;
        }
        false
    }
    fn can_continue(&mut self, _: &mut Mob, _: &mut Cx) -> bool {
        false
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        stand_if_possible(m);
        let sound = super::ai::voice_sounds(m)[0];
        let at = [m.position.x, m.position.y, m.position.z];
        cx.events.push(MobEvent::Sound(sound, at, 1.0, 1.0));
    }
}
pub fn hoof_steps(s: Species) -> bool {
    matches!(
        s,
        Species::Horse
            | Species::Donkey
            | Species::Mule
            | Species::SkeletonHorse
            | Species::ZombieHorse
    )
}
pub fn play_step(
    m: &mut Mob,
    w: &MobWorld,
    pos: [i32; 3],
    state: vise_world::registry::BlockStateId,
    events: &mut Vec<MobEvent>,
) {
    if matches!(
        &*w.facts.get(state).name,
        "minecraft:water" | "minecraft:lava" | "minecraft:bubble_column"
    ) {
        return;
    }
    let above = w.world.block(pos[0], pos[1] + 1, pos[2]);
    let sound_type = if &*w.facts.get(above).name == "minecraft:snow" {
        &w.facts.get(above).sounds
    } else {
        &w.facts.get(state).sounds
    };
    let (volume, pitch) = (sound_type.volume, sound_type.pitch);
    let at = [m.position.x, m.position.y, m.position.z];
    let baby = m.is_baby();
    let can_gallop = !matches!(m.species(), Species::Donkey | Species::Mule);
    if m.rider.is_some() && can_gallop {
        m.ai.steps.gallop += 1;
        let count = m.ai.steps.gallop;
        if count > 5 && count % 3 == 0 {
            events.push(MobEvent::Sound(
                "entity.horse.gallop",
                at,
                volume * 0.15,
                pitch,
            ));
            if m.species() == Species::Horse && m.random.next_int_bound(10).unwrap_or(1) == 0 {
                let breathe = if baby {
                    "entity.baby_horse.breathe"
                } else {
                    "entity.horse.breathe"
                };
                events.push(MobEvent::Sound(breathe, at, volume * 0.6, pitch));
            }
        } else if count <= 5 {
            events.push(MobEvent::Sound(
                "entity.horse.step_wood",
                at,
                volume * 0.15,
                pitch,
            ));
        }
    } else if is_wood_sound_type(sound_type) {
        events.push(MobEvent::Sound(
            "entity.horse.step_wood",
            at,
            volume * 0.15,
            pitch,
        ));
    } else {
        let step = if baby {
            "entity.baby_horse.step"
        } else {
            "entity.horse.step"
        };
        events.push(MobEvent::Sound(step, at, volume * 0.15, pitch));
    }
}
fn is_wood_sound_type(sounds: &vise_items::blocks::Sounds) -> bool {
    let step = sounds
        .step
        .strip_prefix("minecraft:")
        .unwrap_or(&sounds.step);
    let place = sounds
        .place
        .strip_prefix("minecraft:")
        .unwrap_or(&sounds.place);
    match step {
        "block.wood.step" => place == "block.wood.place",
        "block.nether_wood.step"
        | "block.stem.step"
        | "block.cherry_wood.step"
        | "block.bamboo_wood.step" => true,
        _ => false,
    }
}
pub fn ridden_step(m: &mut Mob) {
    let Some(r) = m.rider else {
        return;
    };
    if hoof_steps(m.species()) && r.forward <= 0.0 {
        m.ai.steps.gallop = 0;
    }
    if super::nautilus::is_nautilus(m.species()) {
        return super::nautilus::ridden(m);
    }
    if m.species() == Species::HappyGhast {
        return super::happy_ghast::ridden(m);
    }
    if !(super::camel::is_camel(m.species()) && super::camel::refuse_to_move(m)) {
        m.yaw = r.yaw;
        m.head_pitch = r.pitch * 0.5;
    }
    m.body_yaw = m.yaw;
    m.head_yaw = m.yaw;
    m.ai.nav.stop();

    if matches!(m.species(), Species::Pig | Species::Strider) {
        m.head_pitch = r.pitch * 0.5;
        let factor = if m.species() == Species::Pig {
            0.225
        } else if super::strider::suffocating(m) {
            0.35
        } else {
            0.55
        };
        let boost = boost_factor(m);
        m.set_speed((m.movement_speed() * factor) as f32 * boost);
        m.ai.xxa = 0.0;
        m.ai.zza = 1.0;
        return;
    }
    if super::camel::is_camel(m.species()) {
        if r.forward > 0.0 && super::camel::sitting(m) {
            let mut ev = Vec::new();
            super::camel::stand_up(m, &mut ev);
        }
        if super::camel::refuse_to_move(m) {
            m.ai.xxa = 0.0;
            m.ai.zza = 0.0;
            return;
        }
        let forward = if r.forward <= 0.0 {
            r.forward * 0.25
        } else {
            r.forward
        };
        let bonus = if r.sprint && m.extra.anim_ticks == 0 {
            0.1
        } else {
            0.0
        };
        m.set_speed(m.movement_speed() as f32 + bonus);
        m.ai.xxa = r.strafe * 0.5;
        m.ai.zza = forward;
        if r.jump > 0.0 {
            let mut ev = Vec::new();
            super::camel::dash(m, r.jump, &mut ev);
            if let Some(rider) = m.rider.as_mut() {
                rider.jump = 0.0;
            }
        }
        return;
    }
    let standing = m.extra.counter > 0;
    if m.on_ground && r.jump == 0.0 && standing && !m.extra.stand_sliding {
        m.ai.xxa = 0.0;
        m.ai.zza = 0.0;
        return;
    }
    let forward = if r.forward <= 0.0 {
        r.forward * 0.25
    } else {
        r.forward
    };
    let speed = m.movement_speed() as f32;
    m.set_speed(speed);
    m.ai.xxa = r.strafe * 0.5;
    m.ai.zza = forward;
    if m.on_ground && r.jump > 0.0 {
        let impulse = m.extra.vector[0] as f32 * r.jump;
        m.velocity.y = f64::from(impulse);
        if forward > 0.0 {
            let (s, c) = (m.yaw.to_radians().msin(), m.yaw.to_radians().mcos());
            m.velocity.x += f64::from(-0.4 * s * r.jump);
            m.velocity.z += f64::from(0.4 * c * r.jump);
        }
        if let Some(rider) = m.rider.as_mut() {
            rider.jump = 0.0;
        }
    }
}
pub fn tail_tick(m: &mut Mob) {
    if m.random.next_int_bound(200).unwrap_or(1) == 0 {
        m.anim.v[TAIL] = 1.0;
    }
    if m.anim.v[TAIL] > 0.0 {
        m.anim.v[TAIL] += 1.0;
        if m.anim.v[TAIL] > 8.0 {
            m.anim.v[TAIL] = 0.0;
        }
    }
}
pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if super::camel::is_camel(m.species()) {
        return super::camel::ai_step(m, events);
    }
    if super::llama::is_llama(m.species()) {
        return super::llama::ai_step(m, w);
    }
    if m.alive() && m.random.next_int_bound(900).unwrap_or(1) == 0 && m.death_time == 0 {
        m.health = (m.health + 1.0).min(m.attrs.max_health);
    }
    if m.alive() && m.rider.is_none() {
        let below = w.facts.get(w.world.block(
            m.position.x.floor() as i32,
            (m.position.y - 0.5).floor() as i32,
            m.position.z.floor() as i32,
        ));
        if !m.extra.flag
            && m.random.next_int_bound(300).unwrap_or(1) == 0
            && &*below.name == "minecraft:grass_block"
        {
            m.extra.flag = true;
            m.extra.counter2 = 0;
        }
    }
    if m.extra.flag {
        m.extra.counter2 += 1;
        if m.extra.counter2 > 50 {
            m.extra.counter2 = 0;
            m.extra.flag = false;
        }
    }
    stand_tick(m);
    animate(m);
}
pub fn stand_tick(m: &mut Mob) {
    if m.extra.counter > 0 {
        m.extra.counter -= 1;
    }
}
pub fn animate(m: &mut Mob) {
    let (eating, standing) = (m.extra.flag, m.extra.counter > 0);
    let v = &mut m.anim.v;
    if standing {
        v[EAT] = 0.0;
        m.anim.vo[EAT] = 0.0;
        v[STAND] = (v[STAND] + (1.0 - v[STAND]) * 0.4 + 0.05).min(1.0);
    } else {
        m.extra.stand_sliding = false;
        if eating {
            v[EAT] = (v[EAT] + (1.0 - v[EAT]) * 0.4 + 0.05).min(1.0);
        } else {
            v[EAT] = (v[EAT] - v[EAT] * 0.4 - 0.05).max(0.0);
        }
        let s = v[STAND];
        v[STAND] = (s + (0.8 * s * s * s - s) * 0.6 - 0.05).max(0.0);
    }
    v[MOUTH] = (v[MOUTH] - v[MOUTH] * 0.7 - 0.05).max(0.0);
}
pub fn client_tick(m: &mut Mob) {
    m.anim.vo = m.anim.v;
    animate(m);
}
pub fn set_flags(m: &mut Mob, flags: u8) {
    m.extra.flag = flags & 16 != 0;
    m.extra.counter = i32::from(flags & 32 != 0);
}
pub fn grazes(s: Species) -> bool {
    is_equine(s) && !super::camel::is_camel(s) && !super::llama::is_llama(s)
}
pub fn texture(m: &Mob) -> String {
    let baby = if m.is_baby() { "_baby" } else { "" };
    match m.species() {
        Species::Horse => {
            let c = COLORS[(m.extra.variant as usize).min(6)];
            format!("entity/horse/horse_{c}{baby}")
        }
        Species::Donkey => format!("entity/horse/donkey{baby}"),
        Species::Mule => format!("entity/horse/mule{baby}"),
        Species::SkeletonHorse => format!("entity/horse/horse_skeleton{baby}"),
        Species::Camel => format!("entity/camel/camel{baby}"),
        Species::CamelHusk => "entity/camel/camel_husk".into(),
        _ => format!("entity/horse/horse_zombie{baby}"),
    }
}
pub fn markings(m: &Mob) -> Option<String> {
    if m.species() != Species::Horse {
        return None;
    }
    let mk = MARKINGS[(m.extra.variant2 as usize).min(4)];
    if mk.is_empty() {
        return None;
    }
    let baby = if m.is_baby() { "_baby" } else { "" };
    Some(format!("entity/horse/horse_markings_{mk}{baby}"))
}
pub fn model_name(s: Species) -> &'static str {
    match s {
        Species::Horse => "horse",
        Species::Donkey => "donkey",
        Species::Mule => "mule",
        Species::SkeletonHorse => "skeleton_horse",
        Species::Camel | Species::CamelHusk => "camel",
        _ => "zombie_horse",
    }
}
pub fn saddle_texture(s: Species) -> &'static str {
    match s {
        Species::Donkey => "entity/equipment/donkey_saddle/saddle",
        Species::Mule => "entity/equipment/mule_saddle/saddle",
        Species::SkeletonHorse => "entity/equipment/skeleton_horse_saddle/saddle",
        Species::ZombieHorse => "entity/equipment/zombie_horse_saddle/saddle",
        Species::Camel => "entity/equipment/camel_saddle/saddle",
        Species::CamelHusk => "entity/equipment/camel_husk_saddle/saddle",
        _ => "entity/equipment/horse_saddle/saddle",
    }
}
pub fn anim(m: &Mob, s: &State, p: &mut Poses) {
    let baby = m.is_baby();
    let donkey = matches!(m.species(), Species::Donkey | Species::Mule);
    let (stand_angle, stand_y, stand_z, stand_x_off, tail_off) = match (baby, donkey) {
        (false, _) => (PI / 12.0, 12.0, 4.0, -PI / 3.0, 0.0),
        (true, false) => (PI / 12.0, 4.0, 0.0, -PI / 3.0, -PI / 2.0),
        (true, true) => (PI / 3.0, 1.0, 0.5, 0.0, -PI / 4.0),
    };
    let clamped = s.y_rot.clamp(-20.0, 20.0);
    let mut head_x = s.x_rot.to_radians();
    let (speed, pos) = (s.speed, s.pos);
    if speed > 0.2 {
        head_x += (pos * 0.8).mcos() * 0.15 * speed;
    }
    let eating = m.anim.lerp(EAT, s.partial);
    let standing = m.anim.lerp(STAND, s.partial);
    let istand = 1.0 - standing;
    let feeding = m.anim.lerp(MOUTH, s.partial);
    let in_water = if m.fluids.in_water { 0.2 } else { 1.0 };
    let leg1 = (in_water * pos * 0.6662 + PI).mcos();
    let leg_x = leg1 * 0.8 * speed;
    let base = (1.0 - standing.max(eating)) * (PI / 6.0 + head_x + feeding * s.age.msin() * 0.05);
    p.xrot(
        "head_parts",
        standing * (PI / 12.0 + head_x) + eating * (2.181_661_6 + s.age.msin() * 0.05) + base,
    );
    p.yrot(
        "head_parts",
        standing * clamped.to_radians() + (1.0 - standing.max(eating)) * clamped.to_radians(),
    );
    {
        let hp = p.get("head_parts");
        let lerp = |t: f32, a: f32, b: f32| a + (b - a) * t;
        if baby && donkey {
            hp[1] = lerp(eating, hp[1], -1.2);
            hp[2] = lerp(standing, hp[2], -3.6);
        } else if baby {
            hp[1] += lerp(eating, lerp(standing, 0.0, -2.0), 2.0);
            hp[2] = lerp(standing, hp[2], -4.0);
        } else {
            hp[1] += lerp(eating, lerp(standing, 0.0, -8.0), 7.0);
            hp[2] = lerp(standing, hp[2], -4.0);
        }
    }
    let body_x = p.get("body")[3];
    p.xrot("body", standing * (-PI / 4.0) + istand * body_x);
    {
        let lf = p.get("left_front_leg");
        lf[1] -= stand_y * standing;
        lf[2] += stand_z * standing;
        let (y, z) = (lf[1], lf[2]);
        let rf = p.get("right_front_leg");
        rf[1] = y;
        rf[2] = z;
    }
    let stand = stand_angle * standing;
    let bob = (s.age * 0.6 + PI).mcos();
    let rleg = (stand_x_off + bob) * standing + leg_x * istand;
    let lleg = (stand_x_off - bob) * standing - leg_x * istand;
    p.xrot("left_hind_leg", stand - leg1 * 0.5 * speed * istand);
    p.xrot("right_hind_leg", stand + leg1 * 0.5 * speed * istand);
    p.xrot("left_front_leg", rleg);
    p.xrot("right_front_leg", lleg);
    if baby && donkey {
        let lerp = |t: f32, a: f32, b: f32| a + (b - a) * t;
        let y = lerp(standing, p.get("left_hind_leg")[1], -0.3);
        p.get("left_hind_leg")[1] = y;
        p.get("right_hind_leg")[1] = lerp(standing, y, -0.3);
    }
    let age = if baby { 0.5 } else { 1.0 };
    p.xrot("tail", tail_off + PI / 6.0 + speed * 0.75);
    {
        let t = p.get("tail");
        t[1] += speed * age;
        t[2] += speed * 2.0 * age;
    }
    p.yrot(
        "tail",
        if m.anim.v[TAIL] > 0.0 {
            (s.age * 0.7).mcos()
        } else {
            0.0
        },
    );
    if baby && donkey {
        let head_x = -30.0f32 * vise_core::geometry::trig::DEG_TO_RAD;
        let base =
            (1.0 - standing.max(eating)) * (PI / 6.0 + head_x + feeding * s.age.msin() * 0.05);
        p.xrot(
            "head_parts",
            standing * (PI / 12.0 + head_x) + eating * (PI / 2.0 + s.age.msin() * 0.05) + base,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mobs::Damage;
    use crate::mobs::finalize::base_value;
    use crate::mobs::sensor_scan_tests::kind;
    use vise_core::geometry::Vec3;
    use std::sync::Arc;

    fn horse(species: Species) -> Mob {
        let mut kind = crate::mobs::sensor_scan_tests::kind();
        let k = Arc::make_mut(&mut kind);
        k.species = species;
        k.max_health = 53.0;
        Mob::new(1, kind, Vec3::ZERO, 0.0)
    }

    #[test]
    fn rolled_health_and_speed_are_attribute_bases() {
        for species in [Species::Horse, Species::Donkey, Species::Mule] {
            let mut m = horse(species);
            finalize(&mut m);
            assert!((15.0..=30.0).contains(&m.attrs.max_health));
            assert_eq!(
                base_value(&m, Attr::MaxHealth),
                f64::from(m.attrs.max_health)
            );
            assert_eq!(m.health, m.attrs.max_health);
        }
        let mut m = horse(Species::Horse);
        finalize(&mut m);
        assert_eq!(base_value(&m, Attr::MovementSpeed), m.attrs.speed);
    }

    fn equine(id: u32, species: Species, name: &str) -> Mob {
        let mut kind = kind();
        let k = Arc::make_mut(&mut kind);
        k.id = name.into();
        k.species = species;
        k.max_health = 30.0;
        Mob::new(id, kind, Vec3::new(f64::from(id), 64.0, 0.0), 0.0)
    }
    fn rears(species: Species, name: &str, n: u32) -> u32 {
        (1..=n)
            .filter(|&id| {
                let mut m = equine(id, species, name);
                assert!(m.damage(1.0, Damage::player([0.0, 64.0, 0.0])));
                m.extra.counter == 20
            })
            .count() as u32
    }

    #[test]
    fn a_hurt_horse_rears_one_hit_in_three() {
        let n = rears(Species::Horse, "minecraft:horse", 300);
        assert!((60..=140).contains(&n), "{n} of 300 hits reared");
    }

    #[test]
    fn the_rear_ends_after_twenty_ticks() {
        let mut m = equine(1, Species::Horse, "minecraft:horse");
        stand_if_possible(&mut m);
        for _ in 0..19 {
            stand_tick(&mut m);
        }
        assert!(m.extra.counter > 0);
        stand_tick(&mut m);
        assert_eq!(m.extra.counter, 0);
    }

    #[test]
    fn llamas_and_camels_never_rear_when_hurt() {
        assert_eq!(rears(Species::Llama, "minecraft:llama", 60), 0);
        assert_eq!(rears(Species::Camel, "minecraft:camel", 60), 0);
    }
}
