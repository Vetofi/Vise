
use crate::identifier::Identifier;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockSettings {
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    pub light: Option<u8>,
    pub requires_tool: Option<bool>,
    pub friction: Option<f32>,
    pub sounds: Option<Identifier>,
}

impl BlockSettings {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn hardness(mut self, v: f32) -> Self {
        self.hardness = Some(v);
        self
    }
    pub fn explosion_resistance(mut self, v: f32) -> Self {
        self.explosion_resistance = Some(v);
        self
    }
    pub fn light(mut self, v: u8) -> Self {
        self.light = Some(v.min(15));
        self
    }
    pub fn requires_tool(mut self, v: bool) -> Self {
        self.requires_tool = Some(v);
        self
    }
    pub fn friction(mut self, v: f32) -> Self {
        self.friction = Some(v);
        self
    }
    pub fn sounds(mut self, v: Identifier) -> Self {
        self.sounds = Some(v);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlockDef {
    pub template: Identifier,
    pub settings: BlockSettings,
    pub item: bool,
}

impl BlockDef {
    pub fn new(template: Identifier, settings: BlockSettings) -> Self {
        Self {
            template,
            settings,
            item: true,
        }
    }
    pub fn without_item(mut self) -> Self {
        self.item = false;
        self
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Epic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemDef {
    pub template: Identifier,
    pub max_stack_size: Option<u8>,
    pub rarity: Rarity,
}

impl ItemDef {
    pub fn new(template: Identifier) -> Self {
        Self {
            template,
            max_stack_size: None,
            rarity: Rarity::Common,
        }
    }
    pub fn max_stack_size(mut self, v: u8) -> Self {
        self.max_stack_size = Some(v.clamp(1, 99));
        self
    }
    pub fn rarity(mut self, v: Rarity) -> Self {
        self.rarity = v;
        self
    }
}
