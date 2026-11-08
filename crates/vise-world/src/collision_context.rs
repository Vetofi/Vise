use crate::{entity_obstruction::EntityIdentity, fluid::FluidState};
use vise_core::{
    geometry::{Axis, VoxelShape},
    position::BlockPos,
};

#[derive(Clone, Copy, Debug)]
pub struct CollisionItem<'a> {
    pub item: &'a str,
    pub count: i32,
}
impl CollisionItem<'_> {
    pub const EMPTY: Self = Self {
        item: "minecraft:air",
        count: 0,
    };

    pub fn is_item(self, item: &str) -> bool {
        let held = if self.count <= 0 {
            "minecraft:air"
        } else {
            self.item
        };
        held == item
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CollisionEntity<'a> {
    pub identity: EntityIdentity,
    pub y: f64,
    pub descending: bool,
    pub main_hand: Option<CollisionItem<'a>>,
}

#[derive(Clone, Copy, Debug)]
enum Height {
    Default,
    Bottom(f64),
}

#[derive(Clone, Copy, Debug)]
pub struct CollisionContext<'a> {
    height: Height,
    descending: bool,
    placement: bool,
    always_fluid: bool,
    held: CollisionItem<'a>,
    entity: Option<EntityIdentity>,
    living: bool,
    position_only: bool,
}
impl<'a> CollisionContext<'a> {
    pub const fn empty(always_fluid: bool) -> Self {
        Self {
            height: Height::Default,
            descending: false,
            placement: false,
            always_fluid,
            held: CollisionItem::EMPTY,
            entity: None,
            living: false,
            position_only: false,
        }
    }

    pub const fn position(y: f64) -> Self {
        Self {
            height: Height::Bottom(y),
            position_only: true,
            ..Self::empty(false)
        }
    }

    pub fn for_entity(entity: CollisionEntity<'a>, always_fluid: bool) -> Self {
        Self {
            height: Height::Bottom(entity.y),
            descending: entity.descending,
            placement: false,
            always_fluid,
            held: entity.main_hand.unwrap_or(CollisionItem::EMPTY),
            entity: Some(entity.identity),
            living: entity.main_hand.is_some(),
            position_only: false,
        }
    }

    pub fn placement(player: Option<CollisionEntity<'a>>) -> Self {
        let y = player.map_or(-f64::MAX, |player| player.y);
        Self::with_position(player, y)
    }

    pub fn with_position(entity: Option<CollisionEntity<'a>>, y: f64) -> Self {
        let mut context = entity.map_or_else(
            || Self {
                height: Height::Bottom(-f64::MAX),
                ..Self::empty(false)
            },
            |entity| Self {
                height: Height::Bottom(y),
                ..Self::for_entity(entity, false)
            },
        );
        context.placement = true;
        context
    }

    pub fn entity(&self) -> Option<EntityIdentity> {
        self.entity
    }
    pub fn living_entity(&self) -> Option<EntityIdentity> {
        self.entity.filter(|_| self.living)
    }
    pub fn is_descending(&self) -> bool {
        self.descending
    }
    pub fn is_placement(&self) -> bool {
        self.placement
    }
    pub fn always_collide_with_fluid(&self) -> bool {
        self.always_fluid
    }
    pub fn is_holding_item(&self, item: &str) -> bool {
        !self.position_only && self.held.is_item(item)
    }

    pub fn is_above(&self, shape: &VoxelShape, position: BlockPos, default: bool) -> bool {
        match self.height {
            Height::Default => default,
            Height::Bottom(bottom) => {
                bottom > (f64::from(position.y) + shape.max(Axis::Y) - f64::from(1.0e-5f32))
            }
        }
    }

    pub fn can_stand_on_fluid<E>(
        &self,
        first: FluidState,
        second: FluidState,
        mut entity_can_stand: impl FnMut(EntityIdentity, FluidState) -> Result<bool, E>,
    ) -> Result<bool, E> {
        if !self.living {
            return Ok(false);
        }
        Ok(
            entity_can_stand(self.entity.expect("living context has an entity"), second)?
                && !first.kind().same_family(second.kind()),
        )
    }
}
