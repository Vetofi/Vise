
use crate::{
    event::{ActionResult, Event},
    identifier::Identifier,
};
use std::sync::LazyLock;

pub type BlockPos = [i32; 3];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemView {
    pub id: Identifier,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerRef {
    pub uuid: u128,
    pub name: String,
    pub permission: u8,
    pub dimension: Identifier,
    pub position: [f64; 3],
    pub game_mode: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerTick {
    pub world_running: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldLifecycle {
    pub epoch: u64,
    pub tick: u64,
    pub error: Option<String>,
}

pub static START_SERVER_TICK: LazyLock<Event<ServerTick>> = LazyLock::new(Event::new);
pub static END_SERVER_TICK: LazyLock<Event<ServerTick>> = LazyLock::new(Event::new);
pub static WORLD_READY: LazyLock<Event<WorldLifecycle>> = LazyLock::new(Event::new);
pub static WORLD_OPEN_FAILED: LazyLock<Event<WorldLifecycle>> = LazyLock::new(Event::new);
pub static WORLD_CLOSED: LazyLock<Event<WorldLifecycle>> = LazyLock::new(Event::new);
pub static WORLD_CLOSE_FAILED: LazyLock<Event<WorldLifecycle>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockBreak {
    pub pos: BlockPos,
    pub state: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockPlace {
    pub item: ItemView,
    pub pos: BlockPos,
    pub state: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockChange {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub old: u32,
    pub new: u32,
}

pub static BLOCK_BREAK: LazyLock<Event<BlockBreak, ActionResult>> = LazyLock::new(Event::new);
pub static BLOCK_PLACED: LazyLock<Event<BlockPlace>> = LazyLock::new(Event::new);
pub static BLOCK_CHANGED: LazyLock<Event<BlockChange>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseBlock {
    pub item: ItemView,
    pub pos: BlockPos,
    pub face: [i32; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UseItem {
    pub item: ItemView,
}

pub static USE_BLOCK: LazyLock<Event<UseBlock, ActionResult>> = LazyLock::new(Event::new);
pub static USE_ITEM: LazyLock<Event<UseItem, ActionResult>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TooltipLine {
    pub text: String,
    pub color: Option<u32>,
}

impl TooltipLine {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
        }
    }
    pub fn colored(text: impl Into<String>, color: u32) -> Self {
        Self {
            text: text.into(),
            color: Some(color),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub item: ItemView,
    pub lines: Vec<TooltipLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyInput {
    pub key: String,
    pub pressed: bool,
}

pub static ITEM_TOOLTIP: LazyLock<Event<Tooltip>> = LazyLock::new(Event::new);
pub static KEY_INPUT: LazyLock<Event<KeyInput>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuClick {
    pub player: u128,
    pub window: i32,
    pub slot: i32,
    pub button: i32,
    pub mode: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuClose {
    pub player: u128,
    pub window: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MouseScroll {
    pub steps: f32,
}

pub static MENU_CLICK: LazyLock<Event<MenuClick>> = LazyLock::new(Event::new);
pub static MENU_CLOSE: LazyLock<Event<MenuClose>> = LazyLock::new(Event::new);
pub static MOUSE_SCROLL: LazyLock<Event<MouseScroll, ActionResult>> = LazyLock::new(Event::new);

pub static PLAYER_JOIN: LazyLock<Event<PlayerRef>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerLeave {
    pub player: PlayerRef,
    pub reason: String,
}

pub static PLAYER_LEAVE: LazyLock<Event<PlayerLeave>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerChat {
    pub player: PlayerRef,
    pub text: String,
}

pub static PLAYER_CHAT: LazyLock<Event<PlayerChat>> = LazyLock::new(Event::new);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NeighborChange {
    pub dimension: Identifier,
    pub pos: BlockPos,
    pub state: u32,
    pub changed: u32,
    pub moved: bool,
}

pub static NEIGHBOR_CHANGED: LazyLock<Event<NeighborChange>> = LazyLock::new(Event::new);
