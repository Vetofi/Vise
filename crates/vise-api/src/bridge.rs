
use crate::{
    command::{COMMAND_REGISTRATION, CommandRegistry},
    entrypoint::Environment,
    event::ActionResult,
    events::{self, *},
    loader::{LoadError, Mod, ModLoader},
    registry::Registries,
};
use std::sync::{Mutex, OnceLock, RwLock};

static PENDING: Mutex<Vec<Mod>> = Mutex::new(Vec::new());
static LOADED: OnceLock<Result<Vec<String>, LoadError>> = OnceLock::new();
static REGISTRIES: RwLock<Option<Registries>> = RwLock::new(None);

pub fn add_mod(m: Mod) -> Result<(), LoadError> {
    if LOADED.get().is_some() {
        return Err(LoadError::TooLate(m.metadata.id));
    }
    PENDING.lock().unwrap_or_else(|e| e.into_inner()).push(m);
    Ok(())
}

pub fn init(environment: Environment) -> &'static Result<Vec<String>, LoadError> {
    LOADED.get_or_init(|| {
        let mut loader = ModLoader::new(environment);
        for m in std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner())) {
            loader.add(m)?;
        }
        loader.load()?;
        let ids = loader.loaded().to_vec();
        *REGISTRIES.write().unwrap_or_else(|e| e.into_inner()) = Some(loader.into_registries());
        Ok(ids)
    })
}

pub fn with_registries<T>(f: impl FnOnce(&Registries) -> T) -> Option<T> {
    REGISTRIES
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .as_ref()
        .map(f)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldPhase {
    OpenReady,
    OpenFailed,
    CloseFinal,
    CloseFailed,
}

pub fn server_tick_start(world_running: bool) {
    START_SERVER_TICK.invoke(&mut ServerTick { world_running });
}

pub fn server_tick_end(world_running: bool) {
    END_SERVER_TICK.invoke(&mut ServerTick { world_running });
}

pub fn world_lifecycle(phase: WorldPhase, epoch: u64, tick: u64, error: &str) {
    let error = (!error.is_empty()).then(|| error.to_owned());
    let mut ctx = WorldLifecycle { epoch, tick, error };
    match phase {
        WorldPhase::OpenReady => WORLD_READY.invoke(&mut ctx),
        WorldPhase::OpenFailed => WORLD_OPEN_FAILED.invoke(&mut ctx),
        WorldPhase::CloseFinal => WORLD_CLOSED.invoke(&mut ctx),
        WorldPhase::CloseFailed => WORLD_CLOSE_FAILED.invoke(&mut ctx),
    }
}

pub fn destroy_block(pos: BlockPos, state: u32) -> bool {
    events::BLOCK_BREAK.invoke_action(&mut BlockBreak { pos, state }) == ActionResult::Fail
}

pub fn place_block(item: ItemView, pos: BlockPos, state: u32) {
    BLOCK_PLACED.invoke(&mut BlockPlace { item, pos, state });
}

pub fn block_changed(dimension: crate::Identifier, pos: BlockPos, old: u32, new: u32) {
    BLOCK_CHANGED.invoke(&mut BlockChange {
        dimension,
        pos,
        old,
        new,
    });
}

pub fn use_item_on(item: ItemView, pos: BlockPos, face: [i32; 3]) -> bool {
    USE_BLOCK.invoke_action(&mut UseBlock { item, pos, face }) != ActionResult::Pass
}

pub fn use_item(item: ItemView) -> bool {
    USE_ITEM.invoke_action(&mut UseItem { item }) != ActionResult::Pass
}

pub fn tooltip(item: ItemView, lines: &mut Vec<TooltipLine>) {
    let mut ctx = Tooltip {
        item,
        lines: std::mem::take(lines),
    };
    ITEM_TOOLTIP.invoke(&mut ctx);
    *lines = ctx.lines;
}

pub fn key(key: &str, pressed: bool) {
    KEY_INPUT.invoke(&mut KeyInput {
        key: key.to_owned(),
        pressed,
    });
}

pub fn menu_click(player: u128, window: i32, slot: i32, button: i32, mode: i32) {
    MENU_CLICK.invoke(&mut MenuClick {
        player,
        window,
        slot,
        button,
        mode,
    });
}

pub fn menu_close(player: u128, window: i32) {
    MENU_CLOSE.invoke(&mut MenuClose { player, window });
}

pub fn mouse_scroll(steps: f32) -> bool {
    MOUSE_SCROLL.invoke_action(&mut MouseScroll { steps }) != ActionResult::Pass
}

pub fn player_join(player: PlayerRef) {
    let mut player = player;
    PLAYER_JOIN.invoke(&mut player);
}

pub fn player_leave(player: PlayerRef, reason: &str) {
    PLAYER_LEAVE.invoke(&mut PlayerLeave {
        player,
        reason: reason.to_owned(),
    });
}

pub fn player_chat(player: PlayerRef, text: &str) {
    PLAYER_CHAT.invoke(&mut PlayerChat {
        player,
        text: text.to_owned(),
    });
}

pub fn neighbor_changed(
    dimension: crate::Identifier,
    pos: BlockPos,
    state: u32,
    changed: u32,
    moved: bool,
) {
    NEIGHBOR_CHANGED.invoke(&mut NeighborChange {
        dimension,
        pos,
        state,
        changed,
        moved,
    });
}

static COMMANDS: Mutex<Option<CommandRegistry>> = Mutex::new(None);

pub fn register_commands() -> Vec<(String, u8)> {
    let mut registry = CommandRegistry::default();
    COMMAND_REGISTRATION.invoke(&mut registry);
    let usages = registry.usages();
    *COMMANDS.lock().unwrap_or_else(|e| e.into_inner()) = Some(registry);
    usages
}

pub fn perform_command(player: &PlayerRef, input: &str) -> Option<Vec<String>> {
    let registry = COMMANDS.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
    registry.run(player, input)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{command::Command, identifier::Identifier};
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    fn item() -> ItemView {
        ItemView {
            id: Identifier::vanilla("stick"),
            count: 1,
        }
    }

    #[test]
    fn break_is_cancelled_only_by_fail() {
        BLOCK_BREAK.register(|b| {
            if b.pos == [1, 2, 3] {
                ActionResult::Fail
            } else {
                ActionResult::Success
            }
        });
        assert!(destroy_block([1, 2, 3], 7));
        assert!(!destroy_block([9, 9, 9], 7));
    }

    #[test]
    fn use_is_claimed_by_anything_but_pass() {
        USE_BLOCK.register(|u| {
            if u.pos == [5, 5, 5] {
                ActionResult::Consume
            } else {
                ActionResult::Pass
            }
        });
        USE_ITEM.register(|u| {
            if u.item.count == 64 {
                ActionResult::Success
            } else {
                ActionResult::Pass
            }
        });
        assert!(use_item_on(item(), [5, 5, 5], [0, 1, 0]));
        assert!(!use_item_on(item(), [6, 5, 5], [0, 1, 0]));
        assert!(use_item(ItemView {
            count: 64,
            ..item()
        }));
        assert!(!use_item(item()));
    }

    #[test]
    fn tooltip_lines_round_trip() {
        ITEM_TOOLTIP.register(|t| {
            if t.item.id.path() == "tooltip_item" {
                t.lines.push(TooltipLine::colored("extra", 0xFF0000FF));
            }
        });
        let mut lines = vec![TooltipLine::new("Name")];
        tooltip(
            ItemView {
                id: Identifier::vanilla("tooltip_item"),
                count: 1,
            },
            &mut lines,
        );
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].color, Some(0xFF0000FF));
    }

    #[test]
    fn lifecycle_and_ticks_reach_listeners() {
        let seen = Arc::new(AtomicU32::new(0));
        let s = seen.clone();
        WORLD_OPEN_FAILED.register(move |w| {
            assert_eq!(w.error.as_deref(), Some("boom"));
            assert_eq!(w.epoch, 3);
            s.fetch_add(1, Ordering::SeqCst);
        });
        world_lifecycle(WorldPhase::OpenFailed, 3, 0, "boom");
        let s = seen.clone();
        END_SERVER_TICK.register(move |t| {
            assert!(!t.world_running);
            s.fetch_add(10, Ordering::SeqCst);
        });
        server_tick_end(false);
        assert_eq!(seen.load(Ordering::SeqCst), 11);
    }

    #[test]
    fn menu_and_scroll_events_reach_listeners() {
        let seen = Arc::new(AtomicU32::new(0));
        let s = seen.clone();
        MENU_CLICK.register(move |c| {
            if c.window == 77 && c.slot == 3 {
                s.fetch_add(1, Ordering::SeqCst);
            }
        });
        let s = seen.clone();
        MENU_CLOSE.register(move |c| {
            if c.window == 77 && c.player == 9 {
                s.fetch_add(10, Ordering::SeqCst);
            }
        });
        menu_click(9, 77, 3, 0, 0);
        menu_close(9, 77);
        assert_eq!(seen.load(Ordering::SeqCst), 11);

        MOUSE_SCROLL.register(|s| {
            if s.steps == 42.0 {
                ActionResult::Consume
            } else {
                ActionResult::Pass
            }
        });
        assert!(mouse_scroll(42.0));
        assert!(!mouse_scroll(1.0));
    }

    #[test]
    fn player_and_neighbour_events_reach_listeners() {
        let seen = Arc::new(AtomicU32::new(0));
        let who = PlayerRef {
            uuid: 4242,
            name: "joiner".into(),
            permission: 0,
            dimension: Identifier::vanilla("overworld"),
            position: [0.0; 3],
            game_mode: 0,
        };
        let s = seen.clone();
        PLAYER_JOIN.register(move |p| {
            if p.uuid == 4242 {
                s.fetch_add(1, Ordering::SeqCst);
            }
        });
        let s = seen.clone();
        PLAYER_LEAVE.register(move |l| {
            if l.player.uuid == 4242 && l.reason == "bye" {
                s.fetch_add(10, Ordering::SeqCst);
            }
        });
        let s = seen.clone();
        NEIGHBOR_CHANGED.register(move |n| {
            if n.pos == [7, 7, 7] && n.moved {
                s.fetch_add(100, Ordering::SeqCst);
            }
        });
        let s = seen.clone();
        PLAYER_CHAT.register(move |c| {
            if c.player.uuid == 4242 && c.text == "hi" {
                s.fetch_add(1000, Ordering::SeqCst);
            }
        });
        player_chat(who.clone(), "hi");
        player_join(who.clone());
        player_leave(who, "bye");
        neighbor_changed(Identifier::vanilla("overworld"), [7, 7, 7], 1, 2, true);
        assert_eq!(seen.load(Ordering::SeqCst), 1111);
    }

    #[test]
    fn commands_register_and_run() {
        COMMAND_REGISTRATION.register(|r| {
            r.register(
                Command::new("bridge_ping")
                    .unwrap()
                    .executes(|c| c.reply("pong")),
            )
            .unwrap();
        });
        assert!(register_commands().iter().any(|(u, _)| u == "bridge_ping"));
        let p = PlayerRef {
            uuid: 1,
            name: "a".into(),
            permission: 0,
            dimension: Identifier::vanilla("overworld"),
            position: [0.0; 3],
            game_mode: 0,
        };
        assert_eq!(
            perform_command(&p, "bridge_ping"),
            Some(vec!["pong".to_string()])
        );
        assert_eq!(perform_command(&p, "other"), None);
    }
}
