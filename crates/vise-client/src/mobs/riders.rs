use super::{
    Mob, MobWorld, Seen, Species,
    goals::{JUMP, LOOK, MOVE},
};
use vise_core::geometry::{Aabb, Vec3};
use vise_world::nbt::{Compound, Tag};

const CONTROLLED: u8 = MOVE | JUMP | LOOK;
const BOARDING_COOLDOWN: i32 = 60;
const MAX_PASSENGERS_ON_ONE_HOGLIN: i32 = 3;

pub fn passenger_attachment(v: &Mob) -> [f64; 3] {
    let [x, y, z] = base_attachment(v);
    let rear = super::equine::rear_offset(v);
    [x + rear[0], y + rear[1], z + rear[2]]
}
fn base_attachment(v: &Mob) -> [f64; 3] {
    match v.species() {
        Species::Chicken => [0.0, 0.7, -0.1],
        Species::Spider => [0.0, 0.765, 0.0],
        Species::Ravager => [0.0, 2.2625, -0.0625],
        Species::Horse => [0.0, 1.44375, 0.0],
        Species::Donkey => [0.0, 1.1125, 0.0],
        Species::Mule => [0.0, 1.2125, 0.0],
        Species::SkeletonHorse | Species::ZombieHorse => [0.0, 1.31875, 0.0],
        Species::Pig => [0.0, 0.86875, 0.0],
        Species::Hoglin if v.is_baby() => [0.0, 0.875, 0.0],
        Species::Hoglin => [0.0, 1.49375, 0.0],
        Species::Nautilus | Species::ZombieNautilus => [0.0, 1.1375, 0.0],
        Species::Camel | Species::CamelHusk => super::camel::passenger_attachment(v),
        _ => [0.0, f64::from(v.height()), 0.0],
    }
}
pub fn vehicle_attachment(r: &Mob) -> f64 {
    use Species as S;
    match r.species() {
        S::Zombie | S::Husk | S::Drowned if r.is_baby() => 0.1875,
        S::ZombieVillager if r.is_baby() => 0.125,
        S::Piglin if r.is_baby() => 0.1875,
        S::Zombie
        | S::Husk
        | S::Drowned
        | S::ZombieVillager
        | S::ZombifiedPiglin
        | S::Piglin
        | S::PiglinBrute
        | S::Skeleton
        | S::Stray
        | S::Bogged
        | S::Parched
        | S::WitherSkeleton => 0.7,
        S::Pillager | S::Vindicator | S::Evoker | S::Illusioner => 0.6,
        _ => 0.0,
    }
}
fn rotated([x, y, z]: [f64; 3], yaw: f32) -> [f64; 3] {
    let a = -f64::from(yaw).to_radians();
    let (sin, cos) = (a.sin(), a.cos());
    [x * cos + z * sin, y, z * cos - x * sin]
}
pub fn seat(attachment: [f64; 3], rider_offset: f64, at: Vec3, yaw: f32) -> Vec3 {
    let [x, y, z] = rotated(attachment, yaw);
    Vec3::new(at.x + x, at.y + y - rider_offset, at.z + z)
}

pub fn link(mobs: &mut [Mob], rider: u32, vehicle: u32) {
    if rider == vehicle {
        return;
    }
    if let Some(m) = mobs.iter_mut().find(|m| m.id == rider) {
        m.extra.mount = Some(vehicle);
        m.extra.passenger = true;
    }
    if let Some(m) = mobs.iter_mut().find(|m| m.id == vehicle) {
        m.extra.rider = Some(rider);
    }
}
fn top_passenger(mobs: &[Mob], vehicle: u32, counter: i32) -> u32 {
    let first = mobs
        .iter()
        .find(|m| m.id == vehicle)
        .and_then(|m| m.extra.rider);
    match first {
        Some(passenger) if counter != 1 => top_passenger(mobs, passenger, counter - 1),
        _ => vehicle,
    }
}
pub fn start_riding(mobs: &mut [Mob], rider: u32, vehicle: u32) -> bool {
    let Some(r) = mobs.iter().find(|m| m.id == rider) else {
        return false;
    };
    let on_hoglin = mobs
        .iter()
        .any(|m| m.id == vehicle && m.species() == Species::Hoglin);
    let vehicle = if r.species() == Species::Piglin && r.is_baby() && on_hoglin {
        top_passenger(mobs, vehicle, MAX_PASSENGERS_ON_ONE_HOGLIN)
    } else {
        vehicle
    };
    if rider == vehicle || r.extra.mount == Some(vehicle) {
        return false;
    }
    let mut up = Some(vehicle);
    while let Some(v) = up {
        up = mobs.iter().find(|m| m.id == v).and_then(|m| m.extra.mount);
        if up == Some(rider) {
            return false;
        }
    }
    let Some(v) = mobs.iter().find(|m| m.id == vehicle) else {
        return false;
    };
    if r.extra.boarding_cooldown > 0 || v.extra.rider.is_some() || v.rider.is_some() {
        return false;
    }
    link(mobs, rider, vehicle);
    true
}
pub fn stop_riding(m: &mut Mob, at: Option<Vec3>) {
    if m.extra.mount.take().is_none() {
        return;
    }
    m.extra.passenger = false;
    m.extra.boarding_cooldown = BOARDING_COOLDOWN;
    if let Some(at) = at {
        m.position = at;
        m.previous = at;
    }
}
pub fn dismount_location(w: &MobWorld, vehicle: &Seen, width: f32, height: f32) -> Vec3 {
    let on_top = Vec3::new(
        vehicle.position.x,
        vehicle.bounds.max_position().y,
        vehicle.position.z,
    );
    if vehicle.species == Species::Piglin {
        return on_top;
    }
    let (bx, by, bz) = (
        vehicle.position.x.floor() as i32,
        vehicle.position.y.floor() as i32,
        vehicle.position.z.floor() as i32,
    );
    let facing = (f64::from(vehicle.yaw) / 90.0 + 0.5).floor() as i32;
    let (fx, fz) = [(0, 1), (-1, 0), (0, -1), (1, 0)][(facing & 3) as usize];
    let (rx, rz) = (-fz, fx);
    let offsets = [
        (rx, rz),
        (-rx, -rz),
        (rx - fx, rz - fz),
        (-rx - fx, -rz - fz),
        (fx + rx, fz + rz),
        (fx - rx, fz - rz),
        (-fx, -fz),
        (fx, fz),
    ];
    let half = f64::from(width / 2.0);
    for (dx, dz) in offsets {
        let (x, z) = (bx + dx, bz + dz);
        let floor = w.block_floor_height(x, by, z);
        if floor.is_infinite() || floor >= 1.0 {
            continue;
        }
        let at = Vec3::new(
            f64::from(x) + 0.5,
            f64::from(by) + floor,
            f64::from(z) + 0.5,
        );
        let bounds = Aabb::new(
            Vec3::new(at.x - half, at.y, at.z - half),
            Vec3::new(at.x + half, at.y + f64::from(height), at.z + half),
        );
        if w.no_collision(bounds) {
            return at;
        }
    }
    on_top
}

const RANGED_DISTANCE: f64 = 8.0;
fn ranged(s: Species) -> bool {
    matches!(
        s,
        Species::Skeleton
            | Species::Stray
            | Species::Bogged
            | Species::Parched
            | Species::Pillager
            | Species::Witch
            | Species::Piglin
    )
}
pub fn drive(m: &mut Mob, w: &MobWorld) {
    let Some(rider) = m.extra.rider else {
        m.ai.goals.disabled &= !CONTROLLED;
        return;
    };
    let seen = w.seen(rider).filter(|s| s.alive);
    let info = seen
        .and_then(|s| s.target)
        .and_then(|t| w.target_info(t))
        .filter(|t| t.alive && !(t.is_player && t.creative));
    let (Some(seen), Some(t)) = (seen, info) else {
        m.ai.goals.disabled &= !CONTROLLED;
        return;
    };
    m.ai.goals.disabled |= CONTROLLED;
    m.look_at_speed([t.position.x, t.eye_y, t.position.z], 30.0, 30.0);
    let close =
        ranged(seen.species) && m.distance_sqr(t.position) < RANGED_DISTANCE * RANGED_DISTANCE;
    if close {
        m.nav_stop();
    } else if m.ai.nav.is_done() || (m.age + m.id).is_multiple_of(10) {
        m.nav_move_to_entity(w, [t.position.x, t.position.y, t.position.z], 0, 1.0);
    }
}
pub fn mob_controlled(m: &Mob, w: &MobWorld) -> bool {
    m.extra.rider.and_then(|r| w.seen(r)).is_some_and(|r| {
        !matches!(
            r.species,
            Species::Slime | Species::MagmaCube | Species::SulfurCube
        )
    })
}
pub fn keeps_alive(m: &Mob) -> bool {
    m.extra.mount.is_some()
}

pub fn write_nbt(m: &Mob, map: &mut Compound) {
    match m.species() {
        Species::Chicken => {
            map.insert(
                "IsChickenJockey".into(),
                Tag::Byte(i8::from(m.extra.jockey)),
            );
        }
        Species::SkeletonHorse => {
            map.insert("SkeletonTrap".into(), Tag::Byte(i8::from(m.extra.trap)));
            map.insert("SkeletonTrapTime".into(), Tag::Int(m.extra.trap_time));
        }
        _ => {}
    }
}
pub fn read_nbt(m: &mut Mob, tag: &Tag) {
    let int = |key: &str| match tag.get(key) {
        Some(Tag::Int(v)) => *v,
        Some(Tag::Byte(v)) => i32::from(*v),
        Some(Tag::Short(v)) => i32::from(*v),
        _ => 0,
    };
    match m.species() {
        Species::Chicken => m.extra.jockey = int("IsChickenJockey") != 0,
        Species::SkeletonHorse => {
            m.extra.trap = int("SkeletonTrap") != 0;
            m.extra.trap_time = int("SkeletonTrapTime");
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachments_turn_with_the_vehicle() {
        let behind = [0.0, 2.2625, -0.0625];
        let [x, y, z] = rotated(behind, 0.0);
        assert!(x.abs() < 1e-9 && (z + 0.0625).abs() < 1e-9 && y == 2.2625);
        let [x, _, z] = rotated(behind, 90.0);
        assert!((x - 0.0625).abs() < 1e-9 && z.abs() < 1e-9);
    }
    #[test]
    fn archers_keep_their_distance_and_brawlers_close_in() {
        assert!(ranged(Species::Skeleton) && ranged(Species::Pillager));
        assert!(!ranged(Species::Zombie) && !ranged(Species::Vindicator));
    }
}
