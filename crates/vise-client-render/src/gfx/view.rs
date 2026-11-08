
use glam::Mat4;

const MAX_VIEWS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub view: Mat4,
    pub projection: Mat4,
    pub aspect: f32,
    pub tan_half_fov: f32,
}

impl View {
    pub fn screen(yaw: f32, pitch: f32, fov_degrees: f32, aspect: f32, far: f32) -> Self {
        Self {
            view: crate::camera::view_matrix(yaw, pitch),
            projection: super::world_projection(fov_degrees, aspect, far),
            aspect,
            tan_half_fov: (fov_degrees.to_radians() / 2.0).tan(),
        }
    }

    pub fn view_proj(&self, pose: Mat4) -> Mat4 {
        self.projection * pose * self.view
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ViewSet {
    views: [View; MAX_VIEWS],
    len: usize,
    cull: View,
}

impl ViewSet {
    pub fn single(view: View) -> Self {
        Self {
            views: [view; MAX_VIEWS],
            len: 1,
            cull: view,
        }
    }

    pub fn primary(&self) -> &View {
        &self.views[0]
    }

    pub fn views(&self) -> &[View] {
        &self.views[..self.len]
    }

    pub fn cull(&self) -> &View {
        &self.cull
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_view_proj(fov: f32, yaw: f32, pitch: f32, pose: Mat4, aspect: f32, far: f32) -> Mat4 {
        super::super::world_projection(fov, aspect, far)
            * pose
            * crate::camera::view_matrix(yaw, pitch)
    }

    fn poses() -> [Mat4; 3] {
        [
            Mat4::IDENTITY,
            Mat4::from_translation(glam::Vec3::new(0.013, -0.027, 0.0))
                * Mat4::from_rotation_z(0.021),
            Mat4::from_rotation_y(0.4) * Mat4::from_scale(glam::Vec3::new(1.1, 0.9, 1.0)),
        ]
    }

    #[test]
    fn the_screen_view_matches_the_legacy_projection_bit_for_bit() {
        for (fov, yaw, pitch, aspect, far) in [
            (70.0, 0.0, 0.0, 16.0 / 9.0, 512.0),
            (110.0, -137.25, 61.5, 21.0 / 9.0, 8192.0),
            (30.0, 359.9, -90.0, 4.0 / 3.0, 96.0),
            (82.333, 12.5, 7.75, 1.0, 1024.0),
        ] {
            let view = View::screen(yaw, pitch, fov, aspect, far);
            for pose in poses() {
                let old = legacy_view_proj(fov, yaw, pitch, pose, aspect, far);
                let new = view.view_proj(pose);
                assert_eq!(
                    old.to_cols_array().map(f32::to_bits),
                    new.to_cols_array().map(f32::to_bits),
                    "fov {fov} yaw {yaw} pitch {pitch}"
                );
            }
            assert_eq!(view.view, crate::camera::view_matrix(yaw, pitch));
            assert_eq!(
                view.tan_half_fov.to_bits(),
                (fov.to_radians() / 2.0).tan().to_bits()
            );
            assert_eq!(view.aspect.to_bits(), aspect.to_bits());
        }
    }

    #[test]
    fn a_single_view_set_draws_and_culls_with_its_view() {
        let view = View::screen(45.0, 10.0, 70.0, 16.0 / 9.0, 512.0);
        let set = ViewSet::single(view);
        assert_eq!(set.views(), &[view]);
        assert_eq!(*set.primary(), view);
        assert_eq!(*set.cull(), view);
    }
}
