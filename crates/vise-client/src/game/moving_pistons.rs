use super::Game;
use vise_client::mesher::Vertex;
use vise_world::{piston_shape::MovingPistonState, registry::BlockStateId};

impl Game<'_> {
    pub(super) fn moving_piston_geometry(
        &self,
        camera: glam::DVec3,
        alpha: f64,
        out: &mut Vec<Vertex>,
    ) {
        let partial = alpha as f32;
        let live = self
            .world
            .moving_pistons
            .iter()
            .map(|&(p, s)| (p, s, false));
        let finished = self
            .world
            .moving_piston_fallback
            .iter()
            .filter(|(p, _)| self.section_mesh_pending(*p))
            .map(|&(p, s)| (p, s, true));
        let states: Vec<_> = live.chain(finished).collect();
        for (p, piston, placed) in states {
            let Some((block, base)) = self.moving_piston_blocks(piston, partial) else {
                continue;
            };
            let step = piston.movement_direction().normal();
            let at = vise_core::position::BlockPos::new(p[0], p[1], p[2]);
            let from = if placed {
                at
            } else {
                vise_core::position::BlockPos::new(
                    p[0] - step[0],
                    p[1] - step[1],
                    p[2] - step[2],
                )
            };
            let offset = piston.interpolated_offset(partial);
            let corner = glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
            let moved = corner + glam::Vec3::from_array(offset).as_dvec3();
            self.moving_block_vertices(block, from, moved - camera, out);
            if let Some(base) = base {
                self.moving_block_vertices(base, at, corner - camera, out);
            }
        }
    }
    pub(super) fn section_mesh_pending(&self, p: [i32; 3]) -> bool {
        let Some(y) = p[1].checked_sub(self.world.layout().min_y()) else {
            return false;
        };
        let section = (p[0] >> 4, y.div_euclid(16), p[2] >> 4);
        self.world.dirty_sections.contains(&section)
            || self.streaming.is_meshing(section)
            || self.pending_uploads.iter().any(|(key, _)| *key == section)
    }
    fn moving_piston_blocks(
        &self,
        piston: MovingPistonState,
        partial: f32,
    ) -> Option<(BlockStateId, Option<BlockStateId>)> {
        let facts = self.res.facts();
        if facts.is_air(piston.moved_state) {
            return None;
        }
        let progress = piston.interpolated_progress(partial);
        let name = &*facts.get(piston.moved_state).name;
        if name == "minecraft:piston_head" {
            let short = if progress <= 0.5 { "true" } else { "false" };
            return Some((
                self.with_props(piston.moved_state, &[("short", short)])?,
                None,
            ));
        }
        if piston.source && !piston.extending {
            let registry = &self.pack.assets.registry;
            let facing = registry
                .state(piston.moved_state)?
                .properties
                .get("facing")?
                .clone();
            let kind = if name == "minecraft:sticky_piston" {
                "sticky"
            } else {
                "normal"
            };
            let short = if progress >= 0.5 { "true" } else { "false" };
            let head = self.with_props(
                registry.default_state("minecraft:piston_head")?,
                &[("type", kind), ("facing", &facing), ("short", short)],
            )?;
            let base = self.with_props(piston.moved_state, &[("extended", "true")])?;
            return Some((head, Some(base)));
        }
        Some((piston.moved_state, None))
    }
    fn moving_block_vertices(
        &self,
        state: BlockStateId,
        seed: vise_core::position::BlockPos,
        at: glam::DVec3,
        out: &mut Vec<Vertex>,
    ) {
        let chunks = &self.world.chunks;
        let (cx, cz) = (seed.x >> 4, seed.z >> 4);
        let region: [[Option<&vise_client::chunk::Chunk>; 3]; 3] = std::array::from_fn(|dz| {
            std::array::from_fn(|dx| {
                chunks
                    .get(&(cx + dx as i32 - 1, cz + dz as i32 - 1))
                    .map(|c| &**c)
            })
        });
        let tint = self.pack.context.tables.block_tints(
            state,
            [seed.x, seed.y, seed.z],
            &region,
            self.res.biomes(),
            self.streaming.biome_zoom,
            self.streaming.biome_blend,
        );
        let tables = &self.pack.context.tables;
        let light = |quad: &vise_assets::bake::ModelQuad| {
            let coords = tables.flat_light(state, [seed.x, seed.y, seed.z], quad, |p| {
                self.world.light(p[0], p[1], p[2])
            });
            (coords >> 16 & 0xF0) | ((coords & 0xF0) << 8)
        };
        self.tinted_block_model_vertices(
            state,
            vise_core::random::position_seed(seed),
            glam::Mat4::from_translation(at.as_vec3()),
            light,
            false,
            tint,
            out,
        );
    }
}
