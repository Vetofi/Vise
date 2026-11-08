use vise_core::random::{LegacyRandom, RandomSource};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Column {
    pub x: i32,
    pub z: i32,
    pub bottom_y: i32,
    pub top_y: i32,
    pub u_offset: f32,
    pub v_offset: f32,
    pub light_coords: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnVertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub color: u32,
    pub light_coords: i32,
}

pub fn column_seed(x: i32, z: i32) -> i32 {
    (x.wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971)))
        ^ (z.wrapping_mul(z)
            .wrapping_mul(418_711)
            .wrapping_add(z.wrapping_mul(13761)))
}

pub fn column_random(x: i32, z: i32) -> LegacyRandom {
    LegacyRandom::new(i64::from(column_seed(x, z)))
}

#[derive(Clone, Copy, Debug)]
pub struct Parameters {
    pub rain_speed: f32,
    pub rain_tick_offset: f32,
    pub snow_base: [f64; 2],
    pub snow_drift: [f32; 2],
}
impl Parameters {
    pub fn new(x: i32, z: i32) -> Self {
        let rain_speed = 3.0 + column_random(x, z).next_float();
        let mut random = column_random(x, z);
        let u = random.next_double();
        let du = random.next_gaussian() as f32;
        let v = random.next_double();
        let dv = random.next_gaussian() as f32;
        Self {
            rain_speed,
            rain_tick_offset: (x
                .wrapping_mul(x)
                .wrapping_mul(3121)
                .wrapping_add(x.wrapping_mul(45_238_971))
                .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
                .wrapping_add(z.wrapping_mul(13761))
                & 255) as f32,
            snow_base: [u, v],
            snow_drift: [du, dv],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn column(
        self,
        snow: bool,
        ticks: i64,
        x: i32,
        bottom_y: i32,
        top_y: i32,
        z: i32,
        light_coords: i32,
        partial: f32,
    ) -> Column {
        let time = (ticks & 131_071) as f32 + partial;
        let (u_offset, v_offset, light_coords) = if snow {
            let u = (self.snow_base[0] + f64::from(time * 0.01 * self.snow_drift[0])) as f32;
            let v = (self.snow_base[1] + f64::from(time * self.snow_drift[1] * 0.001)) as f32;
            let fall = -((ticks & 511) as f32 + partial) / 512.0;
            let (block, sky) = (light_coords >> 4 & 15, light_coords >> 20 & 15);
            (
                u,
                fall + v,
                ((block * 3 + 15) / 4) << 4 | ((sky * 3 + 15) / 4) << 20,
            )
        } else {
            let offset = -(((ticks & 131_071) as f32 + self.rain_tick_offset) + partial) / 32.0
                * self.rain_speed;
            (0.0, offset % 32.0, light_coords)
        };
        Column {
            x,
            z,
            bottom_y,
            top_y,
            u_offset,
            v_offset,
            light_coords,
        }
    }
}

pub struct ParameterCache {
    entries: Box<[Option<((i32, i32), Parameters)>]>,
}
impl Default for ParameterCache {
    fn default() -> Self {
        Self {
            entries: vec![None; 32 * 32].into_boxed_slice(),
        }
    }
}
impl ParameterCache {
    pub fn get(&mut self, x: i32, z: i32) -> Parameters {
        let index = ((z as u32 & 31) * 32 + (x as u32 & 31)) as usize;
        let entry = &mut self.entries[index];
        if let Some((key, parameters)) = entry
            && *key == (x, z)
        {
            return *parameters;
        }
        let parameters = Parameters::new(x, z);
        *entry = Some(((x, z), parameters));
        parameters
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub uv: [f32; 2],
    pub rect: [f32; 4],
    pub animation: [f32; 4],
    pub bases: [u32; 4],
    pub color: u32,
    pub light: u32,
    pub snow: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Time {
    pub values: [f32; 4],
}
impl Time {
    pub fn new(ticks: i64, partial: f32) -> Self {
        Self {
            values: [(ticks & 131_071) as f32, (ticks & 511) as f32, partial, 0.0],
        }
    }
}

pub fn vertices(
    column: &Column,
    parameters: Parameters,
    snow: bool,
    camera: [f64; 3],
    radius: i32,
    intensity: f32,
    rect: [f32; 4],
) -> [Vertex; 4] {
    let quad = column_quad(
        column,
        camera,
        if snow { 0.8 } else { 1.0 },
        radius,
        intensity,
    );
    let lc = column.light_coords;
    let light = ((lc >> 20 & 15) * 16) as u32 | (((lc >> 4 & 15) * 16) as u32) << 8;
    let [u, v] = parameters.snow_base.map(f64::to_bits);
    quad.map(|q| Vertex {
        pos: q.pos,
        uv: q.uv,
        rect,
        animation: [
            parameters.rain_speed,
            parameters.rain_tick_offset,
            parameters.snow_drift[0],
            parameters.snow_drift[1],
        ],
        bases: [u as u32, (u >> 32) as u32, v as u32, (v >> 32) as u32],
        color: q.color,
        light,
        snow: u32::from(snow),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn rain_column(
    random: &mut impl RandomSource,
    ticks: i64,
    x: i32,
    bottom_y: i32,
    top_y: i32,
    z: i32,
    light_coords: i32,
    partial: f32,
) -> Column {
    let wrapped_ticks = (ticks & 131_071) as i32;
    let tick_offset = x
        .wrapping_mul(x)
        .wrapping_mul(3121)
        .wrapping_add(x.wrapping_mul(45_238_971))
        .wrapping_add(z.wrapping_mul(z).wrapping_mul(418_711))
        .wrapping_add(z.wrapping_mul(13761))
        & 0xFF;
    let speed = 3.0 + random.next_float();
    let offset = -((wrapped_ticks + tick_offset) as f32 + partial) / 32.0 * speed;
    Column {
        x,
        z,
        bottom_y,
        top_y,
        u_offset: 0.0,
        v_offset: offset % 32.0,
        light_coords,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn snow_column(
    random: &mut impl RandomSource,
    ticks: i64,
    x: i32,
    bottom_y: i32,
    top_y: i32,
    z: i32,
    light_coords: i32,
    partial: f32,
) -> Column {
    let wrapped_ticks = (ticks & 131_071) as i32;
    let time = wrapped_ticks as f32 + partial;
    let u = random.next_double();
    let u = (u + f64::from(time * 0.01 * random.next_gaussian() as f32)) as f32;
    let v = random.next_double();
    let v = (v + f64::from(time * random.next_gaussian() as f32 * 0.001)) as f32;
    let v_offset = -((ticks & 511) as f32 + partial) / 512.0;
    let (block, sky) = (light_coords >> 4 & 15, light_coords >> 20 & 15);
    Column {
        x,
        z,
        bottom_y,
        top_y,
        u_offset: u,
        v_offset: v_offset + v,
        light_coords: ((block * 3 + 15) / 4) << 4 | ((sky * 3 + 15) / 4) << 20,
    }
}

fn column_size(index: usize) -> (f32, f32) {
    static SIZES: std::sync::LazyLock<[(f32, f32); 32 * 32]> = std::sync::LazyLock::new(|| {
        std::array::from_fn(|index| {
            let delta_x = (index % 32) as f32 - 16.0;
            let delta_z = (index / 32) as f32 - 16.0;
            let distance = f64::from(delta_x * delta_x + delta_z * delta_z).sqrt() as f32;
            (-delta_z / distance, delta_x / distance)
        })
    });
    SIZES[index]
}

pub fn column_quad(
    column: &Column,
    camera: [f64; 3],
    max_alpha: f32,
    radius: i32,
    intensity: f32,
) -> [ColumnVertex; 4] {
    let radius_sq = (radius * radius) as f32;
    let relative_x = (f64::from(column.x) + 0.5 - camera[0]) as f32;
    let relative_z = (f64::from(column.z) + 0.5 - camera[2]) as f32;
    let distance_sq = relative_x * relative_x + relative_z * relative_z;
    let t = (distance_sq / radius_sq).min(1.0);
    let alpha = (max_alpha + t * (0.5 - max_alpha)) * intensity;
    let color = ((alpha * 255.0).floor() as i32 as u32) << 24 | 0xFF_FFFF;
    let index = (column.z - camera[2].floor() as i32 + 16) * 32
        + (column.x - camera[0].floor() as i32 + 16);
    let (size_x, size_z) = column_size(index as usize);
    let (half_x, half_z) = (size_x / 2.0, size_z / 2.0);
    let (x0, x1) = (relative_x - half_x, relative_x + half_x);
    let y1 = (f64::from(column.top_y) - camera[1]) as f32;
    let y0 = (f64::from(column.bottom_y) - camera[1]) as f32;
    let (z0, z1) = (relative_z - half_z, relative_z + half_z);
    let (u0, u1) = (column.u_offset, column.u_offset + 1.0);
    let v0 = column.bottom_y as f32 * 0.25 + column.v_offset;
    let v1 = column.top_y as f32 * 0.25 + column.v_offset;
    let vertex = |pos, uv| ColumnVertex {
        pos,
        uv,
        color,
        light_coords: column.light_coords,
    };
    [
        vertex([x0, y1, z0], [u0, v0]),
        vertex([x1, y1, z1], [u1, v0]),
        vertex([x1, y0, z1], [u1, v1]),
        vertex([x0, y0, z0], [u0, v1]),
    ]
}

pub fn tile(quad: &[ColumnVertex; 4], mut emit: impl FnMut([[f32; 3]; 4], [[f32; 2]; 4])) {
    let [u0, v0] = quad[0].uv;
    let [u1, v1] = quad[2].uv;
    let (top_left, top_right) = (quad[0].pos, quad[1].pos);
    let (bottom_right, bottom_left) = (quad[2].pos, quad[3].pos);
    let lerp3 = |a: [f32; 3], b: [f32; 3], t: f32| -> [f32; 3] {
        std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
    };
    let at = |s: f32, t: f32| {
        lerp3(
            lerp3(top_left, top_right, s),
            lerp3(bottom_left, bottom_right, s),
            t,
        )
    };
    if !(u1 > u0 && v1 > v0) {
        return;
    }
    for (va, vb) in pieces(v0, v1) {
        let repeat_v = va.floor();
        let (ta, tb) = ((va - v0) / (v1 - v0), (vb - v0) / (v1 - v0));
        let (la, lb) = (va - repeat_v, vb - repeat_v);
        for (ua, ub) in pieces(u0, u1) {
            let repeat_u = ua.floor();
            let (sa, sb) = ((ua - u0) / (u1 - u0), (ub - u0) / (u1 - u0));
            let (ka, kb) = (ua - repeat_u, ub - repeat_u);
            emit(
                [at(sa, ta), at(sb, ta), at(sb, tb), at(sa, tb)],
                [[ka, la], [kb, la], [kb, lb], [ka, lb]],
            );
        }
    }
}

fn pieces(a: f32, b: f32) -> impl Iterator<Item = (f32, f32)> {
    let first = a.floor() + 1.0;
    let cuts = move || {
        std::iter::once(a)
            .chain(
                (0..)
                    .map(move |k| first + k as f32)
                    .take_while(move |&k| k < b),
            )
            .chain(std::iter::once(b))
    };
    cuts().zip(cuts().skip(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_size_table_matches_original_arithmetic() {
        for z in 0..32 {
            for x in 0..32 {
                let delta_x = x as f32 - 16.0;
                let delta_z = z as f32 - 16.0;
                let distance = f64::from(delta_x * delta_x + delta_z * delta_z).sqrt() as f32;
                let expected = [-delta_z / distance, delta_x / distance];
                let actual = column_size(z * 32 + x);
                for (actual, expected) in [actual.0, actual.1].into_iter().zip(expected) {
                    if x == 16 && z == 16 {
                        assert!(expected.is_nan());
                        assert!(actual.is_nan());
                    } else {
                        assert_eq!(actual.to_bits(), expected.to_bits(), "column ({x}, {z})");
                    }
                }
            }
        }
    }

    #[test]
    fn cached_parameters_match_original_random_draws() {
        let mut cache = ParameterCache::default();
        for i in 0..2048i32 {
            let x = i.wrapping_mul(45_238_971);
            let z = i.wrapping_mul(-418_711).wrapping_add(i32::MIN);
            for ticks in [i64::MIN, -1, 0, 511, 512, 131_071, 131_072, i64::MAX] {
                for partial in [0.0, 0.5, 0.999] {
                    for snow in [false, true] {
                        let mut random = column_random(x, z);
                        let reference = if snow {
                            snow_column(&mut random, ticks, x, -64, 320, z, 0xf000f0, partial)
                        } else {
                            rain_column(&mut random, ticks, x, -64, 320, z, 0xf000f0, partial)
                        };
                        assert_eq!(
                            cache
                                .get(x, z)
                                .column(snow, ticks, x, -64, 320, z, 0xf000f0, partial),
                            reference
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn parameter_cache_reuses_full_keys_and_stays_bounded() {
        let mut cache = ParameterCache::default();
        let expected = cache.get(-1, i32::MAX);
        let index = 31 * 32 + 31;
        cache.entries[index].as_mut().unwrap().1.rain_speed = 123.0;
        assert_eq!(cache.get(-1, i32::MAX).rain_speed, 123.0);
        cache.get(31, -1);
        assert_eq!(cache.get(-1, i32::MAX).rain_speed, expected.rain_speed);
        for i in 0..100_000i32 {
            cache.get(i.wrapping_mul(3121), i.wrapping_mul(418_711));
        }
        assert_eq!(cache.entries.len(), 1024);
        assert!(cache.entries.iter().flatten().count() <= 1024);
    }

    #[test]
    fn unsplit_vertices_keep_live_geometry_light_and_atlas_inputs() {
        let x = i32::MAX - 5;
        let z = i32::MIN + 5;
        let parameters = Parameters::new(x, z);
        let camera = [f64::from(x) - 2.25, 80.75, f64::from(z) + 0.25];
        for (bottom_y, top_y, light_coords, radius, intensity, rect) in [
            (64, 90, 0, 5, 0.25, [0.1, 0.2, 0.03, 0.04]),
            (85, 95, 0xf000f0, 10, 1.0, [0.5, 0.5, 0.1, 0.2]),
        ] {
            let column = Column {
                x,
                z,
                bottom_y,
                top_y,
                light_coords,
                u_offset: 0.0,
                v_offset: 0.0,
            };
            let quad = column_quad(&column, camera, 1.0, radius, intensity);
            let vertices = vertices(&column, parameters, false, camera, radius, intensity, rect);
            for (a, b) in vertices.iter().zip(quad) {
                assert_eq!(a.pos, b.pos);
                assert_eq!(a.uv, b.uv);
                assert_eq!(a.color, b.color);
                assert_eq!(a.rect, rect);
                assert_eq!(a.light, if light_coords == 0 { 0 } else { 0xf0f0 });
            }
        }
    }

    #[test]
    fn rain_repeats_every_four_blocks() {
        let column = Column {
            x: 3,
            z: 0,
            bottom_y: 60,
            top_y: 80,
            u_offset: 0.0,
            v_offset: 0.0,
            light_coords: 15 << 20,
        };
        let quad = column_quad(&column, [0.5, 70.0, 0.5], 1.0, 10, 1.0);
        let mut pieces = Vec::new();
        tile(&quad, |pos, uv| pieces.push((pos, uv)));
        assert_eq!(pieces.len(), 5);
        for (pos, uv) in &pieces {
            assert!(((pos[0][1] - pos[3][1]) - 4.0).abs() < 1e-4);
            assert_eq!(uv[0], [0.0, 0.0]);
            assert_eq!(uv[2], [1.0, 1.0]);
        }
    }

    #[test]
    fn snow_drifts_across_one_seam() {
        let column = Column {
            x: 3,
            z: 2,
            bottom_y: 64,
            top_y: 66,
            u_offset: 0.25,
            v_offset: 0.0,
            light_coords: 0,
        };
        let quad = column_quad(&column, [0.5, 70.0, 0.5], 0.8, 10, 1.0);
        let mut pieces = Vec::new();
        tile(&quad, |pos, uv| pieces.push((pos, uv)));
        assert_eq!(pieces.len(), 2);
        assert_eq!(pieces[0].1[0], [0.25, 0.0]);
        assert_eq!(pieces[1].1[2], [0.25, 0.5]);
    }

    #[test]
    fn pieces_cut_at_whole_numbers() {
        let all = |a, b| pieces(a, b).collect::<Vec<_>>();
        assert_eq!(all(0.25, 2.5), [(0.25, 1.0), (1.0, 2.0), (2.0, 2.5)]);
        assert_eq!(all(1.0, 2.0), [(1.0, 2.0)]);
        assert_eq!(all(-0.5, 0.5), [(-0.5, 0.0), (0.0, 0.5)]);
        assert_eq!(all(3.25, 3.75), [(3.25, 3.75)]);
    }

    #[test]
    fn camera_column_is_degenerate() {
        let column = Column {
            x: 0,
            z: 0,
            bottom_y: 60,
            top_y: 80,
            u_offset: 0.0,
            v_offset: 0.0,
            light_coords: 0,
        };
        let quad = column_quad(&column, [0.5, 70.0, 0.5], 1.0, 10, 1.0);
        for vertex in quad {
            assert!(vertex.pos[0].is_nan());
            assert!(vertex.pos[2].is_nan());
            assert!(vertex.pos[1].is_finite());
        }
    }
}
