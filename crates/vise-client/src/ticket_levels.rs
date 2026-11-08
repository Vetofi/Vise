use crate::fast_hash::FxHashMap;
use std::sync::Arc;

pub const ENTITY_TICKING: u8 = 31;
pub const BLOCK_TICKING: u8 = 32;
const NONE: (u8, u8) = (BLOCK_TICKING + 1, BLOCK_TICKING + 1);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sources {
    pub players: Vec<(i32, i32)>,
    pub forced: Vec<(i32, i32)>,
    pub view_distance: i32,
    pub simulation_distance: i32,
}

#[derive(Clone, Default)]
pub struct Levels(Option<Arc<Map>>);

enum Map {
    Grid {
        x0: i32,
        z0: i32,
        width: u32,
        depth: u32,
        cells: Box<[(u8, u8)]>,
    },
    Sparse(FxHashMap<(i32, i32), (u8, u8)>),
}

const GRID_CELLS: u64 = 1 << 19;

impl Map {
    fn get(&self, x: i32, z: i32) -> (u8, u8) {
        match self {
            Self::Grid {
                x0,
                z0,
                width,
                depth,
                cells,
            } => {
                let (dx, dz) = (x.wrapping_sub(*x0) as u32, z.wrapping_sub(*z0) as u32);
                if dx < *width && dz < *depth {
                    cells[(dz * width + dx) as usize]
                } else {
                    NONE
                }
            }
            Self::Sparse(map) => map.get(&(x, z)).copied().unwrap_or(NONE),
        }
    }
}

impl Levels {
    fn get(&self, x: i32, z: i32) -> Option<(u8, u8)> {
        Some(self.0.as_ref()?.get(x, z))
    }
    pub fn entity_ticking(&self, x: i32, z: i32) -> bool {
        self.get(x, z)
            .is_none_or(|(l, s)| l <= ENTITY_TICKING && s <= ENTITY_TICKING)
    }
    pub fn block_ticking(&self, x: i32, z: i32) -> bool {
        self.get(x, z)
            .is_none_or(|(l, s)| l <= BLOCK_TICKING && s <= BLOCK_TICKING)
    }
    pub fn random_ticking(&self, x: i32, z: i32) -> bool {
        self.get(x, z)
            .is_none_or(|(l, s)| l <= BLOCK_TICKING && s <= ENTITY_TICKING)
    }
    pub fn loaded_ticking(&self, x: i32, z: i32) -> bool {
        self.get(x, z).is_none_or(|(l, _)| l <= BLOCK_TICKING)
    }
    pub fn entity_ticking_at(&self, x: i32, z: i32) -> bool {
        self.entity_ticking(x >> 4, z >> 4)
    }
    pub fn block_ticking_at(&self, x: i32, z: i32) -> bool {
        self.block_ticking(x >> 4, z >> 4)
    }
}

#[derive(Default)]
pub struct ChunkTickets {
    sources: Option<Sources>,
    levels: Levels,
}

impl ChunkTickets {
    pub fn update(&mut self, mut sources: Option<Sources>) {
        if let Some(s) = sources.as_mut() {
            s.players.sort_unstable();
            s.players.dedup();
            s.forced.sort_unstable();
        }
        if sources == self.sources {
            return;
        }
        self.levels = Levels(sources.as_ref().map(|s| Arc::new(compute(s))));
        self.sources = sources;
    }
    pub fn levels(&self) -> Levels {
        self.levels.clone()
    }
}

fn reach(s: &Sources) -> (i32, i32, i32) {
    let view = s.view_distance.clamp(0, 32);
    let simulation = s.simulation_distance.clamp(0, 32);
    let player_sim = (i32::from(ENTITY_TICKING) - simulation).max(0);
    let reach = (view + 1).max(i32::from(BLOCK_TICKING) - player_sim);
    (view, player_sim, reach)
}

fn compute(s: &Sources) -> Map {
    let (view, player_sim, reach) = reach(s);
    let boxes = s
        .players
        .iter()
        .map(|&p| (p, reach))
        .chain(s.forced.iter().map(|&f| (f, 1)));
    let Some((x0, z0, x1, z1)) = boxes.fold(None, |b, ((x, z), r)| {
        let (lo_x, lo_z, hi_x, hi_z) = (
            i64::from(x) - i64::from(r),
            i64::from(z) - i64::from(r),
            i64::from(x) + i64::from(r),
            i64::from(z) + i64::from(r),
        );
        Some(match b {
            None => (lo_x, lo_z, hi_x, hi_z),
            Some((a, b, c, d)) => (lo_x.min(a), lo_z.min(b), hi_x.max(c), hi_z.max(d)),
        })
    }) else {
        return Map::Sparse(FxHashMap::default());
    };
    let (width, depth) = ((x1 - x0 + 1) as u64, (z1 - z0 + 1) as u64);
    if width * depth > GRID_CELLS {
        return Map::Sparse(compute_sparse(s));
    }
    let (x0, z0, width, depth) = (x0 as i32, z0 as i32, width as u32, depth as u32);
    let mut cells = vec![NONE; (width * depth) as usize].into_boxed_slice();
    let mut lower = |x: i32, z: i32, loading: i32, sim: i32| {
        let i = ((z - z0) as u32 * width + (x - x0) as u32) as usize;
        let e = &mut cells[i];
        e.0 = e.0.min(loading.min(i32::from(NONE.0)) as u8);
        e.1 = e.1.min(sim.min(i32::from(NONE.1)) as u8);
    };
    for &(px, pz) in &s.players {
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let d = dx.abs().max(dz.abs());
                lower(
                    px + dx,
                    pz + dz,
                    i32::from(ENTITY_TICKING) + (d - view).max(0),
                    player_sim + d,
                );
            }
        }
    }
    for &(fx, fz) in &s.forced {
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let level = i32::from(ENTITY_TICKING) + dx.abs().max(dz.abs());
                lower(fx + dx, fz + dz, level, level);
            }
        }
    }
    Map::Grid {
        x0,
        z0,
        width,
        depth,
        cells,
    }
}

fn compute_sparse(s: &Sources) -> FxHashMap<(i32, i32), (u8, u8)> {
    let (view, player_sim, reach) = reach(s);
    let mut levels: FxHashMap<(i32, i32), (u8, u8)> = FxHashMap::default();
    let mut lower = |key, loading: i32, sim: i32| {
        let e = levels.entry(key).or_insert(NONE);
        e.0 = e.0.min(loading.min(i32::from(NONE.0)) as u8);
        e.1 = e.1.min(sim.min(i32::from(NONE.1)) as u8);
    };
    for &(px, pz) in &s.players {
        for dz in -reach..=reach {
            for dx in -reach..=reach {
                let d = dx.abs().max(dz.abs());
                lower(
                    (px + dx, pz + dz),
                    i32::from(ENTITY_TICKING) + (d - view).max(0),
                    player_sim + d,
                );
            }
        }
    }
    for &(fx, fz) in &s.forced {
        for dz in -1i32..=1 {
            for dx in -1i32..=1 {
                let level = i32::from(ENTITY_TICKING) + dx.abs().max(dz.abs());
                lower((fx + dx, fz + dz), level, level);
            }
        }
    }
    levels.retain(|_, l| *l != NONE);
    levels
}

#[cfg(test)]
mod tests {
    use super::*;

    fn levels(players: &[(i32, i32)], forced: &[(i32, i32)], view: i32, sim: i32) -> Levels {
        let mut t = ChunkTickets::default();
        t.update(Some(Sources {
            players: players.to_vec(),
            forced: forced.to_vec(),
            view_distance: view,
            simulation_distance: sim,
        }));
        t.levels()
    }

    #[test]
    fn simulation_distance_inside_the_view() {
        let l = levels(&[(0, 0)], &[], 8, 4);
        assert!(l.entity_ticking(4, -4) && !l.entity_ticking(5, 0));
        assert!(l.random_ticking(-4, 4) && !l.random_ticking(0, 5));
        assert!(l.block_ticking(5, 5) && !l.block_ticking(6, 0));
        assert!(l.loaded_ticking(9, 9) && !l.loaded_ticking(10, 0));
    }

    #[test]
    fn view_distance_inside_the_simulation() {
        let l = levels(&[(0, 0)], &[], 4, 10);
        assert!(l.entity_ticking(4, 0) && !l.entity_ticking(5, 0));
        assert!(l.random_ticking(5, 0) && !l.random_ticking(6, 0));
        assert!(l.block_ticking(5, 5) && !l.block_ticking(6, 0));
    }

    #[test]
    fn every_player_and_forced_chunk() {
        let l = levels(&[(0, 0), (40, 0)], &[(-100, 7)], 4, 4);
        assert!(l.entity_ticking(44, 0) && l.entity_ticking(0, -4));
        assert!(!l.entity_ticking(20, 0));
        assert!(l.entity_ticking(-100, 7) && !l.entity_ticking(-101, 7));
        assert!(l.block_ticking(-101, 8) && !l.block_ticking(-102, 7));
        assert!(!l.random_ticking(-101, 7));
        assert!(l.entity_ticking_at(-1600, 112) && !l.block_ticking_at(-1617, 112));
    }

    #[test]
    fn no_tickets_tick_nothing_and_no_sources_everything() {
        let none = levels(&[], &[], 10, 10);
        assert!(!none.block_ticking(0, 0) && !none.entity_ticking(0, 0));
        let everywhere = ChunkTickets::default().levels();
        assert!(everywhere.random_ticking(1000, -1000));
    }

    #[test]
    fn rebuilt_only_when_sources_change() {
        let mut t = ChunkTickets::default();
        let s = |players: Vec<(i32, i32)>| {
            Some(Sources {
                players,
                forced: Vec::new(),
                view_distance: 2,
                simulation_distance: 2,
            })
        };
        t.update(s(vec![(0, 0), (5, 5)]));
        let first = t.levels();
        t.update(s(vec![(5, 5), (0, 0)]));
        assert!(Arc::ptr_eq(
            first.0.as_ref().unwrap(),
            t.levels().0.as_ref().unwrap()
        ));
        t.update(s(vec![(1, 0), (5, 5)]));
        assert!(t.levels().entity_ticking(3, 0) && !first.entity_ticking(3, 0));
        t.update(None);
        assert!(t.levels().0.is_none());
    }

    #[test]
    fn grid_matches_the_map() {
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut next = |n: i32| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % (2 * n as u64 + 1)) as i32 - n
        };
        for case in 0..200 {
            let spread = [3, 20, 80][case % 3];
            let players: Vec<_> = (0..1 + case % 12)
                .map(|_| (next(spread), next(spread)))
                .collect();
            let forced: Vec<_> = (0..case % 4)
                .map(|_| (next(spread), next(spread)))
                .collect();
            let s = Sources {
                players,
                forced,
                view_distance: 2 + next(12).abs(),
                simulation_distance: 2 + next(12).abs(),
            };
            let Map::Grid { .. } = compute(&s) else {
                panic!("case {case} fell back to the map");
            };
            let (grid, map) = (compute(&s), Map::Sparse(compute_sparse(&s)));
            let r = spread + 50;
            for z in -r..=r {
                for x in -r..=r {
                    assert_eq!(grid.get(x, z), map.get(x, z), "case {case} at ({x}, {z})");
                }
            }
        }
    }

    #[test]
    fn far_apart_tickets_use_the_map() {
        let l = levels(
            &[(0, 0), (100_000, -100_000)],
            &[(i32::MAX - 2, i32::MIN + 2)],
            10,
            10,
        );
        assert!(matches!(**l.0.as_ref().unwrap(), Map::Sparse(_)));
        assert!(l.entity_ticking(100_010, -100_010) && !l.block_ticking(50_000, 0));
        assert!(l.entity_ticking(i32::MAX - 2, i32::MIN + 2));
    }

    #[test]
    #[ignore]
    fn levels_cost() {
        for players in [1usize, 10, 25, 50] {
            let at: Vec<_> = (0..players)
                .map(|i| {
                    let a = i as f64 * 2.399;
                    let d = if i % 5 == 0 { 30.0 } else { 3.0 };
                    ((a.cos() * d) as i32, (a.sin() * d) as i32)
                })
                .collect();
            let s = Sources {
                players: at.clone(),
                forced: Vec::new(),
                view_distance: 10,
                simulation_distance: 10,
            };
            let rounds = 200;
            let t = std::time::Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(compute_sparse(std::hint::black_box(&s)));
            }
            let map_build = t.elapsed().as_secs_f64() * 1e3 / rounds as f64;
            let t = std::time::Instant::now();
            for _ in 0..rounds {
                std::hint::black_box(compute(std::hint::black_box(&s)));
            }
            let grid_build = t.elapsed().as_secs_f64() * 1e3 / rounds as f64;
            let (map, grid) = (Map::Sparse(compute_sparse(&s)), compute(&s));
            let ask = |m: &Map| {
                let t = std::time::Instant::now();
                let mut n = 0u32;
                for _ in 0..rounds {
                    for &(px, pz) in &at {
                        for z in pz - 12..=pz + 12 {
                            for x in px - 12..=px + 12 {
                                n +=
                                    u32::from(std::hint::black_box(m.get(x, z)).0 <= BLOCK_TICKING);
                            }
                        }
                    }
                }
                std::hint::black_box(n);
                t.elapsed().as_secs_f64() * 1e3 / rounds as f64
            };
            println!(
                "{players} players: build {map_build:.3} -> {grid_build:.3} ms, {} lookups {:.3} -> {:.3} ms",
                players * 625,
                ask(&map),
                ask(&grid)
            );
        }
    }
}
