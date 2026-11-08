use super::Game;
use vise_client::{
    hud_layout::GuiDraw,
    locator_bar::{ClientWaypoints, View},
};
use vise_protocol::packets::play::TrackedWaypoint;

const PLAYER_EYES: f64 = 1.62;

fn uuid(ints: [i32; 4]) -> u128 {
    ints.iter()
        .fold(0u128, |acc, &i| acc << 32 | u128::from(i as u32))
}

impl Game<'_> {
    pub(super) fn handle_waypoint(&mut self, packet: &TrackedWaypoint) {
        self.waypoints.apply(packet);
    }
    pub(super) fn clear_waypoints(&mut self) {
        self.waypoints = ClientWaypoints::default();
    }
    pub(super) fn track_experience_display(&mut self) {
        let progress = self.experience.progress;
        let (last, seen, _) = &mut self.experience_display;
        if progress != *last {
            *last = progress;
            if *seen {
                self.experience_display.2 = self.ticks;
            } else {
                *seen = true;
            }
        }
        if self.station.as_ref().is_some_and(|s| {
            matches!(
                s.kind,
                super::stations::Kind::Enchanting | super::stations::Kind::Anvil
            )
        }) {
            self.experience_display.2 = self.ticks;
        }
    }
    pub(super) fn experience_recent(&self) -> bool {
        let (_, _, start) = self.experience_display;
        start != 0 && start + 100 > self.ticks
    }
    fn own_uuid(&self) -> u128 {
        match &self.remote {
            Some(r) => r.profile(),
            None => self.level.player_uuid.map_or(0, uuid),
        }
    }
    fn waypoint_entity(&self, id: u128, alpha: f64) -> Option<([i32; 3], [f64; 3])> {
        let at = |p: [f64; 3], q: [f64; 3], eyes: f64| {
            let pos: [f64; 3] = std::array::from_fn(|i| q[i] + (p[i] - q[i]) * alpha);
            (p.map(|c| c.floor() as i32), [pos[0], pos[1] + eyes, pos[2]])
        };
        if let Some(r) = &self.remote {
            let entity = r.player_entity(id)?;
            let m = self.mobs.iter().find(|m| m.id == entity)?;
            let v = |v: vise_core::geometry::Vec3| [v.x, v.y, v.z];
            return Some(at(v(m.position), v(m.previous), m.eye_height()));
        }
        let host = self.lan.as_ref()?;
        let g = host.hub.guests.iter().find(|g| g.uuid == id && g.placed)?;
        Some(at(g.position, g.previous_position, PLAYER_EYES))
    }
    pub(super) fn locator_bar_draws(&self, gui_width: i32, gui_height: i32) -> Vec<GuiDraw> {
        let (Some((camera_pos, yaw, pitch, fov)), Some(p)) = (self.locator_camera, &self.player)
        else {
            return Vec::new();
        };
        let alpha = self.clock.camera_fraction();
        let (sy, cy) = (
            f64::from(yaw.to_radians()).sin(),
            f64::from(yaw.to_radians()).cos(),
        );
        let (sp, cp) = (
            f64::from(pitch.to_radians()).sin(),
            f64::from(pitch.to_radians()).cos(),
        );
        let forward = [-sy * cp, -sp, cy * cp];
        let up = [-sy * sp, cp, cy * sp];
        let focal = 1.0 / f64::from((fov / 2.0).to_radians()).tan();
        let project = |o: [f64; 3]| {
            let dot = |a: [f64; 3]| a[0] * o[0] + a[1] * o[1] + a[2] * o[2];
            let depth = dot(forward);
            let y = dot(up) * focal / depth;
            [0.0, y, if depth < 0.0 { 2.0 } else { 0.5 }]
        };
        let entity = |id: u128| self.waypoint_entity(id, alpha);
        let view = View {
            gui_width,
            gui_height,
            camera_uuid: self.own_uuid(),
            camera_entity_pos: [p.position.x, p.position.y, p.position.z],
            camera_pos,
            yaw,
            pitch,
            fov,
            project: &project,
            entity: &entity,
            styles: &self.waypoint_styles,
            tick: self.ticks,
        };
        self.waypoints.draw(&view)
    }
}
