use crate::blocks::BlockFacts;
use glam::DVec3;
use vise_world::registry::BlockRegistry;
use std::collections::HashMap;

pub const MAX: usize = 16;
pub const BURNING: u8 = 15;
const REACH: f64 = 96.0;

#[derive(Debug, Default)]
pub struct Luminance(HashMap<String, u8>);

impl Luminance {
    pub fn new(facts: &BlockFacts, registry: &BlockRegistry) -> Self {
        let mut map = HashMap::new();
        for (id, state) in facts.states.iter().enumerate() {
            if state.emission == 0 || *state.item != *state.name {
                continue;
            }
            if registry.default_state(&state.name).map(|d| d.0 as usize) == Some(id) {
                map.insert(state.item.to_string(), state.emission);
            }
        }
        map.insert("minecraft:lava_bucket".into(), 15);
        Self(map)
    }

    pub fn of(&self, item: &str) -> u8 {
        self.0.get(item).copied().unwrap_or(0)
    }

    #[cfg(test)]
    fn from_pairs(pairs: &[(&str, u8)]) -> Self {
        Self(pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub at: DVec3,
    pub level: u8,
}

pub fn select(sources: &[Source], camera: DVec3, out: &mut Vec<[f32; 4]>) {
    out.clear();
    let key = |s: &Source| s.at.distance_squared(camera);
    let mut near: Vec<&Source> = sources
        .iter()
        .filter(|s| s.level > 0 && key(s) <= REACH * REACH)
        .collect();
    near.sort_by(|a, b| key(a).total_cmp(&key(b)));
    out.extend(near.into_iter().take(MAX).map(|s| {
        let p = (s.at - camera).as_vec3();
        [p.x, p.y, p.z, f32::from(s.level.min(15))]
    }));
}

pub fn level_at(pos: [f32; 3], lights: &[[f32; 4]]) -> f32 {
    lights.iter().fold(0.0f32, |level, l| {
        let d =
            ((pos[0] - l[0]).powi(2) + (pos[1] - l[1]).powi(2) + (pos[2] - l[2]).powi(2)).sqrt();
        level.max(l[3] - d)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_sources_win() {
        let camera = DVec3::new(100.0, 64.0, 100.0);
        let mut sources: Vec<Source> = (0..40)
            .map(|i| Source {
                at: camera + DVec3::new(f64::from(i), 0.0, 0.0),
                level: 14,
            })
            .collect();
        sources.push(Source {
            at: camera + DVec3::new(0.5, 0.0, 0.0),
            level: 0,
        });
        sources.push(Source {
            at: camera + DVec3::new(0.0, 0.0, 200.0),
            level: 15,
        });
        let mut out = Vec::new();
        select(&sources, camera, &mut out);
        assert_eq!(out.len(), MAX);
        assert_eq!(out[0], [0.0, 0.0, 0.0, 14.0]);
        assert_eq!(out[MAX - 1][0], (MAX - 1) as f32);
        select(&[], camera, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn light_falls_off_a_level_per_block() {
        let torch = [[0.0, 0.0, 0.0, 14.0]];
        assert_eq!(level_at([0.0, 0.0, 0.0], &torch), 14.0);
        assert_eq!(level_at([3.0, 4.0, 0.0], &torch), 9.0);
        assert_eq!(level_at([20.0, 0.0, 0.0], &torch), 0.0);
        let two = [[0.0, 0.0, 0.0, 14.0], [10.0, 0.0, 0.0, 15.0]];
        assert_eq!(level_at([8.0, 0.0, 0.0], &two), 13.0);
        assert_eq!(level_at([0.0; 3], &[]), 0.0);
    }

    #[test]
    fn item_luminance() {
        let l = Luminance::from_pairs(&[("minecraft:torch", 14)]);
        assert_eq!(l.of("minecraft:torch"), 14);
        assert_eq!(l.of("minecraft:stone"), 0);
    }
}
