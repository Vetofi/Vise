use super::{Game, ray_box, view_vector};
use vise_client::{
    mobs::Who,
    projectiles::{Projectile, ProjectileKind},
};
use vise_core::geometry::{Aabb, Vec3};

const PICK_RADIUS: f64 = 1.0;

pub(super) fn redirectable(p: &Projectile) -> bool {
    !p.stuck
        && matches!(
            p.kind,
            ProjectileKind::LargeFireball { .. }
                | ProjectileKind::WindCharge
                | ProjectileKind::BreezeWindCharge
        )
}

pub(super) fn aim_deflect(
    p: &mut Projectile,
    look: Option<Vec3>,
    owner: Option<Who>,
    guest: Option<u32>,
) {
    if let Some(look) = look {
        p.velocity = look;
        p.needs_sync = true;
    }
    p.owner = owner;
    p.guest = guest;
    p.acceleration = 0.1;
}

fn pick_box(p: &Projectile) -> Aabb {
    let size = match p.kind {
        ProjectileKind::LargeFireball { .. } => 1.0,
        _ => 0.3125,
    };
    let half = size / 2.0 + PICK_RADIUS;
    Aabb::new(
        Vec3::new(
            p.position.x - half,
            p.position.y - PICK_RADIUS,
            p.position.z - half,
        ),
        Vec3::new(
            p.position.x + half,
            p.position.y + size + PICK_RADIUS,
            p.position.z + half,
        ),
    )
}

impl Game<'_> {
    pub(super) fn redirectable_hit(&self) -> Option<(usize, f64)> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = if self.creative { 5.0 } else { 3.0 };
        self.creatures
            .projectiles
            .iter()
            .enumerate()
            .filter(|(_, p)| redirectable(p))
            .filter_map(|(i, p)| ray_box(eye, dir, reach, pick_box(p)).map(|t| (i, t)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    pub(super) fn attack_redirectable(&mut self) -> bool {
        let Some((i, t)) = self.redirectable_hit() else {
            return false;
        };
        if self.mob_hit().is_some_and(|(_, d)| d < t) {
            return false;
        }
        if let (Some(hit), Some(eye)) = (self.pick(), self.eye_position()) {
            let d = (0..3)
                .map(|a| (hit.point[a] - eye[a]).powi(2))
                .sum::<f64>()
                .sqrt();
            if d < t {
                return false;
            }
        }
        self.deflect(i);
        true
    }

    pub(super) fn within_interaction_range(&self, i: usize) -> bool {
        let (Some(eye), Some(p)) = (self.eye_position(), self.creatures.projectiles.get(i)) else {
            return false;
        };
        let b = pick_box(p);
        let (lo, hi) = (b.min_position(), b.max_position());
        let (lo, hi) = (
            [lo.x + PICK_RADIUS, lo.y + PICK_RADIUS, lo.z + PICK_RADIUS],
            [hi.x - PICK_RADIUS, hi.y - PICK_RADIUS, hi.z - PICK_RADIUS],
        );
        let d2: f64 = (0..3)
            .map(|a| (eye[a] - eye[a].clamp(lo[a], hi[a])).powi(2))
            .sum();
        let range = if self.creative { 5.0 } else { 3.0 } + 3.0;
        d2 < range * range
    }

    pub(super) fn owner_look(&self, owner: Option<Who>, guest: Option<u32>) -> Option<Vec3> {
        let (pitch, yaw) = match (owner?, guest) {
            (Who::Player, Some(conn)) => {
                let lan = self.lan.as_ref()?;
                let g = lan.hub.guests.iter().find(|g| g.conn == conn)?;
                (g.pitch, g.yaw)
            }
            (Who::Player, None) => {
                self.player.as_ref()?;
                (self.pitch, self.yaw)
            }
            (Who::Mob(id), _) => {
                let m = self.mobs.iter().find(|m| m.id == id)?;
                (m.head_pitch, m.yaw)
            }
        };
        Some(vise_world::view_direction::view_vector(pitch, yaw))
    }

    pub(super) fn deflect(&mut self, i: usize) {
        let dir = view_vector(self.yaw, self.pitch);
        let guest = self.lan_acting;
        let p = &mut self.creatures.projectiles[i];
        p.velocity = Vec3::new(dir[0], dir[1], dir[2]);
        p.yaw = dir[0].atan2(dir[2]).to_degrees() as f32;
        p.pitch = dir[1].atan2(dir[0].hypot(dir[2])).to_degrees() as f32;
        p.owner = Some(Who::Player);
        p.guest = guest;
        p.left_owner = false;
        p.acceleration = 0.1;
        p.needs_sync = true;
        self.reset_attack_strength();
        self.swing_arm();
        let from = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        self.audio
            .play("minecraft:entity.player.attack.nodamage", from, 1.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_ghast_fireballs_and_wind_charges_are_redirectable() {
        let at = Vec3::new(0.0, 64.0, 0.0);
        let large = Projectile::fireball(
            ProjectileKind::LargeFireball { power: 1 },
            at,
            None,
            [1.0, 0.0, 0.0],
        );
        let small = Projectile::fireball(ProjectileKind::SmallFireball, at, None, [1.0, 0.0, 0.0]);
        assert!(redirectable(&large));
        assert!(!redirectable(&small));
        let b = pick_box(&large);
        assert_eq!(b.min_position().x, -1.5);
        assert_eq!(b.max_position().y, 66.0);
    }

    fn wind_charge() -> Projectile {
        let mut p = Projectile::new(
            ProjectileKind::WindCharge,
            Vec3::new(0.0, 70.0, 0.0),
            Some(Who::Mob(7)),
        );
        p.velocity = Vec3::new(0.0, 1.5, 0.0);
        p
    }

    #[test]
    fn a_redirected_wind_charge_flies_along_the_new_owners_look() {
        let mut p = wind_charge();
        let look = Vec3::new(0.0, -1.0, 0.0);
        aim_deflect(&mut p, Some(look), Some(Who::Player), Some(3));
        assert_eq!(p.velocity, look);
        assert_eq!(p.owner, Some(Who::Player));
        assert_eq!(p.guest, Some(3));
        assert_eq!(p.acceleration, 0.1);
        assert!(p.needs_sync);
    }

    #[test]
    fn with_no_owner_here_it_keeps_its_movement_but_changes_hands() {
        let mut p = wind_charge();
        aim_deflect(&mut p, None, None, None);
        assert_eq!(p.velocity, Vec3::new(0.0, 1.5, 0.0));
        assert_eq!(p.owner, None);
        assert_eq!(p.acceleration, 0.1);
        assert!(!p.needs_sync);
    }
}
