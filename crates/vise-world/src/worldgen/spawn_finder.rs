use super::climate::{Parameter, quantize};
use vise_core::density::{DensitySampler, SampleError, SamplerContext};
use vise_core::jmath;
use std::sync::Arc;

const MAX_RADIUS: f32 = 2048.0;

#[derive(Clone, Debug)]
pub struct SpawnTargetPoint {
    pub parameters: Vec<(Arc<dyn DensitySampler>, Parameter)>,
}
impl SpawnTargetPoint {
    pub fn sample_fitness(
        &self,
        context: &mut SamplerContext,
        position: [i32; 3],
    ) -> Result<i64, SampleError> {
        let mut fitness = 0i64;
        for (function, parameter) in &self.parameters {
            let distance = parameter.distance(quantize(function.sample_value(context, position)?));
            fitness = fitness.wrapping_add(distance.wrapping_mul(distance));
        }
        Ok(fitness)
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    x: i32,
    z: i32,
    fitness: i64,
}

pub fn find_spawn_position(
    targets: &[SpawnTargetPoint],
    context: &mut SamplerContext,
) -> Result<[i32; 2], SampleError> {
    let mut best = fitness(targets, context, 0, 0)?;
    radial_search(targets, context, &mut best, MAX_RADIUS, 512.0)?;
    radial_search(targets, context, &mut best, 512.0, 32.0)?;
    Ok([best.x, best.z])
}

fn radial_search(
    targets: &[SpawnTargetPoint],
    context: &mut SamplerContext,
    best: &mut Candidate,
    max_radius: f32,
    increment: f32,
) -> Result<(), SampleError> {
    let mut angle = 0f32;
    let mut radius = increment;
    let (origin_x, origin_z) = (best.x, best.z);
    while radius <= max_radius {
        let x = origin_x.wrapping_add((jmath::sin(f64::from(angle)) * f64::from(radius)) as i32);
        let z = origin_z.wrapping_add((jmath::cos(f64::from(angle)) * f64::from(radius)) as i32);
        let candidate = fitness(targets, context, x, z)?;
        if candidate.fitness < best.fitness {
            *best = candidate;
        }
        angle += increment / radius;
        if f64::from(angle) > std::f64::consts::PI * 2.0 {
            angle = 0.0;
            radius += increment;
        }
    }
    Ok(())
}

fn fitness(
    targets: &[SpawnTargetPoint],
    context: &mut SamplerContext,
    x: i32,
    z: i32,
) -> Result<Candidate, SampleError> {
    let quart = [(x >> 2) << 2, 0, (z >> 2) << 2];
    let mut best = i64::MAX;
    for target in targets {
        best = best.min(target.sample_fitness(context, quart)?);
    }
    let bias = i64::from(x) * i64::from(x) + i64::from(z) * i64::from(z);
    Ok(Candidate {
        x,
        z,
        fitness: best.wrapping_mul(2048 * 2048).wrapping_add(bias),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Gradient;
    impl DensitySampler for Gradient {
        fn sample_value(&self, _: &mut SamplerContext, p: [i32; 3]) -> Result<f32, SampleError> {
            Ok(p[0] as f32 / 1000.0)
        }
    }

    #[test]
    fn the_closest_fitting_column_wins() {
        let targets = [SpawnTargetPoint {
            parameters: vec![(Arc::new(Gradient), Parameter::new(1000, 10_000))],
        }];
        let [x, z] = find_spawn_position(&targets, &mut SamplerContext::default()).unwrap();
        assert!(x >= 100, "{x} {z}");
        assert!(x * x + z * z < 160 * 160, "{x} {z}");
    }
}
