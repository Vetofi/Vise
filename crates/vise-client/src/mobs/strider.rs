use super::{
    Mob, MobEvent, MobWorld, Species,
    data::LAND,
    goals::{
        Breed, Cx, FollowParent, Goal, JUMP, LookAtPlayer, MOVE, MoveToBlock, Panic,
        RandomLookAround, Stroll, Tempt, goal_clone, reduced,
    },
    path::PathType,
};
use crate::blocks::Fluid;
use vise_core::random::RandomSource;

const SUFFOCATING: f64 = -0.34_f32 as f64;

pub fn register(m: &mut Mob) {
    m.set_malus(PathType::Water, -1.0);
    m.set_malus(PathType::Lava, 0.0);
    m.set_malus(PathType::FireInNeighbor, 0.0);
    m.set_malus(PathType::Fire, 0.0);
    let tempt = m.kind.tempt.clone();
    let g = &mut m.ai.goals;
    g.add(1, Panic::new(1.65));
    g.add(2, Breed::new(1.0));
    g.add(3, Tempt::new(1.4, tempt, false));
    g.add(4, GoToLava(MoveToBlock::new(1.0, 8, 2)));
    g.add(5, FollowParent::new(1.0));
    g.add(7, Stroll::new(1.0).interval(60));
    g.add(8, LookAtPlayer::new(8.0));
    g.add(8, RandomLookAround::default());
    g.add(9, LookAtPlayer::species(Species::Strider, 8.0));
}
pub fn suffocating(m: &Mob) -> bool {
    m.species() == Species::Strider && m.extra.flag
}
pub fn movement_speed(m: &Mob, base: f64) -> f64 {
    if suffocating(m) {
        base + base * SUFFOCATING
    } else {
        base
    }
}
pub fn pre_tick(m: &mut Mob, w: &MobWorld, events: &mut Vec<MobEvent>) {
    if m.ai.goals.is_running("Tempt") && m.random.next_int_bound(140).unwrap_or(1) == 0 {
        m.voice("entity.strider.happy", events);
    } else if m.ai.panicking && m.random.next_int_bound(60).unwrap_or(1) == 0 {
        m.voice("entity.strider.retreat", events);
    }
    if m.no_ai {
        return;
    }
    let warm = |p: [i32; 3]| {
        w.has_tag(
            "minecraft:strider_warm_blocks",
            w.block_name(p[0], p[1], p[2]),
        )
    };
    let inside = [
        m.position.x.floor() as i32,
        m.position.y.floor() as i32,
        m.position.z.floor() as i32,
    ];
    let in_warm = warm(inside) || warm(m.block_on_legacy(w)) || m.fluids.lava > 0.0;
    let on_warm_strider = m
        .extra
        .mount
        .and_then(|id| w.seen(id))
        .is_some_and(|v| v.species == Species::Strider && !v.cold_strider);
    m.extra.flag = !in_warm && !on_warm_strider;
}
pub fn float(m: &mut Mob, w: &MobWorld) {
    if !m.fluids.in_lava {
        return;
    }
    let (x, y, z) = (
        m.position.x.floor() as i32,
        m.position.y.floor() as i32,
        m.position.z.floor() as i32,
    );
    let above = m.position.y > f64::from(y) + 0.5 - f64::from(1.0e-5_f32);
    if above && w.fluid(x, y + 1, z) != Fluid::Lava {
        m.on_ground = true;
    } else {
        let v = m.velocity;
        m.velocity = vise_core::geometry::Vec3::new(v.x * 0.5, v.y * 0.5 + 0.05, v.z * 0.5);
    }
}
pub fn walk_target_value(m: &Mob, w: &MobWorld, p: [i32; 3]) -> f32 {
    if w.fluid(p[0], p[1], p[2]) == Fluid::Lava {
        10.0
    } else if m.fluids.in_lava {
        f32::NEG_INFINITY
    } else {
        0.0
    }
}
fn lava_target(w: &MobWorld, p: [i32; 3]) -> bool {
    w.block_name(p[0], p[1], p[2]) == "minecraft:lava"
        && w.data.state(w.world.block(p[0], p[1] + 1, p[2])).flags & LAND != 0
}
#[derive(Clone, Debug)]
struct GoToLava(MoveToBlock);
impl Goal for GoToLava {
    goal_clone!();
    fn flags(&self) -> u8 {
        MOVE | JUMP
    }
    fn name(&self) -> &'static str {
        "StriderGoToLavaGoal"
    }
    fn every_tick(&self) -> bool {
        true
    }
    fn can_use(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        if m.fluids.in_lava {
            return false;
        }
        if self.0.next_start > 0 {
            self.0.next_start -= 1;
            return false;
        }
        self.0.next_start = reduced(200 + m.random.next_int_bound(200).unwrap_or(0));
        self.0.find_nearest(m, cx.w, lava_target)
    }
    fn can_continue(&mut self, m: &mut Mob, cx: &mut Cx) -> bool {
        !m.fluids.in_lava && lava_target(cx.w, self.0.block)
    }
    fn start(&mut self, m: &mut Mob, cx: &mut Cx) {
        self.0.start(m, cx.w);
    }
    fn tick(&mut self, m: &mut Mob, cx: &mut Cx) {
        let b = self.0.block;
        let center = [
            f64::from(b[0]) + 0.5,
            f64::from(b[1]) + 0.5,
            f64::from(b[2]) + 0.5,
        ];
        let d2 = (m.position.x - center[0]).powi(2)
            + (m.position.y - center[1]).powi(2)
            + (m.position.z - center[2]).powi(2);
        if d2 >= 1.0 {
            self.0.reached = false;
            self.0.try_ticks += 1;
            if self.0.try_ticks % 20 == 0 {
                let speed = self.0.speed;
                m.nav_move_to(cx.w, [center[0], f64::from(b[1]), center[2]], speed);
            }
        } else {
            self.0.reached = true;
            self.0.try_ticks -= 1;
        }
    }
}
