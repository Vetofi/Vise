use super::{states::BlockId, update::NeighborUpdater};
use crate::fluid::FluidKind;
use vise_core::{
    position::{BlockPos, ChunkPos},
    ticks::{
        ChunkTicks, LevelTicks, ScheduleResult, ScheduledTick, TickError, TickPriority, TickTypeId,
    },
};

pub struct Ticks {
    pub blocks: LevelTicks,
    pub fluids: LevelTicks,
    sub_tick: i64,
    pub random_tick_value: i32,
    pub(crate) updater: NeighborUpdater,
    pub comparator_outputs: std::collections::HashMap<BlockPos, i32>,
    pub torch_toggles: Vec<(BlockPos, i64)>,
    pub moving_pistons: Vec<(BlockPos, super::pistons::MovingPiston)>,
    pub piston_pushes: Vec<(BlockPos, super::pistons::MovingPiston, f32)>,
    pub(crate) next_block_entity_id: u64,
    pub block_events: Vec<super::pistons::BlockEventData>,
    pub(crate) handling_tick: bool,
    pub(crate) capture: Option<Vec<CapturedTick>>,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct CapturedTick {
    pub fluid: bool,
    pub kind: u32,
    pub pos: BlockPos,
    pub delay: i32,
}
impl Default for Ticks {
    fn default() -> Self {
        Self::new(0)
    }
}
impl Ticks {
    pub fn new(random_tick_seed: i32) -> Self {
        Self {
            blocks: LevelTicks::default(),
            fluids: LevelTicks::default(),
            sub_tick: 0,
            random_tick_value: random_tick_seed,
            updater: NeighborUpdater::default(),
            comparator_outputs: Default::default(),
            torch_toggles: Vec::new(),
            moving_pistons: Vec::new(),
            piston_pushes: Vec::new(),
            next_block_entity_id: 0,
            block_events: Vec::new(),
            handling_tick: false,
            capture: None,
        }
    }
    pub fn add_chunk(
        &mut self,
        pos: ChunkPos,
        blocks: ChunkTicks,
        fluids: ChunkTicks,
    ) -> Result<(), TickError> {
        self.blocks.add_chunk(pos, blocks)?;
        self.fluids.add_chunk(pos, fluids)
    }
    pub fn load_moving_piston(&mut self, pos: BlockPos, mut piston: super::pistons::MovingPiston) {
        self.moving_pistons.retain(|(p, _)| *p != pos);
        self.next_block_entity_id += 1;
        piston.id = self.next_block_entity_id;
        self.moving_pistons.push((pos, piston));
    }
    pub fn add_empty_chunk(&mut self, pos: ChunkPos) -> Result<(), TickError> {
        self.add_chunk(pos, ChunkTicks::default(), ChunkTicks::default())
    }
    pub fn remove_chunk(&mut self, pos: ChunkPos) -> (Option<ChunkTicks>, Option<ChunkTicks>) {
        (self.blocks.remove_chunk(pos), self.fluids.remove_chunk(pos))
    }
    pub fn next_sub_tick(&mut self) -> i64 {
        let value = self.sub_tick;
        self.sub_tick = self.sub_tick.wrapping_add(1);
        value
    }
    pub fn schedule_block(
        &mut self,
        game_time: i64,
        pos: BlockPos,
        block: BlockId,
        delay: i32,
        priority: TickPriority,
    ) -> ScheduleResult {
        if let Some(captured) = &mut self.capture {
            captured.push(CapturedTick {
                fluid: false,
                kind: u32::from(block.0),
                pos,
                delay,
            });
            return ScheduleResult::Added;
        }
        let tick = ScheduledTick {
            kind: TickTypeId(u32::from(block.0)),
            pos,
            trigger_tick: game_time.wrapping_add(i64::from(delay)),
            priority,
            sub_tick_order: self.next_sub_tick(),
        };
        self.blocks.schedule(tick)
    }
    pub fn schedule_fluid(
        &mut self,
        game_time: i64,
        pos: BlockPos,
        fluid: FluidKind,
        delay: i32,
    ) -> ScheduleResult {
        if let Some(captured) = &mut self.capture {
            captured.push(CapturedTick {
                fluid: true,
                kind: fluid as u32,
                pos,
                delay,
            });
            return ScheduleResult::Added;
        }
        let tick = ScheduledTick {
            kind: TickTypeId(fluid as u32),
            pos,
            trigger_tick: game_time.wrapping_add(i64::from(delay)),
            priority: TickPriority::Normal,
            sub_tick_order: self.next_sub_tick(),
        };
        self.fluids.schedule(tick)
    }
    pub fn has_block_tick(&self, pos: BlockPos, block: BlockId) -> bool {
        self.blocks
            .has_scheduled(pos, TickTypeId(u32::from(block.0)))
    }
    pub fn will_block_tick_this_tick(&mut self, pos: BlockPos, block: BlockId) -> bool {
        self.blocks
            .will_tick_this_tick(pos, TickTypeId(u32::from(block.0)))
    }
    pub fn block_random_pos(&mut self, x: i32, y: i32, z: i32, y_mask: i32) -> BlockPos {
        self.random_tick_value = self
            .random_tick_value
            .wrapping_mul(3)
            .wrapping_add(1_013_904_223);
        let value = self.random_tick_value >> 2;
        BlockPos::new(
            x + (value & 15),
            y + ((value >> 16) & y_mask),
            z + ((value >> 8) & 15),
        )
    }
}
