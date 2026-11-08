use super::{Behaviours, Dimension, LevelAccess, Sim, Ticks, update::rel};
use crate::registry::BlockStateId;
use vise_core::{geometry::Direction, position::BlockPos};

fn axis_directions(axis: &str) -> [Direction; 2] {
    match axis {
        "x" => [Direction::West, Direction::East],
        "z" => [Direction::North, Direction::South],
        _ => [Direction::Down, Direction::Up],
    }
}

impl<L: LevelAccess + ?Sized> Sim<'_, L> {
    pub(crate) fn creaking_active(&self) -> bool {
        self.level.dimension() == Dimension::Overworld
            && (12600..23401).contains(&self.level.day_time().rem_euclid(24000))
    }
    pub(crate) fn heart_has_required_logs(&self, state: BlockStateId, pos: BlockPos) -> bool {
        let b = self.b;
        let axis = b.table.text(state, b.props.axis);
        axis_directions(axis).into_iter().all(|d| {
            let neighbour = self.state(rel(pos, d));
            b.in_set(neighbour, &b.tags.pale_oak_logs)
                && b.table.text(neighbour, b.props.axis) == axis
        })
    }
    pub(crate) fn heart_update_state(&self, state: BlockStateId, pos: BlockPos) -> BlockStateId {
        let b = self.b;
        let uprooted = b.table.text(state, b.props.creaking_heart_state) == "uprooted";
        if uprooted && self.heart_has_required_logs(state, pos) {
            let next = if self.creaking_active() {
                "awake"
            } else {
                "dormant"
            };
            b.table.set_text(state, b.props.creaking_heart_state, next)
        } else {
            state
        }
    }
    pub(crate) fn heart_tick(&mut self, state: BlockStateId, pos: BlockPos) {
        let next = self.heart_update_state(state, pos);
        if next != state {
            self.set_block_and_update(pos, next);
        }
    }
    pub(crate) fn heart_placement(&self, state: BlockStateId, pos: BlockPos) -> BlockStateId {
        self.heart_update_state(state, pos)
    }
}

impl Behaviours {
    pub fn creaking_active<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
    ) -> bool {
        self.sim(level, ticks).creaking_active()
    }
    pub fn creaking_heart_has_logs<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut Ticks,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        self.sim(level, ticks).heart_has_required_logs(state, pos)
    }
}
