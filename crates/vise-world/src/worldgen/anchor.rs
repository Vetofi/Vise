use super::{DecodeError, Object, int_range, required};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldGenerationContext {
    pub min_y: i32,
    pub height: i32,
    pub sea_level: i32,
}
impl WorldGenerationContext {
    pub const fn new(min_y: i32, height: i32, sea_level: i32) -> Self {
        Self {
            min_y,
            height,
            sea_level,
        }
    }

    pub fn with_generator(
        level_min: i32,
        level_height: i32,
        generator_min: i32,
        generator_height: i32,
        generator_sea_level: i32,
    ) -> Self {
        Self::new(
            level_min.max(generator_min),
            level_height.min(generator_height),
            generator_sea_level,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerticalAnchor {
    Absolute(i32),
    AboveBottom(i32),
    BelowTop(i32),
    RelativeToSeaLevel(i32),
}
impl VerticalAnchor {
    pub const BOTTOM: Self = Self::AboveBottom(0);
    pub const TOP: Self = Self::BelowTop(0);
    pub const SEA_LEVEL: Self = Self::RelativeToSeaLevel(0);

    pub fn resolve(self, context: WorldGenerationContext) -> i32 {
        match self {
            Self::Absolute(y) => y,
            Self::AboveBottom(offset) => context.min_y.wrapping_add(offset),
            Self::BelowTop(offset) => context
                .height
                .wrapping_sub(1)
                .wrapping_add(context.min_y)
                .wrapping_sub(offset),
            Self::RelativeToSeaLevel(offset) => context.sea_level.wrapping_add(offset),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, DecodeError> {
        let object: Object = serde_json::from_str(json)?;
        let field = |name| {
            required(&object, name)
                .and_then(|raw| int_range(raw, name, -2032, 2031))
                .ok()
        };
        let exclusive = |left, right| match (left, right) {
            (Some(value), None) | (None, Some(value)) => Some(value),
            _ => None,
        };
        exclusive(
            field("absolute").map(Self::Absolute),
            exclusive(
                field("above_bottom").map(Self::AboveBottom),
                exclusive(
                    field("below_top").map(Self::BelowTop),
                    field("relative_to_sea_level").map(Self::RelativeToSeaLevel),
                ),
            ),
        )
        .ok_or(DecodeError::Field("vertical anchor"))
    }

    pub fn to_json(self) -> Result<String, DecodeError> {
        let (name, value) = match self {
            Self::Absolute(y) => ("absolute", y),
            Self::AboveBottom(offset) => ("above_bottom", offset),
            Self::BelowTop(offset) => ("below_top", offset),
            Self::RelativeToSeaLevel(offset) => ("relative_to_sea_level", offset),
        };
        if !(-2032..=2031).contains(&value) {
            return Err(DecodeError::Field("vertical anchor"));
        }
        Ok(format!("{{\"{name}\":{value}}}"))
    }
}
