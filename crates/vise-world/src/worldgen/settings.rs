use super::{DecodeError, Object, int_range, required};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoiseSettings {
    pub min_y: i32,
    pub height: i32,
}
impl NoiseSettings {
    pub const fn new(min_y: i32, height: i32) -> Self {
        Self { min_y, height }
    }
    pub fn create(min_y: i32, height: i32) -> Result<Self, DecodeError> {
        if min_y.wrapping_add(height) > 2032 {
            return Err(DecodeError::Field("noise upper bound"));
        }
        if height % 16 != 0 {
            return Err(DecodeError::Field("noise height alignment"));
        }
        if min_y % 16 != 0 {
            return Err(DecodeError::Field("noise minimum alignment"));
        }
        Ok(Self::new(min_y, height))
    }
    pub fn from_json(json: &str) -> Result<Self, DecodeError> {
        let object: Object = serde_json::from_str(json)?;
        let min_y = int_range(required(&object, "min_y")?, "noise minimum", -2032, 2031)?;
        let height = int_range(required(&object, "height")?, "noise height", 0, 4064)?;
        Self::create(min_y, height)
    }
    pub fn clamp_to_height(self, accessor_minimum: i32, accessor_height: i32) -> Self {
        let min_y = self.min_y.max(accessor_minimum);
        let upper = self
            .min_y
            .wrapping_add(self.height)
            .min(accessor_minimum.wrapping_add(accessor_height));
        Self::new(min_y, upper.wrapping_sub(min_y))
    }
}
