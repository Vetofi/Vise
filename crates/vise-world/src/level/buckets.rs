use super::{
    Behaviours, Dimension, LevelAccess, Sim, Ticks,
    classes::{Class, Interface},
};
use crate::{
    fluid::{FluidKind, FluidState},
    registry::BlockStateId,
};
use vise_core::{position::BlockPos, random::RandomSource};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BucketContent {
    Fluid(FluidKind),
    Powder,
    MobWater,
    Nothing,
}

impl BucketContent {
    pub fn is_water(self) -> bool {
        matches!(self, Self::Fluid(FluidKind::Water) | Self::MobWater)
    }
}

impl<L: LevelAccess + ?Sized> Sim<'_, L> {
    pub(crate) fn bucket_pickup(&mut self, pos: BlockPos, creative: bool) -> Option<&'static str> {
        let b = self.b;
        let state = self.state(pos);
        let item = if b.instance_of(state, Class::Liquid) {
            match b.fluid(state).kind() {
                FluidKind::Water | FluidKind::FlowingWater => "minecraft:water_bucket",
                FluidKind::Lava | FluidKind::FlowingLava => "minecraft:lava_bucket",
                FluidKind::Empty => return None,
            }
        } else if b.implements(state, Interface::SimpleWaterlogged)
            && b.table.has(state, b.props.waterlogged)
        {
            "minecraft:water_bucket"
        } else if b.is_block(state, b.named.powder_snow) {
            "minecraft:powder_snow_bucket"
        } else if b.is_block(state, b.named.bubble_column) {
            "minecraft:water_bucket"
        } else {
            return None;
        };
        self.pickup_block(pos, state, creative).then_some(item)
    }
    pub(crate) fn bucket_empty(
        &mut self,
        pos: BlockPos,
        content: BucketContent,
        sound: &str,
        sneaking: bool,
        creative: bool,
    ) -> bool {
        let b = self.b;
        let fluid = match content {
            BucketContent::Powder => {
                if self.is_in_valid_bounds(pos) && b.is_air(self.state(pos)) {
                    let snow = b.table.default_state(b.named.powder_snow);
                    self.set_block_and_update(pos, snow);
                    self.game_event(pos, "minecraft:fluid_place");
                    let sound: Arc<str> = sound.into();
                    self.sound(pos, &sound, 1.0, 1.0);
                    return true;
                }
                return false;
            }
            BucketContent::Nothing => {
                let sound: Arc<str> = sound.into();
                self.sound(pos, &sound, 1.0, 1.0);
                return true;
            }
            BucketContent::MobWater => FluidKind::Water,
            BucketContent::Fluid(fluid) => fluid,
        };
        let mob = content == BucketContent::MobWater;
        let state = self.state(pos);
        let may_replace = !b.instance_of(state, Class::EndPortal)
            && !b.instance_of(state, Class::EndGateway)
            && (b.is_replaceable(state) || !b.is_solid(state));
        let liquid_container = self.is_liquid_container(state);
        let container = liquid_container && self.can_place_liquid(state, fluid, creative);
        let place_liquid = may_replace || container;
        if !(b.is_air(state) || place_liquid && !sneaking) {
            return false;
        }
        let water = matches!(fluid, FluidKind::Water | FluidKind::FlowingWater);
        if water && self.level.dimension() == Dimension::Nether {
            let pitch = 2.6 + (self.random().next_float() - self.random().next_float()) * 0.8;
            self.emit(super::LevelEvent::Sound {
                pos,
                sound: "minecraft:block.fire.extinguish".into(),
                volume: 0.5,
                pitch,
            });
            return true;
        }
        if liquid_container && water {
            self.place_liquid(
                pos,
                state,
                FluidState::new(FluidKind::Water, 8, false),
                creative,
            );
            self.empty_sound(pos, sound, mob);
            return true;
        }
        if may_replace && !b.instance_of(state, Class::Liquid) {
            self.destroy_block(pos, true, super::flags::UPDATE_LIMIT);
        }
        let source = FluidState::new(
            if water {
                FluidKind::Water
            } else {
                FluidKind::Lava
            },
            8,
            false,
        );
        let legacy: BlockStateId = b.legacy_block(source);
        if !self.set_block(pos, legacy, super::flags::UPDATE_ALL_IMMEDIATE)
            && !self.fluid_at(pos).is_source()
        {
            return false;
        }
        self.empty_sound(pos, sound, mob);
        true
    }
    fn empty_sound(&mut self, pos: BlockPos, sound: &str, mob: bool) {
        let sound: Arc<str> = sound.into();
        self.sound(pos, &sound, 1.0, 1.0);
        if !mob {
            self.game_event(pos, "minecraft:fluid_place");
        }
    }
}

impl Behaviours {
    pub fn bucket_pickup<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        creative: bool,
    ) -> Option<&'static str> {
        self.sim(level, ticks).bucket_pickup(pos, creative)
    }
    pub fn bucket_empty<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        content: BucketContent,
        sound: &str,
        sneaking: bool,
        creative: bool,
    ) -> bool {
        self.sim(level, ticks)
            .bucket_empty(pos, content, sound, sneaking, creative)
    }
}
