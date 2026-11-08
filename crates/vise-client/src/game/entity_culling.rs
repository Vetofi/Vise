use super::{Game, Vertex};
use vise_client::{
    blocks::{RenderShape, StateFacts},
    entity_culling::Culler,
};

fn opaque(facts: &StateFacts) -> bool {
    facts.render_shape == RenderShape::Model
        && facts.can_occlude
        && facts.solid_render
        && facts.full_collision
}

impl Game<'_> {
    pub(super) fn prepare_entity_culling(&mut self, camera: glam::DVec3) {
        self.pack_shadow_casters = match (&self.renderer, &self.pack_ids) {
            (Some(r), Some(_)) => r.pack_shadow_casters(),
            _ => (false, false),
        };
        let at = camera.to_array().map(|v| v.floor() as i32);
        self.entity_culler = (self.settings.vise.entity_culling
            && self.settings.resource_packs.iter().all(|id| id == "vanilla")
            && self.player.is_some()
            && !self.spectator()
            && !opaque(self.res.facts().get(self.world.block(at[0], at[1], at[2]))))
        .then(|| Culler::new(camera.to_array(), 8_192));
    }

    pub(super) fn cull_entity_vertices(&self, vertices: &mut Vec<Vertex>, start: usize) {
        let Some(culler) = &self.entity_culler else {
            return;
        };
        let hidden = culler.hidden(vertices, start, |pos| {
            let state = self.world.block(pos[0], pos[1], pos[2]);
            if !opaque(self.res.facts().get(state)) {
                return false;
            }
            let Some(y) = pos[1].checked_sub(self.world.bottom_y()) else {
                return false;
            };
            let section = (pos[0] >> 4, y.div_euclid(16), pos[2] >> 4);
            self.renderer
                .as_ref()
                .is_some_and(|r| r.has_section(section))
                && !self.world.dirty_sections.contains(&section)
                && !self.streaming.is_meshing(section)
                && !self.pending_uploads.iter().any(|(key, _)| *key == section)
        });
        if hidden && !(self.pack_shadow_casters.0 && self.hide_spans_from(start, vertices.len())) {
            vertices.truncate(start);
        }
    }
}

impl Game<'_> {
    pub(super) fn view_scale(&self) -> f64 {
        let distance = match &self.remote {
            Some(r) => self.settings.render_distance.min(r.view_distance),
            None => self.settings.render_distance,
        };
        entity_view_scale(distance, self.settings.entity_distance)
    }
}

pub(super) fn entity_view_scale(render_distance: i32, entity_distance: f32) -> f64 {
    (f64::from(render_distance) / 8.0).clamp(1.0, 2.5) * f64::from(entity_distance)
}

pub(super) fn default_range(size: glam::DVec3, view_scale: f64) -> f64 {
    let edge = (size.x + size.y + size.z) / 3.0;
    let edge = if edge.is_nan() { 1.0 } else { edge };
    edge * 64.0 * view_scale
}

impl Game<'_> {
    pub(super) fn entity_in_view(
        &self,
        camera: glam::DVec3,
        position: glam::DVec3,
        range: f64,
        lo: glam::DVec3,
        hi: glam::DVec3,
    ) -> bool {
        if (position - camera).length_squared() >= range * range {
            return false;
        }
        self.view_frustum.as_ref().is_none_or(|view| {
            view.intersects(
                (lo - camera - glam::DVec3::splat(0.5)).as_vec3(),
                (hi - camera + glam::DVec3::splat(0.5)).as_vec3(),
            )
        })
    }
}

pub(super) fn at(p: vise_core::geometry::Vec3) -> glam::DVec3 {
    glam::DVec3::new(p.x, p.y, p.z)
}

impl Game<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn standing_entity_in_view(
        &self,
        camera: glam::DVec3,
        previous: glam::DVec3,
        position: glam::DVec3,
        alpha: f64,
        width: f64,
        height: f64,
        range: f64,
        grow: f64,
    ) -> bool {
        let p = previous.lerp(position, alpha);
        let half = glam::DVec3::new(width / 2.0 + grow, grow, width / 2.0 + grow);
        let lo = p - half;
        let hi = p + glam::DVec3::new(half.x, height + grow, half.z);
        self.entity_in_view(camera, position, range, lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_scale_follows_the_render_distance() {
        assert_eq!(entity_view_scale(4, 1.0), 1.0);
        assert_eq!(entity_view_scale(8, 1.0), 1.0);
        assert_eq!(entity_view_scale(12, 1.0), 1.5);
        assert_eq!(entity_view_scale(20, 1.0), 2.5);
        assert_eq!(entity_view_scale(32, 1.0), 2.5);
        assert_eq!(entity_view_scale(12, 0.5), 0.75);
    }

    #[test]
    fn a_chicken_is_drawn_forty_eight_blocks_away() {
        let range = default_range(glam::DVec3::new(0.4, 0.7, 0.4), entity_view_scale(12, 1.0));
        assert_eq!(range, 48.0);
    }

    #[test]
    fn a_broken_bounding_box_falls_back_to_one_block() {
        let range = default_range(
            glam::DVec3::new(f64::NAN, 0.7, 0.4),
            entity_view_scale(12, 1.0),
        );
        assert_eq!(range, 96.0);
    }
}
