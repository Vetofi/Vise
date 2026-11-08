use super::{
    Game, LevelDimension,
    execution_role::ExecutionRole,
    native_lifecycle::{NativeLifecycleContext, ServerTickPhase, WorldPhase},
    native_server::{NativeCommand, NativeServerPlayer},
};
use vise_api::{
    Environment, Identifier,
    bridge::{self, WorldPhase as ApiWorldPhase},
    events::{ItemView, PlayerRef, TooltipLine},
};
use vise_client::{items::ItemStack, text::Text};
use std::sync::atomic::{AtomicBool, Ordering};

static LOADED: AtomicBool = AtomicBool::new(false);

fn id_or(s: &str, fallback: &str) -> Identifier {
    Identifier::parse(s).unwrap_or_else(|_| Identifier::vanilla(fallback))
}

pub(super) fn item_view(stack: &ItemStack) -> ItemView {
    ItemView { id: id_or(&stack.item, "air"), count: i32::try_from(stack.count).unwrap_or(i32::MAX) }
}

pub(super) fn dimension_id(dimension: LevelDimension) -> Identifier {
    Identifier::vanilla(match dimension {
        LevelDimension::Overworld => "overworld",
        LevelDimension::Nether => "the_nether",
        LevelDimension::End => "the_end",
    })
}

pub(super) fn block_changed(
    dimension: LevelDimension,
    pos: vise_core::position::BlockPos,
    old: vise_world::registry::BlockStateId,
    new: vise_world::registry::BlockStateId,
) {
    bridge::block_changed(dimension_id(dimension), [pos.x, pos.y, pos.z], old.0, new.0);
}

pub(super) fn neighbor_changed(
    dimension: LevelDimension,
    pos: vise_core::position::BlockPos,
    state: vise_world::registry::BlockStateId,
    changed: vise_world::level::BlockId,
    moved: bool,
) {
    bridge::neighbor_changed(
        dimension_id(dimension),
        [pos.x, pos.y, pos.z],
        state.0,
        u32::from(changed.0),
        moved,
    );
}

pub(super) fn player_joined(player: &NativeServerPlayer) {
    bridge::player_join(player_ref(player));
}

pub(super) fn player_chat(player: &NativeServerPlayer, text: &str) {
    bridge::player_chat(player_ref(player), text);
}

pub(super) fn player_left(player: &NativeServerPlayer, reason: &str) {
    bridge::player_leave(player_ref(player), reason);
}

fn player_ref(player: &NativeServerPlayer) -> PlayerRef {
    PlayerRef {
        uuid: player.uuid,
        name: player.name.clone(),
        permission: player.permission,
        dimension: id_or(&player.dimension, "overworld"),
        position: player.position,
        game_mode: player.game_mode,
    }
}

pub(super) fn world_lifecycle(context: &NativeLifecycleContext, phase: WorldPhase, error: &str) {
    let phase = match phase {
        WorldPhase::WorldOpenReady => ApiWorldPhase::OpenReady,
        WorldPhase::WorldOpenFailed => ApiWorldPhase::OpenFailed,
        WorldPhase::WorldCloseFinal => ApiWorldPhase::CloseFinal,
        WorldPhase::WorldCloseFailed => ApiWorldPhase::CloseFailed,
    };
    bridge::world_lifecycle(phase, context.epoch.get(), context.tick.get(), error);
}

pub(super) fn server_tick(phase: ServerTickPhase) {
    match phase {
        ServerTickPhase::ServerTickStart { world_running } => bridge::server_tick_start(world_running),
        ServerTickPhase::ServerTickEnd { world_running } => bridge::server_tick_end(world_running),
    }
}

pub(super) fn register_commands(commands: &mut Vec<NativeCommand>) {
    for (usage, level) in bridge::register_commands() {
        let mut command = NativeCommand::new(&usage);
        command.level = level;
        commands.push(command);
    }
}

pub(super) fn tooltip(stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
    let original = std::mem::take(lines);
    let mut api: Vec<TooltipLine> = original
        .iter()
        .map(|(text, color)| TooltipLine { text: text.plain(), color: (*color != u32::MAX).then_some(*color) })
        .collect();
    let before = api.clone();
    bridge::tooltip(item_view(stack), &mut api);
    if api == before {
        *lines = original;
        return;
    }
    let mut spare: Vec<Option<(Text, u32)>> = original.into_iter().map(Some).collect();
    for line in api {
        let reused = before
            .iter()
            .zip(spare.iter_mut())
            .find(|(b, slot)| **b == line && slot.is_some())
            .and_then(|(_, slot)| slot.take());
        lines.push(reused.unwrap_or_else(|| (Text::legacy(&line.text), line.color.unwrap_or(u32::MAX))));
    }
}

impl Game<'_> {
    pub(super) fn mod_api_tick(&mut self) {
        if LOADED.load(Ordering::Relaxed) {
            return;
        }
        let environment = match self.options.execution_role {
            ExecutionRole::Client => Environment::Client,
            ExecutionRole::DedicatedServer => Environment::Server,
            ExecutionRole::IntegratedServer => return,
        };
        LOADED.store(true, Ordering::Relaxed);
        self.native_register_mods();
        match bridge::init(environment) {
            Ok(ids) if !ids.is_empty() => log::info!("vise-api mods: {}", ids.join(", ")),
            Ok(_) => {}
            Err(e) => log::error!("vise-api mods did not load: {e}"),
        }
    }

    pub(super) fn mod_api_command(&mut self, player: &NativeServerPlayer, command: &str) -> bool {
        let Some(lines) = bridge::perform_command(&player_ref(player), command) else {
            return false;
        };
        for line in lines {
            self.native_feedback(Text::legacy(&line));
        }
        true
    }
}
