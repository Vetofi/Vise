use super::Game;
use vise_client::{
    entities::{ExperienceOrb, ItemEntity},
    projectiles::Projectile,
};

const LIFE_TIME: u32 = 3;

pub(super) enum Taken {
    Item(Box<ItemEntity>),
    Orb(Box<ExperienceOrb>),
    Arrow(Box<Projectile>),
}
impl Taken {
    fn position(&self) -> [f64; 3] {
        let p = match self {
            Taken::Item(i) => i.position,
            Taken::Orb(o) => o.position,
            Taken::Arrow(a) => a.position,
        };
        [p.x, p.y, p.z]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Collector {
    Player,
    Mob(u32),
    Guest(u32),
}

pub(super) struct Pickup {
    pub taken: Taken,
    collector: Collector,
    life: u32,
    target: [f64; 3],
    target_old: [f64; 3],
}

#[derive(Default)]
pub(super) struct Pickups {
    pub live: Vec<Pickup>,
    added: Vec<Pickup>,
}
impl Pickups {
    pub fn clear(&mut self) {
        self.live.clear();
        self.added.clear();
    }
}

pub(super) fn position(p: &Pickup, partial: f64) -> [f64; 3] {
    let time = (p.life as f32 + partial as f32) / LIFE_TIME as f32;
    let time = f64::from(time * time);
    let from = p.taken.position();
    std::array::from_fn(|i| {
        let target = p.target_old[i] + partial * (p.target[i] - p.target_old[i]);
        from[i] + time * (target - from[i])
    })
}

pub(super) fn guest_target(host: &super::lan::LanHost, id: u32) -> Option<[f64; 3]> {
    let m = host.models.values().find(|m| m.id == id)?;
    let (p, eye) = (m.position, m.eye_height());
    Some([p.x, (p.y + (p.y + eye)) / 2.0, p.z])
}

impl Game<'_> {
    fn collector_target(&self, collector: Collector) -> Option<[f64; 3]> {
        let (p, eye) = match collector {
            Collector::Player => {
                let p = self.player.as_ref()?;
                (p.position, f64::from(p.dimensions.eye_height))
            }
            Collector::Mob(id) => {
                let m = self.mobs.iter().find(|m| m.id == id)?;
                (m.position, m.eye_height())
            }
            Collector::Guest(id) => {
                return guest_target(self.lan.as_ref()?, id);
            }
        };
        Some([p.x, (p.y + (p.y + eye)) / 2.0, p.z])
    }
    pub(super) fn add_pickup(&mut self, taken: Taken, collector: Collector) {
        if let Some(target) = self.collector_target(collector) {
            self.add_pickup_toward(taken, collector, target);
        }
    }
    pub(super) fn add_pickup_toward(
        &mut self,
        taken: Taken,
        collector: Collector,
        target: [f64; 3],
    ) {
        self.pickups.added.push(Pickup {
            taken,
            collector,
            life: 0,
            target,
            target_old: target,
        });
    }
    pub(super) fn tick_pickups(&mut self) {
        let mut live = std::mem::take(&mut self.pickups.live);
        live.retain_mut(|p| {
            p.life += 1;
            if p.life == LIFE_TIME {
                return false;
            }
            p.target_old = p.target;
            if let Some(t) = self.collector_target(p.collector) {
                p.target = t;
            }
            true
        });
        live.append(&mut self.pickups.added);
        self.pickups.live = live;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn orb_at(x: f64, y: f64, z: f64) -> Taken {
        let at = vise_core::geometry::Vec3::new(x, y, z);
        Taken::Orb(Box::new(ExperienceOrb {
            id: 0,
            position: at,
            previous: at,
            velocity: vise_core::geometry::Vec3::new(0.0, 0.0, 0.0),
            value: 1,
            age: 0,
            on_ground: false,
            count: 1,
            ticks: 0,
        }))
    }

    #[test]
    fn flies_along_the_square_of_its_age_to_the_moving_target() {
        let mut p = Pickup {
            taken: orb_at(0.0, 64.0, 0.0),
            collector: Collector::Player,
            life: 0,
            target: [3.0, 65.0, 0.0],
            target_old: [3.0, 65.0, 0.0],
        };
        assert_eq!(position(&p, 0.0), [0.0, 64.0, 0.0]);
        let t = (0.5f32 / 3.0) * (0.5 / 3.0);
        let mid = position(&p, 0.5);
        assert!((mid[0] - 3.0 * f64::from(t)).abs() < 1e-9, "{mid:?}");
        p.life = 2;
        p.target_old = [3.0, 65.0, 0.0];
        p.target = [4.0, 65.0, 0.0];
        let end = position(&p, 1.0);
        assert!(
            (end[0] - 4.0).abs() < 1e-9 && (end[1] - 65.0).abs() < 1e-9,
            "{end:?}"
        );
    }
}
