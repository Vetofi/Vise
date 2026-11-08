use super::{Mob, MobEvent, MobWorld, Species, Who};
use vise_core::geometry::Vec3;
use vise_world::nbt::Tag;

pub const ELASTIC: f64 = 6.0;
pub const SNAP: f64 = 12.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Holder {
    Player,
    Mob(u32),
    Knot([i32; 3]),
}
impl Holder {
    pub fn who(self) -> Option<Who> {
        match self {
            Holder::Player => Some(Who::Player),
            Holder::Mob(id) => Some(Who::Mob(id)),
            Holder::Knot(_) => None,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pending {
    Uuid([i32; 4]),
    Knot([i32; 3]),
}
pub fn pending_from_nbt(tag: Option<&Tag>) -> Option<Pending> {
    match tag? {
        Tag::IntArray(p) if p.len() == 3 => Some(Pending::Knot([p[0], p[1], p[2]])),
        t @ Tag::Compound(_) => crate::save::uuid_from_tag(t.get("UUID")).map(Pending::Uuid),
        _ => None,
    }
}
pub fn to_nbt(holder: Holder, uuid: Option<[i32; 4]>) -> Option<Tag> {
    match holder {
        Holder::Knot(p) => Some(Tag::IntArray(p.to_vec())),
        _ => uuid.map(|u| Tag::compound([("UUID", Tag::IntArray(u.to_vec()))])),
    }
}
#[derive(Clone, Copy, Debug)]
pub struct HolderState {
    pub position: Vec3,
    pub center: Vec3,
    pub width: f64,
    pub height: f64,
    pub yaw: f32,
    pub velocity: Vec3,
}
pub fn knot_state(pos: [i32; 3]) -> HolderState {
    let position = Vec3::new(
        f64::from(pos[0]) + 0.5,
        f64::from(pos[1]) + 0.375,
        f64::from(pos[2]) + 0.5,
    );
    HolderState {
        position,
        center: Vec3::new(position.x, position.y + 0.25, position.z),
        width: 0.375,
        height: 0.5,
        yaw: 0.0,
        velocity: Vec3::ZERO,
    }
}
pub fn holder_state(w: &MobWorld, holder: Holder) -> Option<HolderState> {
    match holder {
        Holder::Player => {
            let p = w.player.as_ref()?;
            let size = p.bounds.max_position().subtract(p.bounds.min_position());
            Some(HolderState {
                position: p.position,
                center: Vec3::new(p.position.x, p.position.y + size.y / 2.0, p.position.z),
                width: size.x,
                height: size.y,
                yaw: p.yaw,
                velocity: p.velocity,
            })
        }
        Holder::Mob(id) => {
            let s = w.seen(id).filter(|s| s.alive)?;
            let lo = s.bounds.min_position();
            let hi = s.bounds.max_position();
            Some(HolderState {
                position: s.position,
                center: Vec3::new(
                    (lo.x + hi.x) / 2.0,
                    (lo.y + hi.y) / 2.0,
                    (lo.z + hi.z) / 2.0,
                ),
                width: hi.x - lo.x,
                height: hi.y - lo.y,
                yaw: s.yaw,
                velocity: s.velocity,
            })
        }
        Holder::Knot(pos) => {
            let fence = w.has_tag("minecraft:fences", w.block_name(pos[0], pos[1], pos[2]));
            fence.then(|| knot_state(pos))
        }
    }
}
fn rot_y(v: [f64; 3], angle: f64) -> [f64; 3] {
    let (s, c) = (angle.sin(), angle.cos());
    [v[0] * c + v[2] * s, v[1], v[2] * c - v[0] * s]
}
pub fn spring(
    position: Vec3,
    yaw: f32,
    size: (f64, f64),
    holder: &HolderState,
    own_motion: Vec3,
) -> Option<([f64; 3], f64)> {
    let (w, h) = size;
    let lever = rot_y([0.0, 0.5 * h, 0.5 * w], -f64::from(yaw.to_radians()));
    let object = [
        position.x + lever[0],
        position.y + lever[1],
        position.z + lever[2],
    ];
    let arm = rot_y(
        [0.0, 0.5 * holder.height, 0.0],
        -f64::from(holder.yaw.to_radians()),
    );
    let pivot = [
        holder.position.x + arm[0],
        holder.position.y + arm[1],
        holder.position.z + arm[2],
    ];
    let d = [
        pivot[0] - object[0],
        pivot[1] - object[1],
        pivot[2] - object[2],
    ];
    let distance = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if distance < ELASTIC {
        return None;
    }
    let scale = (distance - ELASTIC) / distance;
    let mut force = [d[0] * scale, d[1] * scale, d[2] * scale];
    let torque = lever[2] * force[0] - lever[0] * force[2];
    let same_direction =
        own_motion.x * force[0] + own_motion.y * force[1] + own_motion.z * force[2] >= 0.0;
    if same_direction {
        for f in &mut force {
            *f *= f64::from(0.3_f32);
        }
    }
    Some((force, torque))
}
fn check_fall_distance(m: &mut Mob) {
    if m.velocity.y > -0.5 && m.fall_distance > 1.0 {
        m.fall_distance = 1.0;
    }
}
pub fn can_be_leashed(m: &Mob) -> bool {
    use Species as S;
    match m.species() {
        S::Hoglin | S::Zoglin => true,
        S::Villager
        | S::WanderingTrader
        | S::Turtle
        | S::Panda
        | S::Bat
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole => false,
        S::Wolf => !super::tame::angry(m),
        _ => !m.hostile() && !m.removed,
    }
}
pub fn tick(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    let Some(holder) = m.extra.leash else {
        m.extra.leash_spin = 0.0;
        return;
    };
    if !m.alive() {
        events.push(MobEvent::LeashBreak {
            mob: m.id,
            drop: true,
            sound: false,
        });
        return;
    }
    let Some(state) = holder_state(w, holder) else {
        if !matches!(holder, Holder::Player) {
            events.push(MobEvent::LeashBreak {
                mob: m.id,
                drop: true,
                sound: false,
            });
        }
        return;
    };
    let elastic = if m.species() == super::Species::HappyGhast {
        10.0
    } else {
        ELASTIC
    };
    m.ai.home = Some((
        [
            state.position.x.floor() as i32,
            state.position.y.floor() as i32,
            state.position.z.floor() as i32,
        ],
        elastic as i32 - 1,
    ));
    let b = m.bounds();
    let (lo, hi) = (b.min_position(), b.max_position());
    let center = Vec3::new(
        (lo.x + hi.x) / 2.0,
        (lo.y + hi.y) / 2.0,
        (lo.z + hi.z) / 2.0,
    );
    let distance = ((state.center.x - center.x).powi(2)
        + (state.center.y - center.y).powi(2)
        + (state.center.z - center.z).powi(2))
    .sqrt();
    if distance > SNAP {
        events.push(MobEvent::LeashBreak {
            mob: m.id,
            drop: true,
            sound: true,
        });
        return;
    }
    let stretched = distance > ELASTIC - state.width - f64::from(m.width());
    let pulled = stretched
        && spring(
            m.position,
            m.yaw,
            (f64::from(m.width()), f64::from(m.height())),
            &state,
            m.velocity,
        )
        .is_some_and(|(force, torque)| {
            m.extra.leash_spin += 10.0 * torque;
            let rel = Vec3::new(
                state.velocity.x - m.velocity.x,
                state.velocity.y - m.velocity.y,
                state.velocity.z - m.velocity.z,
            );
            m.velocity = Vec3::new(
                m.velocity.x + force[0] * 0.8 + rel.x * 0.11,
                m.velocity.y + force[1] * 0.2 + rel.y * 0.11,
                m.velocity.z + force[2] * 0.8 + rel.z * 0.11,
            );
            check_fall_distance(m);
            super::equine::on_elastic_leash_pull(m, w, events);
            true
        });
    if !pulled {
        close_range(m, w, &state);
    }
    m.yaw -= m.extra.leash_spin as f32;
    let friction = if m.on_ground {
        0.6 * 0.91
    } else if m.is_in_liquid() {
        0.8
    } else {
        0.91
    };
    m.extra.leash_spin *= friction;
}
pub fn remove(m: &mut Mob) -> Option<Holder> {
    let was = m.extra.leash.take();
    if was.is_some() {
        m.ai.home = None;
    }
    was
}
fn close_range(m: &mut Mob, w: &MobWorld, holder: &HolderState) {
    if m.ai.panicking {
        return;
    }
    let d = Vec3::new(
        holder.position.x - m.position.x,
        holder.position.y - m.position.y,
        holder.position.z - m.position.z,
    );
    let dist = d.length();
    let len = d.length();
    let scale = if len > 0.0 {
        (dist - 2.0).max(0.0) / len
    } else {
        0.0
    };
    let target = [
        m.position.x + d.x * scale,
        m.position.y + d.y * scale,
        m.position.z + d.z * scale,
    ];
    let speed = follow_speed(m);
    m.nav_move_to(w, target, speed);
}
pub fn follow_speed(m: &Mob) -> f64 {
    if super::llama::is_llama(m.species()) {
        2.0
    } else {
        1.0
    }
}
pub fn leash_offset(m: &Mob) -> [f64; 3] {
    if super::llama::is_llama(m.species()) {
        [0.0, 0.75 * m.eye_height(), f64::from(m.width()) * 0.5]
    } else if m.species() == super::Species::Wolf {
        [
            0.0,
            f64::from(0.6_f32 * m.eye_height() as f32),
            f64::from(m.width() * 0.4),
        ]
    } else if m.species() == super::Species::Allay {
        [0.0, m.eye_height() * 0.6, f64::from(m.width()) * 0.1]
    } else if m.species() == super::Species::Bee {
        [0.0, 0.5 * m.eye_height(), f64::from(m.width() * 0.2)]
    } else {
        [0.0, m.eye_height(), f64::from(m.width() * 0.4)]
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotating_about_y_matches_vanilla() {
        let v = rot_y([0.0, 1.0, 1.0], std::f64::consts::FRAC_PI_2);
        assert!((v[0] - 1.0).abs() < 1e-9 && v[2].abs() < 1e-9);
    }
    fn holder(x: f64) -> HolderState {
        let position = Vec3::new(x, 64.0, 0.0);
        HolderState {
            position,
            center: Vec3::new(x, 64.9, 0.0),
            width: 0.6,
            height: 1.8,
            yaw: 0.0,
            velocity: Vec3::ZERO,
        }
    }
    #[test]
    fn a_slack_rope_does_not_pull() {
        let pulled = spring(
            Vec3::new(0.0, 64.0, 0.0),
            0.0,
            (0.9, 1.87),
            &holder(3.0),
            Vec3::ZERO,
        );
        assert!(pulled.is_none());
    }
    #[test]
    fn a_stretched_rope_pulls_toward_the_holder() {
        let (force, _) = spring(
            Vec3::new(0.0, 64.0, 0.0),
            0.0,
            (0.9, 1.87),
            &holder(10.0),
            Vec3::ZERO,
        )
        .expect("past the slack");
        assert!(force[0] > 0.0);
        assert!(force[0] < 10.0 - ELASTIC);
    }
    #[test]
    fn moving_with_the_pull_damps_it() {
        let at = Vec3::new(0.0, 64.0, 0.0);
        let still = spring(
            at,
            0.0,
            (0.9, 1.87),
            &holder(10.0),
            Vec3::new(-0.1, 0.0, 0.0),
        )
        .expect("pull")
        .0;
        let following = spring(
            at,
            0.0,
            (0.9, 1.87),
            &holder(10.0),
            Vec3::new(0.1, 0.0, 0.0),
        )
        .expect("pull")
        .0;
        assert!((following[0] - still[0] * 0.3_f32 as f64).abs() < 1e-9);
    }
    #[test]
    fn leash_tags_round_trip() {
        let knot = to_nbt(Holder::Knot([1, 2, 3]), None).expect("knot tag");
        assert_eq!(
            pending_from_nbt(Some(&knot)),
            Some(Pending::Knot([1, 2, 3]))
        );
        let held = to_nbt(Holder::Player, Some([4, 5, 6, 7])).expect("uuid tag");
        assert_eq!(
            pending_from_nbt(Some(&held)),
            Some(Pending::Uuid([4, 5, 6, 7]))
        );
        assert_eq!(to_nbt(Holder::Mob(1), None), None);
    }
    #[test]
    fn a_knot_sits_on_the_fence_post() {
        let k = knot_state([10, 64, -3]);
        assert_eq!(k.position.x, 10.5);
        assert_eq!(k.position.y, 64.375);
        assert_eq!(k.center.y, 64.625);
    }
}
