use super::{
    Mob, MobEvent, MobWorld,
    anim::{Poses, State as AnimState},
    control::MoveKind,
    goals::{Cx, Goal, LookAtPlayer, MOVE, Panic, RandomLookAround, Stroll, goal_clone},
    nav::NavKind,
};
use crate::sculk::{self, Context, Info};
use vise_core::geometry::Vec3;
use vise_core::geometry::trig::Mth;

pub const HOLD: usize = 0;
pub const SPIN: usize = 1;
const LISTENER_RADIUS: i64 = 16;

const TIME_TO_FORGET_NOTEBLOCK: i32 = 600;
const MAX_NOTEBLOCK_DISTANCE: i32 = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GlobalPos {
    pub dimension: &'static str,
    pub pos: [i32; 3],
}
impl GlobalPos {
    pub fn is_close_enough(&self, dimension: &str, pos: [i32; 3], max_distance: i32) -> bool {
        let chessboard = (0..3)
            .map(|i| self.pos[i].wrapping_sub(pos[i]).wrapping_abs())
            .max()
            .unwrap_or(0);
        self.dimension == dimension && chessboard <= max_distance
    }
}
pub fn dimension_id(id: &str) -> &'static str {
    match id {
        "minecraft:the_nether" => "minecraft:the_nether",
        "minecraft:the_end" => "minecraft:the_end",
        _ => "minecraft:overworld",
    }
}
pub fn dimension(w: &MobWorld) -> &'static str {
    if w.nether {
        "minecraft:the_nether"
    } else if w.end {
        "minecraft:the_end"
    } else {
        "minecraft:overworld"
    }
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub vibration: sculk::Data,
    pub noteblock: Option<GlobalPos>,
}

pub fn register(m: &mut Mob) {
    m.extra.allay = Some(Box::default());
    m.ai.mov.kind = MoveKind::Flying {
        max_turn: 20.0,
        hovers: true,
    };
    m.ai.nav.kind = NavKind::Flying;
    m.set_required_path_length(48.0);
    let g = &mut m.ai.goals;
    g.add(0, Panic::new(2.5));
    g.add(1, Fetch);
    g.add(2, Deliver);
    g.add(3, StayClose);
    g.add(4, Stroll::flying(1.0));
    g.add(5, LookAtPlayer::new(6.0));
    g.add(6, RandomLookAround::default());
}
fn block_pos(m: &Mob) -> [i32; 3] {
    [
        m.position.x.floor() as i32,
        m.position.y.floor() as i32,
        m.position.z.floor() as i32,
    ]
}
pub fn can_receive_vibration(m: &Mob, dimension: &str, pos: [i32; 3]) -> bool {
    if m.no_ai {
        return false;
    }
    let Some(liked_noteblock_position) = m.extra.allay.as_ref().and_then(|st| st.noteblock) else {
        return true;
    };
    liked_noteblock_position.is_close_enough(dimension, block_pos(m), MAX_NOTEBLOCK_DISTANCE)
        && liked_noteblock_position.pos == pos
}
pub fn hear_noteblock(m: &mut Mob, dimension: &'static str, pos: [i32; 3]) {
    let heard = GlobalPos { dimension, pos };
    let Some(st) = m.extra.allay.as_mut() else {
        return;
    };
    let liked_noteblock_cooldown_ticks = TIME_TO_FORGET_NOTEBLOCK;
    let liked_noteblock_position = st.noteblock;
    match liked_noteblock_position {
        None => {
            st.noteblock = Some(heard);
            m.extra.ints[2] = liked_noteblock_cooldown_ticks;
        }
        Some(liked) if liked == heard => m.extra.ints[2] = liked_noteblock_cooldown_ticks,
        Some(_) => {}
    }
}
fn receive_vibration(m: &mut Mob, dimension: &'static str, info: Info) {
    if info.event == "minecraft:note_block_play" {
        hear_noteblock(m, dimension, info.pos.map(|v| v.floor() as i32));
    }
}
pub fn noteblock_deposit(
    m: &mut Mob,
    dimension: &str,
    is_note_block: impl Fn([i32; 3]) -> bool,
) -> Option<[f64; 3]> {
    let liked = m.extra.allay.as_ref()?.noteblock?;
    if liked.is_close_enough(dimension, block_pos(m), MAX_NOTEBLOCK_DISTANCE)
        && is_note_block(liked.pos)
        && m.extra.ints[2] > 0
    {
        return Some([
            f64::from(liked.pos[0]) + 0.5,
            f64::from(liked.pos[1]) + 1.5,
            f64::from(liked.pos[2]) + 0.5,
        ]);
    }
    m.extra.allay.as_mut()?.noteblock = None;
    None
}
fn deposit(m: &mut Mob, cx: &Cx) -> Option<[f64; 3]> {
    let dimension = dimension(cx.w);
    if let Some(p) = noteblock_deposit(m, dimension, |p| {
        cx.w.block_name(p[0], p[1], p[2]) == "minecraft:note_block"
    }) {
        return Some(p);
    }
    if !m.extra.tame {
        return None;
    }
    let p = cx.w.player.as_ref().filter(|p| p.alive && !p.spectator)?;
    (p.position.distance_squared(m.position) <= 64.0 * 64.0).then_some([
        p.position.x,
        p.position.y,
        p.position.z,
    ])
}
#[derive(Clone, Debug)]
struct Fetch;
impl Goal for Fetch {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, _cx: &mut Cx) -> bool {
        m.extra.point.is_some() && m.extra.ints[3] <= 0
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(p) = m.extra.point
            && (m.ai.nav.is_done() || m.age.is_multiple_of(10))
        {
            m.nav_move_to(cx.w, p, 1.75);
        }
    }
}
#[derive(Clone, Debug)]
struct Deliver;
impl Goal for Deliver {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.extra.ints[1] > 0 && m.extra.point.is_none() && deposit(m, cx).is_some()
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(p) = deposit(m, cx)
            && (m.ai.nav.is_done() || m.age.is_multiple_of(10))
        {
            m.nav_move_to(cx.w, p, 2.25);
        }
    }
}
#[derive(Clone, Debug)]
struct StayClose;
impl Goal for StayClose {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.extra.point.is_none()
            && deposit(m, cx).is_some_and(|p| {
                let d = (p[0] - m.position.x).powi(2)
                    + (p[1] - m.position.y).powi(2)
                    + (p[2] - m.position.z).powi(2);
                d >= 256.0
            })
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        m.extra.point.is_none()
            && deposit(m, cx).is_some_and(|p| {
                let at = [m.position.x, m.position.y, m.position.z];
                (0..3)
                    .map(|i| (p[i].floor() as i32 - at[i].floor() as i32).abs())
                    .sum::<i32>()
                    > 4
            })
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        if let Some(p) = deposit(m, cx)
            && (m.ai.nav.is_done() || m.age.is_multiple_of(10))
        {
            m.nav_move_to(cx.w, p, 2.25);
        }
    }
}
pub fn listen(
    m: &mut Mob,
    now: i64,
    event: &'static str,
    pos: [f64; 3],
    ctx: &Context,
    dimension: &str,
    occluded: impl Fn([f64; 3], [f64; 3]) -> bool,
) {
    let eye = [m.position.x, m.eye_y(), m.position.z];
    let Some(st) = m.extra.allay.as_ref() else {
        return;
    };
    if st.vibration.current.is_some() {
        return;
    }
    let block = |p: [f64; 3]| p.map(|v| i64::from(v.floor() as i32));
    let (source, listener) = (block(pos), block(eye));
    let d = [0, 1, 2].map(|i| source[i] - listener[i]);
    if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > LISTENER_RADIUS * LISTENER_RADIUS {
        return;
    }
    if !sculk::valid_vibration(sculk::allay_can_listen(event), event, ctx)
        || !can_receive_vibration(m, dimension, block(pos).map(|v| v as i32))
        || occluded(pos, eye)
    {
        return;
    }
    let d = [pos[0] - eye[0], pos[1] - eye[1], pos[2] - eye[2]];
    let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() as f32;
    if let Some(st) = m.extra.allay.as_mut() {
        st.vibration.schedule(
            Info {
                event,
                distance,
                pos,
                source: ctx.source,
                owner: ctx.owner,
            },
            now,
        );
    }
}
pub fn tick(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    let tick = m
        .extra
        .allay
        .as_mut()
        .map(|st| {
            st.vibration
                .tick(w.game_time, true, false, |d| d.floor() as i32)
        })
        .unwrap_or_default();
    if let Some((info, travel)) = tick.started {
        events.push(MobEvent::Vibration {
            from: info.pos,
            to: [m.position.x, m.eye_y(), m.position.z],
            ticks: travel,
        });
    }
    if let Some(info) = tick.received {
        receive_vibration(m, dimension(w), info);
    }
}
pub fn travel(m: &mut Mob, w: &MobWorld, input: Vec3, events: &mut Vec<MobEvent>) -> bool {
    let speed = m.ai.speed;
    m.travel_flying_speeds(w, input, events, 0.02, 0.02, speed);
    true
}
pub fn ai_step(m: &mut Mob) {
    if m.alive() && m.age.is_multiple_of(10) {
        m.health = (m.health + 1.0).min(m.attrs.max_health);
    }
    for i in [2, 3] {
        if m.extra.ints[i] > 0 {
            m.extra.ints[i] -= 1;
        }
    }
    if m.extra.egg_time > 0 {
        m.extra.egg_time -= 1;
    }
    animate(m);
}
pub fn animate(m: &mut Mob) {
    let holding = m.extra.main_hand.is_some();
    let v = &mut m.anim.v;
    v[HOLD] = (v[HOLD] + if holding { 1.0 } else { -1.0 }).clamp(0.0, 5.0);
    if m.extra.flag {
        m.extra.ints[4] += 1;
        let spinning = (m.extra.ints[4] as f32 % 55.0) < 15.0;
        v[SPIN] = (v[SPIN] + if spinning { 1.0 } else { -1.0 }).clamp(0.0, 15.0);
    } else {
        m.extra.ints[4] = 0;
        v[SPIN] = 0.0;
        m.anim.vo[SPIN] = 0.0;
    }
}
pub fn anim(m: &Mob, s: &AnimState, p: &mut Poses) {
    use std::f32::consts::PI;
    let (speed, pos) = (s.speed, s.pos);
    let flap_speed = (s.age * 20.0).to_radians() + pos;
    let flap = flap_speed.mcos() * PI * 0.15 + speed;
    let idle_bob = (s.age * 9.0).to_radians();
    let flying = (speed / 0.3).min(1.0);
    let idle = 1.0 - flying;
    let hold = m.anim.lerp(HOLD, s.partial) / 5.0;
    if m.extra.flag {
        let dance = (s.age * 8.0).to_radians() + speed;
        let freq = dance.mcos() * 16f32.to_radians();
        let spin = m.anim.lerp(SPIN, s.partial) / 15.0;
        let spinning = (m.extra.ints[4] as f32 % 55.0) < 15.0;
        if spinning {
            p.yrot("root", PI * 4.0 * spin);
        }
        p.zrot("root", freq * (1.0 - spin));
        p.yrot("head", dance.mcos() * 30f32.to_radians() * (1.0 - spin));
        p.zrot("head", dance.mcos() * 14f32.to_radians() * (1.0 - spin));
    } else {
        p.xrot("head", s.x_rot.to_radians());
        p.yrot("head", s.y_rot.to_radians());
    }
    p.xrot("right_wing", 0.436_332_32 * (1.0 - flying));
    p.yrot("right_wing", -PI / 4.0 + flap);
    p.xrot("left_wing", 0.436_332_32 * (1.0 - flying));
    p.yrot("left_wing", PI / 4.0 - flap);
    p.xrot("body", flying * PI / 4.0);
    let arm_x = hold * (-PI / 3.0 + (-1.134_464 + PI / 3.0) * flying);
    p.get("root")[1] += (f64::from(idle_bob).cos() as f32) * 0.25 * idle;
    p.xrot("right_arm", arm_x);
    p.xrot("left_arm", arm_x);
    let arm_bob = idle * (1.0 - hold);
    let amount = 0.436_332_32 - (idle_bob + PI * 1.5).mcos() * PI * 0.075 * arm_bob;
    p.zrot("left_arm", -amount);
    p.zrot("right_arm", amount);
    p.yrot("right_arm", 0.279_252_68 * hold);
    p.yrot("left_arm", -0.279_252_68 * hold);
}
