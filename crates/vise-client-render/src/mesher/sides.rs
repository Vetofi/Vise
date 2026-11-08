use super::TerrainVertex;

const BUCKETS: [Bucket; 7] = [
    Bucket::Side(0, true),
    Bucket::Side(2, true),
    Bucket::Other,
    Bucket::Side(1, true),
    Bucket::Side(2, false),
    Bucket::Side(0, false),
    Bucket::Side(1, false),
];
const OTHER: usize = 2;
const MARGIN: f32 = 1.0 / 16.0;

#[derive(Clone, Copy)]
enum Bucket {
    Side(usize, bool),
    Other,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sides {
    ends: [u32; 7],
    planes: [f32; 7],
}

impl Sides {
    pub fn unsorted(quads: u32) -> Self {
        let mut ends = [0; 7];
        ends[OTHER..].fill(quads);
        Self {
            ends,
            planes: [0.0; 7],
        }
    }

    pub fn quads(&self) -> u32 {
        self.ends[6]
    }

    pub fn visible(&self, eye: [f32; 3], mut draw: impl FnMut(u32, u32)) {
        let mut start = 0;
        let mut run: Option<u32> = None;
        for (i, bucket) in BUCKETS.iter().enumerate() {
            let end = self.ends[i];
            if end == start {
                continue;
            }
            let seen = match *bucket {
                Bucket::Side(axis, true) => eye[axis] > self.planes[i] - MARGIN,
                Bucket::Side(axis, false) => eye[axis] < self.planes[i] + MARGIN,
                Bucket::Other => true,
            };
            match (seen, run) {
                (true, None) => run = Some(start),
                (false, Some(first)) => {
                    draw(first, start - first);
                    run = None;
                }
                _ => {}
            }
            start = end;
        }
        if let Some(first) = run {
            draw(first, start - first);
        }
    }
}

fn bucket(quad: &[TerrainVertex; 4]) -> usize {
    let p = quad.map(|v| v.pos);
    for axis in 0..3 {
        if p.iter().any(|v| v[axis] != p[0][axis]) {
            continue;
        }
        let (u, w) = ((axis + 1) % 3, (axis + 2) % 3);
        let area = |a: usize, b: usize, c: usize| {
            (p[b][u] - p[a][u]) * (p[c][w] - p[a][w]) - (p[b][w] - p[a][w]) * (p[c][u] - p[a][u])
        };
        let (first, second) = (area(0, 1, 2), area(0, 2, 3));
        let positive = first >= 0.0 && second >= 0.0;
        let negative = first <= 0.0 && second <= 0.0;
        if positive == negative {
            return OTHER;
        }
        return BUCKETS
            .iter()
            .position(|b| matches!(*b, Bucket::Side(a, s) if a == axis && s == positive))
            .expect("every side has a bucket");
    }
    OTHER
}

pub fn sort(solid: &mut Vec<TerrainVertex>) -> Sides {
    let (quads, rest) = solid.as_chunks::<4>();
    debug_assert!(rest.is_empty(), "solid vertices come in quads");
    let buckets: Vec<u8> = quads.iter().map(|q| bucket(q) as u8).collect();
    let mut counts = [0u32; 7];
    let mut planes = [0f32; 7];
    for (quad, &b) in quads.iter().zip(&buckets) {
        let b = usize::from(b);
        if let Bucket::Side(axis, positive) = BUCKETS[b] {
            let p = quad[0].pos[axis];
            planes[b] = match (counts[b], positive) {
                (0, _) => p,
                (_, true) => planes[b].min(p),
                (_, false) => planes[b].max(p),
            };
        }
        counts[b] += 1;
    }
    let mut ends = [0u32; 7];
    let mut total = 0;
    for (end, count) in ends.iter_mut().zip(counts) {
        total += count;
        *end = total;
    }
    if counts.iter().filter(|&&c| c > 0).count() > 1 {
        let mut next: [u32; 7] = std::array::from_fn(|b| ends[b] - counts[b]);
        let mut sorted = vec![quads[0]; quads.len()];
        for (quad, &b) in quads.iter().zip(&buckets) {
            let slot = &mut next[usize::from(b)];
            sorted[*slot as usize] = *quad;
            *slot += 1;
        }
        *solid = sorted.into_flattened();
    }
    Sides { ends, planes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quad(corners: [[f32; 3]; 4], layer: u16) -> [TerrainVertex; 4] {
        corners.map(|p| TerrainVertex::new(p, [0.0; 2], layer, 0, 0))
    }

    fn cube(origin: [f32; 3], layer: u16) -> Vec<TerrainVertex> {
        let [x, y, z] = origin;
        let (a, b, c) = (x + 1.0, y + 1.0, z + 1.0);
        [
            [[a, y, z], [a, b, z], [a, b, c], [a, y, c]],
            [[x, y, z], [x, y, c], [x, b, c], [x, b, z]],
            [[x, b, z], [x, b, c], [a, b, c], [a, b, z]],
            [[x, y, z], [a, y, z], [a, y, c], [x, y, c]],
            [[x, y, c], [a, y, c], [a, b, c], [x, b, c]],
            [[x, y, z], [x, b, z], [a, b, z], [a, y, z]],
        ]
        .into_iter()
        .flat_map(|q| quad(q, layer))
        .collect()
    }

    fn runs(sides: &Sides, eye: [f32; 3]) -> Vec<(u32, u32)> {
        let mut out = Vec::new();
        sides.visible(eye, |first, count| out.push((first, count)));
        out
    }

    fn front_facing(solid: &[TerrainVertex], eye: [f32; 3]) -> Vec<u16> {
        let sub = |a: [f32; 3], b: [f32; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        solid
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|q| {
                [[0, 1, 2], [0, 2, 3]].iter().any(|t| {
                    let (e1, e2) = (sub(q[t[1]].pos, q[t[0]].pos), sub(q[t[2]].pos, q[t[0]].pos));
                    let n = [
                        e1[1] * e2[2] - e1[2] * e2[1],
                        e1[2] * e2[0] - e1[0] * e2[2],
                        e1[0] * e2[1] - e1[1] * e2[0],
                    ];
                    let to_eye = sub(eye, q[t[0]].pos);
                    n[0] * to_eye[0] + n[1] * to_eye[1] + n[2] * to_eye[2] > 0.0
                })
            })
            .map(|q| q[0].layer)
            .collect()
    }

    #[test]
    fn cube_faces_go_to_their_buckets_in_order() {
        let mut solid = cube([3.0, 4.0, 5.0], 0);
        for (i, v) in solid.iter_mut().enumerate() {
            v.layer = (i / 4) as u16;
        }
        let sides = sort(&mut solid);
        assert_eq!(sides.ends, [1, 2, 2, 3, 4, 5, 6]);
        let layers: Vec<u16> = solid.iter().step_by(4).map(|v| v.layer).collect();
        assert_eq!(layers, [0, 4, 2, 5, 1, 3]);
        assert_eq!(sides.planes[0], 4.0);
        assert_eq!(sides.planes[5], 3.0);
        assert_eq!(sides.planes[3], 5.0);
    }

    #[test]
    fn the_drawn_runs_hold_every_quad_that_faces_the_eye() {
        let mut solid = Vec::new();
        let mut layer = 0;
        for origin in [
            [0.0, 0.0, 0.0],
            [7.0, 3.0, 12.0],
            [15.0, 15.0, 15.0],
            [2.0, 9.0, 4.0],
        ] {
            for q in cube(origin, 0).chunks(4) {
                solid.extend(q.iter().map(|v| TerrainVertex { layer, ..*v }));
                layer += 1;
            }
        }
        let slope = [
            [1.0, 1.0, 1.0],
            [2.0, 1.0, 1.0],
            [2.0, 2.0, 2.0],
            [1.0, 2.0, 2.0],
        ];
        let bowtie = [
            [5.0, 1.0, 1.0],
            [5.0, 2.0, 2.0],
            [5.0, 1.0, 2.0],
            [5.0, 2.0, 1.0],
        ];
        for corners in [slope, bowtie] {
            solid.extend(quad(corners, layer));
            layer += 1;
        }
        let sides = sort(&mut solid);
        assert_eq!(sides.quads(), u32::from(layer));
        let eyes = [-20.0f32, -0.5, 0.0, 3.5, 8.0, 15.99, 16.0, 30.0];
        for &x in &eyes {
            for &y in &eyes {
                for &z in &eyes {
                    let eye = [x, y, z];
                    let mut drawn = Vec::new();
                    for (first, count) in runs(&sides, eye) {
                        drawn.extend(
                            solid[first as usize * 4..(first + count) as usize * 4]
                                .iter()
                                .step_by(4)
                                .map(|v| v.layer),
                        );
                    }
                    for layer in front_facing(&solid, eye) {
                        assert!(drawn.contains(&layer), "{eye:?} misses quad {layer}");
                    }
                }
            }
        }
    }

    #[test]
    fn runs_merge_across_hidden_and_empty_buckets() {
        let mut solid = cube([0.0; 3], 0);
        let sides = sort(&mut solid);
        assert_eq!(runs(&sides, [8.0, 8.0, 8.0]), [(0, 3)]);
        assert_eq!(runs(&sides, [-8.0, -8.0, -8.0]), [(3, 3)]);
        assert_eq!(runs(&sides, [-8.0, 8.0, 8.0]), [(1, 2), (4, 1)]);
        assert!(runs(&sides, [0.5; 3]).is_empty());
        assert_eq!(runs(&sides, [0.5, 1.0, 0.5]), [(2, 1)]);
    }

    #[test]
    fn unsorted_meshes_draw_everything() {
        let sides = Sides::unsorted(9);
        assert_eq!(sides.quads(), 9);
        assert_eq!(runs(&sides, [-100.0; 3]), [(0, 9)]);
        assert!(runs(&Sides::unsorted(0), [0.0; 3]).is_empty());
    }

    #[test]
    fn one_bucket_keeps_the_mesh_as_it_is() {
        let mut solid: Vec<_> = (0..3)
            .flat_map(|i| {
                quad(
                    [
                        [1.0, 0.0, 0.0],
                        [1.0, 1.0, 0.0],
                        [1.0, 1.0, 1.0],
                        [1.0, 0.0, 1.0],
                    ],
                    i,
                )
            })
            .collect();
        let before = solid.clone();
        let sides = sort(&mut solid);
        assert_eq!(sides.ends, [3; 7]);
        assert_eq!(
            solid.iter().map(|v| v.layer).collect::<Vec<_>>(),
            before.iter().map(|v| v.layer).collect::<Vec<_>>()
        );
    }
}
