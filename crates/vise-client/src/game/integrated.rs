use super::{
    Game, Options, PackResources, Resources, UiResources, WorldIcon,
    execution_role::ExecutionRole,
    lan::{LanSettings, Owner},
    world_loading::WorldRequest,
};
use vise_client::{
    Result,
    audio::Audio,
    lan::{Lan, MAX_PLAYERS, NetConfig, OwnerProfile, hub::HubConfig},
    settings::Settings,
    title::WorldOptions,
};
use vise_protocol::connection::Pipe;
use vise_protocol::{crypto::offline_uuid, text::translatable};
use std::{
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicI32, AtomicU16, AtomicU32, Ordering},
        mpsc::{Receiver, Sender, channel, sync_channel},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Default)]
struct Flags {
    paused: AtomicBool,
    stop: AtomicBool,
    failure: Mutex<Option<String>>,
    published: AtomicU16,
    smoothed_tick_ms: AtomicU32,
    render_distance: AtomicI32,
    simulation_distance: AtomicI32,
    entity_distance: AtomicU32,
    saving: AtomicBool,
}

type Task = Box<dyn FnOnce(&mut Game<'_>) + Send>;

pub(super) struct Integrated {
    flags: Arc<Flags>,
    thread: Option<JoinHandle<()>>,
    tasks: Sender<Task>,
    pub world_name: String,
}
impl Integrated {
    pub fn smoothed_tick_ms(&self) -> f32 {
        f32::from_bits(self.flags.smoothed_tick_ms.load(Ordering::Relaxed))
    }
    pub fn is_saving(&self) -> bool {
        self.flags.saving.load(Ordering::Relaxed)
    }
    pub fn set_paused(&self, paused: bool) {
        self.flags.paused.store(paused, Ordering::Relaxed);
    }
    pub fn set_distances(&self, render: i32, simulation: i32, entity: f32) {
        self.flags.render_distance.store(render, Ordering::Relaxed);
        self.flags
            .simulation_distance
            .store(simulation, Ordering::Relaxed);
        self.flags
            .entity_distance
            .store(entity.to_bits(), Ordering::Relaxed);
    }
    pub fn published(&self) -> Option<u16> {
        Some(self.flags.published.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Game<'_>) -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = sync_channel(1);
        self.tasks
            .send(Box::new(move |g| {
                let _ = tx.send(f(g));
            }))
            .ok()?;
        rx.recv_timeout(Duration::from_secs(10)).ok()
    }
    pub fn stop(mut self) -> Result<()> {
        self.halt()
    }
    pub fn request_stop(&self) {
        self.flags.stop.store(true, Ordering::Relaxed);
    }
    pub fn stopped(&self) -> bool {
        self.thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
    fn halt(&mut self) -> Result<()> {
        self.request_stop();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| "the integrated server panicked")?;
        }
        match self
            .flags
            .failure
            .lock()
            .map_err(|_| "server status poisoned")?
            .as_ref()
        {
            Some(error) => Err(error.clone().into()),
            None => Ok(()),
        }
    }
}
impl Drop for Integrated {
    fn drop(&mut self) {
        if let Err(error) = self.halt() {
            log::error!("Vise: integrated server shutdown: {error}");
        }
    }
}

pub(super) struct Serving {
    flags: Arc<Flags>,
    tasks: Receiver<Task>,
    pub(super) owner: u128,
    pending: Option<Publish>,
}
impl Serving {
    pub(super) fn set_saving(&self, saving: bool) {
        self.flags.saving.store(saving, Ordering::Relaxed);
    }
}
enum Publish {
    Open(TcpListener, LanSettings),
    Close,
}

type Ready = std::result::Result<(Pipe, String), String>;

pub(super) struct Starting {
    handle: Integrated,
    ready: Receiver<Ready>,
    player: String,
    folder: String,
}

struct Launch {
    folder: String,
    name: Option<String>,
    seed: Option<i64>,
    creative: bool,
    create: Option<WorldOptions>,
    owner: u128,
    player: String,
    owner_profile: Option<Arc<OwnerProfile>>,
}

impl Game<'_> {
    pub(super) fn has_singleplayer_server(&self) -> bool {
        self.remote.is_none() || self.integrated.is_some()
    }
    pub(super) fn is_published(&self) -> bool {
        self.lan.is_some()
            || self
                .integrated
                .as_ref()
                .is_some_and(|s| s.published().is_some())
    }
    pub(super) fn open_integrated(
        &mut self,
        folder: &str,
        name: Option<&str>,
        seed: Option<i64>,
        creative: bool,
        create: Option<&WorldOptions>,
    ) -> Result<()> {
        let auth = vise_client::auth::current();
        let player = auth
            .as_ref()
            .map_or_else(|| self.options.player_name(), |a| a.name.clone());
        let launch = Launch {
            folder: folder.to_owned(),
            name: name.map(str::to_owned),
            seed,
            creative,
            create: create.cloned(),
            owner: offline_uuid(&player),
            player: player.clone(),
            owner_profile: auth.as_ref().map(|a| {
                let id = a.uuid;
                Arc::new(OwnerProfile::new(move || {
                    vise_client::skins::fetch_own_properties(id)
                        .into_iter()
                        .map(|(name, value, signature)| {
                            vise_protocol::packets::login::Property {
                                name,
                                value,
                                signature,
                            }
                        })
                        .collect()
                }))
            }),
        };
        let mut options = self.options.clone();
        options.dedicated = Some(Vec::new());
        options.server_thread = false;
        options.execution_role = ExecutionRole::IntegratedServer;
        options.control = None;
        options.benchmark = None;
        options.frames = None;
        options.world_frames = None;
        options.report = None;
        options.server = None;
        options.world = None;
        self.join_world_rest()?;
        let (res, pack, settings) = (self.res.clone(), self.pack.clone(), self.settings.clone());
        let flags = Arc::new(Flags::default());
        flags
            .render_distance
            .store(settings.render_distance, Ordering::Relaxed);
        flags
            .simulation_distance
            .store(settings.simulation_distance, Ordering::Relaxed);
        flags
            .entity_distance
            .store(settings.entity_distance.to_bits(), Ordering::Relaxed);
        let (ready_tx, ready) = channel();
        let (tasks, server_tasks) = channel();
        let server_flags = flags.clone();
        let thread = std::thread::Builder::new()
            .name("Server thread".into())
            .spawn(move || {
                vise_threads::favour_current();
                let result = serve(
                    &res,
                    pack,
                    options,
                    settings,
                    launch,
                    server_flags.clone(),
                    server_tasks,
                    &ready_tx,
                );
                if let Err(e) = result {
                    log::error!("Vise: integrated server: {e}");
                    *server_flags.failure.lock().unwrap() = Some(e.to_string());
                    let _ = ready_tx.send(Err(e.to_string()));
                }
            })?;
        let handle = Integrated {
            flags,
            thread: Some(thread),
            tasks,
            world_name: String::new(),
        };
        self.integrated_starting = Some(Starting {
            handle,
            ready,
            player,
            folder: folder.to_owned(),
        });
        if self.window.is_none() || super::synchronous_startup(&self.options) {
            while self.integrated_starting.is_some() {
                self.poll_integrated_start()?;
                if self.integrated_starting.is_some() {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        Ok(())
    }

    pub(super) fn poll_integrated_start(&mut self) -> Result<()> {
        let Some(starting) = &self.integrated_starting else {
            return Ok(());
        };
        let (pipe, world_name) = match starting.ready.try_recv() {
            Ok(Ok(ready)) => ready,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(()),
            Ok(Err(e)) => {
                self.integrated_starting = None;
                return Err(e.into());
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.integrated_starting = None;
                return Err("the integrated server stopped".into());
            }
        };
        let Some(Starting {
            mut handle,
            player,
            folder,
            ..
        }) = self.integrated_starting.take()
        else {
            return Ok(());
        };
        handle.world_name = world_name;
        self.server_packs_world_pack(&folder);
        self.connect_local(pipe, &player)?;
        self.world_icon = Some(WorldIcon {
            path: std::env::current_dir()?
                .join("saves")
                .join(&folder)
                .join("icon.png"),
            last_attempt: None,
        });
        self.integrated = Some(handle);
        Ok(())
    }

    fn run_integrated(
        &mut self,
        launch: Launch,
        flags: Arc<Flags>,
        tasks: Receiver<Task>,
        ready: &Sender<Ready>,
    ) -> Result<()> {
        let result = self.run_integrated_inner(launch, flags, tasks, ready);
        if let Err(error) = &result {
            self.native_lifecycle_close_failed(&error.to_string());
        }
        result
    }

    fn run_integrated_inner(
        &mut self,
        launch: Launch,
        flags: Arc<Flags>,
        tasks: Receiver<Task>,
        ready: &Sender<Ready>,
    ) -> Result<()> {
        if let Err(e) = self.integrated_open(&launch, &flags, tasks, ready) {
            self.native_lifecycle_world_failed(&e.to_string());
            return Err(e);
        }
        self.native_lifecycle_world_ready();
        if let (Some(script), Some(output)) = (
            std::env::var_os("VISE_COMMAND_SCRIPT"),
            std::env::var_os("VISE_COMMAND_OUTPUT"),
        ) {
            let result = self.run_command_harness(script.as_ref(), output.as_ref());
            let stopped = self.stop_integrated();
            return result.and(stopped);
        }
        let mut next = Instant::now();
        let mut was_paused = false;
        while !flags.stop.load(Ordering::Relaxed) {
            self.run_tasks();
            if self.agent.hold_server {
                self.wait_for_next_server_tick(&mut next, true);
                continue;
            }
            let paused = flags.paused.load(Ordering::Relaxed);
            if paused && !was_paused {
                log::info!("Saving and pausing game...");
                self.save_game_with(false)?;
            }
            was_paused = paused;
            if paused {
                self.tick_server_timed(true)?;
            } else {
                self.follow_distances(&flags);
                let started = Instant::now();
                self.tick_server_timed(false)?;
                self.clock.record(started.elapsed());
                self.debug_charts.log_tick(started, started.elapsed());
                flags.smoothed_tick_ms.store(
                    self.debug_charts.smoothed_tick_ms.to_bits(),
                    Ordering::Relaxed,
                );
            }
            self.apply_publish();
            self.wait_for_next_server_tick(&mut next, paused);
        }
        self.stop_integrated()
    }

    fn integrated_open(
        &mut self,
        launch: &Launch,
        flags: &Arc<Flags>,
        tasks: Receiver<Task>,
        ready: &Sender<Ready>,
    ) -> Result<()> {
        self.native_lifecycle_open_started();
        self.load_world(WorldRequest::new(
            &launch.folder,
            launch.name.as_deref(),
            launch.seed,
            launch.creative,
            launch.create.as_ref(),
        )?)?;
        let saved = Box::new(self.level.clone());
        self.mods.dev = self.options.dev_mods.clone();
        self.start_mods();
        let view_distance = self.settings.render_distance.max(2);
        let config = HubConfig {
            allow_flight: true,
            default_game_mode: self.level.mode(),
            guest_commands: false,
            force_game_mode: false,
            hardcore: self.level.hardcore,
            view_distance,
            simulation_distance: self.settings.simulation_distance.max(2),
            dimension: self.dim.id().to_owned(),
            levels: self.level_shapes,
            seed: self.level.seed,
            max_players: MAX_PLAYERS,
            dedicated: true,
            integrated: true,
        };
        let owner = launch.owner;
        let published = flags.clone();
        let net = NetConfig {
            listen: false,
            online: vise_client::lan::OnlineMode::lan(),
            gate: Some(Arc::new(move |_, uuid, _| {
                (uuid != owner && published.published.load(Ordering::Relaxed) == 0)
                    .then(|| translatable("multiplayer.disconnect.not_whitelisted", &[]))
            })),
            levels: self.level_shapes,
            owner_profile: launch.owner_profile.clone(),
            ..NetConfig::lan(
                0,
                format!("{} - {}", launch.player, self.level.name),
                MAX_PLAYERS,
            )
        };
        let settings = LanSettings {
            port: 0,
            guest_commands: false,
            force_game_mode: false,
        };
        self.lan_host(settings, net, config, None)?;
        let commands = self.allow_commands();
        let pipe = {
            let host = self
                .lan
                .as_mut()
                .ok_or("the integrated server did not start")?;
            host.hub.ops.insert(owner, if commands { 4 } else { 0 });
            host.hub.singleplayer_owner = Some(owner);
            host.owner = Some(Owner {
                uuid: owner,
                saved: Some(saved),
            });
            host.hub.lan.connect_local()?
        };
        self.serving = Some(Serving {
            flags: flags.clone(),
            tasks,
            owner,
            pending: None,
        });
        let _ = ready.send(Ok((pipe, self.level.name.clone())));
        self.settle_spawn()
    }

    fn stop_integrated(&mut self) -> Result<()> {
        self.native_lifecycle.begin_close();
        if let Some(s) = self.serving.take() {
            s.flags.published.store(0, Ordering::Relaxed);
        }
        if let Some(mut host) = self.lan.take() {
            host.award_leave_game_to_all();
            for g in &host.hub.guests {
                self.save_guest(&host, g);
            }
            let conns: Vec<u32> = host.hub.guests.iter().map(|g| g.conn).collect();
            for conn in conns {
                host.hub.kick(
                    conn,
                    translatable("multiplayer.disconnect.server_shutdown", &[]),
                );
            }
            self.lan = Some(host);
        }
        std::thread::sleep(Duration::from_millis(100));
        self.free_player_beds()?;
        self.save_game()?;
        self.lan_close(false);
        self.save.flush()?;
        self.native_lifecycle_world_closed();
        self.session_lock = None;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ServerOptions {
    pub default_mode: i32,
    pub personal_mode: i32,
    pub cheats: bool,
    pub difficulty: u8,
    pub hardcore: bool,
    pub difficulty_locked: bool,
    pub port: Option<u16>,
    pub guest_commands: bool,
    pub force: bool,
}

impl Game<'_> {
    pub(super) fn serving(&self) -> bool {
        self.serving.is_some()
    }
    pub(super) fn owner_console_command(&mut self, command: &str) -> Vec<String> {
        let Some(owner) = self.serving.as_ref().map(|s| s.owner) else {
            return Vec::new();
        };
        let Some(mut host) = self.lan.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(g) = host.hub.guests.iter().find(|g| g.uuid == owner && g.placed) {
            let conn = g.conn;
            let (back, theirs) = (self.dim, super::Dim::from_id(&g.dimension));
            let entered = theirs == back || self.enter_level(Some(&mut host), theirs).is_ok();
            if entered && let Some(swap) = self.enter_guest(&mut host, conn) {
                let command = command.trim_start_matches('/').to_owned();
                self.lan = Some(host);
                let lines = self.as_console(|g| {
                    g.chat_capture = Some(Vec::new());
                    g.run_command(&command);
                    g.chat_capture.take().unwrap_or_default()
                });
                host = self.lan.take().expect("a command puts the hub back");
                self.leave_guest(&mut host, swap);
                host.hub.sync_all_effects();
                self.run_level_teleports(&mut host);
                self.announce_guest_advancements(&mut host);
                for line in lines {
                    out.push(line.plain());
                    host.hub.send_to(
                        conn,
                        &vise_protocol::packets::play::Clientbound::SystemChat(
                            vise_protocol::packets::play::SystemChat {
                                content: vise_client::lan::data::component_nbt(
                                    &line.component(),
                                ),
                                overlay: false,
                            },
                        ),
                    );
                }
                self.send_admin_feedback(&mut host, Some(conn));
            }
            if theirs != back
                && let Err(e) = self.enter_level(Some(&mut host), back)
            {
                log::error!("could not reopen {}: {e}", back.id());
            }
        }
        self.lan = Some(host);
        out
    }

    pub(super) fn in_owner_level<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let owner = self.serving.as_ref().map(|s| s.owner);
        let dim = self.lan.as_ref().and_then(|h| {
            let g = h.hub.guests.iter().find(|g| Some(g.uuid) == owner)?;
            Some(super::Dim::from_id(&g.dimension))
        });
        let back = dim.and_then(|d| self.enter_command_level(d));
        let out = f(self);
        if let Some(back) = back {
            self.enter_command_level(back);
        }
        out
    }

    fn run_tasks(&mut self) {
        let tasks: Vec<Task> = match &self.serving {
            Some(s) => s.tasks.try_iter().collect(),
            None => return,
        };
        for task in tasks {
            task(self);
            self.apply_publish();
        }
    }
    pub(super) fn published_port(&self) -> Option<u16> {
        let s = self.serving.as_ref()?;
        Some(s.flags.published.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }
    pub(super) fn publish(&mut self, settings: LanSettings) -> std::result::Result<u16, String> {
        let Some(s) = self.serving.as_mut() else {
            return Err("not an integrated server".into());
        };
        if s.flags.published.load(Ordering::Relaxed) != 0 {
            return Err("already published".into());
        }
        let listener = Lan::bind(None, settings.port).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        log::info!("Published LAN server on port {port}");
        s.flags.published.store(port, Ordering::Relaxed);
        s.pending = Some(Publish::Open(listener, LanSettings { port, ..settings }));
        self.apply_publish();
        Ok(port)
    }
    pub(super) fn unpublish(&mut self) -> bool {
        let Some(s) = self.serving.as_mut() else {
            return false;
        };
        if s.flags.published.swap(0, Ordering::Relaxed) == 0 {
            return false;
        }
        s.pending = Some(Publish::Close);
        self.apply_publish();
        true
    }
    fn follow_distances(&mut self, flags: &Flags) {
        self.settings.entity_distance =
            f32::from_bits(flags.entity_distance.load(Ordering::Relaxed));
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        let view = flags.render_distance.load(Ordering::Relaxed).max(2);
        let old = host.hub.config.view_distance;
        if view != old {
            log::info!("Changing view distance to {view}, from {old}");
            host.hub.set_view_distance(view);
            self.settings.render_distance = view;
        }
        let simulation = flags.simulation_distance.load(Ordering::Relaxed).max(2);
        let old = host.hub.config.simulation_distance;
        if simulation != old {
            log::info!("Changing simulation distance to {simulation}, from {old}");
            host.hub.set_simulation_distance(simulation);
            self.settings.simulation_distance = simulation;
        }
    }

    fn apply_publish(&mut self) {
        if self.lan.is_none() {
            return;
        }
        let Some(s) = self.serving.as_mut() else {
            return;
        };
        let (Some(pending), owner) = (s.pending.take(), s.owner) else {
            return;
        };
        let allow_commands = self.allow_commands();
        let Some(host) = self.lan.as_mut() else {
            return;
        };
        match pending {
            Publish::Open(listener, settings) => {
                if let Err(e) = host.hub.lan.listen_on(listener, true) {
                    log::error!("Vise: Open to LAN: {e}");
                }
                host.settings = settings;
                host.hub.config.force_game_mode = settings.force_game_mode;
                host.hub.config.guest_commands = settings.guest_commands && allow_commands;
                host.hub.resend_commands();
            }
            Publish::Close => {
                host.hub.lan.stop_listening();
                log::info!(
                    "Unpublishing integrated server (was on port {})",
                    host.settings.port
                );
                let others: Vec<u32> = host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| g.uuid != owner)
                    .map(|g| g.conn)
                    .collect();
                for conn in others {
                    host.hub.kick(
                        conn,
                        translatable("multiplayer.disconnect.server_shutdown", &[]),
                    );
                }
            }
        }
    }
    pub(super) fn server_options(&self) -> ServerOptions {
        let owner = self.serving.as_ref().map(|s| s.owner);
        let host = self.lan.as_ref();
        let personal_mode = host
            .and_then(|h| h.hub.guests.iter().find(|g| Some(g.uuid) == owner))
            .map_or(self.level.mode(), |g| g.game_mode);
        let published = self.published_port();
        ServerOptions {
            default_mode: self.level.mode(),
            personal_mode,
            cheats: self.allow_commands(),
            difficulty: self.ui.difficulty,
            hardcore: self.level.hardcore,
            difficulty_locked: self.level.difficulty_locked,
            port: published,
            guest_commands: published.is_some() && host.is_some_and(|h| h.settings.guest_commands),
            force: published.is_none() || host.is_some_and(|h| h.settings.force_game_mode),
        }
    }
    pub(super) fn set_owner_mode(&mut self, mode: i32) {
        let Some(owner) = self.serving.as_ref().map(|s| s.owner) else {
            return;
        };
        if let Some(host) = self.lan.as_mut()
            && let Some(conn) = host
                .hub
                .guests
                .iter()
                .find(|g| g.uuid == owner)
                .map(|g| g.conn)
        {
            host.hub.set_game_mode(conn, mode);
        }
    }
    pub(super) fn owner_allow_commands(&mut self, allow: bool) {
        let Some(owner) = self.serving.as_ref().map(|s| s.owner) else {
            return;
        };
        let published = self.published_port().is_some();
        if let Some(host) = self.lan.as_mut() {
            host.hub.ops.insert(owner, if allow { 4 } else { 0 });
            if published {
                host.hub.config.guest_commands = allow && host.settings.guest_commands;
            }
            host.hub.resend_commands();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn serve(
    res: &Arc<Resources>,
    pack: PackResources,
    options: Options,
    settings: Settings,
    launch: Launch,
    flags: Arc<Flags>,
    tasks: Receiver<Task>,
    ready: &Sender<Ready>,
) -> Result<()> {
    let mut server = server_game(res, pack, options, settings)?;
    server.run_integrated(launch, flags, tasks, ready)
}

pub(super) enum ComponentHarness {
    Dispenser(String),
    Sculk(String),
}

impl Game<'_> {
    pub(super) fn run_component_harness(&self, fixture: ComponentHarness) -> Result<()> {
        let (res, pack, settings) = (self.res.clone(), self.pack.clone(), self.settings.clone());
        let mut options = self.options.clone();
        options.execution_role = ExecutionRole::IntegratedServer;
        options.dedicated = Some(Vec::new());
        options.server_thread = false;
        let result = std::thread::Builder::new()
            .name("Server thread".into())
            .spawn(move || {
                let run = || -> Result<()> {
                    let mut server = server_game(&res, pack, options, settings)?;
                    match fixture {
                        ComponentHarness::Dispenser(spec) => server.dispense_fixture(&spec),
                        ComponentHarness::Sculk(spec) => server.sculk_fixture(&spec),
                    }
                };
                run().map_err(|e| e.to_string())
            })?
            .join()
            .map_err(|_| "the component harness server panicked")?;
        result.map_err(Into::into)
    }
}

fn server_game(
    res: &Arc<Resources>,
    pack: PackResources,
    options: Options,
    settings: Settings,
) -> Result<Game<'_>> {
    let gui = UiResources::server(&options.resources)?;
    let audio = Audio::silent(&options.resources);
    Game::new(res, pack, gui, options, settings, audio, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shutdown_request_does_not_join_a_server_still_saving() {
        let flags = Arc::new(Flags::default());
        let (release, wait) = channel();
        let (tasks, _) = channel();
        let server = Integrated {
            flags: flags.clone(),
            thread: Some(std::thread::spawn(move || wait.recv().unwrap())),
            tasks,
            world_name: String::new(),
        };
        server.request_stop();
        assert!(flags.stop.load(Ordering::Relaxed));
        assert!(!server.stopped());
        release.send(()).unwrap();
        server.stop().unwrap();
    }

    #[test]
    fn shutdown_reports_a_failed_server_save() {
        let flags = Arc::new(Flags::default());
        let saving = flags.clone();
        let (tasks, _) = channel();
        let server = Integrated {
            flags,
            thread: Some(std::thread::spawn(move || {
                *saving.failure.lock().unwrap() = Some("disk write failed".into());
            })),
            tasks,
            world_name: String::new(),
        };
        assert_eq!(server.stop().unwrap_err().to_string(), "disk write failed");
    }
}
