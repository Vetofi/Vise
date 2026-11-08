use crate::streaming::Band;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    Square,
    Circle,
}

const STRIP_WIDTH: i32 = 256;
const BAND_CHUNKS: i32 = 1024;

pub struct Pregen {
    pub center: (i32, i32),
    pub radius: i32,
    pub shape: Shape,
    strips: Vec<(i32, i32)>,
    strip: usize,
    row: i32,
    pub done: u64,
    pub total: u64,
}

impl Pregen {
    pub fn new(center: (i32, i32), radius: i32, shape: Shape) -> Self {
        let radius = radius.max(0);
        let (x0, width) = (center.0 - radius, 2 * radius + 1);
        let count = (width + STRIP_WIDTH - 1) / STRIP_WIDTH;
        let edge = |i: i32| x0 + (i64::from(width) * i64::from(i) / i64::from(count)) as i32;
        let strips = (0..count).map(|i| (edge(i), edge(i + 1) - 1)).collect();
        let mut pregen = Self {
            center,
            radius,
            shape,
            strips,
            strip: 0,
            row: 0,
            done: 0,
            total: 0,
        };
        pregen.total = (center.1 - radius..=center.1 + radius)
            .map(|z| pregen.row_chunks(x0, x0 + width - 1, z))
            .sum();
        pregen.row = pregen.rows().map_or(0, |r| r.0);
        pregen
    }

    fn half_width(&self, d: i32) -> Option<i32> {
        let (r, d) = (i64::from(self.radius), i64::from(d));
        match self.shape {
            Shape::Square => (d.abs() <= r).then_some(self.radius),
            Shape::Circle => (d * d <= r * r).then(|| (r * r - d * d).isqrt() as i32),
        }
    }

    fn contains(&self, x: i32, z: i32) -> bool {
        self.half_width(z - self.center.1)
            .is_some_and(|h| (x - self.center.0).abs() <= h)
    }

    fn row_chunks(&self, x0: i32, x1: i32, z: i32) -> u64 {
        self.half_width(z - self.center.1).map_or(0, |h| {
            let (lo, hi) = (x0.max(self.center.0 - h), x1.min(self.center.0 + h));
            (i64::from(hi) - i64::from(lo) + 1).max(0) as u64
        })
    }

    fn rows(&self) -> Option<(i32, i32)> {
        let &(x0, x1) = self.strips.get(self.strip)?;
        let nearest = (x0 - self.center.0).max(self.center.0 - x1).max(0);
        let h = self.half_width(nearest)?;
        Some((self.center.1 - h, self.center.1 + h))
    }

    pub fn band(&self) -> Option<Band> {
        let &(x0, x1) = self.strips.get(self.strip)?;
        let (_, last) = self.rows()?;
        let rows = (BAND_CHUNKS + (x1 - x0)) / (x1 - x0 + 1);
        Some(Band {
            x0,
            x1,
            z0: self.row,
            z1: (self.row + rows - 1).min(last),
            circle: (self.shape == Shape::Circle).then_some((self.center, self.radius)),
        })
    }

    pub fn advance(&mut self, lit: impl Fn(i32, i32) -> bool) -> bool {
        let mut moved = false;
        while let (Some(&(x0, x1)), Some((_, last))) = (self.strips.get(self.strip), self.rows()) {
            let z = self.row;
            if !(x0..=x1).all(|x| !self.contains(x, z) || lit(x, z)) {
                break;
            }
            self.done += self.row_chunks(x0, x1, z);
            self.row += 1;
            if self.row > last {
                self.strip += 1;
                self.row = self.rows().map_or(self.row, |r| r.0);
            }
            moved = true;
        }
        moved
    }

    pub fn first_row(&self) -> Vec<(i32, i32)> {
        let Some(&(x0, x1)) = self.strips.get(self.strip) else {
            return Vec::new();
        };
        (x0..=x1)
            .filter(|&x| self.contains(x, self.row))
            .map(|x| (x, self.row))
            .collect()
    }

    pub fn finished(&self) -> bool {
        self.strip >= self.strips.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_every_chunk_of_the_area_once() {
        for shape in [Shape::Square, Shape::Circle] {
            for (center, radius) in [((0, 0), 0), ((3, -7), 5), ((-40, 12), 130), ((0, 0), 300)] {
                let mut p = Pregen::new(center, radius, shape);
                let r = i64::from(radius);
                let area = (-radius..=radius)
                    .flat_map(|dz| (-radius..=radius).map(move |dx| (dx, dz)))
                    .filter(|&(dx, dz)| {
                        shape == Shape::Square
                            || i64::from(dx).pow(2) + i64::from(dz).pow(2) <= r * r
                    })
                    .count() as u64;
                assert_eq!(p.total, area, "{shape:?} {radius}");
                let mut seen = std::collections::HashSet::new();
                while !p.finished() {
                    let band = p.band().unwrap();
                    assert!(band.z0 <= band.z1);
                    let row = p.first_row();
                    for &(x, z) in &row {
                        assert!(seen.insert((x, z)), "{x} {z} twice");
                    }
                    assert!(
                        p.advance(|x, z| z == band.z0 && row.contains(&(x, z))) || row.is_empty()
                    );
                }
                assert_eq!(
                    (p.done, seen.len() as u64),
                    (area, area),
                    "{shape:?} {radius}"
                );
            }
        }
    }

    #[test]
    fn a_huge_area_is_planned_at_once() {
        let started = std::time::Instant::now();
        let p = Pregen::new((0, 0), 30_000_000 >> 4, Shape::Circle);
        assert!(p.total > 11_000_000_000_000);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
