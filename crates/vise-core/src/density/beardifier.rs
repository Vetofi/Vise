use super::{DensitySampler, SampleError, SamplerContext, check_buffer};
use crate::noise::DensityVolume;
use std::sync::{Arc, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StructureBounds {
    minimum: [i32; 3],
    maximum: [i32; 3],
}
impl StructureBounds {
    pub fn new(minimum: [i32; 3], maximum: [i32; 3]) -> Option<Self> {
        (0..3)
            .all(|axis| minimum[axis] <= maximum[axis])
            .then_some(Self { minimum, maximum })
    }
    pub fn minimum(self) -> [i32; 3] {
        self.minimum
    }
    pub fn maximum(self) -> [i32; 3] {
        self.maximum
    }
    pub fn contains(self, position: [i32; 3]) -> bool {
        (0..3).all(|i| position[i] >= self.minimum[i] && position[i] <= self.maximum[i])
    }
    fn intersects(self, minimum: [i32; 3], maximum: [i32; 3]) -> bool {
        (0..3).all(|i| self.maximum[i] >= minimum[i] && self.minimum[i] <= maximum[i])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainAdjustment {
    None,
    Bury,
    BeardThin,
    BeardBox,
    Encapsulate,
}
#[derive(Clone, Copy, Debug)]
pub struct RigidPiece {
    pub bounds: StructureBounds,
    pub adjustment: TerrainAdjustment,
    pub ground_level_delta: i32,
}
#[derive(Clone, Copy, Debug)]
pub struct JigsawJunction {
    pub source_x: i32,
    pub source_ground_y: i32,
    pub source_z: i32,
}

#[derive(Debug)]
pub struct TerrainKernel {
    size: i32,
    top: i32,
    bottom: i32,
    weights: Vec<f32>,
}
impl TerrainKernel {
    pub fn new(size: i32, distance: i32, top: i32, bottom: i32) -> Self {
        let size = size.max(0);
        let radius = size / 2;
        let mut weights = vec![0.; (size as usize).pow(3)];
        for x in 0..size {
            for y in 0..size {
                for z in 0..size {
                    let (kx, ky, kz) = (
                        f64::from(x - radius),
                        f64::from(y - radius) + 0.5,
                        f64::from(z - radius),
                    );
                    let squared = kx * kx + ky * ky + kz * kz;
                    weights[((z * size + x) * size + y) as usize] =
                        crate::jmath::pow(std::f64::consts::E, -squared / f64::from(distance))
                            as f32;
                }
            }
        }
        Self {
            size,
            top,
            bottom,
            weights,
        }
    }
    pub fn radius(&self) -> i32 {
        self.size / 2
    }
    pub fn is_none(&self) -> bool {
        self.size == 0
    }
    fn factor(&self, dx: i32, dy: i32, dz: i32, floor: i32) -> f64 {
        let radius = self.radius();
        let k = [dx, dy, dz].map(|v| v.wrapping_add(radius));
        if k.iter().any(|v| !(0..self.size).contains(v)) {
            return 0.;
        }
        let weight = self.weights[((k[2] * self.size + k[0]) * self.size + k[1]) as usize];
        let floor = f64::from(floor) + 0.5;
        let squared = f64::from(dx) * f64::from(dx) + floor * floor + f64::from(dz) * f64::from(dz);
        let multiplier = (floor * (1. / (squared / 2.).sqrt()) / 2.).abs();
        let sign = if floor > 0. { self.top } else { self.bottom };
        multiplier * f64::from(weight) * f64::from(sign)
    }
}
#[derive(Clone, Debug)]
pub struct KernelPiece {
    pub bounds: StructureBounds,
    pub kernel: Arc<TerrainKernel>,
}
#[derive(Clone, Debug)]
pub struct KernelJunction {
    pub source: [i32; 3],
    pub kernel: Arc<TerrainKernel>,
}

#[derive(Clone, Debug, Default)]
pub struct Beardifier {
    pieces: Vec<RigidPiece>,
    junctions: Vec<JigsawJunction>,
    kernel_pieces: Vec<KernelPiece>,
    kernel_junctions: Vec<KernelJunction>,
    affected: Option<StructureBounds>,
}
impl Beardifier {
    pub fn new(
        pieces: Vec<RigidPiece>,
        junctions: Vec<JigsawJunction>,
        affected: Option<StructureBounds>,
    ) -> Self {
        Self {
            pieces,
            junctions,
            affected,
            ..Self::default()
        }
    }
    pub fn with_kernels(
        mut self,
        pieces: Vec<KernelPiece>,
        junctions: Vec<KernelJunction>,
    ) -> Self {
        self.kernel_pieces = pieces;
        self.kernel_junctions = junctions;
        self
    }
    fn sample_unchecked(&self, [x, y, z]: [i32; 3]) -> f32 {
        let mut value = 0.;
        let kernel = kernel();
        for piece in &self.pieces {
            let low = piece.bounds.minimum;
            let high = piece.bounds.maximum;
            let dx = 0.max(low[0].wrapping_sub(x).max(x.wrapping_sub(high[0])));
            let dz = 0.max(low[2].wrapping_sub(z).max(z.wrapping_sub(high[2])));
            let ground = low[1].wrapping_add(piece.ground_level_delta);
            let dy = y.wrapping_sub(ground);
            let distance_y = match piece.adjustment {
                TerrainAdjustment::None => 0,
                TerrainAdjustment::Bury | TerrainAdjustment::BeardThin => dy,
                TerrainAdjustment::BeardBox => {
                    0.max(ground.wrapping_sub(y).max(y.wrapping_sub(high[1])))
                }
                TerrainAdjustment::Encapsulate => {
                    0.max(low[1].wrapping_sub(y).max(y.wrapping_sub(high[1])))
                }
            };
            value += match piece.adjustment {
                TerrainAdjustment::None => 0.,
                TerrainAdjustment::Bury => bury(dx as f32, distance_y as f32 / 2., dz as f32),
                TerrainAdjustment::BeardThin | TerrainAdjustment::BeardBox => {
                    beard(kernel, dx, distance_y, dz, dy) * 0.8
                }
                TerrainAdjustment::Encapsulate => {
                    bury(dx as f32 / 2., distance_y as f32 / 2., dz as f32 / 2.) * 0.8
                }
            };
        }
        for junction in &self.junctions {
            let dx = x.wrapping_sub(junction.source_x);
            let dy = y.wrapping_sub(junction.source_ground_y);
            let dz = z.wrapping_sub(junction.source_z);
            value += beard(kernel, dx, dy, dz, dy) * 0.4;
        }
        if self.kernel_pieces.is_empty() && self.kernel_junctions.is_empty() {
            return value;
        }
        let mut density = f64::from(value);
        for piece in &self.kernel_pieces {
            let (low, high) = (piece.bounds.minimum, piece.bounds.maximum);
            let d: [i32; 3] = std::array::from_fn(|i| {
                0.max(
                    low[i]
                        .wrapping_sub([x, y, z][i])
                        .max([x, y, z][i].wrapping_sub(high[i])),
                )
            });
            density += piece
                .kernel
                .factor(d[0], d[1], d[2], y.wrapping_sub(low[1]))
                * 0.8;
        }
        for junction in &self.kernel_junctions {
            let d: [i32; 3] =
                std::array::from_fn(|i| [x, y, z][i].wrapping_sub(junction.source[i]));
            density += junction.kernel.factor(d[0], d[1], d[2], d[1]) * 0.4;
        }
        density as f32
    }
}
impl DensitySampler for Beardifier {
    fn sample_value(&self, _: &mut SamplerContext, position: [i32; 3]) -> Result<f32, SampleError> {
        Ok(
            if self
                .affected
                .is_some_and(|bounds| bounds.contains(position))
            {
                self.sample_unchecked(position)
            } else {
                0.
            },
        )
    }
    fn sample_volume(
        &self,
        _: &mut SamplerContext,
        buffer: &mut [f32],
        volume: DensityVolume,
    ) -> Result<(), SampleError> {
        check_buffer(buffer, volume)?;
        buffer.fill(0.);
        let Some(bounds) = self.affected else {
            return Ok(());
        };
        let minimum = volume.minimum();
        if !bounds.intersects(minimum, volume.maximum()) {
            return Ok(());
        }
        let steps = volume.steps();
        let sizes = volume.sizes();
        let low: [i32; 3] = std::array::from_fn(|i| {
            bounds.minimum[i]
                .wrapping_sub(minimum[i])
                .max(0)
                .div_euclid(steps[i])
        });
        let high: [i32; 3] = std::array::from_fn(|i| {
            ((sizes[i] - 1) as i32).min(
                bounds.maximum[i]
                    .wrapping_sub(minimum[i])
                    .div_euclid(steps[i]),
            )
        });
        for z in low[2]..=high[2] {
            for x in low[0]..=high[0] {
                for y in low[1]..=high[1] {
                    let index = volume
                        .index([x as usize, y as usize, z as usize])
                        .expect("bounded structure density indices");
                    let local = [x, y, z];
                    let point = std::array::from_fn(|i| {
                        minimum[i].wrapping_add(local[i].wrapping_mul(steps[i]))
                    });
                    buffer[index] = self.sample_unchecked(point);
                }
            }
        }
        Ok(())
    }
}
fn bury(x: f32, y: f32, z: f32) -> f32 {
    let squared = (x * x + y * y) + z * z;
    if squared >= 36. {
        0.
    } else {
        1. - squared.sqrt() / 6.
    }
}
fn beard(kernel: &[f32; 13824], x: i32, y: i32, z: i32, ground_delta: i32) -> f32 {
    let indices = [x, y, z].map(|v| v.wrapping_add(12));
    if indices.iter().any(|v| !(0..24).contains(v)) {
        return 0.;
    }
    let vertical = ground_delta as f32 + 0.5;
    let squared = (x as f32 * x as f32 + vertical * vertical) + z as f32 * z as f32;
    let factor = (-vertical * fast_inverse_sqrt(f64::from(squared / 2.)) as f32) / 2.;
    factor * kernel[((indices[2] * 24 + indices[0]) * 24 + indices[1]) as usize]
}
fn fast_inverse_sqrt(value: f64) -> f64 {
    let half = 0.5 * value;
    let bits = 6_910_469_410_427_058_090i64.wrapping_sub((value.to_bits() as i64) >> 1);
    let estimate = f64::from_bits(bits as u64);
    estimate * (1.5 - half * estimate * estimate)
}
fn kernel() -> &'static [f32; 13824] {
    static KERNEL: OnceLock<[f32; 13824]> = OnceLock::new();
    KERNEL.get_or_init(|| {
        std::array::from_fn(|i| {
            let z = (i / 576) as f64 - 12.;
            let x = ((i / 24) % 24) as f64 - 12.;
            let y = (i % 24) as f64 - 12. + 0.5;
            let squared = (x * x + y * y) + z * z;
            crate::jmath::pow(std::f64::consts::E, -squared / 16.) as f32
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(beardifier: &Beardifier, position: [i32; 3]) -> f32 {
        beardifier
            .sample_value(&mut SamplerContext::default(), position)
            .unwrap()
    }

    #[test]
    fn kernel_pieces_carve_above_the_floor_only() {
        let kernel = Arc::new(TerrainKernel::new(24, 24, -1, 0));
        let bounds = StructureBounds::new([0; 3], [4; 3]).unwrap();
        let affected = StructureBounds::new([-20; 3], [20; 3]);
        let b = Beardifier::new(Vec::new(), Vec::new(), affected)
            .with_kernels(vec![KernelPiece { bounds, kernel }], Vec::new());
        assert!((sample(&b, [2, 0, 2]) + 0.559_823_4).abs() < 1e-6);
        assert!((sample(&b, [7, 0, 2]) + 0.063_254_3).abs() < 1e-6);
        assert_eq!(sample(&b, [2, -3, 2]), 0.);
        assert_eq!(sample(&b, [17, 2, 2]), 0.);
    }

    #[test]
    fn kernel_junctions_bury_at_four_tenths() {
        let kernel = Arc::new(TerrainKernel::new(24, 24, 1, 1));
        let b = Beardifier::new(
            Vec::new(),
            Vec::new(),
            StructureBounds::new([-20; 3], [80; 3]),
        )
        .with_kernels(
            Vec::new(),
            vec![KernelJunction {
                source: [0, 64, 0],
                kernel,
            }],
        );
        assert!((sample(&b, [0, 64, 0]) - 0.279_911_7).abs() < 1e-6);
        assert!(sample(&b, [0, 60, 0]) > 0.);
        assert!(TerrainKernel::new(0, 0, 1, 1).is_none());
    }
}
