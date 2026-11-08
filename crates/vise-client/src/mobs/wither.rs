use super::{
    DamageKind, Mob, MobEvent, MobWorld, Species, Who,
    anim::{Poses, State},
    control::{MoveKind, rotlerp},
    goals::{
        Cx, Goal, HurtByTarget, JUMP, LOOK, LookAtPlayer, MOVE, NearestTarget, Prey,
        RandomLookAround, Stroll, goal_clone,
    },
    nav::NavKind,
};
use crate::projectiles::{Projectile, ProjectileKind};
use vise_core::geometry::trig::Mth;
use vise_core::{geometry::Vec3, random::RandomSource};

pub const INVULNERABLE: i32 = 220;

pub fn register(m: &mut Mob) {
    m.ai.mov.kind = MoveKind::Flying {
        max_turn: 10.0,
        hovers: false,
    };
    m.ai.nav.kind = NavKind::Flying;
    let g = &mut m.ai.goals;
    g.add(0, DoNothing);
    g.add(
        2,
        super::witch::RangedAttack::new(1.0, 40, 40, 20.0, |m, who, _, cx| {
            if let Some(t) = cx.w.target_info(who) {
                let at = [
                    t.position.x,
                    t.position.y + (t.eye_y - t.position.y) * 0.5,
                    t.position.z,
                ];
                let dangerous = m.random.next_float() < 0.001;
                shoot(m, 0, at, dangerous, cx.events);
            }
        }),
    );
    g.add(5, Stroll::flying(1.0));
    g.add(6, LookAtPlayer::new(8.0));
    g.add(7, RandomLookAround::default());
    m.ai.targets.add(1, HurtByTarget::new());
    m.ai.targets.add(
        2,
        NearestTarget::new(Prey::Filter(|s| !undead(s.species)), false).interval(0),
    );
    m.ai.targets
        .add(2, NearestTarget::player(false).interval(0));
}
pub fn shielded_from(victim: Species, attacker: Species) -> bool {
    victim == Species::Wither && undead(attacker)
}
fn undead(s: Species) -> bool {
    matches!(
        s,
        Species::Zombie
            | Species::Husk
            | Species::Drowned
            | Species::ZombieVillager
            | Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::WitherSkeleton
            | Species::ZombieHorse
            | Species::SkeletonHorse
            | Species::Phantom
            | Species::ZombifiedPiglin
            | Species::Zoglin
            | Species::Wither
    )
}
#[derive(Clone, Debug)]
struct DoNothing;
impl Goal for DoNothing {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP | LOOK
    }
    fn can_use(&mut self, m: &mut Mob, _cx: &mut Cx) -> bool {
        m.extra.counter > 0
    }
}
pub fn make_invulnerable(m: &mut Mob) {
    m.extra.counter = INVULNERABLE;
    m.health = m.attrs.max_health / 3.0;
}
pub fn powered(m: &Mob) -> bool {
    m.health <= m.attrs.max_health / 2.0
}
pub fn head_pos(m: &Mob, i: usize) -> [f64; 3] {
    if i == 0 {
        return [m.position.x, m.position.y + 3.0, m.position.z];
    }
    let a = (m.body_yaw + 180.0 * (i as f32 - 1.0)).to_radians();
    [
        m.position.x + f64::from(a.mcos()) * 1.3,
        m.position.y + 2.2,
        m.position.z + f64::from(a.msin()) * 1.3,
    ]
}
fn shoot(m: &mut Mob, head: usize, at: [f64; 3], dangerous: bool, events: &mut Vec<MobEvent>) {
    let h = head_pos(m, head);
    if !m.silent {
        events.push(MobEvent::LevelEventSound {
            sound: "entity.wither.shoot",
            at: [m.position.x, m.position.y, m.position.z],
            volume: 2.0,
            spread: 0.2,
        });
    }
    let dir = [at[0] - h[0], at[1] - h[1], at[2] - h[2]];
    let p = Projectile::fireball(
        ProjectileKind::WitherSkull { dangerous },
        Vec3::new(h[0], h[1], h[2]),
        Some(Who::Mob(m.id)),
        dir,
    );
    events.push(MobEvent::Projectile(p));
}
pub fn steer(m: &mut Mob) {
    let mut v = Vec3::new(m.velocity.x, m.velocity.y * 0.6, m.velocity.z);
    if let (Some(t), Some(_)) = (m.extra.point, m.ai.target) {
        let mut yd = v.y;
        if m.position.y < t[1] || !powered(m) && m.position.y < t[1] + 5.0 {
            yd = yd.max(0.0);
            yd += 0.3 - yd * f64::from(0.6_f32);
        }
        v.y = yd;
        let (dx, dz) = (t[0] - m.position.x, t[2] - m.position.z);
        if dx * dx + dz * dz > 9.0 {
            let len = (dx * dx + dz * dz).sqrt();
            v.x += dx / len * 0.3 - v.x * 0.6;
            v.z += dz / len * 0.3 - v.z * 0.6;
        }
    }
    m.velocity = v;
    if v.x * v.x + v.z * v.z > 0.05 {
        m.yaw = (vise_core::geometry::trig::atan2(v.z, v.x) as f32)
            * (180.0_f32 / std::f32::consts::PI)
            - 90.0;
    }
}
fn target_pos(w: &MobWorld, id: i32) -> Option<([f64; 3], f64, bool)> {
    match id {
        -1 => w
            .player
            .as_ref()
            .filter(|p| p.alive && !p.creative && !p.spectator)
            .map(|p| ([p.position.x, p.position.y, p.position.z], p.eye[1], true)),
        0 => None,
        id => w
            .others
            .iter()
            .find(|s| s.id == id as u32 && s.alive)
            .map(|s| ([s.position.x, s.position.y, s.position.z], s.eye_y, false)),
    }
}
pub fn client_smoke(m: &mut Mob, events: &mut Vec<MobEvent>) {
    for i in 0..3 {
        let h = head_pos(m, i);
        let r = &mut m.random;
        let at = [
            h[0] + r.next_gaussian() * 0.3,
            h[1] + r.next_gaussian() * 0.3,
            h[2] + r.next_gaussian() * 0.3,
        ];
        events.push(MobEvent::ParticleAt {
            kind: "smoke",
            at,
            velocity: [0.0; 3],
        });
    }
}
fn turn_heads(m: &mut Mob, find: impl Fn(i32) -> Option<([f64; 3], f64)>) {
    for i in 0..2 {
        match find(m.extra.ints[i]) {
            Some((p, eye)) => {
                let h = head_pos(m, i + 1);
                let (xd, yd, zd) = (p[0] - h[0], eye - h[1], p[2] - h[2]);
                let sd = (xd * xd + zd * zd).sqrt();
                let y_rot = vise_core::geometry::trig::atan2_degrees(zd, xd) - 90.0;
                let x_rot = -vise_core::geometry::trig::atan2_degrees(yd, sd);
                m.anim.v[2 + i] = rotlerp(m.anim.v[2 + i], x_rot, 40.0);
                m.anim.v[i] = rotlerp(m.anim.v[i], y_rot, 10.0);
            }
            None => m.anim.v[i] = rotlerp(m.anim.v[i], m.body_yaw, 10.0),
        }
    }
}
pub fn client_tick(m: &mut Mob, find: impl Fn(i32) -> Option<([f64; 3], f64)>) {
    m.anim.vo = m.anim.v;
    turn_heads(m, find);
}
pub fn ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    turn_heads(m, |id| target_pos(w, id).map(|(p, eye, _)| (p, eye)));
    let powered = powered(m);
    for i in 0..3 {
        let h = head_pos(m, i);
        for _ in 0..3 {
            m.random.next_gaussian();
        }
        events.push(MobEvent::Particles {
            kind: "smoke",
            at: h,
            count: 0,
        });
        let _ = powered;
    }
    if m.extra.counter > 0 {
        for _ in 0..3 {
            m.random.next_gaussian();
            m.random.next_float();
            m.random.next_gaussian();
        }
    }
}
pub fn custom_server_ai_step(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if m.extra.counter > 0 {
        let left = m.extra.counter - 1;
        if left <= 0 {
            let name = match &m.custom_name {
                Some(name) => crate::text::Text::literal(name.clone()),
                None => crate::text::Text::translate("entity.minecraft.wither", &[]),
            };
            events.push(MobEvent::ExplosionSource {
                mob: m.id,
                charged: false,
                name,
            });
            events.push(MobEvent::Explode(
                [m.position.x, m.eye_y(), m.position.z],
                7.0,
                m.species(),
            ));
            if !m.silent {
                events.push(MobEvent::LevelEventSound {
                    sound: "entity.wither.spawn",
                    at: [m.position.x, m.position.y, m.position.z],
                    volume: 1.0,
                    spread: 0.0,
                });
            }
        }
        m.extra.counter = left;
        if m.age.is_multiple_of(10) {
            m.health = (m.health + 10.0).min(m.attrs.max_health);
        }
        return;
    }
    m.extra.point = m
        .target_info(w)
        .map(|t| [t.position.x, t.position.y, t.position.z]);
    let age = m.age as i32;
    for i in 1..3 {
        let k = i - 1;
        if age < m.extra.ints[2 + k] {
            continue;
        }
        m.extra.ints[2 + k] = age + 10 + m.random.next_int_bound(10).unwrap_or(0);
        m.extra.ints[4 + k] += 1;
        if m.extra.ints[4 + k] > 16 {
            let p = m.position;
            let r = &mut m.random;
            let mut between = |lo: f64, hi: f64| r.next_double() * (hi - lo) + lo;
            let at = [
                between(p.x - 10.0, p.x + 10.0),
                between(p.y - 5.0, p.y + 5.0),
                between(p.z - 10.0, p.z + 10.0),
            ];
            shoot(m, i + 1, at, true, events);
            m.extra.ints[4 + k] = 0;
        }
        let id = m.extra.ints[k];
        if id != 0 {
            match target_pos(w, id) {
                Some((p, eye, _))
                    if (p[0] - m.position.x).powi(2)
                        + (p[1] - m.position.y).powi(2)
                        + (p[2] - m.position.z).powi(2)
                        <= 900.0
                        && m.sees(
                            w,
                            if id == -1 {
                                Who::Player
                            } else {
                                Who::Mob(id as u32)
                            },
                        ) =>
                {
                    let at = [p[0], p[1] + (eye - p[1]) * 0.5, p[2]];
                    shoot(m, i + 1, at, false, events);
                    m.extra.ints[2 + k] = age + 40 + m.random.next_int_bound(20).unwrap_or(0);
                    m.extra.ints[4 + k] = 0;
                }
                _ => m.extra.ints[k] = 0,
            }
        } else {
            let p = m.position;
            let near = |q: Vec3| {
                (q.x - p.x).abs() <= 20.0 && (q.y - p.y).abs() <= 8.0 && (q.z - p.z).abs() <= 20.0
            };
            let mut found: Vec<i32> = w
                .others
                .iter()
                .filter(|s| s.alive && s.id != m.id && !undead(s.species) && near(s.position))
                .map(|s| s.id as i32)
                .collect();
            if let Some(pl) = &w.player
                && pl.alive
                && !pl.creative
                && !pl.spectator
                && near(pl.position)
            {
                found.push(-1);
            }
            if !found.is_empty() {
                let pick = m.random.next_int_bound(found.len() as i32).unwrap_or(0) as usize;
                m.extra.ints[k] = found[pick];
            }
        }
    }
    if m.extra.counter2 > 0 {
        m.extra.counter2 -= 1;
        if m.extra.counter2 == 0 && w.mob_griefing {
            let width = (m.width() / 2.0 + 1.0).floor() as i32;
            let height = m.height().floor() as i32;
            let b = [
                m.position.x.floor() as i32,
                m.position.y.floor() as i32,
                m.position.z.floor() as i32,
            ];
            let mut destroyed = false;
            for x in b[0] - width..=b[0] + width {
                for y in b[1]..=b[1] + height {
                    for z in b[2] - width..=b[2] + width {
                        let name = w.block_name(x, y, z);
                        let immune = w
                            .block_tags
                            .get("minecraft:wither_immune")
                            .is_some_and(|t| t.contains(name));
                        if name != "minecraft:air" && !name.ends_with("_air") && !immune {
                            events.push(MobEvent::BreakBlock {
                                pos: [x, y, z],
                                drop: true,
                            });
                            let state = w.world.block(x, y, z);
                            destroyed |= !w.facts.get(state).has_class("LiquidBlock");
                        }
                    }
                }
            }
            if destroyed {
                events.push(MobEvent::LevelEventSound {
                    sound: "entity.wither.break_block",
                    at: [m.position.x, m.position.y, m.position.z],
                    volume: 1.0,
                    spread: 0.0,
                });
            }
        }
    }
    if m.age.is_multiple_of(20) {
        m.health = (m.health + 1.0).min(m.attrs.max_health);
    }
}
pub fn accepts(m: &mut Mob, kind: DamageKind, attacker: Option<Who>, others_undead: bool) -> bool {
    if m.extra.counter > 0 && !kind.bypasses_invulnerability() {
        return false;
    }
    if powered(m) && kind == DamageKind::Arrow {
        return false;
    }
    if others_undead || matches!(attacker, Some(Who::Mob(id)) if id == m.id) {
        return false;
    }
    if m.extra.counter2 <= 0 {
        m.extra.counter2 = 20;
    }
    m.extra.ints[4] += 3;
    m.extra.ints[5] += 3;
    true
}
pub fn anim(m: &Mob, s: &State, p: &mut Poses) {
    let body =
        m.previous_body_yaw + super::living::wrap(m.body_yaw - m.previous_body_yaw) * s.partial;
    for (i, head) in ["right_head", "left_head"].into_iter().enumerate() {
        let y = m.anim.lerp(i, s.partial);
        let x = m.anim.lerp(2 + i, s.partial);
        p.yrot(head, (y - body).to_radians());
        p.xrot(head, x.to_radians());
    }
    let anim = (s.age * 0.1).mcos();
    let rib = (0.065 + 0.05 * anim) * std::f32::consts::PI;
    p.xrot("ribcage", rib);
    let tail = p.get("tail");
    tail[0] = -2.0;
    tail[1] = 6.9 + rib.mcos() * 10.0;
    tail[2] = -0.5 + rib.msin() * 10.0;
    p.xrot("tail", (0.265 + 0.1 * anim) * std::f32::consts::PI);
    p.yrot("center_head", s.y_rot.to_radians());
    p.xrot("center_head", s.x_rot.to_radians());
}
pub fn texture(m: &Mob) -> &'static str {
    let t = m.extra.counter;
    if t > 0 && (t > 80 || t / 5 % 2 != 1) {
        "entity/wither/wither_invulnerable"
    } else {
        "entity/wither/wither"
    }
}
pub fn scale(m: &Mob, alpha: f32) -> f32 {
    let mut s = 2.0;
    if m.extra.counter > 0 {
        s -= (m.extra.counter as f32 - alpha) / INVULNERABLE as f32 * 0.5;
    }
    s
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wither_ignores_wither_friends_only() {
        assert!(shielded_from(Species::Wither, Species::Wither));
        assert!(shielded_from(Species::Wither, Species::Skeleton));
        assert!(!shielded_from(Species::Wither, Species::Creeper));
        assert!(!shielded_from(Species::Zombie, Species::Wither));
    }
}
