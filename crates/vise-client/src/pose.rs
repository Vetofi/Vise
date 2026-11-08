use crate::{
    Result,
    motion::{MotionWorld, PlayerMotion},
};
use vise_core::geometry::{Aabb, Vec3};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pose {
    #[default]
    Standing,
    Crouching,
    Swimming,
    FallFlying,
    SpinAttack,
    Sleeping,
}
impl Pose {
    pub fn from_id(id: i32) -> Self {
        match id {
            1 => Self::FallFlying,
            2 => Self::Sleeping,
            3 => Self::Swimming,
            4 => Self::SpinAttack,
            5 => Self::Crouching,
            _ => Self::Standing,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Standing => "STANDING",
            Self::Crouching => "CROUCHING",
            Self::Swimming => "SWIMMING",
            Self::FallFlying => "FALL_FLYING",
            Self::SpinAttack => "SPIN_ATTACK",
            Self::Sleeping => "SLEEPING",
        }
    }
    pub fn dimensions(self, scale: f32) -> Dimensions {
        let (width, height, eye_height) = match self {
            Self::Standing => (0.6 * scale, 1.8 * scale, 1.62 * scale),
            Self::Crouching => (0.6 * scale, 1.5 * scale, 1.27 * scale),
            Self::Swimming | Self::FallFlying | Self::SpinAttack => {
                (0.6 * scale, 0.6 * scale, 0.4 * scale)
            }
            Self::Sleeping => (0.2, 0.2, 0.2),
        };
        Dimensions {
            width,
            height,
            eye_height,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Dimensions {
    pub width: f32,
    pub height: f32,
    pub eye_height: f32,
}
impl Dimensions {
    pub fn bounds(self, position: Vec3) -> Aabb {
        let half = f64::from(self.width / 2.0);
        Aabb::new(
            Vec3::new(position.x - half, position.y, position.z - half),
            Vec3::new(
                position.x + half,
                position.y + f64::from(self.height),
                position.z + half,
            ),
        )
    }
}
impl PlayerMotion {
    pub fn count_down_server_spin(&mut self) {
        if self.auto_spin_attack_ticks > 0 {
            self.auto_spin_attack_ticks -= 1;
            if self.auto_spin_attack_ticks <= 0 {
                self.auto_spin_attack_dmg = 0.0;
            }
        }
    }
    pub fn dimensions_for_pose(&mut self, pose: Pose) -> Result<Dimensions> {
        Ok(pose.dimensions(self.attribute("minecraft:scale")? as f32))
    }
    pub fn refresh_dimensions(&mut self) -> Result<()> {
        self.dimensions = self.dimensions_for_pose(self.pose)?;
        Ok(())
    }
    pub fn set_pose(&mut self, pose: Pose) -> Result<()> {
        if self.pose != pose {
            self.pose = pose;
            self.refresh_dimensions()?;
        }
        Ok(())
    }
    pub fn set_sleeping(&mut self, sleeping: bool) -> Result<()> {
        if self.sleeping != sleeping {
            self.sleeping = sleeping;
            self.set_pose(if sleeping {
                Pose::Sleeping
            } else {
                Pose::Standing
            })?;
        }
        Ok(())
    }
    pub fn moving_slowly(&self) -> bool {
        self.local_crouching || (self.pose == Pose::Swimming && !self.fluids.was_touching_water)
    }
}
impl MotionWorld<'_, '_, '_> {
    pub fn can_fit_pose(&mut self, pose: Pose) -> Result<bool> {
        let bounds = self
            .player
            .dimensions_for_pose(pose)?
            .bounds(self.player.position);
        self.no_player_collision(bounds.deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7)))
    }
    pub fn update_local_crouching(&mut self) -> Result<()> {
        self.player.local_crouching = !self.player.flying
            && !self.player.swimming
            && self.can_fit_pose(Pose::Crouching)?
            && (self.player.shift_key_down || !self.can_fit_pose(Pose::Standing)?);
        Ok(())
    }
    pub fn update_player_pose(&mut self) -> Result<()> {
        if !self.can_fit_pose(Pose::Swimming)? {
            return Ok(());
        }
        let desired = if self.player.sleeping {
            Pose::Sleeping
        } else if self.player.swimming {
            Pose::Swimming
        } else if self.player.fall_flying {
            Pose::FallFlying
        } else if self.player.auto_spin_attack_ticks > 0 {
            Pose::SpinAttack
        } else if self.player.shift_key_down && !self.player.flying {
            Pose::Crouching
        } else {
            Pose::Standing
        };
        let pose = if self.player.spectator || self.can_fit_pose(desired)? {
            desired
        } else if self.can_fit_pose(Pose::Crouching)? {
            Pose::Crouching
        } else {
            Pose::Swimming
        };
        self.player.set_pose(pose)
    }
    pub fn server_player_pose_tick(&mut self) -> Result<()> {
        self.player.count_down_server_spin();
        self.player.refresh_attributes()?;
        self.update_player_pose()
    }
    pub fn passenger_pose_tick(&mut self) -> Result<()> {
        self.player.swimming = false;
        self.update_player_pose()
    }
}
