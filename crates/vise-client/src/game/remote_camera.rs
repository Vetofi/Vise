use super::Game;
use vise_client::{
    camera::{CameraEntity, LivingCamera},
    mobs::Mob,
};

pub(super) fn camera_after_packet(
    current: Option<i32>,
    player_id: i32,
    camera_id: i32,
    in_level: bool,
) -> Option<i32> {
    if camera_id == player_id {
        None
    } else if in_level {
        Some(camera_id)
    } else {
        current
    }
}

pub(super) fn mob_camera_entity(m: &Mob) -> CameraEntity {
    let attribute =
        |name: &str, default: f64| m.kind.attributes.get(name).copied().unwrap_or(default);
    CameraEntity {
        position: [m.position.x, m.position.y, m.position.z],
        previous: [m.previous.x, m.previous.y, m.previous.z],
        x_rot: m.head_pitch,
        y_rot: m.head_yaw,
        x_rot_o: m.previous_head_pitch,
        y_rot_o: m.previous_head_yaw,
        eye_height: m.eye_height() as f32,
        living: Some(LivingCamera {
            scale: attribute("minecraft:scale", 1.0) as f32,
            camera_distance: attribute("minecraft:camera_distance", 4.0),
            sleeping: None,
            dead: None,
        }),
        mount: None,
    }
}

impl Game<'_> {
    pub(super) fn remote_set_camera(&mut self, camera_id: i32) {
        let in_level = self.mobs.iter().any(|m| m.id == camera_id as u32);
        if let Some(r) = self.remote.as_mut() {
            r.camera = camera_after_packet(r.camera, r.entity_id, camera_id, in_level);
        }
    }
    pub(super) fn remote_camera_id(&self) -> Option<u32> {
        self.remote.as_ref()?.camera.map(|id| id as u32)
    }
    pub(super) fn controls_camera(&self) -> bool {
        self.remote_camera_id().is_none()
    }
    pub(super) fn camera_mob(&self) -> Option<&Mob> {
        let id = self.remote_camera_id()?;
        self.mobs.iter().find(|m| m.id == id)
    }
    pub(super) fn spectator_click_target(&self) -> Option<i32> {
        let mob = self.mob_hit().map(|(i, d)| (self.mobs[i].id as i32, d));
        let object = self.remote_object_hit(false).map(|h| (h.id, h.distance));
        [mob, object]
            .into_iter()
            .flatten()
            .filter(|(_, d)| self.before_block(*d))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }
    pub(super) fn hides_as_camera(&self, id: u32) -> bool {
        self.camera_type.first_person() && self.remote_camera_id() == Some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::camera_after_packet;

    #[test]
    fn set_camera_follows_vanilla() {
        assert_eq!(camera_after_packet(None, 5, 9, true), Some(9));
        assert_eq!(camera_after_packet(Some(9), 5, 5, false), None);
        assert_eq!(camera_after_packet(Some(9), 5, 12, false), Some(9));
        assert_eq!(camera_after_packet(None, 5, 12, false), None);
    }
}
