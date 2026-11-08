use super::{Mob, MobEvent, Species, bee};
use vise_core::geometry::trig::Mth;
use vise_core::random::RandomSource;
use std::f32::consts::PI;

pub fn client_ai_step(m: &mut Mob, out: &mut Vec<MobEvent>) {
    let species = m.species();
    if !matches!(
        species,
        Species::Enderman | Species::Endermite | Species::Blaze
    ) {
        return;
    }
    let (w, h) = (f64::from(m.width()), f64::from(m.height()));
    let p = m.position;
    if species == Species::Blaze && m.random.next_int_bound(24).unwrap_or(1) == 0 && !m.silent {
        let volume = 1.0 + m.random.next_float();
        let pitch = m.random.next_float() * 0.7 + 0.3;
        out.push(MobEvent::LocalSound(
            "entity.blaze.burn",
            [p.x + 0.5, p.y + 0.5, p.z + 0.5],
            volume,
            pitch,
        ));
    }
    for _ in 0..2 {
        let r = &mut m.random;
        let x = p.x + w * (2.0 * r.next_double() - 1.0) * 0.5;
        let y = p.y + h * r.next_double();
        let z = p.z + w * (2.0 * r.next_double() - 1.0) * 0.5;
        let (kind, at, velocity) = if species == Species::Blaze {
            ("large_smoke", [x, y, z], [0.0; 3])
        } else {
            let y = if species == Species::Enderman {
                y - 0.25
            } else {
                y
            };
            let velocity = [
                (r.next_double() - 0.5) * 2.0,
                -r.next_double(),
                (r.next_double() - 0.5) * 2.0,
            ];
            ("portal", [x, y, z], velocity)
        };
        out.push(MobEvent::ParticleAt { kind, at, velocity });
    }
}

pub fn client_particles(m: &Mob, random: &mut impl RandomSource, out: &mut Vec<MobEvent>) {
    match m.species() {
        Species::Phantom => {
            let flap = ((m.id.wrapping_mul(3).wrapping_add(m.age)) as i32 as f32
                * 7.448451
                * (PI / 180.0)
                + PI)
                .mcos();
            let width = m.width() * 1.48;
            let yaw = m.yaw * (PI / 180.0);
            let (c, s) = (yaw.mcos() * width, yaw.msin() * width);
            let h = (0.3 + flap * 0.45) * m.height() * 2.5;
            let p = m.position;
            for (dx, dz) in [(c, s), (-c, -s)] {
                out.push(MobEvent::ParticleAt {
                    kind: "mycelium",
                    at: [p.x + f64::from(dx), p.y + f64::from(h), p.z + f64::from(dz)],
                    velocity: [0.0; 3],
                });
            }
        }
        Species::Bee => {
            if !bee::has_nectar(m)
                || !bee::get(m).is_some_and(|b| b.crops_grown < 10)
                || random.next_float() >= 0.05
            {
                return;
            }
            let mut i = 0;
            while i < random.next_int_bound(2).unwrap_or(0) + 1 {
                let spread = f64::from(0.3_f32);
                let lerp = |delta: f64, from: f64, to: f64| from + delta * (to - from);
                let x = lerp(
                    random.next_double(),
                    m.position.x - spread,
                    m.position.x + spread,
                );
                let z = lerp(
                    random.next_double(),
                    m.position.z - spread,
                    m.position.z + spread,
                );
                out.push(MobEvent::ParticleAt {
                    kind: "falling_nectar",
                    at: [x, m.position.y + f64::from(m.height()) * 0.5, z],
                    velocity: [0.0; 3],
                });
                i += 1;
            }
        }
        _ => {}
    }
}
