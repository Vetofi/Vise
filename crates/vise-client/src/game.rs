
macro_rules! world_items {
    ($s:ident) => {
        match &$s.world_tables {
            Some(t) => &t.items,
            None => $s.res.items(),
        }
    };
}
macro_rules! world_recipes {
    ($s:ident) => {
        match &$s.world_tables {
            Some(t) => &t.recipes,
            None => $s.res.recipes(),
        }
    };
}
macro_rules! world_stations {
    ($s:ident) => {
        match &$s.world_tables {
            Some(t) => &t.stations,
            None => $s.res.stations(),
        }
    };
}

fn pack_files(world_tables: &Option<WorldTables>) -> vise_world::loot::PackOverlay<'_> {
    match world_tables {
        Some(t) => vise_world::loot::PackOverlay { files: &t.files },
        None => vise_world::loot::PackOverlay::NONE,
    }
}

mod advancements;
mod advancements_screen;
mod agent;
mod agent_settings;
mod allays;
mod armor_stands;
mod banners;
mod beacon;
mod beds;
mod beehives;
mod block_entity_anims;
mod block_entity_ticks;
mod block_items;
mod blockctl;
mod boat_passengers;
mod boats;
mod books;
mod bossbars;
mod breezes;
mod brush;
mod bundles;
mod campfire;
mod campfire_douse;
mod cartography;
mod cat_spawner;
mod chat;
mod chiseled_bookshelf;
mod client_ambient;
mod clipboard;
mod code_of_conduct;
mod combat;
mod command_blocks;
mod command_harness;
mod commands;
mod composter;
mod consume;
mod control;
mod copper_golems;
mod creaking;
mod creative_hotbar;
mod creatures;
mod crossbow;
pub(crate) mod cushions;
mod debug_keys;
mod debug_stick;
mod decorated_pot;
mod dedicated;
mod deflect;
mod dialogs;
mod discord;
mod dispense_fixture;
mod dispenser;
mod dragon;
mod dynamic_lights;
mod enchant_effects;
mod end;
mod entity_culling;
mod events;
mod execution_role;
mod explosion;
mod extras;
mod falling_damage;
mod fangs;
mod fireworks;
mod fishing;
mod fov;
mod frames;
mod friends;
mod functions;
mod game_mode;
mod gamemode_switcher;
mod ghasts;
mod glowing;
mod golems;
mod growth;
mod hand;
mod initial_spawn;
mod integrated;
mod item_draw;
mod item_particles;
mod jukebox;
mod lan;
mod lan_actions;
mod lan_attributes;
mod lan_chat;
mod lan_crafting;
mod lan_entities;
mod lan_mobs;
mod lan_mount_menu;
mod lan_particles;
mod lan_pvp;
mod lan_quiet;
mod lan_screen;
mod lan_stations;
mod lan_trading;
mod lan_vehicles;
mod lan_waypoints;
mod launch_api;
mod leads;
mod level_particles;
mod levels;
mod lightning;
mod llamas;
mod locator_bar;
mod loom;
mod mace;
mod machines;
mod maps;
mod minecarts;
mod mob_blocks;
mod mob_gear;
mod mob_sounds;
mod mods;
mod mount_menu;
mod mounts;
mod moving_pistons;
mod music;
mod names;
mod mod_api;
mod native_hooks;
mod native_lifecycle;
mod native_menus;
mod native_panel;
mod native_panel_recipes;
mod native_recipes;
mod native_server;
mod native_shaders;
mod native_world_read;
mod natural_spawning;
mod nautili;
mod net_harness;
mod nether_portal;
mod no_resources;
mod ominous_item_spawner;
mod options;
mod packs;
mod pad;
mod pad_menu;
mod paintings;
mod pandas;
mod passengers;
mod persist;
mod perspective;
mod pick_entity;
mod pickups;
mod piglins;
mod piston_push;
mod player_equip;
mod postprocess;
mod potent_sulfur;
mod potions;
mod powder_snow;
mod pumpkin;
mod raids;
mod recipe_book;
mod remote;
mod remote_camera;
mod remote_chunks;
mod remote_mob_data;
mod remote_mob_particles;
mod remote_mob_use;
mod remote_objects;
mod remote_projectiles;
mod remote_stations;
mod remote_vehicles;
mod render_control;
mod render_tick;
mod respawn_anchor;
mod saves_control;
mod saving;
mod scoreboard;
mod screens;
mod sculk;
mod sculk_fixture;
mod server_links;
mod server_packs;
mod server_travel;
mod shader_packs;
mod shelf;
mod shield;
mod shoulders;
mod sign_edit;
mod sniffers;
mod spawner;
mod spear;
mod spectator;
mod spill;
mod splash_water;
mod startup_menu;
mod station_control;
mod stations;
mod stats;
mod stats_screen;
mod storage;
mod structures;
mod subtitle_hud;
mod sulfur_cubes;
mod text_displays;
mod titles;
mod tnt;
mod totem;
mod trading;
mod trial_spawner;
mod trial_visuals;
mod trident;
mod tutorial;
mod tweaks;
mod ui_control;
mod vault;
mod villagers;
mod wandering_trader;
mod win;
mod wind;
mod wither;
mod world_loading;
mod zombie_reinforcements;
mod zombie_villagers;
use vise_client::{
    Result,
    assets::{Assets, RenderAssets},
    audio::Audio,
    biome::Biomes,
    blocks::{BlockFacts, Fluid},
    clock::FixedStep,
    collisions,
    containers::{BlockEntity, Providers},
    crafting::Recipes,
    creative::{Creative, Names},
    entities::{Experience, ExperienceOrb, ItemEntity},
    environment::Environment,
    first_person::{Hand, SwingAnimation},
    furnace::FurnaceCooking,
    gfx::{Frame, Renderer},
    hud::{HudAtlas, HudBatch, rgba},
    hud_layout, hud_overlays,
    items::{ItemStack, Items},
    keymapping::KeyAction,
    light::LightTables,
    mesher::{MeshTables, Vertex},
    mobs::{Mob, MobType, MobTypes},
    models,
    motion::{MotionEvent, MotionResources, MotionWorld, PlayerMotion},
    mouse::{LookOptions, MouseLook},
    particles::Particle,
    save::{Level, Persistent, SaveDir},
    settings::Settings,
    sprint::Keys,
    streaming::{Streaming, WorkerContext},
    testgen::TestTerrain,
    title::{Action, Title},
    vanillagen::VanillaTerrain,
    world::World,
};
use vise_core::{
    biome::{BiomeZoom, obfuscate_seed},
    geometry::{Aabb, Direction},
    position::BlockPos,
};
use vise_core::{geometry::Vec3, random::RandomSource};
use vise_world::{
    block_collision::BlockCollisionRegistry,
    collision_context::{CollisionContext, CollisionEntity, CollisionItem},
    entity_obstruction::{EntityIdentity, EntityObstructionWorld, ObstructionEntity},
    level::{
        Behaviours, BucketContent, Dimension as LevelDimension, EntityQuery, Landing, LevelAccess,
        LevelEvent, PlaceContext, PlaceOutcome, PlayerContext, PlayerEffect, Ticks,
        flags as level_flags,
    },
    registry::BlockStateId,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    error::EventLoopError,
    event::{DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowId},
};

#[derive(Clone)]
struct Options {
    resources: PathBuf,
    distance: i32,
    distance_override: Option<i32>,
    seed: i64,
    creative: bool,
    test_terrain: bool,
    frames: Option<u64>,
    world_frames: Option<u64>,
    screenshot_after: Option<u64>,
    timeout: Duration,
    report: Option<PathBuf>,
    world: Option<String>,
    look: Option<(f32, f32)>,
    time: Option<i64>,
    weather: Option<String>,
    dimension: Option<String>,
    no_pause: bool,
    control: Option<PathBuf>,
    spikes: Option<f32>,
    offscreen: bool,
    window_size: [u32; 2],
    server: Option<String>,
    player_name: Option<String>,
    dedicated: Option<Vec<String>>,
    benchmark: Option<String>,
    dev_mods: Vec<PathBuf>,
    server_thread: bool,
    execution_role: execution_role::ExecutionRole,
    fullscreen: bool,
    game_dir: Option<PathBuf>,
    quick_play: Option<String>,
    session_file: Option<String>,
    session_stdin: bool,
}
fn warn_in_frame_deprecated(what: &str) {
    eprintln!(
        "Vise: DEPRECATED: {what} runs single player's world in the window, a path that is no longer supported, \
         gets no fixes and will be removed soon. Do not use it: single player runs on its server thread by default."
    );
}
impl Options {
    fn player_name(&self) -> String {
        self.player_name.clone().unwrap_or_else(|| {
            std::env::current_dir().map_or_else(
                |_| vise_client::offline_name::generate(0),
                |dir| vise_client::offline_name::load_or_create(&dir),
            )
        })
    }
    fn read() -> Result<Option<Self>> {
        let Some(mut o) = Self::parse(std::env::args().skip(1))
            .map_err(|e| launch_api::UsageError(e.to_string()))?
        else {
            return Ok(None);
        };
        let session = if o.session_stdin {
            let mut text = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                .map_err(|e| e.to_string())
                .and_then(|_| vise_client::auth::AuthSession::parse(&text))
                .map(Some)
        } else {
            o.session_file
                .take()
                .filter(|p| !p.is_empty())
                .map(|path| vise_client::auth::AuthSession::load(path.as_ref()))
                .transpose()
        };
        match session {
            Ok(Some(session)) => {
                o.player_name = Some(session.name.clone());
                vise_client::auth::set_current(session);
            }
            Ok(None) => {}
            Err(e) => eprintln!("Vise: not signed in: {e}"),
        }
        if let Some(dir) = o.game_dir.take() {
            o.resources = std::path::absolute(&o.resources)?;
            o.report = o.report.map(std::path::absolute).transpose()?;
            o.control = o.control.map(std::path::absolute).transpose()?;
            std::fs::create_dir_all(&dir)
                .and_then(|()| std::env::set_current_dir(&dir))
                .map_err(|e| format!("game folder {}: {e}", dir.display()))?;
        }
        if let Some(world) = o.quick_play.take() {
            if std::env::current_dir()?
                .join("saves")
                .join(&world)
                .join("level.dat")
                .is_file()
            {
                o.world = Some(world);
            } else {
                eprintln!("Quick play: there is no world {world} in saves/");
            }
        }
        Ok(Some(o))
    }
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let sibling = std::env::current_exe()?
            .parent()
            .ok_or("missing executable directory")?
            .join("resources");
        let mut o = Self {
            resources: if sibling.join("vise-resources.json").exists() {
                sibling
            } else {
                std::path::Path::new(".local/vise-client").join(vise_core::VANILLA_VERSION)
            },
            distance: 12,
            distance_override: None,
            seed: 0,
            creative: false,
            test_terrain: false,
            frames: None,
            world_frames: None,
            screenshot_after: None,
            timeout: Duration::from_secs(120),
            report: None,
            world: None,
            look: None,
            time: None,
            weather: None,
            dimension: None,
            no_pause: false,
            control: None,
            offscreen: false,
            window_size: [1280, 720],
            spikes: std::env::var("VISE_SPIKES")
                .ok()
                .and_then(|v| v.parse().ok()),
            server: None,
            player_name: std::env::var("VISE_PLAYER_NAME")
                .ok()
                .filter(|n| !n.is_empty()),
            dedicated: None,
            benchmark: None,
            dev_mods: Vec::new(),
            server_thread: std::env::var("VISE_SERVER_THREAD").map_or(true, |v| {
                let on = v != "0";
                if !on {
                    warn_in_frame_deprecated("VISE_SERVER_THREAD=0");
                }
                on
            }),
            execution_role: execution_role::ExecutionRole::Client,
            fullscreen: false,
            game_dir: None,
            quick_play: None,
            session_file: std::env::var("VISE_SESSION_FILE").ok(),
            session_stdin: false,
        };
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--help" | "-h" => {
                    println!(
                        "Vise — Minecraft Java Edition {} in Rust\n\n  --resources PATH        Prepared resources (vise-prepare)\n  --render-distance N     Chunks, 2..128 (default 12)\n  --world NAME            World folder under saves/ (default \"New World\")\n  --seed N                World seed for new worlds (default 0)\n  --width N, --height N   Window size (default 1280 x 720)\n  --creative              Creative mode (default survival)\n  --test-terrain          Simple test terrain instead of vanilla generation\n  --frames N              Exit after N frames (testing)\n  --world-frames N        Exit after N frames drawn in the world (testing)\n  --smoke-report PATH     Write a JSON report on exit\n  --benchmark SCENARIO    Run a benchmark (standard, village, forest, ocean, mountain), write its report to benchmarks/ and quit\n  --control               Accept vise-ctl commands on a local socket\n  --control-socket PATH   Control socket path (default $XDG_RUNTIME_DIR/vise-control.sock)\n  --server HOST[:PORT]    Join a multiplayer server (offline mode) at start\n  --quickPlaySingleplayer WORLD  Open this world of saves/ at start, only if it exists (vanilla's)\n  --quickPlayMultiplayer HOST[:PORT]  The same as --server (vanilla's name)\n  --name NAME             Player name for multiplayer (default: generated once, kept in vise-player.txt)\n  --session-file PATH     Signed-in account from the launcher (read once, then deleted)\n  --session-stdin         The same account as JSON on standard input\n  --game-dir DIR          Game folder: saves, options.txt, packs, mods (default: the working directory)\n  --home DIR              Logs and crash reports (default $VISE_HOME, else the platform data folder)\n  --fullscreen            Open the window fullscreen\n  --launch-api            Print the Launch API version and options as JSON (docs/api/launch.md)\n  --dev-mod PATH          A content mod (.vmod or folder) to develop: always on\n  --in-frame              DEPRECATED, do not use: unsupported, will be removed soon (also --no-server-thread)\n  --pregen                Add Vise's /pregen command (docs/pregeneration.md)\n\nControls: mouse look, WASD move, Space jump, Left Shift sneak, Left Ctrl sprint,\nleft click break, right click place/use, 1-9/scroll hotbar, E inventory,\nQ drop, F3 debug, Esc pause.",
                        vise_core::VANILLA_VERSION
                    );
                    return Ok(None);
                }
                "--launch-api" => {
                    println!("{}", launch_api::reply(&vise_client::crash::version()));
                    return Ok(None);
                }
                "--fullscreen" => o.fullscreen = true,
                "--session-stdin" => o.session_stdin = true,
                "--dedicated" => {
                    o.execution_role = execution_role::ExecutionRole::DedicatedServer;
                    o.dedicated = Some(args.by_ref().collect());
                    break;
                }
                "--creative" => o.creative = true,
                "--test-terrain" => o.test_terrain = true,
                "--no-pause" => o.no_pause = true,
                "--offscreen" => o.offscreen = true,
                "--control" => o.control = Some(vise_client::control::default_path()),
                "--server-thread" => o.server_thread = true,
                "--in-frame" | "--no-server-thread" => {
                    warn_in_frame_deprecated(&arg);
                    o.server_thread = false;
                }
                "--pregen" => vise_client::command::vanilla::set_vise_commands(true),
                _ => {
                    let value = args
                        .next()
                        .ok_or_else(|| format!("missing value after {arg}"))?;
                    let read = || -> Result<()> {
                        match arg.as_str() {
                            "--resources" => o.resources = value.into(),
                            "--render-distance" | "--radius" => {
                                o.distance = value.parse()?;
                                o.distance_override = Some(o.distance);
                                let max = vise_client::options_file::MAX_RENDER_DISTANCE;
                                if !(2..=max).contains(&o.distance) {
                                    return Err(format!("render distance must be 2..{max}").into());
                                }
                            }
                            "--seed" => {
                                o.seed = value.parse().unwrap_or_else(|_| {
                                    value
                                        .encode_utf16()
                                        .fold(0i32, |h, c| {
                                            h.wrapping_mul(31).wrapping_add(i32::from(c))
                                        })
                                        .into()
                                })
                            }
                            "--frames" => o.frames = Some(value.parse()?),
                            "--world-frames" => {
                                o.world_frames = Some(value.parse()?);
                                o.frames.get_or_insert(u64::MAX);
                            }
                            "--timeout-seconds" => o.timeout = Duration::from_secs(value.parse()?),
                            "--smoke-report" => o.report = Some(value.into()),
                            "--screenshot-after" => o.screenshot_after = Some(value.parse()?),
                            "--width" => o.window_size[0] = value.parse::<u32>()?.max(1),
                            "--height" => o.window_size[1] = value.parse::<u32>()?.max(1),
                            "--world" => o.world = Some(value),
                            "--quickPlaySingleplayer" => {
                                let mut parts = std::path::Path::new(&value).components();
                                if !matches!(
                                    (parts.next(), parts.next()),
                                    (Some(std::path::Component::Normal(_)), None)
                                ) {
                                    return Err("not a folder name of saves/".into());
                                }
                                o.quick_play = Some(value);
                            }
                            "--game-dir" | "--gameDir" => o.game_dir = Some(value.into()),
                            "--home" => {}
                            "--dev-mod" => o.dev_mods.push(std::path::absolute(&value)?),
                            "--benchmark" => {
                                if vise_client::benchmark::scenes(&value).is_none() {
                                    return Err(format!(
                                    "unknown benchmark {value}: standard, village, forest, ocean or mountain"
                                )
                                .into());
                                }
                                o.benchmark = Some(value);
                            }
                            "--server" | "--quickPlayMultiplayer" => o.server = Some(value),
                            "--name" => o.player_name = Some(value),
                            "--session-file" => o.session_file = Some(value),
                            "--control-socket" => o.control = Some(value.into()),
                            "--time" => o.time = Some(value.parse()?),
                            "--weather" => o.weather = Some(value),
                            "--dimension" => {
                                o.dimension = Some(if value.contains(':') {
                                    value
                                } else {
                                    format!("minecraft:{value}")
                                })
                            }
                            "--look" => {
                                let (y, p) =
                                    value.split_once(',').ok_or("--look takes YAW,PITCH")?;
                                o.look = Some((y.parse()?, p.parse()?));
                            }
                            _ => return Err("unknown option".into()),
                        }
                        Ok(())
                    };
                    read().map_err(|e| format!("{arg}: {e}"))?;
                }
            }
        }
        Ok(Some(o))
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Screen {
    Playing,
    GameModeSwitcher,
    Paused,
    Options,
    Chat,
    Creative,
    Inventory,
    Dead,
    Stats,
    Advancements,
    WorldOptions,
    Friends,
    CommandBlock,
    ServerLinks,
    ConfirmLink,
    DialogWarning,
    Win,
}
struct Mining {
    pos: [i32; 3],
    progress: f32,
    ticks: u32,
}
#[derive(Clone, Copy)]
struct Hit {
    pos: [i32; 3],
    face: [i32; 3],
    point: [f64; 3],
}
enum FallStep {
    Falling,
    Gone,
    Dropped(Vec3),
    Landed([i32; 3]),
}
#[derive(Clone, Copy)]
enum GuestUse {
    OnBlock(Hand),
    Item(Hand),
    Entity(Hand),
}

struct GameLevel<'w> {
    world: &'w mut World,
    facts: &'w BlockFacts,
    collision: &'w BlockCollisionRegistry<'w>,
    events: &'w mut Vec<LevelEvent>,
    dimension: LevelDimension,
    darken: i32,
    environment: &'w Environment,
    raining: bool,
    thundering: bool,
    player_box: Option<([f64; 3], [f64; 3])>,
    guest_boxes: Vec<([f64; 3], [f64; 3])>,
    player_collision: Option<PlayerCollision>,
    cart_boxes: Vec<([f64; 3], [f64; 3])>,
    item_frames: &'w [vise_client::entities::ItemFrame],
    carts: &'w [minecarts::Minecart],
    arrow_boxes: Vec<([f64; 3], [f64; 3])>,
    blockers: Blockers<'w>,
    dropped: &'w [ItemEntity],
    orbs: &'w [ExperienceOrb],
    projectiles: &'w [vise_client::projectiles::Projectile],
    biomes: &'w Biomes,
    seed: i64,
    behaviours: &'w Behaviours,
    items: &'w Items,
    recipes: &'w vise_client::crafting::Recipes,
    loot: Option<std::sync::Arc<vise_world::loot::LootData>>,
    registry: &'static vise_world::registry::BlockRegistry,
    rules: vise_world::level::GameRules,
    difficulty: i32,
    fire_players: Vec<[f64; 3]>,
    rand_sequences: &'w mut vise_client::random_sequences::RandomSequences,
    pending: &'w mut spill::Pending,
    spill: &'w mut spill::Spill,
    tickets: vise_client::ticket_levels::Levels,
    random_tick_sections: &'w mut vise_client::random_tick_sections::RandomTickSections,
    lid_openers: &'w HashMap<(i32, i32, i32), i32>,
    client_side: bool,
    native_authoritative: bool,
    native_particles: &'w mut Vec<(
        &'static str,
        vise_client::particles::ParticleOptions,
        Option<BlockStateId>,
        [f64; 3],
        [f64; 3],
        f64,
        i32,
    )>,
}
struct PlayerCollision {
    y: f64,
    descending: bool,
    fall_distance: f64,
    main_hand_item: Arc<str>,
    main_hand_count: i32,
    leather_boots: bool,
}
#[derive(Clone, Copy)]
struct Blockers<'w> {
    mobs: &'w [Mob],
    armor_stands: &'w [vise_client::entities::ArmorStand],
    boats: &'w [boats::Boat],
    crystals: &'w [dragon::Crystal],
    tnts: &'w [tnt::PrimedTnt],
    falling: &'w [FallingBlock],
}
struct BuildingObstruction(Vec<Aabb>);
impl EntityObstructionWorld for BuildingObstruction {
    type Error = std::convert::Infallible;
    fn obstruction_candidates(
        &mut self,
        _excluded: Option<EntityIdentity>,
        bounds: Aabb,
    ) -> std::result::Result<Vec<ObstructionEntity>, Self::Error> {
        Ok(self
            .0
            .iter()
            .filter(|b| b.intersects(bounds))
            .enumerate()
            .map(|(i, &bounds)| ObstructionEntity {
                identity: EntityIdentity(i as u64),
                root_vehicle: EntityIdentity(i as u64),
                bounds,
                removed: false,
                spectator: false,
                blocks_building: true,
            })
            .collect())
    }
}
impl GameLevel<'_> {
    fn mob_trigger_boxes(&self) -> impl Iterator<Item = Aabb> + '_ {
        self.blockers
            .mobs
            .iter()
            .filter(|m| !m.removed && m.species() != vise_client::mobs::Species::Bat)
            .map(Mob::bounds)
            .chain(
                self.blockers
                    .armor_stands
                    .iter()
                    .filter(|s| !s.marker)
                    .map(|s| s.bounds()),
            )
    }
    fn boat_trigger_boxes(&self) -> impl Iterator<Item = Aabb> + '_ {
        let b = self.blockers;
        b.boats.iter().map(boats::Boat::bounds)
    }
    fn other_trigger_boxes(&self) -> impl Iterator<Item = Aabb> + '_ {
        let b = self.blockers;
        let square = |p: Vec3, size: f64| {
            Aabb::new(
                Vec3::new(p.x - size / 2.0, p.y, p.z - size / 2.0),
                Vec3::new(p.x + size / 2.0, p.y + size, p.z + size / 2.0),
            )
        };
        b.boats
            .iter()
            .map(boats::Boat::bounds)
            .chain(b.crystals.iter().map(|c| {
                let p = c.position;
                Aabb::new(
                    Vec3::new(p.x - 1.0, p.y, p.z - 1.0),
                    Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0),
                )
            }))
            .chain(b.tnts.iter().map(tnt::PrimedTnt::bounds))
            .chain(
                b.falling
                    .iter()
                    .map(|f| vise_client::falling_block::bounds(f.position)),
            )
            .chain(self.dropped.iter().map(ItemEntity::bounds))
            .chain(self.orbs.iter().map(move |o| square(o.position, 0.5)))
            .chain(
                self.projectiles
                    .iter()
                    .map(move |p| square(p.position, p.kind.size())),
            )
    }
    fn detector_rail_signal(&self, pos: BlockPos, powered: bool) -> i32 {
        if !powered {
            return 0;
        }
        let (x, y, z) = (f64::from(pos.x), f64::from(pos.y), f64::from(pos.z));
        let area = Aabb::new(
            Vec3::new(x + 0.2, y, z + 0.2),
            Vec3::new(x + 0.8, y + 0.8, z + 0.8),
        );
        let mut container = None;
        for ((lo, hi), cart) in self.cart_boxes.iter().zip(self.carts) {
            let bounds = Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            );
            if !area.intersects(bounds) {
                continue;
            }
            match cart.kind {
                minecarts::CartKind::CommandBlock => return 0,
                minecarts::CartKind::Chest | minecarts::CartKind::Hopper => {
                    container = container.or_else(|| Some(self.cart_signal(cart)));
                }
                _ => {}
            }
        }
        container.unwrap_or(0)
    }
    fn cart_signal(&self, cart: &minecarts::Minecart) -> i32 {
        let key = (i32::MAX - 15, minecarts::CART_KEY_Y, cart.id as i32);
        let slots = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Chest(open)) => open,
            _ => &cart.items,
        };
        minecarts::Minecart::container_signal(slots, self.items)
    }
    fn building_blocker_boxes(&self) -> Vec<Aabb> {
        let boxed = |(lo, hi): &([f64; 3], [f64; 3])| {
            Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            )
        };
        let b = self.blockers;
        self.player_box
            .iter()
            .chain(&self.guest_boxes)
            .chain(&self.cart_boxes)
            .map(boxed)
            .chain(b.mobs.iter().filter(|m| !m.removed).map(Mob::bounds))
            .chain(
                b.armor_stands
                    .iter()
                    .filter(|s| !s.marker)
                    .map(|s| s.bounds()),
            )
            .chain(b.boats.iter().map(boats::Boat::bounds))
            .chain(b.crystals.iter().map(|c| {
                let p = c.position;
                Aabb::new(
                    Vec3::new(p.x - 1.0, p.y, p.z - 1.0),
                    Vec3::new(p.x + 1.0, p.y + 2.0, p.z + 1.0),
                )
            }))
            .chain(b.tnts.iter().map(tnt::PrimedTnt::bounds))
            .chain(
                b.falling
                    .iter()
                    .map(|f| vise_client::falling_block::bounds(f.position)),
            )
            .collect()
    }
}
impl LevelAccess for GameLevel<'_> {
    fn observe_block_changed(&self, pos: BlockPos, old: BlockStateId, new: BlockStateId) {
        if self.native_authoritative {
            self.native_block_changed(self.dimension, pos, old, new);
        }
    }
    fn observe_neighbor_changed(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        changed: vise_world::level::BlockId,
        moved: bool,
    ) {
        if self.native_authoritative {
            self.native_neighbor_changed(self.dimension, pos, state, changed, moved);
        }
    }
    fn is_client_side(&self) -> bool {
        self.client_side
    }
    fn world_surface(&self, x: i32, z: i32) -> i32 {
        self.world.world_surface(x, z, |s| self.facts.is_air(s))
    }
    fn precipitation(&self, pos: BlockPos) -> Option<vise_world::level::Precipitation> {
        self.behaviours.precipitation_info(self, pos)
    }
    fn is_raining_at(&self, pos: BlockPos) -> bool {
        self.behaviours.is_raining_at(self, pos)
    }
    fn block_entity_analog_signal(
        &self,
        pos: BlockPos,
        state: BlockStateId,
        direction: Direction,
    ) -> i32 {
        if let Some(s) = self.registry.state(state)
            && s.name == "minecraft:detector_rail"
        {
            return self.detector_rail_signal(
                pos,
                s.properties.get("powered").is_some_and(|v| v == "true"),
            );
        }
        if let Some(signal) = command_blocks::analog_signal(self.world, (pos.x, pos.y, pos.z)) {
            return signal;
        }
        if let Some(signal) = sculk::sensor_signal(self.world, self.behaviours, state, pos) {
            return signal;
        }
        if let Some((_, comparator)) = jukebox::jukebox_signals(self.world, self.items, pos) {
            return comparator;
        }
        if let Some(signal) = creaking::heart_signal(self.world, pos) {
            return if self.registry.state(state).is_some_and(|s| {
                s.properties
                    .get("creaking_heart_state")
                    .is_some_and(|v| v == "uprooted")
            }) {
                0
            } else {
                signal
            };
        }
        if let Some(signal) = books::lectern_signal(self.world, pos) {
            return signal;
        }
        if let Some(signal) =
            machines::shelf_signal(self.world, self.registry, state, pos, direction)
        {
            return signal;
        }
        machines::container_signal(self.world, self.behaviours, self.items, self.registry, pos)
    }
    fn item_frame_analog_output(&self, pos: BlockPos, facing: Direction) -> Option<i32> {
        let mut frames = self
            .item_frames
            .iter()
            .filter(|f| f.pos == [pos.x, pos.y, pos.z] && f.facing == facing as u8);
        let frame = frames.next().filter(|_| frames.next().is_none())?;
        Some(
            frame
                .item
                .as_ref()
                .map_or(0, |_| i32::from(frame.rotation % 8) + 1),
        )
    }
    fn block_entity_signal(
        &self,
        pos: BlockPos,
        _state: BlockStateId,
        _direction: Direction,
    ) -> i32 {
        if let Some(count) = self.lid_openers.get(&(pos.x, pos.y, pos.z)) {
            return (*count).clamp(0, 15);
        }
        jukebox::jukebox_signals(self.world, self.items, pos).map_or(0, |(signal, _)| signal)
    }
    fn biome_name(&self, pos: BlockPos) -> Option<&str> {
        let chunk = self.world.chunks.get(&(pos.x >> 4, pos.z >> 4))?;
        Some(&self.biomes.get(chunk.biome(pos.x, pos.y, pos.z)).name)
    }
    fn world_seed(&self) -> i64 {
        self.seed
    }
    fn day_time(&self) -> i64 {
        self.world.day_time
    }
    fn game_rules(&self) -> vise_world::level::GameRules {
        self.rules
    }
    fn difficulty(&self) -> i32 {
        self.difficulty
    }
    fn can_spread_fire_around(&self, pos: BlockPos) -> bool {
        let radius = self.rules.fire_spread_radius_around_player;
        radius == -1
            || self.fire_players.iter().any(|p| {
                let d = [
                    p[0] - f64::from(pos.x),
                    p[1] - f64::from(pos.y),
                    p[2] - f64::from(pos.z),
                ];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < f64::from(radius) * f64::from(radius)
            })
    }
    fn increased_fire_burnout(&self, pos: BlockPos) -> bool {
        self.world
            .chunks
            .get(&(pos.x >> 4, pos.z >> 4))
            .is_some_and(|chunk| {
                self.biomes
                    .get(chunk.biome(pos.x, pos.y, pos.z))
                    .increased_fire_burnout
            })
    }
    fn block_state(&self, pos: BlockPos) -> BlockStateId {
        self.world.block(pos.x, pos.y, pos.z)
    }
    fn set_raw(&mut self, pos: BlockPos, state: BlockStateId) -> Option<BlockStateId> {
        if !self.world.loaded(pos.x, pos.z)
            || pos.y < self.world.bottom_y()
            || pos.y >= self.world.top_y()
        {
            return None;
        }
        let previous = self.world.block(pos.x, pos.y, pos.z);
        self.world.set(pos.x, pos.y, pos.z, state).ok()?;
        Some(previous)
    }
    fn is_loaded(&self, pos: BlockPos) -> bool {
        self.world.loaded(pos.x, pos.z)
    }
    fn min_y(&self) -> i32 {
        self.world.shape.min_y
    }
    fn max_y(&self) -> i32 {
        self.world.shape.max_y()
    }
    fn random(&mut self) -> &mut vise_core::random::LegacyRandom {
        &mut self.world.random
    }
    fn game_time(&self) -> i64 {
        self.world.game_time
    }
    fn block_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.world.light(pos.x, pos.y, pos.z) & 15)
    }
    fn sky_light(&self, pos: BlockPos) -> i32 {
        i32::from(self.world.light(pos.x, pos.y, pos.z) >> 4)
    }
    fn sky_darken(&self) -> i32 {
        self.darken
    }
    fn sun_angle(&self, _pos: BlockPos) -> f32 {
        self.environment
            .sample(self.world.day_time, None, None)
            .sun_angle
    }
    fn is_raining(&self) -> bool {
        self.raining
    }
    fn is_thundering(&self) -> bool {
        self.thundering
    }
    fn dimension(&self) -> LevelDimension {
        self.dimension
    }
    fn is_unobstructed(&self, pos: BlockPos, state: BlockStateId) -> bool {
        let player = self.player_collision.as_ref();
        let context = player.map_or_else(
            || CollisionContext::empty(false),
            |player| {
                CollisionContext::for_entity(
                    CollisionEntity {
                        identity: EntityIdentity(0),
                        y: player.y,
                        descending: player.descending,
                        main_hand: Some(CollisionItem {
                            item: &player.main_hand_item,
                            count: player.main_hand_count,
                        }),
                    },
                    false,
                )
            },
        );
        let mut scene = collisions::SceneCollisions {
            world: &*self.world,
            binding: self.collision,
            subject: player.map(|player| vise_client::collisions::PowderSnowSubject {
                fall_distance: player.fall_distance,
                falling_block: false,
                leather_boots: player.leather_boots,
                walkable_type: false,
                lava_walker: false,
            }),
        };
        let Ok(shape) = self
            .collision
            .collision_shape_with_context(&mut scene, state, pos, &context)
        else {
            return false;
        };
        if shape.is_empty() {
            return true;
        }
        let shape = shape.moved(vise_core::geometry::Vec3::new(
            f64::from(pos.x),
            f64::from(pos.y),
            f64::from(pos.z),
        ));
        vise_world::entity_obstruction::is_unobstructed(
            &mut BuildingObstruction(self.building_blocker_boxes()),
            None,
            &shape,
        )
        .unwrap_or(false)
    }
    fn is_ticking_chunk(&self, chunk: vise_core::position::ChunkPos) -> bool {
        self.tickets.block_ticking(chunk.x, chunk.z) && ticking_chunk(self.world, chunk.x, chunk.z)
    }
    fn count_entities(&self, area: Aabb, query: EntityQuery) -> usize {
        let hit = |(lo, hi): &([f64; 3], [f64; 3])| {
            area.intersects(Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            ))
        };
        let carts = || self.cart_boxes.iter().filter(|b| hit(b)).count();
        let mobs = || {
            self.mob_trigger_boxes()
                .filter(|b| area.intersects(*b))
                .count()
        };
        let others = || {
            self.other_trigger_boxes()
                .filter(|b| area.intersects(*b))
                .count()
        };
        let arrows = || self.arrow_boxes.iter().filter(|b| hit(b)).count();
        let players = || {
            self.player_box
                .iter()
                .chain(&self.guest_boxes)
                .filter(|b| hit(b))
                .count()
        };
        match query {
            EntityQuery::Minecart => carts(),
            EntityQuery::Arrows => arrows(),
            EntityQuery::Mobs => players() + mobs(),
            EntityQuery::Everything => players() + carts() + mobs() + others(),
        }
    }
    fn emit(&mut self, event: LevelEvent) {
        match event {
            LevelEvent::DropResources { pos, state } if !self.client_side => {
                self.drop_resources_now(pos, state)
            }
            LevelEvent::DropResources { .. } => {}
            LevelEvent::Sound { .. } if self.client_side => {}
            LevelEvent::BlockEvent {
                kind: "note_custom_sound",
                ..
            } if self.client_side => {}
            LevelEvent::BlockEvent {
                pos,
                state,
                kind: "pop_block_item",
            } => self.pop_block_item(pos, state),
            LevelEvent::Player {
                pos,
                effect: PlayerEffect::DropItem(item),
                ..
            } => self.pop_above(pos, item),
            LevelEvent::Player {
                pos,
                state,
                effect: PlayerEffect::InteractLoot(table),
            } => self.drop_interact_loot(pos, state, table),
            event => self.events.push(event),
        }
    }
    fn remove_block_entity(&mut self, pos: BlockPos, old: BlockStateId, side_effects: bool) -> u8 {
        self.remove_entity(pos, old, side_effects)
    }
    fn load_block_entity(&mut self, pos: BlockPos, mut nbt: vise_world::nbt::Compound) {
        use vise_world::nbt::Tag;
        for (k, v) in [("x", pos.x), ("y", pos.y), ("z", pos.z)] {
            nbt.insert(k.into(), Tag::Int(v));
        }
        if let Some((key, entity)) = vise_client::save::decode_block_entity(&Tag::Compound(nbt))
        {
            self.world.block_entities.insert(key, entity);
        }
    }
    fn set_command_block_powered(&mut self, pos: BlockPos, powered: bool) -> Option<(bool, bool)> {
        command_blocks::set_powered(self.world, self.registry, (pos.x, pos.y, pos.z), powered)
    }
    fn reset_potent_sulfur_countdown(&mut self, pos: BlockPos) {
        if let Some(BlockEntity::PotentSulfur(sulfur)) =
            self.world.block_entities.get_mut(&(pos.x, pos.y, pos.z))
        {
            sulfur.waiting_countdown = -1;
        }
    }
}
#[derive(Clone, Debug)]
struct FallingBlock {
    id: u32,
    state: BlockStateId,
    start_pos: BlockPos,
    position: Vec3,
    previous: Vec3,
    velocity: Vec3,
    age: i32,
    no_gravity: bool,
    drop_item: bool,
    fall_distance: f64,
    stuck: Vec3,
    damage: vise_client::falling_block::Damage,
    saved: Option<Box<([i32; 4], vise_world::nbt::Tag)>>,
}
pub struct Resources {
    core: WorldCore,
    rest: std::sync::OnceLock<WorldRest>,
}

struct WorldCore {
    facts: BlockFacts,
    biomes: Arc<Biomes>,
    environment: Environment,
    collision: BlockCollisionRegistry<'static>,
    motion: MotionResources,
    threads: usize,
}

struct WorldRest {
    items: Items,
    recipes: Recipes,
    transformers: vise_client::block_transform::Transformers,
    mob_types: HashMap<String, Arc<MobType>>,
    mob_data: vise_client::mobs::MobData,
    providers: Providers,
    block_entity_render: vise_client::block_entity_render::BlockEntityRenderer,
    animate: vise_client::animate_tick::AnimateTables,
    fog: vise_client::fog::FogTables,
    music: vise_client::music::MusicTables,
    ambient: vise_client::ambience::AmbientTables,
    nether_environment: Environment,
    end_environment: Environment,
    behaviours: Behaviours,
    player_arm: [Option<vise_client::mobs::Part>; 2],
    player_arm_slim: [Option<vise_client::mobs::Part>; 2],
    player_type: Option<Arc<vise_client::mobs::MobType>>,
    spin_attack: Option<vise_client::mobs::Part>,
    stations: vise_client::stations::StationData,
}

struct WorldTables {
    files: Vec<(String, Vec<Vec<u8>>)>,
    items: Items,
    recipes: Recipes,
    stations: vise_client::stations::StationData,
}

impl Resources {
    fn rest(&self) -> &WorldRest {
        self.rest
            .get()
            .expect("world resources joined before entry")
    }
    fn facts(&self) -> &BlockFacts {
        &self.core.facts
    }
    fn items(&self) -> &Items {
        &self.rest().items
    }
    fn recipes(&self) -> &Recipes {
        &self.rest().recipes
    }
    fn transformers(&self) -> &vise_client::block_transform::Transformers {
        &self.rest().transformers
    }
    fn biomes(&self) -> &Arc<Biomes> {
        &self.core.biomes
    }
    fn environment(&self) -> &Environment {
        &self.core.environment
    }
    fn mob_types(&self) -> &HashMap<String, Arc<MobType>> {
        &self.rest().mob_types
    }
    fn mob_data(&self) -> &vise_client::mobs::MobData {
        &self.rest().mob_data
    }
    fn providers(&self) -> &Providers {
        &self.rest().providers
    }
    fn block_entity_render(&self) -> &vise_client::block_entity_render::BlockEntityRenderer {
        &self.rest().block_entity_render
    }
    fn animate(&self) -> &vise_client::animate_tick::AnimateTables {
        &self.rest().animate
    }
    fn fog(&self) -> &vise_client::fog::FogTables {
        &self.rest().fog
    }
    fn music(&self) -> &vise_client::music::MusicTables {
        &self.rest().music
    }
    fn ambient(&self) -> &vise_client::ambience::AmbientTables {
        &self.rest().ambient
    }
    fn nether_environment(&self) -> &Environment {
        &self.rest().nether_environment
    }
    fn end_environment(&self) -> &Environment {
        &self.rest().end_environment
    }
    fn behaviours(&self) -> &Behaviours {
        &self.rest().behaviours
    }
    fn collision(&self) -> &BlockCollisionRegistry<'static> {
        &self.core.collision
    }
    fn motion(&self) -> &MotionResources {
        &self.core.motion
    }
    fn threads(&self) -> usize {
        self.core.threads
    }
    fn player_arm(&self, slim: bool, left: bool) -> &Option<vise_client::mobs::Part> {
        let arms = if slim {
            &self.rest().player_arm_slim
        } else {
            &self.rest().player_arm
        };
        &arms[usize::from(left)]
    }
    fn player_type(&self) -> &Option<Arc<vise_client::mobs::MobType>> {
        &self.rest().player_type
    }
    fn spin_attack(&self) -> &Option<vise_client::mobs::Part> {
        &self.rest().spin_attack
    }
    fn stations(&self) -> &vise_client::stations::StationData {
        &self.rest().stations
    }
}

pub struct UiResources {
    pub atlas: HudAtlas,
    pub panorama: Arc<vise_client::gfx::panorama::Faces>,
}
impl UiResources {
    pub fn load(root: &std::path::Path, defaults: &std::path::Path) -> Result<Self> {
        Ok(Self {
            atlas: HudAtlas::build_over(root, defaults)?,
            panorama: Arc::new(vise_client::gfx::panorama::Faces::load(root)),
        })
    }
    pub fn server(root: &std::path::Path) -> Result<Self> {
        Ok(Self {
            atlas: HudAtlas::fonts(root)?,
            panorama: Arc::new(vise_client::gfx::panorama::Faces {
                size: 1,
                pixels: [0u8; 4].repeat(6),
            }),
        })
    }
}
#[derive(Clone)]
pub struct PackResources {
    assets: Arc<Assets>,
    render: Arc<RenderAssets>,
    context: Arc<WorkerContext>,
    layer_names: Arc<HashMap<u32, String>>,
    item_renderer: Arc<vise_client::item_render::ItemRenderer>,
    names: Arc<Names>,
    clouds: Arc<(u32, u32, Vec<bool>)>,
    forcefield: Arc<(u32, u32, Vec<u8>)>,
}
impl PackResources {
    fn load(
        root: &std::path::Path,
        assets: Arc<Assets>,
        render: Arc<RenderAssets>,
        facts: &BlockFacts,
        biomes: Arc<Biomes>,
        light: Arc<LightTables>,
    ) -> Result<Self> {
        let visibility = models::load_visibility(root, assets.registry)?;
        let tables = Arc::new(MeshTables::new(&assets, &render, facts, visibility)?);
        let item_renderer = vise_client::item_render::ItemRenderer::load_with_library(
            root,
            render.model_library.clone(),
        )?;
        Ok(Self {
            layer_names: Arc::new(
                render
                    .sprite_layers
                    .iter()
                    .map(|(name, layer)| (*layer, name.trim_start_matches("minecraft:").to_owned()))
                    .collect(),
            ),
            context: Arc::new(WorkerContext {
                assets: assets.clone(),
                render: render.clone(),
                tables,
                biomes,
                light,
            }),
            assets,
            render,
            item_renderer: Arc::new(item_renderer),
            names: Arc::new(Names::load(root)),
            clouds: Arc::new(load_clouds(root)),
            forcefield: Arc::new(load_forcefield(root)),
        })
    }
}
struct DragView {
    slots: HashMap<usize, Option<(u32, bool)>>,
    left: Option<u32>,
}
fn ticking_chunk(world: &World, x: i32, z: i32) -> bool {
    world.chunks.get(&(x, z)).is_some_and(|c| c.lit)
        && (-1..=1).all(|dz| (-1..=1).all(|dx| world.chunks.contains_key(&(x + dx, z + dz))))
}
fn follow_keymap(event: &mut WindowEvent) {
    use winit::keyboard::{Key, NamedKey};
    if let WindowEvent::KeyboardInput { event, .. } = event
        && let Key::Named(named) = &event.logical_key
    {
        let code = match named {
            NamedKey::Escape => KeyCode::Escape,
            NamedKey::CapsLock => KeyCode::CapsLock,
            _ => return,
        };
        event.physical_key = PhysicalKey::Code(code);
    }
}

fn drop_whole_stack(physical_keys: &HashSet<KeyCode>) -> bool {
    physical_keys.contains(&KeyCode::ControlLeft) || physical_keys.contains(&KeyCode::ControlRight)
}

fn select_creative_tab(
    creative: &mut Creative,
    clicks: &mut vise_client::container_screen::Clicks,
    tab: usize,
) {
    creative.select(tab);
    *clicks = Default::default();
    clicks.skip_release();
}

struct WorldIcon {
    path: PathBuf,
    last_attempt: Option<Instant>,
}

struct Game<'a> {
    res: Arc<Resources>,
    rest_loader: Option<WorldRestLoader>,
    pending_entry: Option<PendingWorldEntry>,
    world_loader: Option<world_loading::WorldLoader>,
    pending_pack_reload: bool,
    pack_reload: Option<packs::PackReload>,
    server_packs: server_packs::ServerPackState,
    link_prompt: Option<String>,
    code_of_conduct: Option<code_of_conduct::Prompt>,
    dialogs: dialogs::DialogState,
    integrated_starting: Option<integrated::Starting>,
    world_renderer_ready: bool,
    collision: &'a BlockCollisionRegistry<'a>,
    motion: &'a MotionResources,
    pack: PackResources,
    gui: UiResources,
    fog: vise_client::fog::FogState,
    screen_fx: vise_client::screen_effects::ScreenEffects,
    item_activation: vise_client::item_activation::ItemActivation,
    weather_parameters: vise_client::weather::ParameterCache,
    panorama_rotation: vise_client::gfx::panorama::Rotation,
    world: World,
    streaming: Streaming,
    lod: Option<vise_client::lod::Lod>,
    lod_source_attempted: bool,
    lod_observe: vise_client::fast_hash::FxHashSet<(i32, i32)>,
    far_beacons: HashMap<(i32, i32, i32), Vec<(u32, i32)>>,
    lod_covered_at: Option<(u64, (i32, i32), i32)>,
    options: Options,
    player: Option<PlayerMotion>,
    spawn: Option<Vec3>,
    clock: FixedStep,
    previous_position: Vec3,
    eye: f32,
    previous_eye: f32,
    yaw: f32,
    pitch: f32,
    inventory: Vec<Option<ItemStack>>,
    selected: usize,
    carried: Option<ItemStack>,
    craft_grid: Vec<Option<ItemStack>>,
    craft_size: usize,
    craft_table: Option<[i32; 3]>,
    inv_drag: Option<(i32, Vec<usize>)>,
    inv_clicks: vise_client::container_screen::Clicks,
    tweaks: tweaks::Tweaks,
    bundle_mouse: bundles::BundleMouse,
    last_inv_click: Option<(std::time::Instant, usize, bool)>,
    last_quick_moved: Option<ItemStack>,
    container: Option<(i32, i32, i32)>,
    station: Option<stations::Station>,
    potions: potions::PotionState,
    scratch_slot: Option<ItemStack>,
    mining: Option<Mining>,
    destroying: HashMap<i32, ([i32; 3], u8, i64)>,
    destroy_delay: u32,
    use_delay: u32,
    creative: bool,
    mode: vise_client::command::args::GameMode,
    keys: HashSet<KeyCode>,
    physical_keys: HashSet<KeyCode>,
    physical_buttons: HashSet<MouseButton>,
    left: bool,
    right: bool,
    cursor: (f32, f32),
    cursor_icon: vise_client::widgets::Cursor,
    captured: bool,
    keyboard_mouse: bool,
    view_at: [u32; 2],
    focused: bool,
    focus_lost: Option<Instant>,
    modifiers: winit::keyboard::ModifiersState,
    fullscreen: vise_client::window::FullscreenState,
    mouse: MouseLook,
    agent_pointer: bool,
    smooth_camera: bool,
    toggles: vise_client::keymapping::ToggleKeys,
    was_playing: bool,
    pause: bool,
    debug: bool,
    screen: Screen,
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    entity_vertices: Vec<Vertex>,
    dynamic_lights: Vec<[f32; 4]>,
    luminance: Option<vise_client::dynamic_light::Luminance>,
    see_through_vertices: Vec<Vertex>,
    entity_spans: std::cell::RefCell<Vec<vise_client::gfx::pack_draw::EntitySpan>>,
    glowing_vertices: std::cell::RefCell<Vec<Vertex>>,
    pack_ids: Option<shader_packs::PackIds>,
    started: Instant,
    spectator_gui: vise_client::spectator_menu::SpectatorGui,
    previous: Instant,
    last_input: Instant,
    pad: pad::Pads,
    frames: u64,
    glint_frame: vise_client::item_render::GlintFrame,
    item_level_cache: std::cell::RefCell<
        Option<(
            (u64, i64, i64, &'static str),
            vise_assets::item_model::LevelState,
        )>,
    >,
    world_frames: u64,
    fps: f32,
    fps_frames: u64,
    fps_at: Instant,
    ticks: u64,
    health_hud: hud_layout::HealthHud,
    error: Option<Box<dyn std::error::Error>>,
    message: Option<(String, Instant)>,
    save: Arc<SaveDir>,
    level: Level,
    session_lock: Option<vise_client::save::SessionLock>,
    audio: Audio,
    music: vise_client::music::MusicManager,
    ambience: vise_client::ambience::Ambience,
    sound_paused: bool,
    cart_sounds: HashSet<u32>,
    riding_cart_sounds: Option<u32>,
    was_fall_flying: bool,
    bee_sounds: HashMap<u32, bool>,
    guardian_sounds: HashSet<u32>,
    dropped: Vec<ItemEntity>,
    next_entity: u32,
    particles: vise_client::particles::ParticleEngine,
    pickups: pickups::Pickups,
    sign_edit: Option<sign_edit::SignEditor>,
    book: Option<books::BookScreen>,
    book_last_click: Option<std::time::Instant>,
    lectern_pulses: Vec<((i32, i32, i32), i32)>,
    loading_progress: f32,
    spawn_origin: Option<((i32, i32), i32)>,
    spawn_spiral: vise_client::world_spawn::ChunkSpiral,
    player_spawn: Option<vise_client::world_spawn::PlayerSpawnFinder>,
    spawn_wait: Vec<(i32, i32)>,
    mobs: Vec<Mob>,
    creatures: creatures::Creatures,
    sculk: sculk::SculkState,
    raiding: raids::Raiding,
    attack_ticks: u32,
    attack_item: Option<Arc<str>>,
    last_hurt_by_player: HashMap<u32, Option<u32>>,
    love_cause: HashMap<u32, Option<u32>>,
    died: HashSet<u32>,
    be_anims: vise_client::block_entity_anim::BlockEntityAnims,
    using: Option<(usize, u32)>,
    view: vise_client::camera::Camera,
    limiter: vise_client::window::FramerateLimiter,
    frame_clock: vise_client::window::SystemClock,
    scope_scale: f32,
    frame_ticks: f32,
    title: Option<Title>,
    armor: [Option<ItemStack>; 4],
    offhand: Option<ItemStack>,
    hotbar_pop: [u8; 10],
    hand: Hand,
    shield_cooldown: u32,
    cooldowns: vise_client::cooldowns::Cooldowns,
    command_source: Option<commands::execute::Source>,
    titles: titles::Titles,
    tool_highlight: vise_client::hud_overlays::ToolHighlight,
    saving_indicator: vise_client::hud_overlays::SavingIndicator,
    autosaving: bool,
    applied_modifiers: Vec<(String, String)>,
    experience: Experience,
    orbs: Vec<ExperienceOrb>,
    xp_delay: u32,
    last_level_up: u64,
    swing: vise_client::first_person::SwingState,
    swings_started: u32,
    heights: vise_client::first_person::HandHeights,
    sway: vise_client::first_person::HandSway,
    visible_hands: [(usize, Option<ItemStack>); 2],
    hands_replaced: [bool; 2],
    clicks: vise_client::first_person::Clicks,
    attack_clicks: u32,
    hand_strength: vise_client::attack_strength::AttackStrength,
    settings: Settings,
    tutorial: tutorial::Tutorial,
    tutorial_toasts: Vec<tutorial::HintToast>,
    options_from_title: bool,
    dragging: bool,
    avatar: vise_client::camera::AvatarBob,
    flash: u32,
    rain_sound_time: i32,
    lightning_seen: [u32; 2],
    sleeping: Option<[i32; 3]>,
    sleep_timer: u32,
    player_beds: Vec<(Dim, [i32; 3])>,
    sleep_seen: (i64, i64),
    dim: Dim,
    world_root: PathBuf,
    world_tables: Option<WorldTables>,
    world_flat: bool,
    travelling: Option<(PlayerMotion, Vec3)>,
    server_trips: Vec<server_travel::Trip>,
    level_teleports: Vec<server_travel::LevelTeleport>,
    levels: levels::Levels,
    level_pass: levels::Pass,
    portals: nether_portal::Portals,
    end_travel: bool,
    respawn_config: Option<respawn_anchor::RespawnConfig>,
    respawn_travel: Option<respawn_anchor::RespawnConfig>,
    end_portal_travel: bool,
    win: Option<win::Win>,
    end: end::EndState,
    block_ticks: Ticks,
    tick_chunks: vise_client::tick_chunk_membership::TickChunkMembership,
    tickets: vise_client::ticket_levels::ChunkTickets,
    random_tick_sections: vise_client::random_tick_sections::RandomTickSections,
    level_events: Vec<LevelEvent>,
    falling: Vec<FallingBlock>,
    paintings: Vec<vise_client::entities::Painting>,
    painting_variants: paintings::PaintingVariants,
    item_frames: Vec<vise_client::entities::ItemFrame>,
    frame_model: frames::FrameModel,
    glow_frame_model: frames::FrameModel,
    map_frame_model: frames::FrameModel,
    glow_map_frame_model: frames::FrameModel,
    armor_stands: Vec<vise_client::entities::ArmorStand>,
    cushions: Vec<vise_client::entities::Cushion>,
    others: Vec<commands::OtherEntity>,
    armor_stand_assets: armor_stands::ArmorStandAssets,
    waypoint_styles: vise_client::locator_bar::WaypointStyles,
    waypoints: vise_client::locator_bar::ClientWaypoints,
    locator_camera: Option<([f64; 3], f32, f32, f32)>,
    experience_display: (f32, bool, u64),
    boats: Vec<boats::Boat>,
    boat_models: boats::BoatModels,
    crystal_model: Option<vise_client::mobs::Part>,
    cushion_model: Option<vise_client::mobs::Part>,
    carts: Vec<minecarts::Minecart>,
    jukebox_sounds: HashMap<(i32, i32, i32), u64>,
    bolts: Vec<lightning::Bolt>,
    maps: maps::Maps,
    fight: dragon::Fight,
    dragon: Option<dragon::Dragon>,
    crystals: Vec<dragon::Crystal>,
    dragon_fireballs: Vec<dragon::DragonFireball>,
    breath_clouds: Vec<dragon::Breath>,
    dragon_model: Option<vise_client::mobs::Part>,
    map_patches: Vec<vise_client::font::Patch>,
    text_displays: std::collections::HashMap<i32, text_displays::RemoteTextDisplay>,
    remote_knots: HashMap<u32, [i32; 3]>,
    knot_ids: HashMap<[i32; 3], u32>,
    text_display_vertices: usize,
    skin_downloads: RefCell<vise_client::skins::Downloader>,
    skin_profiles: RefCell<vise_client::skins::Resolver>,
    skin_slots: vise_client::skins::SkinSlots,
    trim_slots: Vec<(String, usize)>,
    cart_models: minecarts::CartModels,
    riding: Option<u32>,
    pending_vehicle: Option<[i32; 4]>,
    ender_items: Vec<Option<ItemStack>>,
    ender_open: Option<(i32, i32, i32)>,
    pending: spill::Pending,
    spill: spill::Spill,
    recipe_ui: recipe_book::RecipeUi,
    mount_menu: mount_menu::MountMenu,
    hook: Option<fishing::Hook>,
    remote_hooks: Vec<fishing::RemoteHook>,
    tridents: Vec<trident::ThrownTrident>,
    rockets: Vec<fireworks::Rocket>,
    ominous_spawners: Vec<ominous_item_spawner::OminousItemSpawner>,
    creaking: creaking::CreakingState,
    spawners: spawner::SpawnerState,
    spawn_scratch: natural_spawning::SpawnScratch,
    trident_model: Option<vise_client::mobs::Part>,
    arrow_model: Option<vise_client::mobs::Part>,
    tnts: Vec<tnt::PrimedTnt>,
    hoppers: HashMap<(i32, i32, i32), machines::HopperState>,
    item_columns: Option<machines::ItemColumns>,
    creative_screen: Creative,
    control: Option<vise_client::control::Server>,
    frame_stats: vise_client::control::FrameStats,
    perf: vise_client::telemetry::Session,
    pending_screenshot: Option<std::sync::mpsc::Sender<String>>,
    pending_bench: Option<(Instant, std::sync::mpsc::Sender<String>)>,
    frame_graph: vise_client::perf_overlay::FrameGraph,
    extras: extras::ExtrasState,
    mods: mods::ModsState,
    recipe_tables: native_recipes::RecipeTables,
    panel: native_panel::Panel,
    native_shaders: native_shaders::NativeShaders,
    native_lifecycle: native_lifecycle::NativeLifecycle,
    pending_uploads: Vec<((i32, i32, i32), vise_client::mesher::SectionMesh)>,
    view_frustum: Option<vise_client::gfx::Frustum>,
    entity_culler: Option<vise_client::entity_culling::Culler>,
    pack_shadow_casters: (bool, bool),
    flight: Option<control::Flight>,
    agent: agent::Agent,
    ui_wait: ui_control::UiWait,
    events: events::Events,
    sky_darken_memo: std::cell::Cell<Option<((i64, u32, u32), i32)>>,
    persist: persist::EntityMemory,
    remote: Option<remote::Remote>,
    lan: Option<lan::LanHost>,
    integrated: Option<integrated::Integrated>,
    level_shapes: vise_client::chunk::LevelShapes,
    serving: Option<integrated::Serving>,
    friends: Option<friends::Sharing>,
    world_options: Option<lan_screen::WorldOptions>,
    options_to_world_options: bool,
    command_block_screen: Option<command_blocks::CommandBlockScreen>,
    debug_keys: vise_client::debug_keys::DebugKeys,
    debug_charts: vise_client::debug_chart::DebugCharts,
    show_hitboxes: bool,
    show_chunk_borders: bool,
    pause_menu: bool,
    previous_mode: Option<vise_client::command::args::GameMode>,
    gamemode_switcher: Option<vise_client::game_mode_switcher::GameModeSwitcher>,
    clipboard: vise_client::clipboard::SystemClipboard,
    text_selection: vise_client::clipboard::AppendSelection,
    saved_for_exit: bool,
    pending_save: Option<saving::ClosingSave>,
    world_icon: Option<WorldIcon>,
    forced_hit: Option<Hit>,
    tick_hit: Option<Option<Hit>>,
    guest_use: Option<GuestUse>,
    lan_events: Vec<(i32, [i32; 3], i32, Option<u32>)>,
    lan_songs: Vec<([i32; 3], Option<String>)>,
    lan_cushion_breaks: Vec<(vise_world::registry::BlockStateId, [f64; 3])>,
    lan_block_events: Vec<([i32; 3], i32, i32, u32)>,
    lan_entity_events: Vec<(i32, i8, [f64; 3])>,
    lan_critical_hits: Vec<(Option<u32>, i32, bool)>,
    lan_kinetic_hits: Vec<Option<u32>>,
    stabbed: HashMap<(Option<u32>, u32), i64>,
    known_movement: Vec3,
    kinetic_feedback: HashMap<i32, i64>,
    lan_global_events: Vec<(i32, [i32; 3], i32)>,
    lid_openers: HashMap<(i32, i32, i32), i32>,
    lan_acting: Option<u32>,
    lan_break_pos: Option<[i32; 3]>,
    lan_acting_name: Option<String>,
    lan_acting_uuid: Option<u128>,
    lan_acting_permission: Option<u8>,
    chat_capture: Option<Vec<vise_client::text::Text>>,
    guest_packets: Vec<vise_protocol::packets::play::Clientbound>,
    guest_cooldowns: Vec<(Arc<str>, i32)>,
    guest_entity_events: Vec<i8>,
    guest_player_sounds: Vec<(String, [f64; 3], f32, f32)>,
    admin_feedback: Vec<(vise_client::text::Text, bool, bool)>,
    console_broadcast: Option<bool>,
    cmds: commands::extra::CommandExtras,
    ui: chat::UiState,
    combat: combat::CombatTracker,
    guest_combat: std::collections::HashMap<u32, combat::CombatTracker>,
    mount: mounts::MountState,
    stats: stats::Stats,
    score: scoreboard::ScoreState,
    fns: functions::FnState,
    bosses: bossbars::BossState,
    boss_overlay: vise_client::boss_overlay::BossOverlay,
    exec: commands::execute::ExecState,
    rand_sequences: vise_client::random_sequences::RandomSequences,
    brushing: brush::Brushing,
    border: vise_client::worldborder::WorldBorder,
    border_renderer: vise_client::border_render::BorderRenderer,
    border_state: vise_client::border_render::BorderRenderState,
    tab_list: vise_client::tab_list::TabList,
    subtitles: subtitle_hud::SubtitleState,
    adv: advancements::AdvState,
    restore_player_state: bool,
    stats_ui: stats_screen::StatsUi,
    adv_ui: advancements_screen::AdvUi,
    links_ui: server_links::LinksUi,
    camera_type: perspective::CameraType,
    fangs: fangs::FangState,
    player_model: Option<Mob>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Dim {
    Overworld,
    Nether,
    End,
}
impl Dim {
    fn id(self) -> &'static str {
        match self {
            Self::Overworld => "minecraft:overworld",
            Self::Nether => "minecraft:the_nether",
            Self::End => "minecraft:the_end",
        }
    }
    fn from_id(id: &str) -> Self {
        match id {
            "minecraft:the_nether" => Self::Nether,
            "minecraft:the_end" => Self::End,
            _ => Self::Overworld,
        }
    }
    fn shape(self) -> vise_client::save::LevelShape {
        use vise_client::save::LevelShape;
        match self {
            Self::Overworld => LevelShape::OVERWORLD,
            Self::Nether => LevelShape::NETHER,
            Self::End => LevelShape::END,
        }
    }
}
fn facing_offset(facing: &str) -> [i32; 3] {
    match facing {
        "north" => [0, 0, -1],
        "south" => [0, 0, 1],
        "west" => [-1, 0, 0],
        "east" => [1, 0, 0],
        "up" => [0, 1, 0],
        _ => [0, -1, 0],
    }
}
fn ray_box(eye: [f64; 3], dir: [f64; 3], reach: f64, b: Aabb) -> Option<f64> {
    let (min, max) = (b.min_position(), b.max_position());
    let (lo, hi) = ([min.x, min.y, min.z], [max.x, max.y, max.z]);
    let (mut t0, mut t1) = (0.0f64, reach);
    for a in 0..3 {
        if dir[a].abs() < 1e-9 {
            if eye[a] < lo[a] || eye[a] > hi[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((lo[a] - eye[a]) / dir[a], (hi[a] - eye[a]) / dir[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        t0 = t0.max(ta);
        t1 = t1.min(tb);
    }
    (t0 <= t1).then_some(t0)
}
fn view_vector(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
fn camera_rotation(camera_type: perspective::CameraType, yaw: f32, pitch: f32) -> glam::Quat {
    let (view_yaw, view_pitch) = if camera_type == perspective::CameraType::ThirdPersonFront {
        (yaw + 180.0, -pitch)
    } else {
        (yaw, pitch)
    };
    glam::Quat::from_euler(
        glam::EulerRot::YXZ,
        std::f32::consts::PI - view_yaw.to_radians(),
        -view_pitch.to_radians(),
        0.0,
    )
}
impl Game<'_> {
    fn eye_position(&self) -> Option<[f64; 3]> {
        let p = self.player.as_ref()?;
        Some([
            p.position.x,
            p.position.y + f64::from(p.dimensions.eye_height),
            p.position.z,
        ])
    }
    fn aim(&self) -> Option<([f64; 3], [f64; 3])> {
        if let Some(m) = self.camera_mob() {
            return Some((
                [m.position.x, m.eye_y(), m.position.z],
                view_vector(m.head_yaw, m.head_pitch),
            ));
        }
        Some((self.eye_position()?, view_vector(self.yaw, self.pitch)))
    }
    fn reach(&self) -> f64 {
        if self.creative { 5.0 } else { 4.5 }
    }
    fn context_outline(&self, pos: [i32; 3], state: BlockStateId) -> Option<[f64; 6]> {
        let name = &self.res.facts().get(state).name;
        if matches!(&**name, "minecraft:light" | "minecraft:scaffolding") {
            return self
                .held()
                .is_some_and(|s| *s.item == **name)
                .then_some([0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
        }
        if !name.ends_with("shulker_box") {
            return None;
        }
        let progress = self.be_anims.shulkers.get(&pos).map_or(0.0, |s| s.progress);
        let facing = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("facing").cloned());
        let d = facing_offset(facing.as_deref().unwrap_or("up"));
        let mut b = [0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        for a in 0..3 {
            match d[a] {
                1 => b[a + 3] += 0.5 * f64::from(progress),
                -1 => b[a] -= 0.5 * f64::from(progress),
                _ => {}
            }
        }
        Some(b)
    }
    fn outline_boxes(&self, pos: [i32; 3]) -> Vec<[f64; 6]> {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if let Some(b) = self.context_outline(pos, state) {
            return vec![std::array::from_fn(|i| f64::from(pos[i % 3]) + b[i])];
        }
        let f = self.res.facts().get(state);
        let o = if f.outline_offset {
            self.pack
                .context
                .tables
                .offset(state, pos[0], pos[2])
                .unwrap_or([0.0; 3])
        } else {
            [0.0; 3]
        };
        let base = [
            f64::from(pos[0]) + o[0],
            f64::from(pos[1]) + o[1],
            f64::from(pos[2]) + o[2],
        ];
        f.outline
            .iter()
            .map(|b| std::array::from_fn(|i| base[i % 3] + b[i]))
            .collect()
    }
    fn pick(&self) -> Option<Hit> {
        if let Some(hit) = self.forced_hit {
            return Some(hit);
        }
        if let Some(hit) = self.tick_hit {
            return hit;
        }
        let (origin, dir) = self.aim()?;
        self.clip_ray(origin, dir, self.reach())
    }
    fn clip_ray(&self, origin: [f64; 3], dir: [f64; 3], reach: f64) -> Option<Hit> {
        let mut pos = origin.map(|v| v.floor() as i32);
        let step = dir.map(|d| if d > 0.0 { 1 } else { -1 });
        let delta = dir.map(|d| {
            if d == 0.0 {
                f64::INFINITY
            } else {
                (1.0 / d).abs()
            }
        });
        let mut t_max = [0.0; 3];
        for i in 0..3 {
            let boundary = if dir[i] > 0.0 {
                f64::from(pos[i]) + 1.0
            } else {
                f64::from(pos[i])
            };
            t_max[i] = if dir[i] == 0.0 {
                f64::INFINITY
            } else {
                (boundary - origin[i]) / dir[i]
            };
        }
        let mut face = [0; 3];
        let mut t = 0.0;
        while t <= reach {
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let f = self.res.facts().get(state);
            if !self.res.facts().is_air(state)
                && (f.fluid == Fluid::None
                    || f.waterlogged
                    || &*f.name == "minecraft:kelp"
                    || f.render_shape == vise_client::blocks::RenderShape::Model)
                && !matches!(
                    &*f.name,
                    "minecraft:water" | "minecraft:lava" | "minecraft:bubble_column"
                )
            {
                let mut best: Option<(f64, [i32; 3])> = None;
                for b in self.outline_boxes(pos) {
                    let (mut t0, mut t1, mut axis_in, mut sign) = (0.0f64, f64::INFINITY, None, 0);
                    let mut miss = false;
                    for a in 0..3 {
                        if dir[a] == 0.0 {
                            miss |= origin[a] < b[a] || origin[a] > b[a + 3];
                            continue;
                        }
                        let (lo, hi) =
                            ((b[a] - origin[a]) / dir[a], (b[a + 3] - origin[a]) / dir[a]);
                        let (near, far) = if lo < hi { (lo, hi) } else { (hi, lo) };
                        if near > t0 {
                            t0 = near;
                            axis_in = Some(a);
                            sign = if dir[a] > 0.0 { -1 } else { 1 };
                        }
                        t1 = t1.min(far);
                    }
                    if miss || t0 > t1 || t0 > reach {
                        continue;
                    }
                    let mut normal = face;
                    if let Some(a) = axis_in {
                        normal = [0; 3];
                        normal[a] = sign;
                    }
                    if best.is_none_or(|(bt, _)| t0 < bt) {
                        best = Some((t0, normal));
                    }
                }
                if let Some((t, face)) = best {
                    return Some(Hit {
                        pos,
                        face,
                        point: [
                            origin[0] + dir[0] * t,
                            origin[1] + dir[1] * t,
                            origin[2] + dir[2] * t,
                        ],
                    });
                }
            }
            let axis = if t_max[0] < t_max[1] {
                if t_max[0] < t_max[2] { 0 } else { 2 }
            } else if t_max[1] < t_max[2] {
                1
            } else {
                2
            };
            t = t_max[axis];
            t_max[axis] += delta[axis];
            pos[axis] += step[axis];
            face = [0; 3];
            face[axis] = -step[axis];
        }
        None
    }
    fn held(&self) -> Option<&ItemStack> {
        self.stack_in(self.held_slot())
    }
    fn held_mut(&mut self) -> &mut Option<ItemStack> {
        self.stack_in_mut(self.held_slot())
    }
    fn glider_worn(&self) -> bool {
        self.armor[1].as_ref().is_some_and(|s| {
            self.res
                .items()
                .get(&s.item)
                .is_some_and(|i| i.glider && (i.max_damage == 0 || s.damage + 1 < i.max_damage))
        })
    }
    pub(crate) fn wears_freeze_immune(&self) -> bool {
        let immune = self
            .res
            .items()
            .item_tags
            .get("minecraft:freeze_immune_wearables");
        self.armor
            .iter()
            .flatten()
            .any(|s| immune.is_some_and(|t| t.contains(&*s.item)))
    }
    pub(crate) fn sync_player_body(&mut self) {
        let glider = self.glider_worn();
        let sleeping = self.sleeping.is_some();
        let item_in_use = self
            .using
            .and_then(|(slot, _)| {
                if slot == shield::OFFHAND_SLOT {
                    self.offhand.as_ref()
                } else {
                    self.inventory.get(slot)?.as_ref()
                }
            })
            .and_then(|s| self.items().get(&s.item));
        let item_use_speed = item_in_use.map(|i| i.use_speed);
        let item_use_slows_sprint = item_in_use.is_some_and(|i| !i.can_sprint);
        let respiration = self.armor[0]
            .as_ref()
            .map_or(0, |s| s.enchantment_level("minecraft:respiration") as i32);
        let leather_boots = self.armor[3]
            .as_ref()
            .is_some_and(|s| &*s.item == "minecraft:leather_boots");
        let freeze_immune = self.wears_freeze_immune();
        let burn_immune = self.wears_burn_from_stepping_immunity();
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.yaw = self.yaw;
        player.pitch = self.pitch;
        player.auto_jump_enabled = self.settings.auto_jump;
        player.item_use_speed = item_use_speed;
        player.item_use_slows_sprint = item_use_slows_sprint;
        player.respiration = respiration;
        player.glider = glider;
        let _ = player.set_sleeping(sleeping);
        player.leather_boots = leather_boots;
        player.freeze_immune = freeze_immune;
        player.burn_from_stepping_immune = burn_immune;
        player.fluids.fast_lava = self.dim == Dim::Nether;
    }
    fn held_slot(&self) -> usize {
        match self.hand {
            Hand::Main => self.selected,
            Hand::Off => shield::OFFHAND_SLOT,
        }
    }
    fn used_hand(&self) -> Option<Hand> {
        self.using.map(|(slot, _)| {
            if slot == shield::OFFHAND_SLOT {
                Hand::Off
            } else {
                Hand::Main
            }
        })
    }
    fn with_hand<T>(&mut self, hand: Hand, f: impl FnOnce(&mut Self) -> T) -> T {
        let prior = std::mem::replace(&mut self.hand, hand);
        let out = f(self);
        self.hand = prior;
        out
    }
    fn give(&mut self, mut stack: ItemStack) -> Option<ItemStack> {
        let tutorial_item = matches!(
            self.tutorial.step,
            tutorial::Step::FindTree | tutorial::Step::PunchTree | tutorial::Step::CraftPlanks
        )
        .then(|| stack.clone());
        let mut added = false;
        let max = self
            .res
            .items()
            .get(&stack.item)
            .map_or(64, |i| i.max_stack);
        let order: Vec<usize> = std::iter::once(self.selected)
            .chain(std::iter::once(shield::OFFHAND_SLOT))
            .chain((0..36).filter(|&i| i != self.selected))
            .collect();
        for &i in &order {
            let slot = if i == shield::OFFHAND_SLOT {
                &mut self.offhand
            } else {
                &mut self.inventory[i]
            };
            if let Some(slot) = slot
                && slot.same_item_same_components(&stack)
                && slot.count < max
            {
                let moved = (max - slot.count).min(stack.count);
                slot.count += moved;
                stack.count -= moved;
                added |= moved > 0;
                if stack.count == 0 {
                    self.tutorial_get_item(tutorial_item.as_ref());
                    return None;
                }
            }
        }
        for i in 0..36 {
            if self.inventory[i].is_none() {
                let moved = stack.count.min(max);
                self.inventory[i] = Some(ItemStack {
                    count: moved,
                    ..stack.clone()
                });
                stack.count -= moved;
                added |= moved > 0;
                if stack.count == 0 {
                    self.tutorial_get_item(tutorial_item.as_ref());
                    return None;
                }
            }
        }
        if added {
            self.tutorial_get_item(tutorial_item.as_ref());
        }
        Some(stack)
    }
    fn block_face_sprite(&self, state: BlockStateId) -> Option<(String, u32)> {
        let pack = &self.pack;
        let models = pack
            .render
            .model_catalog
            .collect(state, vise_core::position::BlockPos::new(0, 0, 0))
            .ok()?;
        let face = models
            .iter()
            .flat_map(|(_, m)| m.faces.iter())
            .find(|f| f.quad.facing == vise_assets::model::Direction::Up)
            .or_else(|| models.iter().flat_map(|(_, m)| m.faces.iter()).next())?;
        let name = pack.layer_names.get(&face.layer)?.clone();
        let tint = if face.quad.tint_index >= 0 {
            let n = &self.res.facts().get(state).name;
            if n.contains("leaves") || n.contains("vine") {
                0x48B518
            } else {
                0x91BD59
            }
        } else {
            0xFFFFFF
        };
        Some((name, tint))
    }
    fn honey_slide_particles(&mut self, at: [f64; 3]) {
        use vise_client::particles::{ParticleOptions, ParticleType};
        let Some(honey) = self
            .pack
            .assets
            .registry
            .default_state("minecraft:honey_block")
        else {
            return;
        };
        let block = [
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        ];
        if let Some((sprite, tint)) = self.particle_sprite(honey, block) {
            for _ in 0..5 {
                self.particles.add(
                    &ParticleOptions::block(ParticleType::Block, sprite.clone(), tint),
                    at,
                    [0.0; 3],
                );
            }
        }
    }
    fn particle_sprite(&self, state: BlockStateId, pos: [i32; 3]) -> Option<(String, u32)> {
        let sprite = Self::block_particle_sprite(&self.pack, state)?;
        let chunks = &self.world.chunks;
        let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
        let region: [[Option<&vise_client::chunk::Chunk>; 3]; 3] = std::array::from_fn(|dz| {
            std::array::from_fn(|dx| {
                chunks
                    .get(&(cx + dx as i32 - 1, cz + dz as i32 - 1))
                    .map(|c| &**c)
            })
        });
        let tint = self
            .pack
            .context
            .tables
            .terrain_particle_tint(
                state,
                pos,
                &region,
                self.res.biomes(),
                self.streaming.biome_zoom,
                self.streaming.biome_blend,
            )
            .unwrap_or(0xFFFFFF);
        Some((sprite, tint))
    }
    fn block_particle_sprite(pack: &PackResources, state: BlockStateId) -> Option<String> {
        let catalog = &pack.render.model_catalog;
        let layer = catalog.particle(state).or_else(|| {
            let models = catalog
                .collect(state, vise_core::position::BlockPos::new(0, 0, 0))
                .ok()?;
            models
                .iter()
                .flat_map(|(_, m)| m.faces.iter())
                .next()
                .map(|f| f.layer)
        })?;
        pack.layer_names.get(&layer).cloned()
    }
    fn particle_shape(&self, pos: [i32; 3], state: BlockStateId) -> Vec<[f64; 6]> {
        let f = self.res.facts().get(state);
        let offset = if f.outline_offset {
            self.pack
                .context
                .tables
                .offset(state, pos[0], pos[2])
                .unwrap_or([0.0; 3])
        } else {
            [0.0; 3]
        };
        f.outline
            .iter()
            .map(|b| std::array::from_fn(|i| b[i] + offset[i % 3]))
            .collect()
    }
    fn destroy_particles(&mut self, pos: [i32; 3], state: BlockStateId) {
        let Some((sprite, tint)) = self.particle_sprite(state, pos) else {
            return;
        };
        let boxes = self.particle_shape(pos, state);
        vise_client::particles::destroy_positions(pos, &boxes, |p, v| {
            let particle = Particle::terrain(self.particles.random(), p, v, sprite.clone(), tint);
            self.particles.push(particle);
        });
    }
    fn crack_particle(&mut self, pos: [i32; 3], face: [i32; 3], state: BlockStateId) {
        let Some((sprite, tint)) = self.particle_sprite(state, pos) else {
            return;
        };
        let boxes = self.particle_shape(pos, state);
        let r = self.particles.random();
        let Some(p) = vise_client::particles::breaking_position(r, pos, face, &boxes) else {
            return;
        };
        let mut particle = Particle::terrain(r, p, [0.0; 3], sprite, tint);
        particle.set_power(0.2);
        particle.scale(0.6);
        self.particles.push(particle);
    }
    fn drop_inventory(&mut self, at: [f64; 3]) {
        if self.ui.rules.bool("keep_inventory") {
            return;
        }
        let mut stacks: Vec<ItemStack> =
            self.inventory.iter_mut().filter_map(Option::take).collect();
        stacks.extend(self.armor.iter_mut().rev().filter_map(Option::take));
        stacks.extend(self.offhand.take());
        stacks.retain(|s| s.enchantment_level("minecraft:vanishing_curse") == 0);
        self.apply_equipment();
        for stack in stacks {
            let r = &mut self.world.random;
            let strength = f64::from(r.next_float()) * 0.5;
            let angle = f64::from(r.next_float()) * std::f64::consts::TAU;
            let v = [-angle.sin() * strength, 0.2, angle.cos() * strength];
            self.spawn_item(stack, at, v, 40);
        }
    }
    fn spawn_item(
        &mut self,
        stack: ItemStack,
        position: [f64; 3],
        velocity: [f64; 3],
        pickup_delay: u32,
    ) {
        self.next_entity += 1;
        let p = Vec3::new(position[0], position[1], position[2]);
        let bob = self.particles.random().next_float() * std::f32::consts::PI * 2.0;
        self.dropped.push(ItemEntity {
            health: 5,
            fire_ticks: 0,
            thrower: None,
            target: None,
            id: self.next_entity,
            stack,
            position: p,
            previous: p,
            velocity: Vec3::new(velocity[0], velocity[1], velocity[2]),
            on_ground: false,
            age: 0,
            pickup_delay,
            bob_offset: bob,
            ticks: 0,
        });
    }
    fn block_loot_at(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        held: Option<&ItemStack>,
    ) -> Vec<ItemStack> {
        let registry = self.pack.assets.registry;
        let dynamic = self.pending.dynamic(pos);
        match world_items!(self).block_loot_sequence(registry, state) {
            Some(sequence) => {
                let random = self.rand_sequences.get(&sequence, self.level.seed);
                self.res
                    .items()
                    .block_drops_by_player(registry, state, held, &dynamic, random)
            }
            None => world_items!(self).block_drops_by_player(
                registry,
                state,
                held,
                &dynamic,
                &mut self.world.random,
            ),
        }
    }
    fn pop_resource(&mut self, pos: [i32; 3], mut stack: ItemStack) {
        self.pending.attach(pos, &mut stack);
        let p = spill::pop_position(&mut self.world.random, pos);
        let v = dispenser::new_item_motion();
        self.spawn_item(stack, p, v, 10);
    }
    fn drop_held(&mut self, all: bool) {
        if self.eye_position().is_none() {
            return;
        }
        let spectator = self.spectator();
        let Some(thrown) = vise_client::items::take_held_drop(
            &mut self.inventory[self.selected],
            all,
            spectator,
        ) else {
            return;
        };
        self.player_drop(thrown);
    }
    pub(super) fn player_drop(&mut self, thrown: ItemStack) {
        let Some(eye) = self.eye_position() else {
            return;
        };
        self.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
        self.stats.add("dropped", &thrown.item, thrown.count as i32);
        self.stats.custom("drop", 1);
        self.throw_from_eyes(thrown, eye, true);
    }
    fn throw_from_eyes(&mut self, thrown: ItemStack, eye: [f64; 3], thrown_from_hand: bool) {
        let (yaw, pitch) = (
            f64::from(self.yaw).to_radians(),
            f64::from(self.pitch).to_radians(),
        );
        let r = &mut self.world.random;
        let spread = f64::from(r.next_float()) * std::f64::consts::TAU;
        let strength = 0.02 * f64::from(r.next_float());
        let v = [
            -yaw.sin() * pitch.cos() * 0.3 + spread.cos() * strength,
            -pitch.sin() * 0.3 + 0.1 + f64::from(r.next_float() - r.next_float()) * 0.1,
            yaw.cos() * pitch.cos() * 0.3 + spread.sin() * strength,
        ];
        self.spawn_item(thrown, [eye[0], eye[1] - 0.3, eye[2]], v, 40);
        if thrown_from_hand {
            let uuid = self.acting_player_uuid();
            self.dropped.last_mut().unwrap().thrower = Some(uuid);
        }
    }
    pub(super) fn acting_player_uuid(&mut self) -> [i32; 4] {
        match self.lan_acting_uuid {
            Some(guest) => vise_client::lan::playerdata::uuid_ints(guest),
            None => self.level.ensure_player_uuid(),
        }
    }
    fn tick_items(&mut self) {
        let mut items = std::mem::take(&mut self.dropped);
        let ticking = self.entity_ticking_area();
        let mut removed = vec![false; items.len()];
        let mut cells = vise_client::entities::ItemCells::new(&items);
        let catalog = world_items!(self);
        let max_stack =
            |stack: &ItemStack| catalog.get(&stack.item).map_or(64, |i| i.max_stack).max(1);
        let merging = self.remote.is_none();
        for i in 0..items.len() {
            if removed[i] {
                continue;
            }
            let item = &mut items[i];
            if !ticking(
                item.position.x.floor() as i32,
                item.position.z.floor() as i32,
            ) {
                continue;
            }
            let no_physics = merging && item.inside_block(&self.world, self.collision);
            if no_physics {
                let speed = self.world.random.next_float() * 0.2 + 0.1;
                item.move_towards_closest_space(&self.world, self.res.facts(), speed);
            }
            let alive = item.tick(
                &self.world,
                self.res.facts(),
                self.collision,
                no_physics,
                &self.res.motion().fluids,
            );
            vise_client::motion::bubble_column_push(
                &self.world,
                self.pack.assets.registry,
                self.res.facts(),
                self.collision,
                item.bounds(),
                &mut item.velocity,
            );
            cells.moved(i, item.previous, item.position);
            let block = |v: Vec3| (v.x.floor(), v.y.floor(), v.z.floor());
            let rate = if block(item.previous) == block(item.position) {
                40
            } else {
                2
            };
            if merging
                && (alive || item.age == 6000 && item.position.y > -128.0)
                && item.ticks.is_multiple_of(rate)
            {
                vise_client::entities::merge_with_neighbours(
                    &mut items,
                    &mut removed,
                    &mut cells,
                    i,
                    &max_stack,
                );
            }
            let item = &mut items[i];
            if !alive || removed[i] {
                removed[i] = true;
                cells.remove(i, item.position);
                continue;
            }
            let environment = item.environment_tick(
                &self.world,
                self.res.facts(),
                self.pack.assets.registry,
                world_items!(self),
            );
            if environment.should_play_lava_hurt_sound(item.age) {
                let pitch = 2.0 + self.world.random.next_float() * 0.4;
                self.audio.play(
                    "minecraft:entity.generic.burn",
                    Some([item.position.x, item.position.y, item.position.z]),
                    0.4,
                    pitch,
                );
            }
            if !environment.alive {
                removed[i] = true;
                cells.remove(i, item.position);
            }
        }
        let mut removed = removed.into_iter();
        items.retain(|_| !removed.next().unwrap_or(false));
        let Some(p) = self.player.as_ref() else {
            self.dropped = items;
            return;
        };
        if p.health.dead_or_dying() || self.spectator() {
            self.dropped = items;
            return;
        }
        let w = f64::from(p.dimensions.width) / 2.0;
        let (min, max) = (
            [
                p.position.x - w - 1.0,
                p.position.y - 0.5,
                p.position.z - w - 1.0,
            ],
            [
                p.position.x + w + 1.0,
                p.position.y + f64::from(p.dimensions.height) + 0.5,
                p.position.z + w + 1.0,
            ],
        );
        let feet = [p.position.x, p.position.y, p.position.z];
        let own = self.level.ensure_player_uuid();
        let mut kept = Vec::with_capacity(items.len());
        for mut item in items {
            let b = item.bounds();
            let touching = b.min_position().x < max[0]
                && b.max_position().x > min[0]
                && b.min_position().y < max[1]
                && b.max_position().y > min[1]
                && b.min_position().z < max[2]
                && b.max_position().z > min[2];
            if touching && item.pickup_delay == 0 && item.target.is_none_or(|t| t == own) {
                let before = item.stack.count;
                let kind = item.stack.item.clone();
                let thrower = item.thrower;
                match self.give(item.stack.clone()) {
                    None => {
                        self.stats.add("picked_up", &kind, before as i32);
                        if let Some(thrower) = thrower {
                            self.trigger_thrown_picked_up_by_player(&item.stack, thrower);
                        }
                        self.add_pickup(
                            pickups::Taken::Item(Box::new(item)),
                            pickups::Collector::Player,
                        );
                        let pitch = ((self.world.random.next_float()
                            - self.world.random.next_float())
                            * 0.7
                            + 1.0)
                            * 2.0;
                        self.audio
                            .play("minecraft:entity.item.pickup", Some(feet), 0.2, pitch);
                        continue;
                    }
                    Some(rest) => {
                        if rest.count < before {
                            self.stats
                                .add("picked_up", &kind, (before - rest.count) as i32);
                            if let Some(thrower) = thrower {
                                self.trigger_thrown_picked_up_by_player(&rest, thrower);
                            }
                            self.add_pickup(
                                pickups::Taken::Item(Box::new(item.clone())),
                                pickups::Collector::Player,
                            );
                            let pitch = ((self.world.random.next_float()
                                - self.world.random.next_float())
                                * 0.7
                                + 1.0)
                                * 2.0;
                            self.audio
                                .play("minecraft:entity.item.pickup", Some(feet), 0.2, pitch);
                        }
                        item.stack = rest;
                    }
                }
            }
            kept.push(item);
        }
        self.dropped = kept;
    }
    fn with_level<R>(
        &mut self,
        f: impl FnOnce(&Behaviours, &mut GameLevel<'_>, &mut Ticks) -> R,
    ) -> R {
        let darken = self.sky_darken();
        let (rain, thunder) = self.world.weather.levels(1.0);
        let spectator = self.spectator();
        let player_box = self.player.as_ref().filter(|_| !spectator).map(|p| {
            let w = f64::from(p.dimensions.width) / 2.0;
            (
                [p.position.x - w, p.position.y, p.position.z - w],
                [
                    p.position.x + w,
                    p.position.y + f64::from(p.dimensions.height),
                    p.position.z + w,
                ],
            )
        });
        let player_collision = self.player.as_ref().filter(|_| !spectator).map(|p| {
            let main_hand = self.inventory.get(self.selected).and_then(Option::as_ref);
            PlayerCollision {
                y: p.position.y,
                descending: p.velocity.y < 0.0,
                fall_distance: p.fall_distance,
                main_hand_item: main_hand.map_or_else(
                    || Arc::from("minecraft:air"),
                    |stack| Arc::clone(&stack.item),
                ),
                main_hand_count: main_hand.map_or(0, |stack| stack.count as i32),
                leather_boots: self.armor[3]
                    .as_ref()
                    .is_some_and(|stack| stack.item.as_ref() == "minecraft:leather_boots"),
            }
        });
        let cart_boxes = self
            .carts
            .iter()
            .map(|c| {
                let p = c.position;
                (
                    [p.x - 0.49, p.y, p.z - 0.49],
                    [p.x + 0.49, p.y + 0.7, p.z + 0.49],
                )
            })
            .collect();
        let arrow_boxes = self
            .creatures
            .projectiles
            .iter()
            .filter(|p| p.arrow.is_some())
            .map(|p| p.position)
            .chain(self.tridents.iter().map(|t| t.position))
            .map(|at| {
                let b = vise_client::arrows::arrow_bounds(at);
                (b.min_position().components(), b.max_position().components())
            })
            .collect();
        let guest_boxes = self.lan_block_trigger_boxes();
        let dimension = match self.dim {
            Dim::Overworld => LevelDimension::Overworld,
            Dim::Nether => LevelDimension::Nether,
            Dim::End => LevelDimension::End,
        };
        let rules = self.level_rules();
        let fire_players: Vec<[f64; 3]> = self
            .player
            .iter()
            .filter(|_| !spectator)
            .map(|p| [p.position.x, p.position.y, p.position.z])
            .chain(self.lan.iter().flat_map(|h| {
                h.hub
                    .guests
                    .iter()
                    .filter(|g| h.hub.here(g) && !g.spectator())
                    .map(|g| g.position)
            }))
            .collect();
        let blockers = Blockers {
            mobs: &self.mobs,
            armor_stands: &self.armor_stands,
            boats: &self.boats,
            crystals: &self.crystals,
            tnts: &self.tnts,
            falling: &self.falling,
        };
        let mut level = GameLevel {
            cart_boxes,
            item_frames: &self.item_frames,
            carts: &self.carts,
            arrow_boxes,
            blockers,
            dropped: &self.dropped,
            orbs: &self.orbs,
            projectiles: &self.creatures.projectiles,
            world: &mut self.world,
            facts: self.res.facts(),
            collision: self.collision,
            events: &mut self.level_events,
            dimension,
            darken,
            environment: match dimension {
                LevelDimension::Overworld => self.res.environment(),
                LevelDimension::Nether => self.res.nether_environment(),
                LevelDimension::End => self.res.end_environment(),
            },
            raining: rain > 0.2,
            thundering: thunder > 0.9,
            player_box,
            guest_boxes,
            player_collision,
            biomes: self.res.biomes(),
            seed: self.level.seed,
            behaviours: self.res.behaviours(),
            items: world_items!(self),
            recipes: world_recipes!(self),
            loot: if self.remote.is_some() {
                None
            } else {
                vise_client::structure_blocks::loot_data(
                    &self.options.resources,
                    &pack_files(&self.world_tables),
                )
            },
            registry: self.pack.assets.registry,
            rules,
            difficulty: i32::from(self.ui.difficulty),
            fire_players,
            rand_sequences: &mut self.rand_sequences,
            pending: &mut self.pending,
            spill: &mut self.spill,
            tickets: self.tickets.levels(),
            random_tick_sections: &mut self.random_tick_sections,
            lid_openers: &self.lid_openers,
            client_side: self.remote.is_some(),
            native_authoritative: self.serving.is_some() || self.options.dedicated.is_some(),
            native_particles: &mut self.creatures.lan_particles,
        };
        let result = f(self.res.behaviours(), &mut level, &mut self.block_ticks);
        self.flush_spill();
        if self.lan.is_some() {
            self.flush_lan_particles();
        }
        result
    }
    fn level_set(&mut self, pos: [i32; 3], state: BlockStateId) -> Result<()> {
        self.with_level(|b, level, ticks| {
            b.set_block(
                level,
                ticks,
                BlockPos::new(pos[0], pos[1], pos[2]),
                state,
                level_flags::UPDATE_ALL,
            );
        });
        self.process_level_events()
    }
    fn attack_block(&mut self, pos: [i32; 3]) -> Result<()> {
        let note = self.with_level(|b, level, ticks| {
            b.attack_block(level, ticks, BlockPos::new(pos[0], pos[1], pos[2]))
        });
        if note {
            self.stats.custom("play_noteblock", 1);
        }
        self.process_level_events()
    }
    fn candle_smoke(&mut self, pos: [i32; 3], state: BlockStateId) {
        use vise_client::particles::{ParticleOptions, ParticleType};
        let candles = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("candles")?.parse::<usize>().ok())
            .unwrap_or(1);
        const OFFSETS: [&[[u8; 3]]; 4] = [
            &[[8, 8, 8]],
            &[[6, 7, 8], [10, 8, 7]],
            &[[8, 5, 10], [6, 7, 8], [9, 8, 7]],
            &[[7, 5, 9], [10, 7, 9], [6, 7, 6], [9, 8, 6]],
        ];
        for o in OFFSETS[candles.clamp(1, 4) - 1] {
            self.particles.add(
                &ParticleOptions::simple(ParticleType::Smoke),
                [
                    f64::from(pos[0]) + f64::from(o[0]) / 16.0,
                    f64::from(pos[1]) + f64::from(o[1]) / 16.0,
                    f64::from(pos[2]) + f64::from(o[2]) / 16.0,
                ],
                [0.0, 0.1, 0.0],
            );
        }
    }
    fn lever_puff(&mut self, pos: [i32; 3]) {
        if self.lan_acting.is_some() {
            return;
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let Some(registry_state) = self.pack.assets.registry.state(state) else {
            return;
        };
        let prop = |key: &str| registry_state.properties.get(key).map(String::as_str);
        if prop("powered") != Some("true") {
            return;
        }
        let facing = match prop("facing") {
            Some("north") => [0, 0, -1],
            Some("south") => [0, 0, 1],
            Some("west") => [-1, 0, 0],
            Some("east") => [1, 0, 0],
            _ => return,
        };
        let connected = match prop("face") {
            Some("ceiling") => [0, -1, 0],
            Some("floor") => [0, 1, 0],
            _ => facing,
        };
        let at = |base: i32, i: usize| {
            f64::from(base) + 0.5 + 0.1 * f64::from(-facing[i]) + 0.2 * f64::from(-connected[i])
        };
        let p = [at(pos[0], 0), at(pos[1], 1), at(pos[2], 2)];
        self.particles.add(
            &vise_client::particles::ParticleOptions::dust([1.0, 0.0, 0.0], 1.0),
            p,
            [0.0; 3],
        );
    }
    fn sync_tick_chunks(&mut self) {
        if !self.tick_chunks.synchronize(&self.world.chunks) {
            return;
        }
        let added = self.tick_chunks.take_added();
        for &key in &added {
            self.register_chunk_ticks(key);
        }
        for &key in self.tick_chunks.removed() {
            self.random_tick_sections.remove(&key);
            self.portals.sections.remove(&key);
            self.block_ticks
                .remove_chunk(vise_core::position::ChunkPos::new(key.0, key.1));
            self.block_ticks
                .moving_pistons
                .retain(|(p, _)| (p.x >> 4, p.z >> 4) != key);
        }
        self.tick_chunks.recycle_added(added);
    }
    fn update_chunk_tickets(&mut self) {
        let mut players: Vec<(i32, i32)> = self.lan.as_ref().map_or_else(Vec::new, |h| {
            h.hub
                .guests
                .iter()
                .filter(|g| h.hub.here(g))
                .map(|g| g.chunk())
                .collect()
        });
        if let Some(p) = &self.player {
            players.push((
                (p.position.x.floor() as i32) >> 4,
                (p.position.z.floor() as i32) >> 4,
            ));
        }
        let sources =
            (!self.options.execution_role.is_client() || !players.is_empty()).then(|| {
                vise_client::ticket_levels::Sources {
                    players,
                    forced: self.streaming.forced.iter().copied().collect(),
                    view_distance: self.settings.render_distance.clamp(2, 32),
                    simulation_distance: self.settings.simulation_distance.max(2),
                }
            });
        self.tickets.update(sources);
    }
    fn start_ticking_chunks(&mut self) {
        let levels = self.tickets.levels();
        let world = &self.world;
        let started = self
            .tick_chunks
            .start_ticking(|(x, z)| levels.loaded_ticking(x, z) && ticking_chunk(world, x, z));
        let time = self.world.game_time - 1;
        for (x, z) in started {
            let pos = vise_core::position::ChunkPos::new(x, z);
            self.block_ticks.blocks.unpack_chunk(pos, time);
            self.block_ticks.fluids.unpack_chunk(pos, time);
        }
    }
    fn tick_blocks(&mut self) -> Result<()> {
        self.sync_tick_chunks();
        self.start_ticking_chunks();
        let levels = self.tickets.levels();
        let ticking: Vec<_> = self
            .world
            .chunks
            .keys()
            .copied()
            .filter(|&(x, z)| levels.random_ticking(x, z) && ticking_chunk(&self.world, x, z))
            .collect();
        let mut inside = Vec::new();
        let guests = self.lan_block_trigger_boxes();
        let boxes = self
            .player
            .iter()
            .filter(|_| !self.spectator())
            .map(|p| {
                let w = f64::from(p.dimensions.width) / 2.0;
                let h = f64::from(p.dimensions.height);
                (
                    [p.position.x - w, p.position.y, p.position.z - w],
                    [p.position.x + w, p.position.y + h, p.position.z + w],
                )
            })
            .chain(self.carts.iter().map(|c| {
                let p = c.position;
                (
                    [p.x - 0.49, p.y, p.z - 0.49],
                    [p.x + 0.49, p.y + 0.7, p.z + 0.49],
                )
            }))
            .chain(guests);
        for (lo, hi) in boxes {
            for x in (lo[0] + 1e-7).floor() as i32..=(hi[0] - 1e-7).floor() as i32 {
                for y in (lo[1] + 1e-7).floor() as i32..=(hi[1] - 1e-7).floor() as i32 {
                    for z in (lo[2] + 1e-7).floor() as i32..=(hi[2] - 1e-7).floor() as i32 {
                        inside.push(BlockPos::new(x, y, z));
                    }
                }
            }
        }
        for p in self
            .creatures
            .projectiles
            .iter()
            .filter(|p| p.arrow.is_some())
        {
            inside.extend(vise_client::arrows::blocks_inside(
                p.previous, p.position,
            ));
        }
        for t in &self.tridents {
            inside.extend(vise_client::arrows::blocks_inside(
                t.position, t.position,
            ));
        }
        let mut standing = Vec::new();
        let feet = self
            .player
            .iter()
            .filter(|_| !self.spectator())
            .map(|p| {
                (
                    [p.position.x, p.position.y, p.position.z],
                    p.ground.on_ground,
                    f64::from(p.dimensions.width) / 2.0,
                )
            })
            .chain(
                self.lan_guest_feet()
                    .into_iter()
                    .map(|(at, on_ground)| (at, on_ground, 0.3)),
            );
        for (at, on_ground, w) in feet {
            let y = (at[1] + 1e-7).floor() as i32;
            for x in (at[0] - w + 1e-7).floor() as i32..=(at[0] + w - 1e-7).floor() as i32 {
                for z in (at[2] - w + 1e-7).floor() as i32..=(at[2] + w - 1e-7).floor() as i32 {
                    standing.push((BlockPos::new(x, y, z), on_ground, at[1]));
                }
            }
        }
        self.with_level(|b, level, ticks| {
            let mut boat_cells: Vec<BlockPos> = Vec::new();
            for bounds in level.boat_trigger_boxes() {
                let (lo, hi) = (bounds.min_position(), bounds.max_position());
                for x in (lo.x + 1e-7).floor() as i32..=(hi.x - 1e-7).floor() as i32 {
                    for y in (lo.y + 1e-7).floor() as i32..=(hi.y - 1e-7).floor() as i32 {
                        for z in (lo.z + 1e-7).floor() as i32..=(hi.z - 1e-7).floor() as i32 {
                            boat_cells.push(BlockPos::new(x, y, z));
                        }
                    }
                }
            }
            for pos in boat_cells {
                b.boat_inside(level, ticks, pos);
            }
            let mut touched: Vec<BlockPos> = Vec::new();
            for bounds in level.mob_trigger_boxes().chain(level.other_trigger_boxes()) {
                let (lo, hi) = (bounds.min_position(), bounds.max_position());
                for x in (lo.x + 1e-7).floor() as i32..=(hi.x - 1e-7).floor() as i32 {
                    for y in (lo.y + 1e-7).floor() as i32..=(hi.y - 1e-7).floor() as i32 {
                        for z in (lo.z + 1e-7).floor() as i32..=(hi.z - 1e-7).floor() as i32 {
                            touched.push(BlockPos::new(x, y, z));
                        }
                    }
                }
            }
            touched.sort_unstable_by_key(|p| (p.x, p.y, p.z));
            touched.dedup();
            for pos in inside.into_iter().chain(touched) {
                b.entity_inside(level, ticks, pos);
            }
            for (pos, on_ground, feet_y) in standing {
                b.entity_on_big_dripleaf(level, ticks, pos, on_ground, feet_y);
            }
        });
        self.process_level_events()?;
        self.tick_scheduled()?;
        self.with_level(|b, level, ticks| {
            for (x, z) in ticking {
                let mut read = None;
                let mut sections = Default::default();
                let pos = vise_core::position::ChunkPos::new(x, z);
                b.tick_chunk_random(level, ticks, pos, |level, from| {
                    if read != Some(level.world.revision) {
                        read = Some(level.world.revision);
                        sections = level
                            .world
                            .chunks
                            .get(&(x, z))
                            .map_or_else(Default::default, |chunk| {
                                level.random_tick_sections.get((x, z), chunk, level.facts)
                            });
                    }
                    sections.from(from)
                });
            }
            b.run_block_events(level, ticks);
            b.tick_block_entities(level, ticks);
        });
        let pushes = std::mem::take(&mut self.block_ticks.piston_pushes);
        self.push_piston_entities(pushes);
        self.process_level_events()?;
        self.sync_moving_pistons();
        Ok(())
    }
    fn sync_moving_pistons(&mut self) {
        let previous = std::mem::take(&mut self.world.moving_pistons);
        let current: Vec<_> = self
            .block_ticks
            .moving_pistons
            .iter()
            .map(|(p, piston)| ([p.x, p.y, p.z], piston.shape_state()))
            .collect();
        let mut fallback: Vec<_> = previous
            .into_iter()
            .filter(|(pos, _)| !current.iter().any(|(next, _)| next == pos))
            .collect();
        let older = std::mem::take(&mut self.world.moving_piston_fallback);
        for (pos, piston) in older {
            let replaced = current.iter().chain(&fallback).any(|(p, _)| *p == pos);
            if !replaced && self.section_mesh_pending(pos) {
                fallback.push((pos, piston));
            }
        }
        self.world.moving_piston_fallback = fallback;
        self.world.moving_pistons = current;
    }
    pub(super) fn tick_scheduled(&mut self) -> Result<()> {
        for fluids in [false, true] {
            if !self.with_level(|b, level, ticks| b.scheduled_begin(level, ticks, fluids)) {
                continue;
            }
            loop {
                let more = self.with_level(|b, level, ticks| {
                    loop {
                        let more = b.scheduled_step(level, ticks, fluids);
                        if !more || !level.events.is_empty() || !level.spill.is_empty() {
                            break more;
                        }
                    }
                });
                if !self.level_events.is_empty() {
                    self.process_level_events()?;
                }
                if !more {
                    break;
                }
            }
            self.with_level(|b, _, ticks| b.scheduled_finish(ticks, fluids));
        }
        Ok(())
    }
    fn process_level_events(&mut self) -> Result<()> {
        let events = std::mem::take(&mut self.level_events);
        self.lan_record_events(&events);
        dispense_fixture::record_events(&events);
        for event in events {
            match event {
                LevelEvent::LevelEvent {
                    id: 2001,
                    pos,
                    data,
                } => {
                    let state = BlockStateId(data as u32);
                    let p = [pos.x, pos.y, pos.z];
                    self.destroy_particles(p, state);
                    if self.res.facts().is_air(state) {
                        continue;
                    }
                    let sounds = self.res.facts().get(state).sounds.clone();
                    let c = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    self.audio.play(
                        &sounds.break_sound,
                        Some(c),
                        (sounds.volume + 1.0) / 2.0,
                        sounds.pitch * 0.8,
                    );
                }
                LevelEvent::LevelEvent {
                    id: 3006,
                    pos,
                    data,
                } => self.sculk_charge_event(pos, data),
                LevelEvent::LevelEvent { id: 3007, pos, .. } => self.shriek_event(pos),
                LevelEvent::LevelEvent {
                    id: 1010,
                    pos,
                    data,
                } => self.jukebox_song_event([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 1011, pos, .. } => {
                    self.jukebox_stop_event([pos.x, pos.y, pos.z])
                }
                LevelEvent::LevelEvent {
                    id: id @ (2019 | 2020),
                    pos,
                    data,
                } => {
                    let face = vise_core::geometry::Direction::ALL[data.clamp(0, 5) as usize];
                    self.breaking_block_effects([pos.x, pos.y, pos.z], face.normal(), id == 2020);
                }
                LevelEvent::LevelEvent {
                    id: 3008,
                    pos,
                    data,
                } => self.brush_complete_event([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 2004, pos, .. } => {
                    self.spawner_burst([pos.x, pos.y, pos.z])
                }
                LevelEvent::LevelEvent {
                    id: 1505,
                    pos,
                    data,
                } => {
                    self.growth_particles(pos, data);
                    let at = [pos.x, pos.y, pos.z].map(|v| f64::from(v) + 0.5);
                    self.audio
                        .play("minecraft:item.bone_meal.use", Some(at), 1.0, 1.0);
                }
                LevelEvent::LevelEvent {
                    id: 2015,
                    pos,
                    data,
                } => self.dragon_egg_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2016,
                    pos,
                    data,
                } => self.shulker_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2017,
                    pos,
                    data,
                } => self.consume_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent {
                    id: 2018,
                    pos,
                    data,
                } => self.enderman_teleport_particles([pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id: 2003, pos, .. } => {
                    self.eye_break_particles([pos.x, pos.y, pos.z]);
                }
                LevelEvent::LevelEvent {
                    id: 2013,
                    pos,
                    data,
                } => self.smash_dust(pos, data),
                LevelEvent::LevelEvent {
                    id: 2002,
                    pos,
                    data,
                } => self.potion_splash_particles(pos, data, false),
                LevelEvent::LevelEvent {
                    id: 2007,
                    pos,
                    data,
                } => self.potion_splash_particles(pos, data, true),
                LevelEvent::LevelEvent {
                    id: id @ 3002..=3005,
                    pos,
                    data,
                } => self.block_particle_event(id, pos, data),
                LevelEvent::LevelEvent {
                    id: 1500,
                    pos,
                    data,
                } => self.composter_fill_event(pos, data == 1),
                LevelEvent::LevelEvent { id: 1504, pos, .. } => self.dripstone_drip_particle(pos),
                LevelEvent::LevelEvent {
                    id: 2006,
                    pos,
                    data,
                } => self.dragon_fireball_burst(pos, data),
                LevelEvent::LevelEvent { id: 2009, pos, .. } => {
                    self.puffs_above_block(pos, vise_client::particles::ParticleType::Cloud)
                }
                LevelEvent::LevelEvent {
                    id: 2012,
                    pos,
                    data,
                } => self.turtle_egg_particles(pos, data),
                LevelEvent::LevelEvent {
                    id: 2014,
                    pos,
                    data,
                } => self.destroy_particles([pos.x, pos.y, pos.z], BlockStateId(data as u32)),
                LevelEvent::LevelEvent { id: 3000, pos, .. } => {
                    let at = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    self.particles.add_with(
                        &vise_client::particles::ParticleOptions::simple(
                            vise_client::particles::ParticleType::ExplosionEmitter,
                        ),
                        at,
                        [0.0; 3],
                        true,
                        true,
                    );
                    let random = self.particles.level_random();
                    let pitch = (1.0 + (random.next_float() - random.next_float()) * 0.2) * 0.7;
                    self.audio.play_unshared(
                        "minecraft:block.end_gateway.spawn",
                        Some(at),
                        10.0,
                        pitch,
                    );
                }
                LevelEvent::LevelEvent { id: 3009, pos, .. } => {
                    self.sniffer_egg_crack_particles(pos)
                }
                LevelEvent::LevelEvent {
                    id: id @ 3011..=3021,
                    pos,
                    data,
                } => self.trial_level_event(id, [pos.x, pos.y, pos.z], data),
                LevelEvent::LevelEvent { id, pos, .. } => {
                    let c = Some([
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ]);
                    let sound = match id {
                        1503 => Some(("minecraft:block.end_portal_frame.fill", 1.0)),
                        1501 => Some(("minecraft:block.lava.extinguish", 0.5)),
                        1009 => Some(("minecraft:block.fire.extinguish", 0.5)),
                        1502 => Some(("minecraft:block.redstone_torch.burnout", 0.5)),
                        1033 => Some(("minecraft:block.chorus_flower.grow", 1.0)),
                        1034 => Some(("minecraft:block.chorus_flower.death", 1.0)),
                        1045 => Some(("minecraft:block.pointed_dripstone.land", 2.0)),
                        1052 => Some(("minecraft:block.sulfur_spike.land", 2.0)),
                        1046 => Some((
                            "minecraft:block.pointed_dripstone.drip_lava_into_cauldron",
                            2.0,
                        )),
                        1047 => Some((
                            "minecraft:block.pointed_dripstone.drip_water_into_cauldron",
                            2.0,
                        )),
                        1019 => Some(("minecraft:entity.zombie.attack_wooden_door", 2.0)),
                        1021 => Some(("minecraft:entity.zombie.break_wooden_door", 2.0)),
                        1030 => Some(("minecraft:block.anvil.use", 1.0)),
                        1029 => Some(("minecraft:block.anvil.destroy", 1.0)),
                        1031 => Some(("minecraft:block.anvil.land", 0.3)),
                        1042 => Some(("minecraft:block.grindstone.use", 1.0)),
                        1044 => Some(("minecraft:block.smithing_table.use", 1.0)),
                        1043 => Some(("minecraft:item.book.page_turn", 1.0)),
                        1053 | 1054 => Some(("minecraft:entity.splash_potion.break", 1.0)),
                        _ => None,
                    };
                    if let Some((name, volume)) = sound {
                        let pitch = match id {
                            1009 => {
                                2.6 + (self.world.random.next_float()
                                    - self.world.random.next_float())
                                    * 0.8
                            }
                            1501 | 1502 => {
                                let r = self.particles.level_random();
                                2.6 + (r.next_float() - r.next_float()) * 0.8
                            }
                            1045 | 1046 | 1047 | 1052 => {
                                self.particles.level_random().next_float() * 0.1 + 0.9
                            }
                            1029 | 1030 | 1042 | 1043 | 1044 | 1053 | 1054 => {
                                self.world.random.next_float() * 0.1 + 0.9
                            }
                            1019 | 1021 => {
                                (self.world.random.next_float() - self.world.random.next_float())
                                    * 0.2
                                    + 1.0
                            }
                            _ => 1.0,
                        };
                        if matches!(
                            id,
                            1019 | 1021 | 1029 | 1030 | 1045 | 1052 | 1053 | 1054 | 1503
                        ) {
                            self.audio.play_unshared(name, c, volume, pitch);
                        } else {
                            self.audio.play(name, c, volume, pitch);
                        }
                    }
                    match id {
                        1501 => self.puffs_above_block(
                            pos,
                            vise_client::particles::ParticleType::LargeSmoke,
                        ),
                        1502 => self.torch_burnout_smoke(pos),
                        1503 => self.end_portal_frame_smoke(pos),
                        _ => {}
                    }
                }
                LevelEvent::Sound {
                    pos,
                    sound,
                    volume,
                    pitch,
                } => {
                    let c = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    let name = if sound.contains(':') {
                        sound.to_string()
                    } else {
                        format!("minecraft:{sound}")
                    };
                    self.audio.play(&name, Some(c), volume, pitch);
                }
                LevelEvent::DropResources { pos, state } => {
                    self.with_level(|_, level, _| level.drop_resources_now(pos, state));
                }
                LevelEvent::FallingBlock {
                    pos,
                    state,
                    hurt_per_distance,
                } => {
                    let mut damage = vise_client::falling_block::Damage::falling(
                        &self.res.facts().get(state).name,
                    );
                    if let Some(per_distance) = hurt_per_distance {
                        damage.hurt_entities = true;
                        damage.per_distance = per_distance;
                    }
                    let p = Vec3::new(
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y),
                        f64::from(pos.z) + 0.5,
                    );
                    self.falling.push(FallingBlock {
                        id: 0,
                        state,
                        start_pos: pos,
                        position: p,
                        previous: p,
                        velocity: Vec3::ZERO,
                        age: 0,
                        no_gravity: false,
                        drop_item: true,
                        fall_distance: 0.0,
                        stuck: Vec3::ZERO,
                        damage,
                        saved: None,
                    });
                }
                LevelEvent::Player { pos, effect, .. } => match effect {
                    PlayerEffect::OpenMenu => self.open_menu([pos.x, pos.y, pos.z]),
                    PlayerEffect::Sleep if self.beds_destroy_on_use() => {
                        self.destroy_bed_on_use([pos.x, pos.y, pos.z])?
                    }
                    PlayerEffect::Sleep => self.try_sleep([pos.x, pos.y, pos.z]),
                    PlayerEffect::GiveItem(item) => {
                        if let Some(rest) = self.give(ItemStack {
                            item,
                            count: 1,
                            damage: 0,
                            extra: None,
                        }) {
                            self.pop_resource([pos.x, pos.y, pos.z], rest);
                        }
                    }
                    PlayerEffect::DropItem(_) => {}
                    PlayerEffect::EatCake => {
                        self.stats.custom("eat_cake_slice", 1);
                        if let Some(p) = self.player.as_mut() {
                            let food = &mut p.health.food;
                            food.food = (food.food + 2).min(20);
                            food.saturation = (food.saturation + 0.4).min(food.food as f32);
                        }
                    }
                    PlayerEffect::RespawnAnchor => {
                        self.respawn_anchor_used([pos.x, pos.y, pos.z])?
                    }
                    PlayerEffect::InteractLoot(_) => {}
                },
                LevelEvent::PrimeTnt { pos, rot } => self.add_primed_tnt(pos, rot, 80),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "dispense",
                } => self.dispense_from(pos, state)?,
                LevelEvent::BlockEvent {
                    pos, kind: "golem", ..
                } => self.check_golem_spawn([pos.x, pos.y, pos.z])?,
                LevelEvent::BlockEvent {
                    pos,
                    kind: "command_block",
                    ..
                } => self.command_block_tick(pos),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "brushable",
                    ..
                } => self.brushable_tick([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos, kind: "hatch", ..
                } => {
                    let at = [
                        f64::from(pos.x) + 0.5,
                        f64::from(pos.y) + 0.5,
                        f64::from(pos.z) + 0.5,
                    ];
                    self.audio
                        .play("minecraft:block.frogspawn.hatch", Some(at), 1.0, 1.0);
                    let count = 2 + self.world.random.next_int_bound(4).unwrap_or(0);
                    for _ in 0..count {
                        let r = &mut self.world.random;
                        let off = |r: &mut vise_core::random::LegacyRandom| {
                            f64::from(r.next_float()) * 0.6 + 0.2
                        };
                        let (dx, dz) = (off(r), off(r));
                        let spawn = [
                            f64::from(pos.x) + dx,
                            f64::from(pos.y) - 0.5,
                            f64::from(pos.z) + dz,
                        ];
                        let _ = self.spawn_mob("minecraft:tadpole", spawn, true);
                    }
                }
                LevelEvent::BlockEvent {
                    pos,
                    kind: "hatch_sniffer",
                    ..
                } => self.hatch_sniffer([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "hatch_turtles",
                } => self.hatch_turtles([pos.x, pos.y, pos.z], state),
                LevelEvent::BlockEvent {
                    pos,
                    kind: "portal_spawn",
                    ..
                } => self.portal_spawn([pos.x, pos.y, pos.z]),
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "spawn_ghastling",
                } => self.spawn_ghastling([pos.x, pos.y, pos.z], state),
                LevelEvent::GameEvent { pos, event, state } => {
                    self.sculk_level_event(pos, event, state)
                }
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "shrieker_respond",
                } => self.shrieker_block_event(pos, state),
                LevelEvent::BlockEntityEvent { pos, a, b, .. } => {
                    self.block_entity_event([pos.x, pos.y, pos.z], a, b)
                }
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "note_particle",
                } => {
                    use vise_client::particles::{ParticleOptions, ParticleType};
                    let note = self
                        .pack
                        .assets
                        .registry
                        .state(state)
                        .and_then(|s| s.properties.get("note")?.parse::<f64>().ok())
                        .unwrap_or(0.0);
                    self.particles.add(
                        &ParticleOptions::simple(ParticleType::Note),
                        [
                            f64::from(pos.x) + 0.5,
                            f64::from(pos.y) + 1.2,
                            f64::from(pos.z) + 0.5,
                        ],
                        [note / 24.0, 0.0, 0.0],
                    );
                }
                LevelEvent::BlockEvent {
                    pos,
                    kind: "note_custom_sound",
                    ..
                } => {
                    if let Some(BlockEntity::Skull(skull)) =
                        self.world.block_entities.get(&(pos.x, pos.y + 1, pos.z))
                        && let Some(sound) = skull.note_block_sound.as_deref()
                    {
                        let at = [
                            f64::from(pos.x) + 0.5,
                            f64::from(pos.y) + 0.5,
                            f64::from(pos.z) + 0.5,
                        ];
                        self.audio.play(sound, Some(at), 3.0, 1.0);
                    }
                }
                LevelEvent::BlockEvent {
                    kind: "candle_smoke",
                    ..
                } if self.lan_acting.is_some() => {}
                LevelEvent::BlockEvent {
                    pos,
                    state,
                    kind: "candle_smoke",
                } => self.candle_smoke([pos.x, pos.y, pos.z], state),
                LevelEvent::GrowTree { .. }
                | LevelEvent::PlaceFeature { .. }
                | LevelEvent::BlockEvent { .. } => {}
            }
        }
        Ok(())
    }
    fn open_menu(&mut self, pos: [i32; 3]) {
        let name = self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            .clone();
        let key = (pos[0], pos[1], pos[2]);
        if self.chest_menu_blocked(key, &name) {
            return;
        }
        if &*name == "minecraft:lectern" {
            self.open_lectern(pos);
            return;
        }
        if vise_client::command_block::Mode::of_block(&name).is_some() {
            self.open_command_block(key);
            return;
        }
        if vise_client::signs::sign_kind(&name).is_some() {
            self.use_sign_empty_hand(pos);
            return;
        }
        if &*name == "minecraft:crafting_table" {
            self.craft_size = 3;
            self.craft_table = Some(pos);
        } else if &*name == "minecraft:ender_chest" {
            self.open_ender_chest(key);
            self.stats.custom("open_enderchest", 1);
            self.anger_piglins(true);
            if self.container_opener_event(key, true) {
                self.container_edge_sound(key, true);
            }
        } else if &*name == "minecraft:beacon" && self.open_station(pos, &name) {
            self.world.block_entities.get_or_insert_with(key, || {
                BlockEntity::for_block(&name).unwrap_or(BlockEntity::Chest(Vec::new()))
            });
        } else if storage::is_shulker_box(&name) && !self.shulker_can_open(pos) {
        } else if let Some(entity) = BlockEntity::for_block(&name) {
            self.world.block_entities.get_or_insert_with(key, || entity);
            self.unpack_opened_loot(key, &name);
            let centre = [
                f64::from(key.0) + 0.5,
                f64::from(key.1) + 0.5,
                f64::from(key.2) + 0.5,
            ];
            self.resolve_container_maps(key, centre);
            self.container = Some(key);
            let first = self.container_opener_event(key, true);
            self.post_block_game_event(
                "minecraft:container_open",
                [key.0, key.1, key.2],
                Some(vise_client::mobs::Who::Player),
                None,
            );
            let stat = match &*name {
                "minecraft:chest" => "open_chest",
                "minecraft:trapped_chest" => "trigger_trapped_chest",
                "minecraft:barrel" => "open_barrel",
                "minecraft:hopper" => "inspect_hopper",
                "minecraft:dropper" => "inspect_dropper",
                "minecraft:dispenser" => "inspect_dispenser",
                n if storage::is_shulker_box(n) => "open_shulker_box",
                _ => "",
            };
            if !stat.is_empty() {
                self.stats.custom(stat, 1);
            }
            if self.guarded_by_piglins(&name) {
                self.anger_piglins(true);
            }
            if first {
                self.container_edge_sound(key, true);
            }
        } else if self.open_mod_menu(key) {
            self.container = Some(key);
        } else if self.open_station(pos, &name) {
        } else {
            self.message = Some((
                format!(
                    "{} is not available yet",
                    name.trim_start_matches("minecraft:").replace('_', " ")
                ),
                Instant::now(),
            ));
            return;
        }
        self.screen = Screen::Inventory;
        self.capture(false);
        self.right = false;
    }
    fn tick_falling(&mut self) -> Result<()> {
        let falling = std::mem::take(&mut self.falling);
        let mut kept = Vec::with_capacity(falling.len());
        let mut collision_error = None;
        let entity_drops = self.ui.rules.bool("entity_drops");
        let ticking = self.entity_ticking_area();
        for mut f in falling {
            if !ticking(f.position.x.floor() as i32, f.position.z.floor() as i32) {
                kept.push(f);
                continue;
            }
            let original = f.clone();
            match self.fall_step(&mut f, entity_drops) {
                Ok(FallStep::Falling) => kept.push(f),
                Ok(FallStep::Gone) => {}
                Ok(FallStep::Dropped(at)) => self.drop_falling_block(f.state, at),
                Ok(FallStep::Landed(pos)) => self.finish_falling_block(f, pos),
                Err(error) => {
                    kept.push(original);
                    collision_error.get_or_insert(error);
                }
            }
        }
        let started = std::mem::replace(&mut self.falling, kept);
        self.falling.extend(started);
        self.process_level_events()?;
        if let Some(error) = collision_error {
            return Err(error);
        }
        Ok(())
    }
    fn fall_step(&self, f: &mut FallingBlock, entity_drops: bool) -> Result<FallStep> {
        f.previous = f.position;
        f.age = f.age.wrapping_add(1);
        if !f.no_gravity {
            f.velocity.y -= 0.04;
        }
        let mut wanted = f.velocity;
        if f.stuck.length_squared() > 1.0e-7 {
            wanted = wanted.multiply(f.stuck);
            f.stuck = Vec3::ZERO;
            f.velocity = Vec3::ZERO;
        }
        let mut scene = collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: Some(collisions::PowderSnowSubject {
                fall_distance: f.fall_distance,
                falling_block: true,
                ..Default::default()
            }),
        };
        let context = CollisionContext::for_entity(
            CollisionEntity {
                identity: EntityIdentity(0),
                y: f.position.y,
                descending: wanted.y < 0.0,
                main_hand: None,
            },
            false,
        );
        let movement = scene.resolve_with_context(
            vise_client::falling_block::bounds(f.position),
            wanted,
            0.0,
            false,
            context,
        )?;
        let next = Vec3::new(
            f.position.x + movement.x,
            f.position.y + movement.y,
            f.position.z + movement.z,
        );
        let on_ground = wanted.y < 0.0 && movement.y != wanted.y;
        let y = next.y.floor() as i32;
        let outside = y <= self.world.bottom_y() || y >= self.world.top_y();
        if !on_ground && (f.age > 100 && outside || f.age > 600) {
            return Ok(if f.drop_item && entity_drops {
                FallStep::Dropped(next)
            } else {
                FallStep::Gone
            });
        }
        if next.y < f.position.y {
            f.fall_distance += f.position.y - next.y;
        }
        let from = f.position;
        f.position = next;
        if on_ground {
            let pos = [
                f.position.x.floor() as i32,
                f.position.y.floor() as i32,
                f.position.z.floor() as i32,
            ];
            return Ok(FallStep::Landed(pos));
        }
        if movement.x != wanted.x {
            f.velocity.x = 0.0;
        }
        if movement.y != wanted.y {
            f.velocity.y = 0.0;
        }
        if movement.z != wanted.z {
            f.velocity.z = 0.0;
        }
        let moved = movement.length_squared();
        let recorded = moved > 1.0e-7 || wanted.length_squared() - moved < 1.0e-7;
        let world = &self.world;
        let facts = self.res.facts();
        let (registry, collision) = (self.pack.assets.registry, self.collision);
        let stuck = vise_client::falling_block::apply_effects_from_blocks(
            from,
            f.position,
            recorded.then_some(wanted),
            &mut f.velocity,
            &mut f.fall_distance,
            |p| {
                use vise_client::falling_block::Inside;
                if &*facts.get(world.block(p.x, p.y, p.z)).name == "minecraft:cobweb" {
                    Inside::Cobweb
                } else if let Some(column) =
                    vise_client::motion::bubble_column_at(world, registry, facts, collision, p)
                {
                    Inside::BubbleColumn(column)
                } else {
                    Inside::Nothing
                }
            },
        );
        if stuck {
            f.stuck = vise_client::falling_block::web_multiplier();
        }
        f.velocity = Vec3::new(
            f.velocity.x * 0.98,
            f.velocity.y * 0.98,
            f.velocity.z * 0.98,
        );
        Ok(FallStep::Falling)
    }
    fn drop_falling_block(&mut self, state: BlockStateId, at: Vec3) {
        let item = self.res.facts().get(state).item.clone();
        if &*item != "minecraft:air" {
            let r = &mut self.world.random;
            let v = [
                r.next_double() * 0.2 - 0.1,
                0.2,
                r.next_double() * 0.2 - 0.1,
            ];
            let stack = ItemStack {
                item,
                count: 1,
                damage: 0,
                extra: None,
            };
            self.spawn_item(stack, [at.x, at.y, at.z], v, 10);
        }
    }
    fn crack_mined_pot(&mut self, pos: [i32; 3], state: BlockStateId) -> BlockStateId {
        let registry = &self.pack.assets.registry;
        let Some(mut pot) = registry
            .state(state)
            .filter(|s| s.name == "minecraft:decorated_pot")
            .cloned()
        else {
            return state;
        };
        let Some(held) = self.held() else {
            return state;
        };
        let breaks = self
            .res
            .items()
            .item_tags
            .get("minecraft:breaks_decorated_pots")
            .is_some_and(|t| t.contains(&*held.item));
        let enchantments = &self.stations().enchantments;
        let kept = held
            .enchantments()
            .iter()
            .any(|(e, _)| enchantments.in_tag("minecraft:prevents_decorated_pot_shattering", e));
        if !breaks || kept {
            return state;
        }
        pot.properties.insert("cracked".into(), "true".into());
        let Ok(cracked) = registry.resolve(&pot) else {
            return state;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.set_block(level, ticks, at, cracked, 260));
        cracked
    }
    fn mine_block_with_held(&mut self, name: &str, hardness: f32) {
        let Some(stack) = self.inventory[self.selected].clone() else {
            return;
        };
        let Some((per_block, max)) = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| Some((i.tool_damage_per_block?, i.max_damage)))
        else {
            return;
        };
        let wears = if &*stack.item == "minecraft:shears" {
            !self.items().block_in_tag("minecraft:fire", name)
        } else {
            hardness != 0.0
        };
        if wears && per_block > 0 && max > 0 {
            self.damage_held(per_block);
        }
        self.stats.add("used", &stack.item, 1);
    }
    fn break_block(&mut self, pos: [i32; 3]) -> Result<()> {
        self.lan_break_pos = Some(pos);
        let broken = self.with_player_actor(|game| game.break_block_as_player(pos));
        self.lan_break_pos = None;
        broken
    }
    fn break_block_as_player(&mut self, pos: [i32; 3]) -> Result<()> {
        if !self.debug_stick_can_destroy(pos)? {
            return Ok(());
        }
        if self.block_action_restricted(pos) {
            return Ok(());
        }
        if !self
            .res
            .items()
            .can_destroy_block(self.held(), self.creative)
        {
            return Ok(());
        }
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let mut canceled = false;
        self.native_destroy_block(pos, state, &mut canceled);
        if canceled {
            return Ok(());
        }
        self.pad_rumble(vise_client::gamepad::Rumble::BlockBroken);
        let broken = state;
        let facts = self.res.facts().get(state).clone();
        let state = self.crack_mined_pot(pos, state);
        if self.guarded_by_piglins(&facts.name) {
            self.anger_piglins(false);
        }
        let key = (pos[0], pos[1], pos[2]);
        if let Some(entity) = self.world.block_entities.get(&key).cloned() {
            let name = self.world.block_names.get(&key).cloned();
            self.pending.record(pos, &entity, &facts.name, name);
        } else if let Some(tag) = spill::saved_entity(&self.world, pos) {
            self.pending.record_saved(pos, &tag);
        }
        self.creaking_heart_broken(pos, true);
        let hive = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Beehive(h)) => self.hive_state(pos).map(|s| ((**h).clone(), s)),
            _ => None,
        };
        let creative = self.creative;
        if &*facts.name == "minecraft:tripwire"
            && self.held().is_some_and(|s| &*s.item == "minecraft:shears")
            && let Some(disarmed) = self.with_props(state, &[("disarmed", "true")])
        {
            let at = BlockPos::new(pos[0], pos[1], pos[2]);
            self.with_level(|b, level, ticks| b.set_block(level, ticks, at, disarmed, 260));
            self.post_block_game_event("minecraft:shear", pos, self.sculk_actor(), None);
        }
        if creative
            && self.ui.rules.bool("block_drops")
            && let Some((h, (honey, _))) = &hive
            && (!h.bees.is_empty() || honey.is_some_and(|l| l > 0))
            && let Some(stack) = self.picked_stack(pos, true)
        {
            let at = pos.map(f64::from);
            self.spawn_item(stack, at, dispenser::new_item_motion(), 10);
        }
        let correct_tool =
            !facts.requires_tool || self.res.items().correct_for_drops(self.held(), &facts.name);
        let removed = self.with_level(|b, level, ticks| {
            b.player_destroy_block(
                level,
                ticks,
                BlockPos::new(pos[0], pos[1], pos[2]),
                creative,
                correct_tool,
            )
        });
        if removed && !creative {
            let held = self.held().cloned();
            self.mine_block_with_held(&facts.name, facts.hardness);
            let harvest = !facts.requires_tool
                || self
                    .res
                    .items()
                    .correct_for_drops(held.as_ref(), &facts.name);
            if harvest {
                self.stats.add("mined", &facts.name, 1);
                if let Some(p) = self.player.as_mut() {
                    p.cause_food_exhaustion(0.005);
                }
                let drops = self.block_loot_at(pos, state, held.as_ref());
                let silk = held
                    .as_ref()
                    .is_some_and(|h| h.enchantment_level("minecraft:silk_touch") > 0);
                for d in drops {
                    if &*d.item != "minecraft:air" {
                        self.pop_resource(pos, d);
                    }
                }
                if let Some((hive, (honey, facing))) = hive {
                    let bees = if silk { hive.bees.len() as i64 } else { 0 };
                    if !silk {
                        self.hive_broken_by_player(pos, hive, honey, facing);
                    }
                    self.trigger_bee_nest_destroyed(broken, held.as_ref(), bees);
                }
                if facts.name.starts_with("minecraft:infested_")
                    && !silk
                    && self.ui.rules.bool("block_drops")
                {
                    let at = [
                        f64::from(pos[0]) + 0.5,
                        f64::from(pos[1]),
                        f64::from(pos[2]) + 0.5,
                    ];
                    if let Some(id) = self.spawn_mob("minecraft:silverfish", at, false) {
                        self.mob_poof(id);
                    }
                }
                let xp = if silk && &*facts.name != "minecraft:spawner" {
                    0
                } else {
                    self.block_experience(&facts.name)
                };
                if xp > 0 {
                    self.spawn_orbs(
                        [
                            f64::from(pos[0]) + 0.5,
                            f64::from(pos[1]) + 0.5,
                            f64::from(pos[2]) + 0.5,
                        ],
                        xp,
                    );
                }
                self.with_level(|b, level, ticks| {
                    b.turtle_egg_destroyed_by_player(
                        level,
                        ticks,
                        BlockPos::new(pos[0], pos[1], pos[2]),
                        broken,
                    );
                });
            }
            let prevents_melting = held
                .as_ref()
                .is_some_and(|h| h.enchantment_level("minecraft:silk_touch") > 0);
            self.with_level(|b, level, ticks| {
                b.ice_destroyed_by_player(
                    level,
                    ticks,
                    BlockPos::new(pos[0], pos[1], pos[2]),
                    broken,
                    prevents_melting,
                );
            });
        }
        self.pending.clear(pos);
        self.process_level_events()?;
        Ok(())
    }
    fn use_item(&mut self) -> Result<Option<Hand>> {
        self.with_player_actor(|game| game.use_item_as_player())
    }
    fn use_item_as_player(&mut self) -> Result<Option<Hand>> {
        if self.spectator() {
            if matches!(self.guest_use, Some(GuestUse::OnBlock(_)))
                && let Some(hit) = self.pick()
            {
                self.spectator_open_menu(hit.pos);
            }
            return Ok(None);
        }
        let guest = self.guest_use;
        if let Some(GuestUse::Entity(hand)) = guest {
            return Ok(self.with_hand(hand, Game::interact_entity)?.then_some(hand));
        }
        let on_block = matches!(guest, Some(GuestUse::OnBlock(_)));
        if !on_block
            && self.riding.is_some()
            && let Some(item) = self.held().map(|s| s.item.clone())
            && self.use_steering_stick(&item)
        {
            return Ok(Some(Hand::Main));
        }
        if guest.is_none() && self.interact_entity()? {
            return Ok(Some(Hand::Main));
        }
        let block_hit = if matches!(guest, Some(GuestUse::Item(_))) || self.entity_targeted() {
            None
        } else {
            self.pick()
        };
        let hands = match guest {
            Some(GuestUse::OnBlock(hand) | GuestUse::Item(hand) | GuestUse::Entity(hand)) => {
                vec![hand]
            }
            None => Hand::BOTH.to_vec(),
        };
        let entity = guest.is_none() && self.entity_targeted();
        for hand in hands {
            if entity && hand == Hand::Off && self.with_hand(hand, Game::interact_entity)? {
                return Ok(Some(hand));
            }
            if self.with_hand(hand, |game| game.use_hand(block_hit.as_ref()))? {
                return Ok(Some(hand));
            }
        }
        Ok(None)
    }
    fn interact_entity(&mut self) -> Result<bool> {
        Ok(self.interact_frame()
            || self.interact_armor_stand()
            || self.interact_cushion()
            || self.interact_boat()
            || self.interact_cart()
            || self.interact_mob()?
            || self.interact_knot())
    }
    pub(super) fn apply_use_cooldown(&mut self, stack: &ItemStack) {
        let default = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.use_cooldown.clone());
        let Some(component) = vise_client::cooldowns::component(stack, default.as_ref()) else {
            return;
        };
        let group = vise_client::cooldowns::group(stack, default.as_ref());
        self.add_cooldown(group, component.ticks());
    }
    pub(super) fn add_cooldown(&mut self, group: Arc<str>, ticks: i32) {
        self.cooldowns.add(Arc::clone(&group), ticks);
        if self.lan_acting.is_some() {
            self.guest_cooldowns.push((group, ticks));
        }
    }
    fn use_hand(&mut self, block_hit: Option<&Hit>) -> Result<bool> {
        let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
        let hands_full = self.stack_in(self.selected).is_some() || self.offhand.is_some();
        let mut anchor_passed = false;
        if !(sneaking && hands_full)
            && let Some(hit) = block_hit
        {
            let held = self.held().cloned();
            if self.use_lectern(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_sign_applicator(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_golem_statue(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_jukebox(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if let Some(swings) = self.use_shelf(hit) {
                let used = held.unwrap_or_else(|| ItemStack::new("minecraft:air", 0));
                self.trigger_block("minecraft:item_used_on_block", hit.pos, &used);
                if swings {
                    self.swing_arm();
                }
                return Ok(true);
            }
            if let Some(swings) = self.use_chiseled_bookshelf(hit, held.as_ref()) {
                if swings {
                    self.swing_arm();
                }
                return Ok(true);
            }
            if self.use_composter(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_vault(hit.pos, held.as_ref())? {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_decorated_pot(hit.pos, held.as_ref()) {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_pumpkin(hit.pos, hit.face, held.as_ref())? {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_beehive(hit.pos, held.as_ref())? {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_campfire_douse(hit.pos, held.as_ref())? {
                self.swing_arm();
                return Ok(true);
            }
            if self.use_redstone_ore(hit.pos, held.as_ref())? {
                self.swing_arm();
                return Ok(true);
            }
            match self.use_respawn_anchor(hit.pos, held.as_ref())? {
                respawn_anchor::AnchorUse::Charged => {
                    self.swing_arm();
                    return Ok(true);
                }
                respawn_anchor::AnchorUse::Pass => anchor_passed = true,
                respawn_anchor::AnchorUse::Other => {}
            }
            let chains_sign = hit.face == [0, -1, 0]
                && held
                    .as_ref()
                    .is_some_and(|s| s.item.ends_with("_hanging_sign"))
                && {
                    let name = &self
                        .res
                        .facts()
                        .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]))
                        .name;
                    name.ends_with("_hanging_sign") && !name.contains("_wall_")
                }
                && matches!(
                    self.world
                        .block_entities
                        .get(&(hit.pos[0], hit.pos[1], hit.pos[2])),
                    Some(BlockEntity::Sign(_))
                )
                && !self.sign_can_execute_click_commands((hit.pos[0], hit.pos[1], hit.pos[2]));
            if !anchor_passed && !chains_sign && self.hand == Hand::Main {
                let face = match hit.face {
                    [0, 1, 0] => Direction::Up,
                    [0, -1, 0] => Direction::Down,
                    [0, 0, -1] => Direction::North,
                    [0, 0, 1] => Direction::South,
                    [-1, 0, 0] => Direction::West,
                    _ => Direction::East,
                };
                let player = self.use_context();
                let at = Vec3::new(hit.point[0], hit.point[1], hit.point[2]);
                let item = held.as_ref().map(|s| s.item.to_string());
                let result = self.with_level(|b, level, ticks| {
                    b.use_item_on(
                        level,
                        ticks,
                        BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
                        face,
                        at,
                        player,
                        item.as_deref(),
                    )
                });
                self.process_level_events()?;
                if result == vise_world::level::InteractionResult::Success {
                    let name = self
                        .res
                        .facts()
                        .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]))
                        .name
                        .clone();
                    match &*name {
                        "minecraft:note_block" => self.stats.custom("tune_noteblock", 1),
                        "minecraft:bell" => self.stats.custom("bell_ring", 1),
                        "minecraft:lever" => self.lever_puff(hit.pos),
                        _ => {}
                    }
                }
                if result.consumes_action() {
                    self.swing_arm();
                    return Ok(true);
                }
            }
        }
        let Some(stack) = self.held().cloned() else {
            return Ok(false);
        };
        let Some(info) = self.items().get(&stack.item).cloned() else {
            return Ok(false);
        };
        if self
            .cooldowns
            .is_on_cooldown(&vise_client::cooldowns::group(
                &stack,
                info.use_cooldown.as_ref(),
            ))
        {
            return Ok(false);
        }
        let on_block = matches!(self.guest_use, Some(GuestUse::OnBlock(_)));
        let mut claimed = false;
        if let Some(hit) = block_hit {
            self.native_use_item_on(&stack, hit.pos, hit.face, &mut claimed);
        }
        if !claimed && !on_block {
            self.native_use_item(&stack, &mut claimed);
        }
        if claimed {
            self.swing_arm();
            return Ok(true);
        }
        if let Some(hit) = block_hit
            && let Some(success) = self.debug_stick_use_on(hit.pos)?
        {
            if self.lan_acting.is_none() {
                self.swing_arm();
            }
            if success {
                self.trigger_block("minecraft:item_used_on_block", hit.pos, &stack);
            }
            return Ok(true);
        }
        if block_hit.is_some() && (self.use_bone_meal(&stack)? || self.ignite_tnt(&stack)?) {
            return Ok(true);
        }
        let spawn_egg = stack.item.ends_with("_spawn_egg");
        use vise_client::first_person::UseResult;
        let swings = self.swings_started;
        match self.use_tool(&stack, block_hit.is_some())? {
            UseResult::Success { swing } => {
                if (UseResult::Success { swing }).should_swing(swings, self.swings_started) {
                    self.swing_arm();
                }
                return Ok(true);
            }
            UseResult::Fail => return Ok(true),
            UseResult::Pass => {}
        }
        if let Some(hit) = block_hit
            && self.use_water_bottle_on_block(hit.pos, hit.face, &stack)?
        {
            return Ok(true);
        }
        if (!on_block && self.use_potion_item(&stack)?)
            || ((block_hit.is_some() || !spawn_egg)
                && (!on_block || spawn_egg)
                && self.throw_item(&stack)?)
        {
            return Ok(true);
        }
        if block_hit.is_some() && &*stack.item == "minecraft:brush" {
            if self.using.is_none() {
                self.using = Some((self.held_slot(), 0));
            }
            return Ok(true);
        }
        if !on_block
            && (self.raise_shield(self.held_slot())
                || self.use_fishing_rod(&stack)
            || self.use_crossbow(&stack)
            || self.use_trident(&stack)
            || self.use_spear(&stack)
            || self.boost_firework(&stack)
            || self.use_empty_map(&stack)
                || self.use_book(&stack)
                || (!(info.block.is_some() && block_hit.is_some())
                    && self.equip_from_hand(&stack)))
        {
            return Ok(true);
        }
        if !on_block && &*stack.item == "minecraft:spyglass" {
            self.use_spyglass();
            return Ok(true);
        }
        if !on_block && info.bundle {
            if self.using.is_none() {
                self.using = Some((self.held_slot(), 0));
            }
            self.swing_arm();
            return Ok(true);
        }
        if !on_block && &*stack.item == "minecraft:goat_horn" {
            self.use_instrument(&stack);
            return Ok(true);
        }
        if !on_block && &*stack.item == "minecraft:bow" {
            if self.using.is_none()
                && (self.creative || self.projectile_slot("minecraft:bow").is_some())
            {
                self.using = Some((self.held_slot(), 0));
            }
            return Ok(true);
        }
        if !on_block && (info.block.is_none() || block_hit.is_none()) && self.start_eating(&info) {
            return Ok(true);
        }
        if !on_block && self.place_boat(&stack) {
            return Ok(true);
        }
        if let Some(hit) = block_hit
            && (self.launch_firework(hit, &stack)
                || self.place_minecart(hit, &stack)
                || self.place_painting(hit, &stack)
                || self.place_item_frame(hit, &stack)
                || self.use_lodestone_compass(hit, &stack)
                || self.place_armor_stand(hit, &stack)
                || self.place_cushion(hit, &stack)
                || self.place_end_crystal(hit, &stack))
        {
            return Ok(true);
        }
        if self.use_ender_eye(&stack, block_hit.is_some())? {
            return Ok(true);
        }
        let water_hit;
        let hit = if matches!(&*stack.item, "minecraft:lily_pad" | "minecraft:frogspawn") {
            let Some(h) = self.pick_fluid() else {
                return Ok(false);
            };
            water_hit = Hit {
                pos: [h.pos[0], h.pos[1] + 1, h.pos[2]],
                ..h
            };
            &water_hit
        } else {
            let Some(hit) = block_hit else {
                return Ok(false);
            };
            hit
        };
        if info.block.is_none() {
            return Ok(false);
        }
        if !self.may_build() && !self.can_place_on_in_adventure(hit.pos) {
            return Ok(!on_block && self.start_eating(&info));
        }
        let face = match hit.face {
            [0, 1, 0] => Direction::Up,
            [0, -1, 0] => Direction::Down,
            [0, 0, -1] => Direction::North,
            [0, 0, 1] => Direction::South,
            [-1, 0, 0] => Direction::West,
            _ => Direction::East,
        };
        let ctx = PlaceContext {
            clicked_pos: BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
            face,
            hit: Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
            inside: false,
            yaw: self.yaw,
            pitch: self.pitch,
            sneaking: self.keys.contains(&KeyCode::ShiftLeft),
            item: stack.item.to_string(),
            can_use_game_master_blocks: self.can_use_game_master_blocks(),
        };
        let outcome = self.with_level(|b, level, ticks| b.place_block(level, ticks, &ctx));
        let PlaceOutcome::Placed { pos, state } = outcome else {
            self.process_level_events()?;
            if self.start_eating(&info) {
                self.stats.add("used", &stack.item, 1);
            }
            return Ok(true);
        };
        let target = [pos.x, pos.y, pos.z];
        let state = self.apply_item_block_state(target, state, &stack)?;
        let name = self.res.facts().get(state).name.clone();
        if let Some(entity) = self.new_block_entity(&name) {
            let key = (target[0], target[1], target[2]);
            self.world.block_entities.insert(key, entity);
            let game_master = self.can_use_game_master_blocks();
            self.apply_placed_item(target, &stack, game_master);
            match self.world.block_entities.get(&key) {
                Some(BlockEntity::CommandBlock(_)) => {
                    let data = stack.data().is_some_and(|d| {
                        d.other(vise_client::block_entity_item::BLOCK_ENTITY_DATA)
                            .is_some()
                    });
                    self.command_block_placed(key, !data);
                }
                Some(BlockEntity::Sign(sign)) if !sign.waxed => self.open_sign_editor(key, true),
                _ => {}
            }
        }
        self.swing_arm();
        let solid_bucket = &*stack.item == "minecraft:powder_snow_bucket";
        if solid_bucket {
            for event in &mut self.level_events {
                if let LevelEvent::Sound { pos: at, sound, .. } = event
                    && *at == pos
                {
                    *sound = "minecraft:item.bucket.empty_powder_snow".into();
                }
            }
        }
        self.process_level_events()?;
        self.stats.add("used", &stack.item, 1);
        self.trigger_block("minecraft:placed_block", target, &stack);
        self.native_place_block(&stack, target, state);
        self.check_wither_spawn(target)?;
        self.check_golem_spawn(target)?;
        self.consume_held();
        if solid_bucket && !self.creative {
            *self.held_mut() = Some(ItemStack::new("minecraft:bucket", 1));
        }
        Ok(true)
    }
    fn use_context(&self) -> PlayerContext {
        PlayerContext {
            yaw: self.yaw,
            pitch: self.pitch,
            may_build: self.may_build(),
            can_eat: self.creative
                || self
                    .player
                    .as_ref()
                    .is_some_and(|p| p.health.food.food < 20),
            game_master: self.can_use_game_master_blocks(),
        }
    }
    fn start_eating(&mut self, info: &vise_client::items::ItemInfo) -> bool {
        if info.consumable.is_none()
            || (info.food.is_some()
                && !self.player.as_ref().is_some_and(|p| {
                    info.can_always_eat || p.health.food.food < 20 || self.creative
                }))
        {
            return false;
        }
        if self.using.is_none() {
            self.using = Some((self.held_slot(), 0));
        }
        true
    }
    fn use_instrument(&mut self, stack: &ItemStack) {
        let Some(instrument) = self.res.items().instrument(stack) else {
            return;
        };
        if self.using.is_none() {
            self.using = Some((self.held_slot(), 0));
        }
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        self.audio
            .play(&instrument.sound, at, instrument.range / 16.0, 1.0);
        self.post_entity_event(
            "minecraft:instrument_play",
            vise_client::mobs::Who::Player,
        );
        if instrument.use_duration > 0.0 {
            let group = vise_client::cooldowns::group(stack, None);
            self.add_cooldown(group, instrument.use_ticks());
        }
        self.stats.add("used", &stack.item, 1);
    }
    fn apply_consume_effect(&mut self, effect: &vise_client::items::ConsumeEffect) {
        use vise_client::items::ConsumeEffect;
        match effect {
            ConsumeEffect::ApplyEffects {
                effects,
                probability,
            } => {
                if self.world.random.next_float() < *probability {
                    for eff in effects {
                        if let Some(def) = vise_client::brewing::effect(&eff.id) {
                            if def.instant {
                                if let Some(a) =
                                    vise_client::effects::instant(def.id, eff.amplifier, 1.0)
                                {
                                    self.apply_effect_action(a);
                                }
                            } else {
                                self.add_player_effect(
                                    vise_client::effects::EffectInstance::new(
                                        def.id,
                                        eff.duration,
                                        eff.amplifier,
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            ConsumeEffect::ClearAllEffects => {
                self.potions.effects.clear();
                self.apply_effect_status();
            }
            ConsumeEffect::RemoveEffects(to_remove) => {
                for target in to_remove {
                    let id = target.trim_start_matches("minecraft:");
                    self.potions.effects.list.retain(|e| e.id != id);
                }
                self.apply_effect_status();
            }
            ConsumeEffect::TeleportRandomly(teleport) => self.teleport_randomly(teleport),
            ConsumeEffect::PlaySound(sound) => {
                let at = self.player.as_ref().map(|p| {
                    let b = vise_core::position::BlockPos::containing(p.position);
                    [b.x, b.y, b.z].map(|v| f64::from(v) + 0.5)
                });
                self.audio.play(sound, at, 1.0, 1.0);
            }
        }
    }
    fn consume_held(&mut self) {
        if self.creative {
            return;
        }
        let cell = self.held_mut();
        if let Some(s) = cell.as_mut() {
            s.count -= 1;
            if s.count == 0 {
                *cell = None;
            }
        }
    }
    fn tick_using(&mut self) {
        let Some(hand) = self.used_hand() else {
            return;
        };
        self.with_hand(hand, Self::tick_used_item);
    }
    fn tick_used_item(&mut self) {
        let Some((slot, ticks)) = self.using else {
            return;
        };
        let Some(stack) = self.stack_in(slot).cloned() else {
            self.using = None;
            return;
        };
        let hand_changed = slot != self.selected && slot != shield::OFFHAND_SLOT;
        if !self.right || hand_changed || self.screen != Screen::Playing {
            self.using = None;
            self.release_used_item(&stack, ticks, hand_changed);
            return;
        }
        self.trigger_using_item(&stack);
        let Some(info) = self.items().get(&stack.item).cloned() else {
            self.using = None;
            return;
        };
        if info.blocks_attacks.is_some() {
            self.using = Some((slot, ticks + 1));
            return;
        }
        if self.tick_drinking(slot, ticks, &stack) {
            return;
        }
        if &*stack.item == "minecraft:bow" {
            self.using = Some((slot, ticks + 1));
            return;
        }
        if &*stack.item == "minecraft:crossbow" {
            self.tick_crossbow(slot, ticks, &stack);
            return;
        }
        if matches!(&*stack.item, "minecraft:trident" | "minecraft:ender_eye") {
            self.using = Some((slot, ticks + 1));
            return;
        }
        if vise_client::spear::Kinetic::of(&stack.item).is_some() {
            self.spear_tick(&stack, ticks);
            self.using = Some((slot, ticks + 1));
            return;
        }
        if &*stack.item == "minecraft:spyglass" {
            if ticks + 1 >= 1200 {
                self.send_use_item_complete();
                self.stop_spyglass();
                self.using = None;
            } else {
                self.using = Some((slot, ticks + 1));
            }
            return;
        }
        if let Some(duration) = self
            .res
            .items()
            .instrument(&stack)
            .map(|i| i.use_ticks().max(0) as u32)
        {
            self.using = (ticks + 1 < duration).then_some((slot, ticks + 1));
            return;
        }
        if &*stack.item == "minecraft:brush" {
            self.tick_brush(slot, ticks);
            return;
        }
        if info.bundle {
            self.tick_bundle(slot, ticks);
            return;
        }
        let Some(consumable) = info.consumable.as_ref() else {
            self.using = None;
            return;
        };
        let duration = consumable.ticks().max(0) as u32;
        let remaining = duration - ticks.min(duration);
        let ticks = ticks + 1;
        if consumable.should_emit(remaining as i32) {
            self.emit_consume(consumable, &stack, 5);
        }
        if ticks >= duration {
            self.using = None;
            self.send_use_item_complete();
            self.apply_use_cooldown(&stack);
            self.finish_consuming(&info, consumable, &stack);
        } else {
            self.using = Some((slot, ticks));
        }
    }
    fn tick_bundle(&mut self, slot: usize, ticks: u32) {
        const USE_DURATION: u32 = 200;
        let remaining = USE_DURATION - ticks.min(USE_DURATION);
        if ticks == 0 || (remaining < USE_DURATION - 10 && remaining.is_multiple_of(2)) {
            self.drop_bundle_content(slot);
        }
        self.using = (ticks + 1 < USE_DURATION).then_some((slot, ticks + 1));
    }
    fn drop_bundle_content(&mut self, slot: usize) {
        let items = self.items();
        let Some(mut bundle) = self.stack_in(slot).cloned() else {
            return;
        };
        let Some(mut contents) = vise_client::bundle::Contents::of(items, &bundle) else {
            return;
        };
        let Some(removed) = contents.remove_one(items) else {
            return;
        };
        contents.store(&mut bundle);
        let item = Arc::clone(&bundle.item);
        *self.stack_in_mut(slot) = Some(bundle);
        let Some(position) = self.player.as_ref().map(|p| p.position) else {
            return;
        };
        let at = [position.x, position.y, position.z];
        let pitch = 0.8 + self.world.random.next_float() * 0.4;
        self.audio
            .play(vise_client::bundle::REMOVE_ONE, Some(at), 0.8, pitch);
        self.player_drop(removed);
        let block = BlockPos::containing(position);
        let pitch = 0.8 + self.world.random.next_float() * 0.4;
        self.audio.play(
            vise_client::bundle::DROP_CONTENTS,
            Some([block.x, block.y, block.z].map(|v| f64::from(v) + 0.5)),
            0.8,
            pitch,
        );
        self.stats.add("used", &item, 1);
    }
    pub(super) fn release_used_item(&mut self, stack: &ItemStack, ticks: u32, hand_changed: bool) {
        match &*stack.item {
            "minecraft:bow" if !hand_changed => self.release_bow(ticks),
            "minecraft:trident" if !hand_changed => self.release_trident(ticks),
            "minecraft:spyglass" => self.stop_spyglass(),
            _ => {}
        }
    }
    fn destroy_progress_event(&mut self, pos: [i32; 3], face: [i32; 3], ticks: u32) {
        let id = if ticks.is_multiple_of(4) { 2020 } else { 2019 };
        let direction = vise_core::geometry::Direction::ALL
            .iter()
            .position(|d| d.normal() == face)
            .unwrap_or(1);
        self.level_event_at(id, pos, direction as i32);
    }
    fn tick_mining(&mut self) -> Result<()> {
        if self.destroy_delay > 0 {
            self.destroy_delay -= 1;
        }
        if !self.left || self.screen != Screen::Playing {
            self.mining = None;
            return Ok(());
        }
        let Some(hit) = self.pick() else {
            self.mining = None;
            return Ok(());
        };
        if self.block_action_restricted(hit.pos)
            || !self
                .res
                .items()
                .can_destroy_block(self.held(), self.creative)
        {
            self.mining = None;
            return Ok(());
        }
        if self.creative {
            if self.destroy_delay == 0 {
                self.break_block(hit.pos)?;
                self.destroy_delay = 5;
            }
            return Ok(());
        }
        if self.destroy_delay > 0 {
            return Ok(());
        }
        if self.mining.as_ref().is_none_or(|m| m.pos != hit.pos) {
            self.attack_block(hit.pos)?;
            self.mining = Some(Mining {
                pos: hit.pos,
                progress: 0.0,
                ticks: 0,
            });
        }
        let block_break_speed = self.block_break_speed();
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        let facts = self.res.facts().get(state);
        if facts.hardness < 0.0 {
            let m = self.mining.as_mut().ok_or("mining")?;
            m.ticks += 1;
            let (pos, ticks) = (m.pos, m.ticks);
            self.destroy_progress_event(pos, hit.face, ticks);
            return Ok(());
        }
        let held = self.held();
        let mut speed = self.items().mining_speed(held, &facts.name);
        speed = self.modify_destroy_speed(speed);
        speed *= block_break_speed;
        let player = self.player.as_ref().ok_or("no player")?;
        if !player.ground.on_ground {
            speed /= 5.0;
        }
        let eye = self.eye_position().unwrap_or_default();
        if self
            .res
            .facts()
            .get(self.world.block(
                eye[0].floor() as i32,
                eye[1].floor() as i32,
                eye[2].floor() as i32,
            ))
            .fluid
            == Fluid::Water
        {
            speed *= self.submerged_mining_speed();
        }
        let harvest = !facts.requires_tool || self.items().correct_for_drops(held, &facts.name);
        let progress = if facts.hardness == 0.0 {
            1.0
        } else {
            speed / facts.hardness / if harvest { 30.0 } else { 100.0 }
        };
        self.swing_arm();
        let m = self.mining.as_mut().ok_or("mining")?;
        m.progress += progress;
        m.ticks += 1;
        if m.progress < 1.0 || m.ticks > 1 {
            let (pos, ticks) = (m.pos, m.ticks);
            self.destroy_progress_event(pos, hit.face, ticks);
        }
        let m = self.mining.as_mut().ok_or("mining")?;
        if m.progress >= 1.0 && (self.lan_acting.is_none() || m.ticks == 1) {
            let pos = m.pos;
            self.mining = None;
            self.break_block(pos)?;
            self.destroy_delay = 5;
        }
        Ok(())
    }
    fn land_column(&self, x: i32, z: i32) -> Option<i32> {
        if self.dim == Dim::Nether {
            for y in (32..120).rev() {
                let floor = self.res.facts().get(self.world.block(x, y - 1, z));
                if floor.full_collision
                    && floor.fluid == Fluid::None
                    && self.res.facts().is_air(self.world.block(x, y, z))
                    && self.res.facts().is_air(self.world.block(x, y + 1, z))
                {
                    return Some(y);
                }
            }
            return None;
        }
        let mut y = self.world.top_y() - 1;
        while y > self.world.bottom_y() {
            let s = self.world.block(x, y, z);
            if !self.res.facts().is_air(s) {
                let f = self.res.facts().get(s);
                let ok = f.full_collision
                    && f.fluid == Fluid::None
                    && !f.name.contains("leaves")
                    && f.hardness >= 0.0;
                return ok.then_some(y + 1);
            }
            y -= 1;
        }
        None
    }
    fn spawn_player(&mut self) -> Result<()> {
        if self.player.is_some() || self.remote.is_some() {
            return Ok(());
        }
        if let Some((_, target)) = &self.travelling {
            if self.win_screen_holds_arrival() {
                return Ok(());
            }
            let (cx, cz) = (
                (target.x.floor() as i32) >> 4,
                (target.z.floor() as i32) >> 4,
            );
            self.streaming.center = (cx, cz);
            if (-2..=2).all(|dz| (-2..=2).all(|dx| self.streaming.is_lit(cx + dx, cz + dz))) {
                let (player, target) = self.travelling.take().ok_or("travel")?;
                if let Some(config) = self.respawn_travel.take() {
                    drop(player);
                    self.place_respawned(config)?;
                } else if self.end_travel {
                    self.place_after_end_travel(player, target)?;
                } else {
                    self.place_after_travel(player, target)?;
                }
            }
            return Ok(());
        }
        let saved = self.level.position.map(|p| Vec3::new(p[0], p[1], p[2]));
        if self.spawn.is_none() {
            self.spawn = self
                .level
                .spawn
                .map(|s| Vec3::new(s[0] + 0.5, s[1], s[2] + 0.5));
        }
        let feet = if let Some(p) = saved {
            let (cx, cz) = ((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4);
            self.streaming.center = (cx, cz);
            if !(-1..=1).all(|dz| (-1..=1).all(|dx| self.streaming.is_lit(cx + dx, cz + dz))) {
                return Ok(());
            }
            self.level.position = None;
            p
        } else if self.spawn.is_none() || self.player_spawn.is_some() {
            if self.spawn.is_none() {
                let Some([x, y, z]) = self.find_world_spawn()? else {
                    return Ok(());
                };
                self.spawn = Some(Vec3::new(
                    f64::from(x) + 0.5,
                    f64::from(y),
                    f64::from(z) + 0.5,
                ));
                self.level.spawn = Some([f64::from(x), f64::from(y), f64::from(z)]);
                self.level.world_spawn.get_or_insert([x, y, z]);
                if vise_client::save::WorldGen::read(&self.world_root)
                    .is_some_and(|g| g.bonus_chest)
                {
                    self.place_bonus_chest([x, y, z])?;
                }
            }
            let s = self
                .level
                .spawn
                .unwrap_or_default()
                .map(|v| v.floor() as i32);
            let Some(feet) = self.find_player_spawn(s)? else {
                return Ok(());
            };
            feet
        } else {
            let s = self.spawn.unwrap_or_default();
            let (cx, cz) = ((s.x.floor() as i32) >> 4, (s.z.floor() as i32) >> 4);
            self.streaming.center = (cx, cz);
            if !(-1..=1).all(|dz| (-1..=1).all(|dx| self.streaming.is_lit(cx + dx, cz + dz))) {
                return Ok(());
            }
            let column = [s.x.floor() as i32, s.y.floor() as i32, s.z.floor() as i32];
            if self.level.world_spawn == Some(column) {
                let Some(feet) = self.find_player_spawn(column)? else {
                    return Ok(());
                };
                feet
            } else {
                let y = self
                    .land_column(column[0], column[2])
                    .map_or(s.y, f64::from);
                Vec3::new(s.x, y.max(s.y), s.z)
            }
        };
        let mut player = PlayerMotion::new(self.motion, feet)?;
        player.yaw = self.yaw;
        let entering = self.restore_player_state;
        player.load_saved_state_once(&mut self.restore_player_state, &self.level)?;
        let spectator = self.mode == vise_client::command::args::GameMode::Spectator;
        player.may_fly = self.creative || spectator;
        player.invulnerable = self.creative || spectator;
        player.spectator = spectator;
        if spectator {
            player.flying = true;
        } else if !player.may_fly {
            player.flying = false;
        }
        player.respiration = self.armor[0]
            .as_ref()
            .map_or(0, |s| s.enchantment_level("minecraft:respiration") as i32);
        player.pitch = self.pitch;
        player.glider = self.glider_worn();
        if player.health.dead_or_dying() {
            self.screen = Screen::Dead;
            self.ui.died_at = Some(Instant::now());
            self.ui.death_text = None;
            self.ui.death_confirm = None;
            self.capture(false);
        }
        self.previous_position = feet;
        if entering {
            self.stop_sounds_for_new_level();
        }
        self.player = Some(player);
        self.pad.player_replaced();
        self.sway = Default::default();
        self.apply_equipment();
        self.eye = 1.62;
        self.previous_eye = 1.62;
        self.clock.reset();
        self.capture_loaded_world();
        Ok(())
    }
    fn landing_block_updates(&mut self, event: &vise_client::landing::FallEvent) {
        use vise_client::landing::FallEvent;
        let (position, flags) = match *event {
            FallEvent::Write {
                position, flags, ..
            } => (position, flags as i32),
            FallEvent::Destroy { position, .. } => (position, level_flags::UPDATE_ALL),
            _ => return,
        };
        self.with_level(|b, level, ticks| b.notify_written(level, ticks, position, flags));
    }
    fn tick(&mut self) -> Result<()> {
        if self.agent.hold_server {
            return Ok(());
        }
        self.tick_debug_keys()?;
        self.tick_debug_charts();
        self.tick_tutorial();
        if self.screen == Screen::Creative {
            self.refresh_creative_tabs();
        }
        if self.remote.is_some() {
            return self.tick_remote();
        }
        self.loading_progress = match self.loading_reason() {
            Some(_) => vise_client::level_loading::smooth(
                self.loading_progress,
                self.level_load_progress(),
            ),
            None => 0.0,
        };
        self.ticks += 1;
        self.track_experience_display();
        let running = match self.level_pass.running {
            Some(running) => running,
            None => self.clock.world_runs(),
        };
        self.boss_overlay.tick_darken(running);
        self.tick_item_activation();
        if running {
            self.world.game_time += 1;
            if self.ui.rules.bool("advance_weather") {
                self.world.weather.tick_timers(&mut self.world.random);
            }
            self.world.weather.tick_levels();
            self.tick_weather_effects();
        }
        self.autosaving = self.ticks.is_multiple_of(6000);
        if let Some(serving) = &self.serving {
            serving.set_saving(self.autosaving);
        }
        if self.autosaving {
            self.save_game_with(false)?;
        }
        if running {
            self.advance_clock();
        }
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        self.tick_totem();
        self.update_totem_ready();
        self.tick_command_extras();
        self.native_tick();
        self.tell_native_now();
        let camera_eye = self.camera_eye_height();
        let stick = self.pad_move();
        if self.riding.is_some() && self.player.is_some() {
            let playing = self.screen == Screen::Playing;
            let held = |k| playing && self.keys.contains(&k);
            let keys = Keys {
                forward: held(KeyCode::KeyW),
                backward: held(KeyCode::KeyS),
                left: held(KeyCode::KeyA),
                right: held(KeyCode::KeyD),
                jump: held(KeyCode::Space),
                shift: held(KeyCode::ShiftLeft),
                sprint: false,
                analog: None,
            }
            .with_stick(stick.filter(|_| playing));
            self.tick_riding(keys)?;
        }
        let riding = self.riding.is_some();
        let entity_collisions = self.player_entity_collisions();
        let spin_targets = self.spin_targets();
        let sprint_window = self.settings.sprint_window;
        self.sync_player_body();
        let Some(player) = self.player.as_mut() else {
            if self.options.dedicated.is_some() {
                return self.tick_world(running);
            }
            return Ok(());
        };
        let (cx, cz) = (
            (player.position.x.floor() as i32) >> 4,
            (player.position.z.floor() as i32) >> 4,
        );
        if !self.streaming.is_lit(cx, cz) {
            return Ok(());
        }
        let playing = self.screen == Screen::Playing && self.sleeping.is_none();
        let held = |k| playing && self.keys.contains(&k);
        let keys = Keys {
            forward: held(KeyCode::KeyW),
            backward: held(KeyCode::KeyS),
            left: held(KeyCode::KeyA),
            right: held(KeyCode::KeyD),
            jump: held(KeyCode::Space),
            shift: held(KeyCode::ShiftLeft),
            sprint: held(KeyCode::ControlLeft),
            analog: None,
        }
        .with_stick(stick.filter(|_| playing));
        if !riding {
            self.previous_position = player.position;
        }
        let spectator = self.mode == vise_client::command::args::GameMode::Spectator;
        player.may_fly = self.creative || spectator;
        player.invulnerable = self.creative || spectator;
        player.spectator = spectator;
        player.spin_targets = spin_targets;
        let grounded = player.ground.on_ground;
        let before = (
            grounded,
            [player.velocity.x, player.velocity.y, player.velocity.z],
            [player.position.x, player.position.y, player.position.z],
        );
        if !riding {
            MotionWorld {
                entities: &entity_collisions,
                player,
                scene: &mut self.world,
                assets: &self.pack.assets,
                collision_binding: self.collision,
                resources: self.motion,
            }
            .controlled_simulation_tick_with_sprint_window(keys, sprint_window)?;
            if grounded && keys.jump && !player.flying && player.velocity.y > 0.2 {
                self.stats.custom("jump", 1);
            }
        }
        let events = std::mem::take(&mut player.events);
        let spin_hit = player.spin_hit.take();
        let glider_wear = std::mem::take(&mut player.glider_wear);
        let feet = [player.position.x, player.position.y, player.position.z];
        let player_velocity = [player.velocity.x, player.velocity.y, player.velocity.z];
        self.previous_eye = self.eye;
        let eye_height = if self.sleeping.is_some() {
            0.2
        } else {
            player.dimensions.eye_height
        };
        self.eye += (camera_eye.unwrap_or(eye_height) - self.eye) * 0.5;
        if let Some((at, velocity)) = self.tick_soul_speed() {
            self.particles.send_particles(
                &vise_client::particles::ParticleOptions::simple(
                    vise_client::particles::ParticleType::Soul,
                ),
                at,
                velocity,
                1.0,
                0,
            );
            if let Some(host) = self.lan.as_ref() {
                lan_particles::send_soul_particle(host, at, velocity);
            }
        }
        if glider_wear > 0
            && !self.creative
            && let Some(stack) = self.armor[1].clone()
        {
            let loss = self.wear(&stack, glider_wear);
            if let Some(elytra) = self.armor[1].as_mut() {
                elytra.damage += loss;
            }
        }
        self.tick_avatar(before, riding);
        self.end_spin_attack(spin_hit);
        self.record_combat(&events);
        self.wear_armor(&events);
        for event in events {
            match event {
                MotionEvent::HurtSound(sound) => self.player_hurt_sound(sound, feet),
                MotionEvent::Emission { state, muffled, .. } => {
                    let sounds = self.res.facts().get(state).sounds.clone();
                    let (volume, pitch) = if muffled {
                        (sounds.volume * 0.05, sounds.pitch * 0.8)
                    } else {
                        (sounds.volume * 0.15, sounds.pitch)
                    };
                    self.audio.play(&sounds.step, Some(feet), volume, pitch);
                }
                MotionEvent::Swim { volume, pitch } => {
                    self.audio
                        .play("minecraft:entity.player.swim", Some(feet), volume, pitch);
                }
                MotionEvent::Chime { volume, pitch } => {
                    self.audio.play(
                        "minecraft:block.amethyst_block.chime",
                        Some(feet),
                        volume,
                        pitch,
                    );
                }
                MotionEvent::Drown => {
                    if let Some(host) = &self.lan {
                        host.hub
                            .entity_event(host.host_entity, combat::DROWN_EVENT, feet);
                    }
                    self.drown_particles(feet, player_velocity);
                }
                MotionEvent::BubbleColumnParticle { bubble, position } => {
                    use vise_client::particles::{ParticleOptions, ParticleType};
                    let (kind, spread, speed) = if bubble {
                        (ParticleType::Bubble, [0.0, 0.01, 0.0], 0.2)
                    } else {
                        (ParticleType::Splash, [0.0; 3], 1.0)
                    };
                    self.particles.send_particles(
                        &ParticleOptions::simple(kind),
                        [position.x, position.y, position.z],
                        spread,
                        speed,
                        1,
                    );
                }
                MotionEvent::Fall(vise_client::landing::FallEvent::Sound {
                    sound,
                    volume,
                    pitch,
                    ..
                }) => {
                    let Some(name) = sound.player_fall_sound() else {
                        continue;
                    };
                    self.audio.play(name, Some(feet), volume, pitch);
                }
                MotionEvent::Fall(vise_client::landing::FallEvent::Statistic(cm)) => {
                    self.stats.custom("fall_one_cm", cm)
                }
                MotionEvent::Fall(vise_client::landing::FallEvent::Particles {
                    state,
                    position,
                    count,
                    speed,
                }) => self.landing_dust(None, state, position, count, speed),
                MotionEvent::Fall(event) => self.landing_block_updates(&event),
                MotionEvent::Snowflake { position, velocity } => self.snowflake(position, velocity),
                MotionEvent::EnterBlock { state } => self.trigger_enter_block(state),
                MotionEvent::HoneySlide {
                    pos,
                    sound,
                    particles,
                } => {
                    if self.world.game_time % 20 == 0 {
                        self.trigger_honey_slide([pos.x, pos.y, pos.z]);
                    }
                    if sound {
                        self.audio
                            .play("minecraft:block.honey_block.slide", Some(feet), 1.0, 1.0);
                    }
                    if particles {
                        self.honey_slide_particles(feet);
                        let id = self.lan.as_ref().map(|h| h.host_entity);
                        if let Some(id) = id {
                            self.lan_entity_event(id, 53, feet);
                        }
                    }
                }
                MotionEvent::InsidePortal { pos } => self.player_inside_nether_portal(pos),
                MotionEvent::CauldronExtinguish { pos } => self.cauldron_extinguish(pos),
                MotionEvent::InsideWitherRose => self.wither_rose_contact(),
                MotionEvent::Bounce { .. } => {
                    self.post_entity_event("minecraft:bounce", vise_client::mobs::Who::Player)
                }
                MotionEvent::SmashLanding {
                    state,
                    position,
                    fall_distance,
                } => self.smash_landing_particles(state, position, fall_distance),
                MotionEvent::Splash {
                    high_speed,
                    volume,
                    pitch,
                } => {
                    let name = if high_speed {
                        "minecraft:entity.player.splash.high_speed"
                    } else {
                        "minecraft:entity.player.splash"
                    };
                    self.audio.play(name, Some(feet), volume, pitch);
                }
                MotionEvent::SplashParticle {
                    bubble,
                    position,
                    velocity,
                } => {
                    use vise_client::particles::{ParticleOptions, ParticleType};
                    let kind = if bubble {
                        ParticleType::Bubble
                    } else {
                        ParticleType::Splash
                    };
                    self.particles.add(
                        &ParticleOptions::simple(kind),
                        [position.x, position.y, position.z],
                        [velocity.x, velocity.y, velocity.z],
                    );
                }
                _ => {}
            }
        }
        self.powder_snow_fire()?;
        self.rain_extinguish_in_frame();
        if self.remote.is_none() {
            self.location_changed_effects(before.2, before.0);
        }
        self.tick_totem();
        self.handle_shoulder_entities();
        let Some(player) = self.player.as_mut() else {
            return Ok(());
        };
        if player.position.y < f64::from(self.world.bottom_y() - 64) {
            let _ = player.set_health(0.0, false);
        }
        let dead = player.health.dead_or_dying();
        if dead && self.screen != Screen::Dead {
            let feet = [player.position.x, player.position.y, player.position.z];
            let at = [feet[0], feet[1] + 1.32, feet[2]];
            let killer = self.combat_killer_kind();
            self.level.last_death =
                Some((self.dim.id().to_owned(), feet.map(|v| v.floor() as i32)));
            self.death_message();
            let bindings = match self.lan.as_ref() {
                Some(host) if !host.hub.guests.is_empty() => host.mob_players.clone(),
                _ => Default::default(),
            };
            self.tell_neutral_mobs_player_died(feet, None, &bindings);
            self.stats.custom("deaths", 1);
            self.score_player_died();
            if let Some(kind) = killer {
                self.stats.add("killed_by", &kind, 1);
                let view = vise_client::advancements::EntityView {
                    kind,
                    ..Default::default()
                };
                self.trigger_entity("minecraft:entity_killed_player", "entity", &view);
            }
            self.stats.reset("custom", "minecraft:time_since_death");
            self.stats.reset("custom", "minecraft:time_since_rest");
            let reward = self
                .experience
                .death_reward(self.ui.rules.bool("keep_inventory") || self.spectator());
            self.spawn_orbs(feet, reward);
            if !self.spectator() {
                self.drop_inventory(at);
            }
            self.audio
                .play("minecraft:entity.player.death", Some(at), 1.0, 1.0);
            if let Some(host) = &self.lan {
                host.hub.entity_event(host.host_entity, 3, feet);
            }
            if self.ui.rules.bool("immediate_respawn") {
                self.respawn_player()?;
            } else {
                self.screen = Screen::Dead;
                self.capture(false);
                self.ui.death_time = 0;
            }
        } else if dead {
            self.ui.death_time += 1;
            if self.ui.death_time >= 20
                && let Some(stack) = self.carried.take()
                && let Some(eye) = self.eye_position()
            {
                self.throw_from_eyes(stack, eye, false);
            }
        }
        self.tick_world(running)
    }
    fn tick_world(&mut self, running: bool) -> Result<()> {
        self.share_spawn_conditions();
        self.update_chunk_tickets();
        vise_client::spans::time("tick_effects", || self.tick_effects(running));
        self.shield_cooldown = self.shield_cooldown.saturating_sub(1);
        self.cooldowns.tick();
        vise_client::spans::time("tick_using", || self.tick_using());
        self.tick_bundle_hover();
        vise_client::spans::time("tick_hud", || self.tick_hud());
        vise_client::spans::time("tick_local_hand", || self.tick_local_hand())?;
        vise_client::spans::time("postprocess_chunks", || self.postprocess_chunks())?;
        if running {
            vise_client::spans::time("tick_blocks", || self.tick_blocks())?;
        }
        if running {
            vise_client::spans::time("tick_paintings", || self.tick_paintings());
            vise_client::spans::time("tick_item_frames", || self.tick_item_frames());
            vise_client::spans::time("tick_armor_stands", || self.tick_armor_stands());
            vise_client::spans::time("tick_cushions", || self.tick_cushions());
            vise_client::spans::time("tick_boats", || self.tick_boats());
            vise_client::spans::time("tick_carts", || self.tick_carts());
            vise_client::spans::time("tick_thunder", || self.tick_thunder());
            vise_client::spans::time("tick_lecterns", || self.tick_lecterns());
            vise_client::spans::time("tick_maps", || self.tick_maps());
            self.tick_lodestone_compasses();
            vise_client::spans::time("tick_bolts", || self.tick_bolts());
            vise_client::spans::time("tick_fishing", || self.tick_fishing());
            vise_client::spans::time("tick_tridents", || self.tick_tridents());
            vise_client::spans::time("tick_rockets", || self.tick_rockets());
            vise_client::spans::time("tick_ominous_item_spawners", || {
                self.tick_ominous_item_spawners()
            });
        }
        if running {
            vise_client::spans::time("entity_portals", || {
                self.handle_entity_portals();
                self.handle_end_gateway_portals();
            });
            vise_client::spans::time("tick_items", || self.tick_items());
            vise_client::spans::time("touch_arrows", || self.touch_arrows());
        }
        vise_client::spans::time("spawn_loaded_entities", || self.spawn_loaded_entities());
        self.restore_root_vehicle();
        if running {
            vise_client::spans::time("tick_mobs", || self.tick_mobs())?;
            vise_client::spans::time("recheck_golem_openers", || self.recheck_golem_openers());
            vise_client::spans::time("tick_experience", || self.tick_experience());
            vise_client::spans::time("entity_portals", || {
                self.find_entity_portals();
                self.resolve_arrivals();
            });
        }
        vise_client::spans::time("tick_lan", || self.tick_lan(false));
        self.tick_friends();
        vise_client::spans::time("tick_hand", || self.tick_hand());
        vise_client::spans::time("tick_sleep", || self.tick_sleep());
        self.announce_sleep_status();
        self.free_left_beds();
        vise_client::spans::time("tick_portal", || self.tick_nether_portal())?;
        if running {
            vise_client::spans::time("tick_end", || self.tick_end())?;
            vise_client::spans::time("end_gateways", || {
                self.find_end_gateway_portals();
                self.resolve_end_gateway_arrivals()
            })?;
            self.keep_gateway_destinations();
        }
        if running {
            vise_client::spans::time("tick_falling", || self.tick_falling())?;
            vise_client::spans::time("tick_tnt", || self.tick_tnt())?;
        }
        if running {
            vise_client::spans::time("tick_block_entities", || self.tick_block_entities())?;
            vise_client::spans::time("tick_sculk_entities", || self.tick_sculk_entities());
            vise_client::spans::time("tick_sculk_blocks", || self.tick_sculk_blocks())?;
        }
        if running {
            vise_client::spans::time("tick_block_entity_anims", || {
                self.tick_block_entity_anims()
            });
            vise_client::spans::time("tick_particles", || self.tick_particles());
            self.tick_client_ambient();
        }
        self.publish_boss_bars();
        if self.right
            && self.use_delay == 0
            && self.using.is_none()
            && self.screen == Screen::Playing
            && self.sleeping.is_none()
        {
            self.use_with_hand()?;
            self.use_delay = 4;
        }
        Ok(())
    }
    fn sync_toggle(&mut self, slot: usize) {
        let key = [KeyCode::ShiftLeft, KeyCode::ControlLeft][slot];
        if self.toggles.is_down(slot) {
            self.keys.insert(key);
        } else {
            self.keys.remove(&key);
        }
    }
    fn mapped_key(&mut self, key: KeyCode, pressed: bool, repeat: bool) -> Result<()> {
        let toggle = match key {
            KeyCode::ShiftLeft => Some((0, self.settings.toggle_crouch)),
            KeyCode::ControlLeft => Some((1, self.settings.toggle_sprint)),
            _ => None,
        };
        if let Some((slot, option)) = toggle {
            let action = match (pressed, repeat) {
                (false, _) => KeyAction::Release,
                (_, false) => KeyAction::Press,
                (_, true) => KeyAction::Repeat,
            };
            let screen_open = self.screen != Screen::Playing || self.title.is_some();
            self.toggles.key(slot, action, screen_open, option);
            self.sync_toggle(slot);
            if !pressed {
                return Ok(());
            }
        } else if !pressed {
            self.keys.remove(&key);
            return Ok(());
        } else {
            self.keys.insert(key);
        }
        if repeat && key != KeyCode::KeyQ {
            return Ok(());
        }
        self.key_pressed(key)
    }
    fn track_toggle_screens(&mut self) {
        let playing = self.screen == Screen::Playing && self.title.is_none();
        if playing == self.was_playing {
            return;
        }
        self.was_playing = playing;
        let options = [
            self.settings.toggle_crouch,
            self.settings.toggle_sprint,
            self.settings.toggle_attack,
            self.settings.toggle_use,
        ];
        if playing {
            self.toggles.screen_closed(options);
        } else {
            self.toggles.screen_opened(options);
        }
        self.sync_toggle(0);
        self.sync_toggle(1);
        self.left = self.toggles.is_down(2);
        self.right = self.toggles.is_down(3);
    }
    fn look_options(&self) -> LookOptions {
        LookOptions {
            sensitivity: f64::from(self.settings.sensitivity),
            smooth_camera: self.smooth_camera,
            scoping: self.scoping(),
            invert_x: self.settings.invert_x,
            invert_y: self.settings.invert_y,
        }
    }
    fn apply_mouse_look(&mut self) {
        let look = self.look_options();
        let time = self.started.elapsed().as_secs_f64();
        let pointer = self.captured || self.agent_pointer;
        let grabbed = pointer && self.screen == Screen::Playing && self.player.is_some();
        let active = self.focused || self.agent_pointer;
        if let Some((xo, yo)) = self
            .mouse
            .handle_accumulated_movement(time, active, grabbed, look)
        {
            self.tutorial.mouse(xo, yo);
            let zoom = self.zoom_turn_scale();
            self.turn_player(xo * zoom, yo * zoom);
        }
    }
    fn pauses_game(&self) -> bool {
        self.has_singleplayer_server()
            && !self.is_published()
            && self.title.is_none()
            && (matches!(
                self.screen,
                Screen::Paused
                    | Screen::Options
                    | Screen::Stats
                    | Screen::Advancements
                    | Screen::WorldOptions
                    | Screen::Friends
                    | Screen::ServerLinks
                    | Screen::ConfirmLink
                    | Screen::DialogWarning
                    | Screen::CommandBlock
            ) || self.book.is_some()
                || self.win_screen_pauses())
    }
    fn pause_if_inactive(&mut self) {
        if self
            .focus_lost
            .is_some_and(|t| t.elapsed() > Duration::from_millis(500))
            && self.control.is_none()
            && self.screen == Screen::Playing
            && self.player.is_some()
            && !self.options.no_pause
            && self.settings.file.bool("pauseOnLostFocus")
            && self.book.is_none()
            && self.sign_edit.is_none()
            && self.server_packs.prompt.is_none()
            && !self.dialog_open()
        {
            self.screen = Screen::Paused;
        }
    }
    fn capture(&mut self, capture: bool) {
        if self.lan_acting.is_some() {
            return;
        }
        if capture && (!self.focused || self.server_packs.prompt.is_some() || self.dialog_open()) {
            return;
        }
        self.reset_inventory_clicks();
        if !capture {
            self.stop_mining();
        }
        self.agent_pointer = false;
        if !self.keyboard_mouse {
            self.captured = false;
            return;
        }
        let Some(window) = &self.window else { return };
        let size = self.renderer.as_ref().map_or_else(
            || window.inner_size().into(),
            |r| [r.config.width, r.config.height],
        );
        let [vx, vy] = self.view_at.map(f64::from);
        let centre = winit::dpi::PhysicalPosition::new(
            vx + f64::from(size[0]) / 2.0,
            vy + f64::from(size[1]) / 2.0,
        );
        let warp = |window: &Window| {
            let _ = window.set_cursor_position(centre);
        };
        let scale = HudBatch::new(size).scale;
        let was_grabbed = self.captured;
        if capture {
            self.mouse.set_ignore_first_move();
            #[cfg(target_os = "macos")]
            warp(window);
            self.captured = window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok();
            #[cfg(not(target_os = "macos"))]
            if self.captured && self.focused {
                warp(window);
            }
        } else {
            if self.captured {
                if self.focused {
                    warp(window);
                }
                let _ = window.set_cursor_grab(CursorGrabMode::None);
                if self.focused {
                    warp(window);
                }
            } else {
                let _ = window.set_cursor_grab(CursorGrabMode::None);
            }
            self.captured = false;
        }
        if let Some(pos) = vise_client::mouse::mouse_position_after_grab_change(
            capture,
            was_grabbed,
            size,
            scale,
        ) {
            self.cursor = pos;
        }
        window.set_cursor_visible(!self.captured);
    }
    fn apply_cursor_icon(&mut self) {
        use vise_client::widgets::Cursor;
        let wanted = vise_client::widgets::take_cursor();
        let effective = if self.settings.allow_cursor_changes {
            wanted
        } else {
            Cursor::Arrow
        };
        if effective == self.cursor_icon || !self.keyboard_mouse {
            return;
        }
        if let Some(window) = &self.window {
            window.set_cursor(match effective {
                Cursor::Arrow => winit::window::CursorIcon::Default,
                Cursor::PointingHand => winit::window::CursorIcon::Pointer,
                Cursor::NotAllowed => winit::window::CursorIcon::NotAllowed,
            });
        }
        self.cursor_icon = effective;
    }
    fn capture_loaded_world(&mut self) {
        if self.title.is_none() && self.screen == Screen::Playing {
            self.capture(true);
        }
    }
    fn draw_item(&self, batch: &mut HudBatch, stack: &ItemStack, x: f32, y: f32) {
        self.draw_item_model(batch, stack, x, y);
        self.draw_item_decorations(batch, stack, x, y, None);
    }
    fn draw_slot_item(
        &self,
        batch: &mut HudBatch,
        drag: Option<&DragView>,
        slot: usize,
        stack: &Option<ItemStack>,
        x: f32,
        y: f32,
    ) {
        let carried;
        let (stack, capped) = match drag.and_then(|d| d.slots.get(&slot)) {
            Some(None) => return,
            Some(Some((count, capped))) => {
                batch.fill(&self.gui.atlas, x, y, 16.0, 16.0, rgba(255, 255, 255, 128));
                carried = self
                    .carried
                    .clone()
                    .map(|c| ItemStack { count: *count, ..c });
                (carried.as_ref(), *capped)
            }
            None => (stack.as_ref(), false),
        };
        if let Some(stack) = stack {
            self.draw_item_model(batch, stack, x, y);
            let text = capped.then(|| format!("\u{a7}e{}", stack.count));
            self.draw_item_decorations(batch, stack, x, y, text);
        }
    }
    fn draw_carried(&mut self, batch: &mut HudBatch) {
        let Some(mut stack) = self.carried.clone() else {
            return;
        };
        if let Some(left) = self.drag_view().and_then(|d| d.left) {
            if left == 0 {
                return;
            }
            stack.count = left;
        }
        self.draw_item(batch, &stack, self.cursor.0 - 8.0, self.cursor.1 - 8.0);
    }
    fn drag_view(&mut self) -> Option<DragView> {
        let (ty, slots) = self.inv_drag.clone()?;
        let mut view = None;
        self.menu_session(|menu, env, to_menu| {
            let menu_slots: Option<Vec<usize>> = slots.iter().map(|&g| to_menu(g)).collect();
            let shown = match menu.quick_craft_preview(env, ty, &menu_slots?) {
                Some((shown, left)) => {
                    let slots = slots.iter().copied().zip(shown).collect();
                    DragView {
                        slots,
                        left: Some(left),
                    }
                }
                None => DragView {
                    slots: slots.iter().map(|&g| (g, None)).collect(),
                    left: None,
                },
            };
            view = Some(shown);
            Some(false)
        });
        view
    }
    fn quick_craft_accepts(&mut self, slot: usize, ty: i32, dragged: usize) -> bool {
        let mut accepts = true;
        self.menu_session(|menu, env, to_menu| {
            accepts = to_menu(slot).is_some_and(|s| menu.quick_craft_accepts(env, s, ty, dragged));
            Some(false)
        });
        accepts
    }
    fn draw_item_decorations(
        &self,
        batch: &mut HudBatch,
        stack: &ItemStack,
        x: f32,
        y: f32,
        count_text: Option<String>,
    ) {
        let info = self.items().get(&stack.item);
        let max_damage = info.map_or(0, |info| info.max_damage as i32);
        let cooldown =
            vise_client::cooldowns::group(stack, info.and_then(|i| i.use_cooldown.as_ref()));
        let partial = self.clock.fraction() as f32;
        let decorations = hud_overlays::ItemDecorations {
            count: stack.count,
            count_text,
            bar: vise_client::bundle::Contents::of(self.items(), stack)
                .map(|c| c.bar())
                .unwrap_or_else(|| hud_overlays::damage_bar(stack.damage as i32, max_damage)),
            cooldown: self.cooldowns.percent(&cooldown, partial),
        };
        let mut draws = vec![hud_layout::GuiDraw::Push(vec![
            hud_layout::PoseOp::Translate(x, y),
        ])];
        draws.extend(hud_overlays::item_decorations(
            0,
            0,
            &decorations,
            &self.gui.atlas.font,
        ));
        draws.push(hud_layout::GuiDraw::Pop);
        hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
    fn hud(&mut self, size: [u32; 2]) -> HudBatch {
        let mut batch = if (self.server_packs.prompt.is_some()
            || self.code_of_conduct.is_some()
            || self.dialog_open())
            && self.player.is_none()
        {
            HudBatch::new(size)
        } else {
            self.hud_layers(size)
        };
        self.draw_dialog(&mut batch);
        self.draw_pack_prompt(&mut batch);
        self.draw_code_of_conduct(&mut batch);
        self.draw_link_prompt(&mut batch);
        self.draw_debug_overlay(&mut batch, size);
        self.frame_graph
            .draw(self.settings.vise.overlay, &mut batch, &self.gui.atlas);
        self.draw_pad_cursor(&mut batch);
        self.resolve_inline_profiles(&batch.inline_profiles);
        batch
    }
    fn hud_layers(&mut self, size: [u32; 2]) -> HudBatch {
        let mut batch = HudBatch::new(size);
        let (w, h) = (batch.width, batch.height);
        if self.screen == Screen::Win {
            self.draw_win_screen(&mut batch);
            return batch;
        }
        if self.screen == Screen::Options {
            self.draw_options(&mut batch);
            return batch;
        }
        if let Some(title) = &mut self.title {
            title.poll_backup();
            if let Some(sound) = title.update_toast(self.settings.notification_display_time) {
                self.audio.play(sound, None, 1.0, 1.0);
            }
        }
        if let Some(title) = &self.title {
            title.draw(
                &self.gui.atlas,
                &mut batch,
                self.cursor,
                self.settings.file.bool("hideSplashTexts"),
            );
            return batch;
        }
        let atlas_ptr: *const HudAtlas = &self.gui.atlas;
        let _ = atlas_ptr;
        if self.remote_overlay(&mut batch) {
            self.draw_pack_toasts(&mut batch, false);
            return batch;
        }
        if let Some(reason) = self.loading_reason() {
            self.draw_level_loading(&mut batch, reason);
            self.draw_pack_toasts(&mut batch, false);
            return batch;
        }
        let spectator = self.spectator();
        let (wi, hi) = (w as i32, h as i32);
        let show_hud = !self.ui.hide_gui;
        let mut bar_height = 39;
        if show_hud {
            self.draw_spyglass(&mut batch);
            let indicator = self.settings.attack_indicator;
            let delay = vise_client::attack_strength::delay(self.main_hand_attack_speed());
            let strength = self.hand_strength.scale(0.0, self.main_hand_attack_speed());
            if self.camera_type.first_person()
                && (!spectator || self.spectator_crosshair())
                && !self.debug_crosshair()
            {
                let attack = (indicator == 1).then_some(strength);
                let full = attack.is_some_and(|a| {
                    hud_layout::full_attack_indicator(a, delay, self.crosshair_living())
                });
                let draws = hud_layout::crosshair(wi, hi, attack, full);
                hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
            }
            if spectator {
                self.draw_spectator_hotbar(&mut batch);
            } else {
                let partial = self.clock.fraction() as f32;
                let inputs = hud_layout::HotbarInputs {
                    selected: self.selected as i32,
                    right_handed: !self.settings.main_hand_left,
                    offhand: self.offhand.is_some(),
                    slots: std::array::from_fn(|i| {
                        self.inventory[i]
                            .as_ref()
                            .map(|_| (f32::from(self.hotbar_pop[i]) - partial).max(0.0))
                    }),
                    offhand_pop: (f32::from(self.hotbar_pop[9]) - partial).max(0.0),
                    attack: (indicator == 2).then_some(strength),
                };
                let draws = hud_layout::hotbar(wi, hi, &inputs);
                hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
                for d in &draws {
                    if let hud_layout::GuiDraw::Item { slot, x, y, .. } = *d {
                        let stack = if slot == 40 {
                            self.offhand.as_ref()
                        } else {
                            self.inventory[slot as usize].as_ref()
                        };
                        if let Some(stack) = stack {
                            self.draw_item(&mut batch, stack, x as f32, y as f32);
                        }
                    }
                }
            }
            self.draw_effect_icons(&mut batch);
            self.draw_boss_bars(&mut batch);
            self.draw_extras(&mut batch);
        }
        if show_hud {
            let has_experience = !self.creative && !spectator;
            let contextual = vise_client::locator_bar::ContextualBar::next(
                self.waypoints.has_waypoints(),
                !spectator && self.jumpable_mount(),
                self.mount_jumping(),
                has_experience,
                self.experience_recent(),
            );
            use vise_client::locator_bar::ContextualBar;
            let vehicle_hearts = if spectator {
                0
            } else {
                self.draw_mount_hud(
                    &mut batch,
                    wi,
                    hi,
                    contextual == ContextualBar::JumpableVehicle,
                )
            };
            bar_height = 39 + 10 * ((vehicle_hearts + 9) / 10);
            if contextual == ContextualBar::Locator {
                let (sprite, [x, y, w, h]) = vise_client::locator_bar::background(wi, hi);
                vise_client::ui::nine_slice(
                    &mut batch,
                    &self.gui.atlas,
                    sprite,
                    x as f32,
                    y as f32,
                    w as f32,
                    h as f32,
                    u32::MAX,
                );
            }
            if !self.creative
                && !spectator
                && let Some(p) = &mut self.player
            {
                let atlas = &self.gui.atlas;
                let width = |s: &str| HudBatch::text_width(atlas, s) as i32;
                let draws = hud_layout::experience(
                    wi,
                    hi,
                    self.experience.progress,
                    self.experience.level,
                    contextual == ContextualBar::Experience,
                    &width,
                );
                hud_layout::paint(&mut batch, atlas, &draws);
                let effects = &self.potions.effects;
                let kind = if effects.get("poison").is_some() {
                    hud_layout::HeartKind::Poisoned
                } else if effects.get("wither").is_some() {
                    hud_layout::HeartKind::Withered
                } else if p.is_fully_frozen() {
                    hud_layout::HeartKind::Frozen
                } else {
                    hud_layout::HeartKind::Normal
                };
                let under_water = p.is_eye_in_fluid(vise_client::motion::WATER);
                let inputs = hud_layout::HealthInputs {
                    tick: self.ticks as i32,
                    millis: self.started.elapsed().as_millis() as i64,
                    health: p.health.health(),
                    max_health: p.attribute("minecraft:max_health").unwrap_or(20.0) as f32,
                    absorption: p.health.absorption(),
                    armor: p
                        .attribute("minecraft:armor")
                        .or_else(|_| p.attribute("armor"))
                        .unwrap_or(0.0) as i32,
                    food: p.health.food.food,
                    saturation: p.health.food.saturation,
                    kind,
                    regeneration: effects.get("regeneration").is_some(),
                    hunger: effects.get("hunger").is_some(),
                    hardcore: self.level.hardcore,
                    damage_cooldown: p.health.hurt.cooldown > 0,
                    air: p.air,
                    max_air: 300,
                    under_water,
                    vehicle_hearts,
                };
                let draws = self.health_hud.layout(&inputs, wi, hi);
                hud_layout::paint(&mut batch, atlas, &draws);
                bar_height = self
                    .health_hud
                    .left_height
                    .max(self.health_hud.right_height);
                if let Some((volume, pitch)) = self.health_hud.bubble_pop_sound.take() {
                    let at = [p.position.x, p.position.y, p.position.z];
                    let players = vise_client::settings::SOUND_SOURCES
                        .iter()
                        .position(|s| *s == "player")
                        .unwrap_or(0);
                    let capture = self.audio.capture.take();
                    self.audio
                        .play_on(players, "ui.hud.bubble_pop", Some(at), volume, pitch);
                    self.audio.capture = capture;
                }
            }
            if contextual == ContextualBar::Locator {
                let draws = self.locator_bar_draws(wi, hi);
                hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
            }
            if !spectator {
                self.draw_selected_name(&mut batch, bar_height, !self.creative);
            }
        }
        let draws = hud_overlays::sleep_overlay(wi, hi, self.sleep_timer as i32);
        hud_layout::paint(&mut batch, &self.gui.atlas, &draws);
        if show_hud {
            self.draw_titles(&mut batch, bar_height);
            self.draw_scoreboard(&mut batch);
            self.draw_subtitles(&mut batch);
        }
        let atlas = &self.gui.atlas;
        if show_hud
            && let Some((text, at)) = &self.message
            && at.elapsed() < Duration::from_secs(3)
        {
            let tw = HudBatch::text_width(atlas, text);
            batch.text(atlas, text, (w - tw) / 2.0, h - 59.0, u32::MAX, true);
        }
        self.draw_chat(&mut batch);
        self.draw_tab_list(&mut batch);
        batch.blur_at = batch.vertices.len() as u32;
        let screen = if self.server_packs.prompt.is_some() {
            Screen::Playing
        } else {
            self.screen
        };
        match screen {
            Screen::Paused if self.pause_menu => self.draw_pause_menu(&mut batch),
            Screen::Paused => {
                let title = self.translate("menu.paused", "Game Paused");
                let tw = HudBatch::text_width(&self.gui.atlas, &title);
                batch.text(
                    &self.gui.atlas,
                    &title,
                    ((batch.width / 2.0).floor() - (tw / 2.0).floor()).floor(),
                    10.0,
                    u32::MAX,
                    true,
                );
            }
            Screen::GameModeSwitcher => self.draw_gamemode_switcher(&mut batch),
            Screen::Dead => self.draw_death_screen(&mut batch),
            Screen::Stats => self.draw_stats(&mut batch),
            Screen::Advancements => self.draw_advancements(&mut batch),
            Screen::WorldOptions => self.draw_world_options(&mut batch),
            Screen::Friends => self.draw_friends_screen(&mut batch),
            Screen::ServerLinks => self.draw_server_links(&mut batch),
            Screen::ConfirmLink => self.draw_confirm_link(&mut batch),
            Screen::DialogWarning => self.draw_dialog_warning(&mut batch),
            Screen::CommandBlock => self.draw_command_block(&mut batch),
            Screen::Inventory => {
                self.draw_inventory(&mut batch);
                self.draw_mod_browser(&mut batch);
            }
            Screen::Creative => {
                self.draw_creative(&mut batch);
            }
            Screen::Chat => self.draw_chat_screen(&mut batch),
            Screen::Playing | Screen::Options | Screen::Win => {}
        }
        self.draw_saving_indicator(&mut batch);
        self.draw_toasts(&mut batch, !show_hud);
        self.draw_tutorial_toasts(&mut batch, !show_hud);
        self.draw_pack_toasts(&mut batch, !show_hud);
        if self.server_packs.prompt.is_none() {
            self.remote_hud(&mut batch);
            self.draw_sign_editor(&mut batch);
            self.draw_book(&mut batch);
        }
        batch
    }
    fn refresh_creative_tabs(&mut self) {
        let permissions = self.can_use_game_master_blocks() && self.settings.operator_items_tab;
        self.creative_screen
            .set_permissions(permissions, &self.pack.names);
    }
    fn creative_inventory_open(&self) -> bool {
        self.screen == Screen::Creative && self.creative_screen.tab().inventory
    }
    fn creative_tab_open(&self) -> bool {
        self.screen == Screen::Creative && !self.creative_screen.tab().inventory
    }
    fn inventory_origin(&self, batch: &HudBatch) -> (f32, f32) {
        if self.screen == Screen::Creative {
            return Self::creative_origin(batch);
        }
        let (x, y) = batch.centered_origin(176.0, self.screen_height());
        (x + self.recipe_book_shift(batch.width), y)
    }
    fn container_kind(&self) -> Option<&BlockEntity> {
        self.container
            .and_then(|k| self.world.block_entities.get(&k))
    }
    fn slots(&self) -> Vec<usize> {
        if self.creative_inventory_open() {
            return Creative::inventory_slots().collect();
        }
        if self.creative_tab_open() {
            return Creative::tab_slots().collect();
        }
        if let Some(v) = self.station_slots().or_else(|| self.mount_slots()) {
            return v;
        }
        let mut v: Vec<usize> = (0..36).collect();
        match self.container_kind() {
            Some(BlockEntity::Chest(_)) => v.extend((0..self.container_size()).map(|i| 300 + i)),
            Some(BlockEntity::Furnace(_)) => v.extend(300..303),
            Some(BlockEntity::Brewing(_)) => v.extend(300..305),
            Some(BlockEntity::Crafter(_)) => v.extend(300..310),
            Some(
                BlockEntity::Sign(_)
                | BlockEntity::Jukebox(_)
                | BlockEntity::Beacon(_)
                | BlockEntity::Lectern(_)
                | BlockEntity::Sculk(_)
                | BlockEntity::Banner(_)
                | BlockEntity::CreakingHeart(_)
                | BlockEntity::Spawner(_)
                | BlockEntity::TrialSpawner(_)
                | BlockEntity::Vault(_)
                | BlockEntity::Beehive(_)
                | BlockEntity::CommandBlock(_)
                | BlockEntity::EndGateway(_)
                | BlockEntity::Skull(_)
                | BlockEntity::DaylightDetector
                | BlockEntity::Campfire(_)
                | BlockEntity::ChiseledBookshelf(_)
                | BlockEntity::Brushable(_)
                | BlockEntity::Shelf(_)
                | BlockEntity::Mod(_)
                | BlockEntity::PotentSulfur(_),
            ) => {}
            None => {
                v.extend((0..self.craft_size * self.craft_size).map(|i| 100 + i));
                v.push(200);
                if self.craft_size == 2 {
                    v.extend(500..505);
                }
            }
        }
        v
    }
    fn slot_position(&self, i: usize) -> (f32, f32) {
        if self.creative_inventory_open()
            && let Some(position) = Creative::inventory_slot_position(i)
        {
            return position;
        }
        if self.creative_tab_open()
            && let Some(position) = Creative::tab_slot_position(i)
        {
            return position;
        }
        if let Some(p) = self
            .station_slot_position(i)
            .or_else(|| self.machine_slot_position(i))
            .or_else(|| self.crafter_slot_position(i))
            .or_else(|| self.mount_slot_position(i))
        {
            return p;
        }
        let table = self.craft_size == 3;
        let shift = match self.chest_rows() {
            Some(rows) => 1.0 + (rows as f32 - 3.0) * 18.0,
            None => 0.0,
        };
        match i {
            0..9 => (8.0 + i as f32 * 18.0, 142.0 + shift),
            9..36 => (
                8.0 + ((i - 9) % 9) as f32 * 18.0,
                84.0 + shift + ((i - 9) / 9) as f32 * 18.0,
            ),
            500..504 => (8.0, 8.0 + (i - 500) as f32 * 18.0),
            504 => (77.0, 62.0),
            300..500 => match self.container_kind() {
                Some(BlockEntity::Furnace(_)) => {
                    [(56.0, 17.0), (56.0, 53.0), (116.0, 35.0)][(i - 300).min(2)]
                }
                _ => (
                    8.0 + ((i - 300) % 9) as f32 * 18.0,
                    18.0 + ((i - 300) / 9) as f32 * 18.0,
                ),
            },
            200 => {
                if table {
                    (124.0, 35.0)
                } else {
                    (154.0, 28.0)
                }
            }
            _ => {
                let g = i - 100;
                let (r, c) = (g / self.craft_size, g % self.craft_size);
                if table {
                    (30.0 + c as f32 * 18.0, 17.0 + r as f32 * 18.0)
                } else {
                    (98.0 + c as f32 * 18.0, 18.0 + r as f32 * 18.0)
                }
            }
        }
    }
    fn slot_at(&self, batch: &HudBatch, x: f32, y: f32) -> Option<usize> {
        let (ox, oy) = self.inventory_origin(batch);
        self.slots().into_iter().find(|&i| {
            let (sx, sy) = self.slot_position(i);
            x >= ox + sx && x < ox + sx + 16.0 && y >= oy + sy && y < oy + sy + 16.0
        })
    }
    fn slot_mut(&mut self, i: usize) -> &mut Option<ItemStack> {
        if i >= mount_menu::SADDLE {
            return self.mount_slot_mut(i);
        }
        if i >= stations::INPUT {
            return self.station_slot_mut(i);
        }
        if (500..504).contains(&i) {
            return &mut self.armor[i - 500];
        }
        if i == 504 {
            return &mut self.offhand;
        }
        if i >= 300 {
            return self.container_slot_mut(i - 300);
        }
        if (100..109).contains(&i) {
            &mut self.craft_grid[i - 100]
        } else {
            &mut self.inventory[i]
        }
    }
    fn slot_view(&mut self, i: usize) -> Option<ItemStack> {
        match i {
            200 => self.craft_result(),
            309 if matches!(self.container_kind(), Some(BlockEntity::Crafter(_))) => {
                self.crafter_result()
            }
            _ => self.slot_mut(i).clone(),
        }
    }
    fn craft_result(&self) -> Option<ItemStack> {
        self.craft_recipe().map(|(_, result)| result)
    }
    fn award_crafted(&mut self, crafts: &[(String, Vec<Option<ItemStack>>)]) {
        for (recipe, ingredients) in crafts {
            self.trigger_recipe_crafted(recipe, ingredients);
        }
        let recipes: Vec<String> = crafts.iter().map(|(r, _)| r.clone()).collect();
        let index = &world_stations!(self).recipe_index;
        let (_, unlocked) = self.level.recipe_book.add_recipes(index, &recipes);
        self.recipe_unlocked(&unlocked);
    }
    fn craft_recipe(&self) -> Option<(String, ItemStack)> {
        let n = self.craft_size;
        let grid: Vec<Option<ItemStack>> = (0..n * n).map(|i| self.craft_grid[i].clone()).collect();
        let scale = |id: i32| self.maps.data.get(&id).map(|m| m.scale);
        self.res
            .recipes()
            .find_with(&grid, n, &scale)
            .map(|(name, result)| (name.to_owned(), result))
    }
    fn draw_inventory_slots(&mut self, batch: &mut HudBatch, ox: f32, oy: f32) {
        let drag = self.drag_view();
        for i in self.slots() {
            let (sx, sy) = self.slot_position(i);
            if i == 504 && self.offhand.is_none() {
                batch.sprite(
                    &self.gui.atlas,
                    "gui/sprites/container/slot/shield",
                    ox + sx,
                    oy + sy,
                );
            }
            if (500..504).contains(&i) && self.armor[i - 500].is_none() {
                let icon = ["helmet", "chestplate", "leggings", "boots"][i - 500];
                batch.sprite(
                    &self.gui.atlas,
                    &format!("gui/sprites/container/slot/{icon}"),
                    ox + sx,
                    oy + sy,
                );
            }
            let stack = self.slot_view(i);
            self.draw_slot_item(batch, drag.as_ref(), i, &stack, ox + sx, oy + sy);
        }
    }
    fn draw_inventory(&mut self, batch: &mut HudBatch) {
        if self.draw_station(batch) {
            return;
        }
        let (ox, oy) = self.inventory_origin(batch);
        {
            let atlas = &self.gui.atlas;
            batch.fill(
                atlas,
                0.0,
                0.0,
                batch.width,
                batch.height,
                rgba(16, 16, 16, 160),
            );
            let part = |batch: &mut HudBatch,
                        name: &str,
                        sx: f32,
                        sy: f32,
                        w: f32,
                        h: f32,
                        dx: f32,
                        dy: f32| {
                if let Some(mut r) = atlas.sprites.get(name).copied() {
                    let (du, dv) = ((r.u1 - r.u0) / r.w as f32, (r.v1 - r.v0) / r.h as f32);
                    r.u0 += sx * du;
                    r.v0 += sy * dv;
                    r.u1 = r.u0 + w * du;
                    r.v1 = r.v0 + h * dv;
                    batch.rect_uv(dx, dy, w, h, r, u32::MAX);
                }
            };
            let grey = rgba(64, 64, 64, 255);
            match self
                .container
                .and_then(|k| self.world.block_entities.get(&k))
            {
                Some(BlockEntity::Chest(_)) if self.draw_machine_screen(batch, ox, oy) => {}
                Some(BlockEntity::Chest(_)) => self.draw_chest_screen(batch, ox, oy),
                Some(BlockEntity::Furnace(f)) => {
                    let (tex, key, fallback) = match f.kind {
                        vise_client::containers::FurnaceKind::Furnace => {
                            ("gui/container/furnace", "container.furnace", "Furnace")
                        }
                        vise_client::containers::FurnaceKind::BlastFurnace => (
                            "gui/container/blast_furnace",
                            "container.blast_furnace",
                            "Blast Furnace",
                        ),
                        vise_client::containers::FurnaceKind::Smoker => {
                            ("gui/container/smoker", "container.smoker", "Smoker")
                        }
                    };
                    part(batch, tex, 0.0, 0.0, 176.0, 166.0, ox, oy);
                    let title = self.remote_menu_title().unwrap_or_else(|| {
                        vise_client::text::Text::literal(
                            self.container_title(self.translate(key, fallback)),
                        )
                        .resolve_legacy()
                    });
                    let tw = atlas.font.width_text(&title);
                    batch.text_styled(
                        atlas,
                        &title,
                        ox + (176.0 - tw) / 2.0,
                        oy + 6.0,
                        grey,
                        false,
                    );
                    let inventory = self.translate("container.inventory", "Inventory");
                    batch.text(atlas, &inventory, ox + 8.0, oy + 72.0, grey, false);
                    if f.lit_remaining > 0 {
                        let total = if f.lit_total == 0 { 200 } else { f.lit_total };
                        let h = ((f.lit_remaining as f32 / total as f32) * 13.0).ceil() + 1.0;
                        part(
                            batch,
                            "gui/sprites/container/furnace/lit_progress",
                            0.0,
                            14.0 - h,
                            14.0,
                            h,
                            ox + 56.0,
                            oy + 36.0 + 14.0 - h,
                        );
                    }
                    if f.cooking_timer > 0 && f.cooking_total > 0 {
                        let w = ((f.cooking_timer as f32 / f.cooking_total as f32) * 24.0).ceil();
                        part(
                            batch,
                            "gui/sprites/container/furnace/burn_progress",
                            0.0,
                            0.0,
                            w,
                            16.0,
                            ox + 79.0,
                            oy + 34.0,
                        );
                    }
                }
                Some(BlockEntity::Brewing(_)) => {}
                Some(BlockEntity::Crafter(_)) => self.draw_crafter_screen(batch, ox, oy),
                Some(
                    BlockEntity::Sign(_)
                    | BlockEntity::Jukebox(_)
                    | BlockEntity::Beacon(_)
                    | BlockEntity::Lectern(_)
                    | BlockEntity::Sculk(_)
                    | BlockEntity::Banner(_)
                    | BlockEntity::CreakingHeart(_)
                    | BlockEntity::Spawner(_)
                    | BlockEntity::TrialSpawner(_)
                    | BlockEntity::Vault(_)
                    | BlockEntity::Beehive(_)
                    | BlockEntity::CommandBlock(_)
                    | BlockEntity::EndGateway(_)
                    | BlockEntity::Skull(_)
                    | BlockEntity::DaylightDetector
                    | BlockEntity::Campfire(_)
                    | BlockEntity::ChiseledBookshelf(_)
                    | BlockEntity::Brushable(_)
                    | BlockEntity::Shelf(_)
                    | BlockEntity::Mod(_)
                    | BlockEntity::PotentSulfur(_),
                ) => {}
                None if self.mount_active() => self.draw_mount_background(batch, ox, oy),
                None => {
                    let texture = if self.craft_size == 3 {
                        "gui/container/crafting_table"
                    } else {
                        "gui/container/inventory"
                    };
                    part(batch, texture, 0.0, 0.0, 176.0, 166.0, ox, oy);
                    let title = self.translate("container.crafting", "Crafting");
                    let tx = if self.craft_size == 3 { 29.0 } else { 97.0 };
                    batch.text(atlas, &title, ox + tx, oy + 6.0, grey, false);
                    if self.craft_size == 3 {
                        let inventory = self.translate("container.inventory", "Inventory");
                        batch.text(atlas, &inventory, ox + 8.0, oy + 72.0, grey, false);
                    }
                }
            }
        }
        if self.container.is_none() && self.craft_size != 3 && !self.mount_active() {
            self.draw_inventory_player(batch, ox, oy);
        }
        self.draw_inventory_slots(batch, ox, oy);
        self.draw_sort_buttons(batch, ox, oy);
        self.draw_recipe_book(batch);
        if let Some(slot) = self.slot_at(batch, self.cursor.0, self.cursor.1) {
            let (sx, sy) = self.slot_position(slot);
            batch.fill(
                &self.gui.atlas,
                ox + sx,
                oy + sy,
                16.0,
                16.0,
                rgba(255, 255, 255, 128),
            );
        }
        self.draw_carried(batch);
        self.draw_slot_tooltip(batch);
        if self.container.is_none() && self.craft_size == 2 && !self.mount_active() {
            self.draw_inventory_effects(batch, (ox, oy), 176);
        }
    }
    pub(super) fn reset_inventory_clicks(&mut self) {
        self.inv_clicks.cancel_drag();
        self.inv_drag = None;
        self.last_inv_click = None;
    }
    pub(super) fn close_container(&mut self) {
        self.mod_browser_closed();
        self.reset_inventory_clicks();
        self.close_station();
        self.mount_closed();
        self.recipe_ui.reset_screen();
        if self.remote.is_some() {
            return self.remote_close_container();
        }
        let mut returned: Vec<ItemStack> = self.carried.take().into_iter().collect();
        returned.extend(self.craft_grid.iter_mut().filter_map(Option::take));
        for stack in returned {
            if let Some(rest) = self.give(stack)
                && let Some(eye) = self.eye_position()
            {
                self.throw_from_eyes(rest, eye, false);
            }
        }
        self.craft_size = 2;
        self.craft_table = None;
        if let Some(key) = self.container.take() {
            self.close_cart_container(key);
            let last = self.container_opener_event(key, false);
            self.post_block_game_event(
                "minecraft:container_close",
                [key.0, key.1, key.2],
                Some(vise_client::mobs::Who::Player),
                None,
            );
            if last {
                self.container_edge_sound(key, false);
            }
            self.world.modified.insert((key.0 >> 4, key.2 >> 4));
        }
        self.close_ender_chest();
    }
    fn inventory_quick_move(&mut self, slot: usize) -> bool {
        self.menu_click(Some(slot), 0, vise_client::menu::Input::QuickMove)
    }
    fn menu_click(
        &mut self,
        slot: Option<usize>,
        button: i32,
        input: vise_client::menu::Input,
    ) -> bool {
        if let Some(slot) = slot
            && slot >= Creative::DISPLAY
            && self.creative_tab_open()
        {
            self.creative_display_click(slot, button, input);
            return true;
        }
        self.menu_ops(&[(slot, button, input)])
    }
    fn creative_display_click(
        &mut self,
        slot: usize,
        button: i32,
        input: vise_client::menu::Input,
    ) {
        use vise_client::creative::{DisplayClick, click_display};
        if self.creative_screen.tab().search {
            let search = &mut self.creative_screen.search;
            search.move_cursor_to_end(false);
            search.highlight = 0;
        }
        let first = self.creative_screen.first_row(&self.pack.names);
        let shown = self
            .creative_screen
            .shown(&self.pack.names, first * 9 + slot - Creative::DISPLAY);
        let catalog = world_items!(self);
        let max = |s: &ItemStack| catalog.get(&s.item).map_or(64, |i| i.max_stack);
        match click_display(&mut self.carried, shown.as_ref(), max, button, input) {
            DisplayClick::Hotbar(i, stack) if i < self.inventory.len() => {
                self.inventory[i] = Some(stack);
            }
            DisplayClick::Offhand(stack) => self.offhand = Some(stack),
            DisplayClick::Drop(stack) if self.remote.is_some() => self.remote_creative_throw(stack),
            DisplayClick::Drop(stack) => self.player_drop(stack),
            _ => {}
        }
    }
    fn menu_ops(&mut self, ops: &[(Option<usize>, i32, vise_client::menu::Input)]) -> bool {
        use vise_client::menu::Input;
        for &(slot, _, input) in ops {
            if let (Some(slot), Input::QuickMove | Input::Swap) = (slot, input) {
                self.unselect_bundle(slot);
            }
        }
        if self.remote.is_some() && self.screen == Screen::Inventory {
            return self.remote_menu_ops(ops);
        }
        if let [(Some(slot), button, input)] = ops {
            self.crafter_slot_clicked(*slot, *button, *input);
        }
        let ops = ops.to_vec();
        self.menu_session(move |menu, env, to_menu| {
            let mut clicks = Vec::new();
            for &(slot, button, input) in &ops {
                let menu_slot = match slot {
                    Some(g) => {
                        if g == 200 && input != Input::QuickMove {
                            return None;
                        }
                        match to_menu(g) {
                            Some(i) => i as i32,
                            None => return None,
                        }
                    }
                    None => -999,
                };
                clicks.push((menu_slot, button, input));
            }
            for (menu_slot, button, input) in clicks {
                if menu.click(env, menu_slot, button, input).is_err() {
                    return Some(false);
                }
            }
            Some(true)
        })
    }
    pub(super) fn player_menu_sound(&mut self, sound: &str) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let at = [p.position.x, p.position.y, p.position.z];
        let (volume, pitch) = if sound == vise_client::bundle::INSERT_FAIL {
            (1.0, 1.0)
        } else {
            (0.8, 0.8 + self.world.random.next_float() * 0.4)
        };
        self.audio.play(sound, Some(at), volume, pitch);
    }
    pub(super) fn menu_session(
        &mut self,
        run: impl FnOnce(
            &mut vise_client::menu::Menu,
            &vise_client::menu::Env,
            &dyn Fn(usize) -> Option<usize>,
        ) -> Option<bool>,
    ) -> bool {
        use vise_client::menu::{Env, Kind, Menu};
        #[derive(Clone, Copy)]
        struct Map {
            container: usize,
            table: bool,
            cells: usize,
            plain: bool,
        }
        impl Map {
            fn main(&self) -> usize {
                match (self.plain, self.table) {
                    (false, _) => self.container,
                    (true, true) => 10,
                    (true, false) => 9,
                }
            }
            fn to_menu(self, g: usize) -> Option<usize> {
                let main = self.main();
                if self.plain {
                    match g {
                        200 => Some(0),
                        100..109 if g - 100 < self.cells => Some(1 + g - 100),
                        500..504 if !self.table => Some(5 + g - 500),
                        504 if !self.table => Some(45),
                        9..36 => Some(main + g - 9),
                        0..9 => Some(main + 27 + g),
                        _ => None,
                    }
                } else {
                    match g {
                        300.. if g < 300 + self.container => Some(g - 300),
                        9..36 => Some(main + g - 9),
                        0..9 => Some(main + 27 + g),
                        _ => None,
                    }
                }
            }
            fn to_game(self, i: usize) -> Option<usize> {
                let main = self.main();
                if self.plain {
                    match i {
                        0 => None,
                        1..=9 if i - 1 < self.cells => Some(100 + i - 1),
                        5..9 if !self.table => Some(500 + i - 5),
                        45 if !self.table => Some(504),
                        _ if i >= main && i < main + 27 => Some(9 + i - main),
                        _ if i >= main + 27 && i < main + 36 => Some(i - main - 27),
                        _ => None,
                    }
                } else if i < self.container {
                    Some(300 + i)
                } else if i < self.container + 27 {
                    Some(9 + i - self.container)
                } else {
                    Some(i - self.container - 27)
                }
            }
        }
        let mut plain_map = None;
        let crafter_open = matches!(self.container_kind(), Some(BlockEntity::Crafter(_)));
        let loom_open = self
            .station
            .as_ref()
            .is_some_and(|s| s.kind == stations::Kind::Loom);
        let merchant_open = self
            .station
            .as_ref()
            .is_some_and(|s| s.kind == stations::Kind::Merchant);
        let stonecutter_open = self
            .station
            .as_ref()
            .is_some_and(|s| s.kind == stations::Kind::Stonecutter);
        let station_kind = self.station.as_ref().map(|s| s.kind);
        let grindstone_open = station_kind == Some(stations::Kind::Grindstone);
        let cartography_open = station_kind == Some(stations::Kind::Cartography);
        let anvil_open = station_kind == Some(stations::Kind::Anvil);
        let smithing_open = station_kind == Some(stations::Kind::Smithing);
        let enchanting_open = station_kind == Some(stations::Kind::Enchanting);
        let beacon_open = station_kind == Some(stations::Kind::Beacon);
        let (kind, game_of): (Kind, Vec<Option<usize>>) = if self.creative_tab_open() {
            let mut v = vec![None; 45];
            v.extend((0..9).map(Some));
            (Kind::ItemPicker, v)
        } else if let Some(columns) = self
            .mount_menu_columns()
            .filter(|_| self.remote.is_some() && self.mount_active())
        {
            let mut v = vec![Some(mount_menu::SADDLE), Some(mount_menu::ARMOR)];
            v.extend((0..columns * 3).map(|i| Some(mount_menu::CHEST + i)));
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Mount(columns), v)
        } else if merchant_open {
            let mut v: Vec<Option<usize>> = vec![
                Some(stations::INPUT),
                Some(stations::INPUT + 1),
                Some(stations::RESULT),
            ];
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Merchant, v)
        } else if crafter_open {
            let mut v: Vec<Option<usize>> = (300..309).map(Some).collect();
            v.extend((9..36).chain(0..9).map(Some));
            v.push(None);
            (Kind::CrafterBlock, v)
        } else if loom_open {
            let mut v: Vec<Option<usize>> = (0..3).map(|i| Some(stations::INPUT + i)).collect();
            v.push(Some(stations::RESULT));
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Loom, v)
        } else if stonecutter_open {
            let mut v = vec![Some(stations::INPUT), Some(stations::RESULT)];
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Stonecutter, v)
        } else if grindstone_open || cartography_open {
            let mut v = vec![
                Some(stations::INPUT),
                Some(stations::INPUT + 1),
                Some(stations::RESULT),
            ];
            v.extend((9..36).chain(0..9).map(Some));
            let kind = if grindstone_open {
                Kind::Grindstone
            } else {
                Kind::Cartography
            };
            (kind, v)
        } else if enchanting_open {
            let mut v = vec![Some(stations::INPUT), Some(stations::INPUT + 1)];
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Enchanting, v)
        } else if beacon_open {
            let mut v = vec![Some(stations::INPUT)];
            v.extend((9..36).chain(0..9).map(Some));
            (Kind::Beacon, v)
        } else if anvil_open || smithing_open {
            let inputs = if anvil_open { 2 } else { 3 };
            let mut v: Vec<Option<usize>> =
                (0..inputs).map(|i| Some(stations::INPUT + i)).collect();
            v.push(Some(stations::RESULT));
            v.extend((9..36).chain(0..9).map(Some));
            (
                if anvil_open {
                    Kind::Anvil
                } else {
                    Kind::Smithing
                },
                v,
            )
        } else {
            let (kind, map) = if self.container.is_some() {
                let size = self.container_size();
                let (kind, count) = match self.container_kind() {
                    Some(BlockEntity::Chest(_)) => {
                        let shulker = self.container.is_some_and(|k| {
                            let id = self.world.block(k.0, k.1, k.2);
                            storage::is_shulker_box(&self.res.facts().get(id).name)
                        });
                        let kind = match size {
                            5 => Kind::Hopper,
                            9 => Kind::Dispenser,
                            27 if shulker => Kind::Shulker,
                            n => Kind::Chest((n / 9).clamp(1, 6)),
                        };
                        (kind, size)
                    }
                    Some(BlockEntity::Brewing(_)) => (Kind::Brewing, 5),
                    Some(BlockEntity::Furnace(f)) => (
                        Kind::Furnace(
                            match f.kind.recipe_type().trim_start_matches("minecraft:") {
                                "blasting" => "blasting",
                                "smoking" => "smoking",
                                _ => "smelting",
                            },
                        ),
                        3,
                    ),
                    _ => return false,
                };
                let map = Map {
                    container: count,
                    table: false,
                    cells: 0,
                    plain: false,
                };
                (kind, map)
            } else if self.station.is_none() && !self.mount_active() {
                let table = self.craft_size == 3;
                let map = Map {
                    container: 0,
                    table,
                    cells: self.craft_size * self.craft_size,
                    plain: true,
                };
                (
                    if table {
                        Kind::Crafting
                    } else {
                        Kind::Inventory
                    },
                    map,
                )
            } else {
                return false;
            };
            let total = Menu::new(kind).slot_count();
            plain_map = Some(map);
            (kind, (0..total).map(|i| map.to_game(i)).collect())
        };
        let to_menu = |g: usize| match plain_map {
            Some(map) => map.to_menu(g),
            None => game_of.iter().position(|x| *x == Some(g)),
        };
        let mut menu = Menu::new(kind);
        if matches!(kind, Kind::Mount(_)) {
            menu.mount = self.mount_menu_entity();
        }
        let total = menu.slot_count();
        let before: Vec<Option<ItemStack>> = game_of
            .iter()
            .map(|g| g.and_then(|g| self.slot_mut(g).clone()))
            .collect();
        let carried = self.carried.clone();
        let creative = self.creative;
        let equipment_before = (self.screen == Screen::Creative).then(|| self.player_equipment());
        let station_menu = self.station.as_ref().map(|s| {
            (
                s.loom.selected.map_or(-1, |i| i as i32),
                s.loom.patterns.clone(),
            )
        });
        let stone = self
            .station
            .as_ref()
            .map(|s| (s.selected.map_or(-1, |i| i as i32), s.stone.clone()));
        let crafter_data = match self.container_kind() {
            Some(BlockEntity::Crafter(c)) => Some((c.disabled, c.triggered)),
            _ => None,
        };
        let mut session = self
            .station
            .as_ref()
            .and_then(|s| s.merchant.as_ref())
            .map(|m| (m.session.clone(), m.mob));
        if let Some((s, mob)) = session.as_mut() {
            self.merchant_sync_in(s, *mob);
        }
        let level_maps = (kind == Kind::Cartography && self.remote.is_none()).then(|| {
            let held: Vec<ItemStack> = before
                .iter()
                .flatten()
                .chain(self.inventory.iter().flatten())
                .chain(self.carried.iter())
                .cloned()
                .collect();
            self.level_maps(&held)
        });
        let next_map = if merchant_open { self.peek_map_id() } else { 0 };
        let tag = |t: &str| self.structure_tag(t);
        let maps = trading::GameMaps {
            terrain: self.end.locator(),
            structure_tag: &tag,
            made: Default::default(),
            next_id: std::cell::Cell::new(next_map),
        };
        let trade_env = vise_client::merchant::TradeEnv {
            trades: &self.stations().trades,
            enchantments: &self.stations().enchantments,
            maps: Some(&maps),
        };
        let env = Env {
            items: self.items(),
            recipes: self.recipes(),
            brewing: &self.stations().brewing,
            banners: &self.stations().banners,
            station_recipes: &self.stations().recipes,
            enchantments: &self.stations().enchantments,
            names: &self.pack.names,
            trade: Some(&trade_env),
            maps: level_maps
                .as_ref()
                .map(|m| m as &dyn vise_client::menu::Maps),
        };
        menu.creative = creative;
        menu.selected = self.selected;
        menu.levels = self.experience.level;
        if anvil_open {
            menu.anvil.name = self.station.as_ref().map(|s| self.anvil_item_name(s));
        }
        for (i, stack) in before.iter().enumerate() {
            menu.set_slot(&env, i, stack.clone());
        }
        let offhand_mapped = game_of.contains(&Some(504));
        if !offhand_mapped {
            menu.set_player_item(40, self.offhand.clone());
        }
        menu.settle(&env);
        if kind == Kind::Loom
            && let Some((selected, patterns)) = station_menu
        {
            menu.data[0] = selected;
            menu.loom_patterns = patterns;
            menu.set_slot(&env, 3, before[3].clone());
        }
        if kind == Kind::Stonecutter
            && let Some((selected, state)) = stone
        {
            menu.data[0] = selected;
            menu.stone = state;
            menu.set_slot(&env, 1, before[1].clone());
        }
        if kind == Kind::Enchanting
            && let Some(s) = self.station.as_ref()
        {
            menu.enchant_changed = false;
            for i in 0..3 {
                menu.data[i] = s.offers.costs[i];
                menu.data[7 + i] = s.offers.clues[i].as_ref().map_or(-1, |(_, l)| *l as i32);
            }
            menu.data[3] = self.level.xp_seed;
        }
        if let Some((disabled, triggered)) = crafter_data {
            for (i, d) in disabled.iter().enumerate() {
                menu.data[i] = i32::from(*d);
            }
            menu.data[9] = i32::from(triggered);
        }
        menu.carried = carried;
        menu.drops.clear();
        if kind == Kind::Merchant {
            menu.merchant = session.map(|(s, _)| s);
        }
        let furnace_before = if matches!(kind, Kind::Furnace(_)) {
            menu.slot(2).cloned().map_or(0, |s| s.count)
        } else {
            0
        };
        let outcome = run(&mut menu, &env, &to_menu);
        let made = maps.made.take();
        match outcome {
            None => return false,
            Some(false) => return true,
            Some(true) => {}
        }
        let predicting = self.remote.is_some();
        if !predicting {
            self.register_exploration_maps(made);
        }
        if let Some(s) = menu.merchant.take() {
            self.merchant_sync_out(s);
        }
        let after: Vec<Option<ItemStack>> = (0..total).map(|i| menu.slot(i).cloned()).collect();
        let furnace_after = if matches!(kind, Kind::Furnace(_)) {
            after[2].as_ref().map_or(0, |s| s.count)
        } else {
            0
        };
        let carried = menu.carried.take();
        let dropped = std::mem::take(&mut menu.dropped);
        let sounds = std::mem::take(&mut menu.sounds);
        for sound in std::mem::take(&mut menu.player_sounds) {
            self.player_menu_sound(sound);
        }
        let crafted = std::mem::take(&mut menu.crafted);
        let brewed = std::mem::take(&mut menu.brewed);
        let equipped = std::mem::take(&mut menu.equipped);
        for (i, stack) in after.into_iter().enumerate() {
            if let Some(g) = game_of[i] {
                *self.slot_mut(g) = stack;
            }
        }
        self.carried = carried;
        if !offhand_mapped {
            self.offhand = menu.player_item(40).cloned();
        }
        if !predicting {
            for stack in dropped {
                self.player_drop(stack);
            }
            self.award_crafted(&crafted);
            for stack in &brewed {
                self.trigger_brewed_potion(stack);
            }
            if furnace_before > furnace_after
                && self.remote.is_none()
                && let (Some(pos), Some(p)) = (self.container, self.player.as_ref())
            {
                let at = [p.position.x, p.position.y, p.position.z];
                self.award_furnace_recipes(pos, at);
            }
        } else if self.creative_inventory_open() {
            for stack in dropped {
                self.remote_creative_throw(stack);
            }
        }
        if kind == Kind::Loom {
            if let Some(s) = self.station.as_mut() {
                s.loom.selected = usize::try_from(menu.data[0]).ok();
                s.loom.patterns = std::mem::take(&mut menu.loom_patterns);
            }
            for sound in sounds.into_iter().filter(|_| !predicting) {
                self.station_sound(sound);
            }
            self.loom_screen_changed();
        } else if kind == Kind::Cartography {
            for sound in sounds.into_iter().filter(|_| !predicting) {
                self.station_sound(sound);
            }
        } else if kind == Kind::Stonecutter {
            if let Some(s) = self.station.as_mut() {
                s.selected = usize::try_from(menu.data[0]).ok();
                s.stone = std::mem::take(&mut menu.stone);
                s.stonecutter_changed();
            }
            for sound in sounds.into_iter().filter(|_| !predicting) {
                self.station_sound(sound);
            }
        }
        if let Some(amount) = menu.ground.take().filter(|_| !predicting)
            && let Some(pos) = self.station.as_ref().map(|s| s.pos)
        {
            self.grindstone_taken(pos, amount);
        }
        if kind == Kind::Anvil || kind == Kind::Smithing {
            self.combiner_session_end(&menu, predicting);
        }
        if kind == Kind::Enchanting && (menu.enchant_changed || before[0] != menu.slot(0).cloned())
        {
            self.station_changed();
        }
        if kind == Kind::CrafterBlock
            && let Some(BlockEntity::Crafter(c)) = self
                .container
                .and_then(|k| self.world.block_entities.get_mut(&k))
        {
            for (i, d) in c.disabled.iter_mut().enumerate() {
                *d = menu.data[i] == 1;
            }
        }
        if matches!(kind, Kind::Furnace(_))
            && let Some(BlockEntity::Furnace(f)) = self
                .container
                .and_then(|k| self.world.block_entities.get_mut(&k))
        {
            f.input_clicked(world_recipes!(self), before[0].clone());
        }
        match equipment_before {
            Some(before) => self.player_equipment_changed(&before),
            None => {
                for (slot, stack) in equipped {
                    self.player_equipped(slot, stack.as_ref());
                }
            }
        }
        self.apply_equipment();
        if let Some(maps) = level_maps {
            self.register_made_maps(maps);
        }
        true
    }
    fn search_tab_hotbar_key(&mut self, code: KeyCode) -> bool {
        use vise_client::creative::{Hover, search_key_swaps};
        let hotbar_key = HOTBAR_DIGITS.contains(&code);
        let numeric = hotbar_key || NUMERIC_KEYS.contains(&code);
        if !numeric {
            return false;
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return false;
        };
        let batch = HudBatch::new(size);
        let hover = match self.slot_at(&batch, self.cursor.0, self.cursor.1) {
            None => Hover::Nothing,
            Some(slot) if slot >= Creative::DISPLAY => {
                let first = self.creative_screen.first_row(&self.pack.names);
                Hover::Display {
                    filled: self
                        .creative_screen
                        .shown(&self.pack.names, first * 9 + slot - Creative::DISPLAY)
                        .is_some(),
                }
            }
            Some(_) => Hover::Slot,
        };
        search_key_swaps(numeric, hotbar_key, hover, self.carried.is_some())
            && self.inventory_hotkey(code)
    }
    fn inventory_hotkey(&mut self, key: KeyCode) -> bool {
        use vise_client::menu::Input;
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return false;
        };
        let batch = HudBatch::new(size);
        let Some(slot) = self.slot_at(&batch, self.cursor.0, self.cursor.1) else {
            return false;
        };
        let digit = HOTBAR_DIGITS.iter().position(|k| *k == key);
        let done = match (digit, key) {
            (Some(n), _) => self.menu_click(Some(slot), n as i32, Input::Swap),
            (None, KeyCode::KeyF) => self.menu_click(Some(slot), 40, Input::Swap),
            (None, KeyCode::KeyQ) => {
                let all = drop_whole_stack(&self.physical_keys);
                self.menu_click(Some(slot), i32::from(all), Input::Throw)
            }
            _ => false,
        };
        if done && let Some(key) = self.container {
            self.container_changed(key);
        }
        done
    }
    fn inventory_press(&mut self, right: bool) {
        use vise_client::container_screen::Target;
        use vise_client::menu::Input;
        if self.mod_browser_press(right) {
            return;
        }
        if !right && self.sort_press() {
            return;
        }
        let (slot, outside) = self.inventory_target();
        let shift = self.has_shift_down();
        let target = Target::of(slot, outside);
        let input = self.inv_clicks.press(
            &target,
            Self::screen_button(right),
            self.carried.is_none(),
            shift,
            false,
        );
        if outside {
            if self.recipe_book_click(right) {
                self.inv_clicks.skip_release();
                return;
            }
            match input {
                Some(input) => self.throw_click(right, input),
                None if self.inv_clicks.quick_crafting().is_some() => {
                    self.inv_drag = Some((i32::from(right), Vec::new()));
                }
                None => {}
            }
            return;
        }
        if self.tweak_press(slot, right, shift) {
            return;
        }
        if let Some(slot) = slot
            && shift
            && slot != 200
        {
            let now = std::time::Instant::now();
            let double = self.last_inv_click.is_some_and(|(at, last, was_right)| {
                last == slot && was_right == right && now.duration_since(at).as_millis() < 250
            });
            self.last_inv_click = Some((now, slot, right));
            if input == Some(Input::QuickMove) || (self.carried.is_some() && !double) {
                self.last_quick_moved = self.slot_view(slot);
            }
            if double && !right {
                if let Some(input) = input
                    && !self.menu_click(Some(slot), 0, input)
                {
                    self.click_inventory(right);
                    return;
                }
                if let Some(moved) = self.last_quick_moved.clone() {
                    if self.remote.is_some() && self.screen != Screen::Creative {
                        self.remote_quick_move_all(slot, &moved);
                    } else {
                        self.menu_session(|menu, env, to_menu| {
                            Some(menu.quick_move_all(env, to_menu(slot)?, &moved).is_ok())
                        });
                    }
                }
                self.last_inv_click = None;
                self.inv_clicks.skip_release();
                return;
            }
        }
        if let Some(slot) = slot
            && !shift
            && slot != 200
        {
            let display = slot >= Creative::DISPLAY;
            let now = std::time::Instant::now();
            let double = !right
                && !display
                && self.carried.is_some()
                && self.last_inv_click.is_some_and(|(at, last, was_right)| {
                    last == slot && !was_right && now.duration_since(at).as_millis() < 250
                });
            self.last_inv_click = Some((now, slot, right));
            if double && self.menu_click(Some(slot), 0, Input::PickupAll) {
                self.last_inv_click = None;
                self.inv_clicks.skip_release();
                return;
            }
            if self.carried.is_some() {
                let ty = i32::from(right);
                let first = if !display && self.quick_craft_accepts(slot, ty, 0) {
                    vec![slot]
                } else {
                    Vec::new()
                };
                self.inv_drag = Some((ty, first));
                return;
            }
        }
        if self.creative_tab_open() {
            match input {
                Some(input) => {
                    self.menu_click(slot, i32::from(right), input);
                }
                None if self.inv_clicks.quick_crafting().is_some() => {
                    self.inv_drag = Some((i32::from(right), Vec::new()));
                }
                None => {}
            }
            return;
        }
        if let (Some(input), Some(_)) = (input, slot)
            && self.remote.is_none()
            && self.menu_click(slot, i32::from(right), input)
        {
            if let Some(key) = self.container {
                self.container_changed(key);
            }
            return;
        }
        self.click_inventory(right);
    }
    fn inventory_drag_move(&mut self) {
        if self.tweaks_on() && self.tweak_drag_move(self.inventory_target().0) {
            return;
        }
        let Some((ty, slots)) = self.inv_drag.clone() else {
            return;
        };
        let Some(r) = self.renderer.as_ref() else {
            return;
        };
        let batch = HudBatch::new([r.config.width, r.config.height]);
        if let Some(slot) = self.slot_at(&batch, self.cursor.0, self.cursor.1)
            && slot != 200
            && slot < Creative::DISPLAY
            && !slots.contains(&slot)
            && self.quick_craft_accepts(slot, ty, slots.len())
            && let Some((_, slots)) = self.inv_drag.as_mut()
        {
            slots.push(slot);
        }
    }
    fn inventory_target(&self) -> (Option<usize>, bool) {
        let Some(r) = self.renderer.as_ref() else {
            return (None, false);
        };
        let batch = HudBatch::new([r.config.width, r.config.height]);
        (
            self.slot_at(&batch, self.cursor.0, self.cursor.1),
            self.clicked_outside(&batch),
        )
    }
    fn screen_button(right: bool) -> vise_client::container_screen::Button {
        use vise_client::container_screen::Button;
        if right { Button::Right } else { Button::Left }
    }
    pub(super) fn has_shift_down(&self) -> bool {
        self.physical_keys.contains(&KeyCode::ShiftLeft)
            || self.physical_keys.contains(&KeyCode::ShiftRight)
    }
    pub(super) fn has_control_down(&self) -> bool {
        self.physical_keys.contains(&KeyCode::ControlLeft)
            || self.physical_keys.contains(&KeyCode::ControlRight)
    }
    fn clicked_outside(&self, batch: &HudBatch) -> bool {
        use vise_client::container_screen::{Book, clicked_outside};
        let image = self.container_image(batch);
        let book = self
            .recipe_book_area(batch)
            .map(|(visible, tab_hovered)| Book {
                visible,
                selected_tab_hovered: tab_hovered,
            });
        clicked_outside(
            f64::from(self.cursor.0),
            f64::from(self.cursor.1),
            image,
            book,
        )
    }
    fn container_image(&self, batch: &HudBatch) -> vise_client::container_screen::Image {
        let (ox, oy) = self.inventory_origin(batch);
        let (shift, width, height) = if self.screen == Screen::Creative {
            (0.0, Creative::WIDTH as i32, Creative::HEIGHT as i32)
        } else {
            let (shift, width) = match self.station.as_ref().map(|s| s.kind) {
                Some(stations::Kind::Merchant) => (-50.0, 276),
                Some(stations::Kind::Beacon) => (beacon::SHIFT, 230),
                _ => (0.0, 176),
            };
            (shift, width, self.screen_height() as i32)
        };
        vise_client::container_screen::Image {
            left: (ox + shift) as i32,
            top: oy as i32,
            width,
            height,
        }
    }
    fn throw_click(&mut self, right: bool, input: vise_client::menu::Input) {
        use vise_client::menu::Input;
        let button = i32::from(right);
        if self.remote.is_some() {
            if self.screen == Screen::Creative {
                self.remote_creative_drop(right);
            } else {
                self.remote_throw(right, input);
            }
            return;
        }
        let before = self.carried.clone();
        if !self.menu_click(None, button, input) && input == Input::Pickup {
            if let Some(c) = self.carried.as_mut() {
                let thrown = if right {
                    let one = ItemStack {
                        count: 1,
                        ..c.clone()
                    };
                    c.count -= 1;
                    if c.count == 0 {
                        self.carried = None;
                    }
                    one
                } else {
                    self.carried.take().expect("some")
                };
                self.player_drop(thrown);
            }
        }
        if self.carried != before {
            self.swing_arm();
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
    }
    fn inventory_mouse_release(&mut self, right: bool) {
        use vise_client::container_screen::{Release, Target};
        self.tweak_release();
        let (slot, outside) = self.inventory_target();
        let dragged = self.inv_drag.as_ref().is_some_and(|(_, s)| !s.is_empty());
        let release = self.inv_clicks.release(
            &Target::of(slot, outside),
            Self::screen_button(right),
            self.carried.is_none(),
            dragged,
            self.has_shift_down(),
            false,
        );
        match (release, Target::of(slot, outside)) {
            (Release::QuickCraft, _) => self.inventory_release(),
            (Release::Click(input), Target::Outside) => {
                self.inv_drag = None;
                self.throw_click(right, input);
            }
            (Release::Click(input), Target::Slot(slot)) if self.inv_drag.is_some() => {
                self.inv_drag = None;
                if !self.menu_click(Some(slot), i32::from(right), input) {
                    self.click_inventory(right);
                }
                if let Some(key) = self.container {
                    self.container_changed(key);
                }
            }
            _ => self.inv_drag = None,
        }
    }
    fn inventory_pick_press(&mut self) {
        use vise_client::container_screen::{Button, Target};
        let (slot, outside) = self.inventory_target();
        let was_crafting = self.inv_clicks.quick_crafting().is_some();
        let input = self.inv_clicks.press(
            &Target::of(slot, outside),
            Button::Middle,
            self.carried.is_none(),
            self.has_shift_down(),
            self.creative,
        );
        match (input, slot) {
            (Some(input), Some(slot)) if !outside => {
                self.menu_click(Some(slot), 2, input);
            }
            (None, _) if !was_crafting && self.inv_clicks.quick_crafting().is_some() => {
                let first = slot.filter(|&s| {
                    s != 200 && s < Creative::DISPLAY && self.quick_craft_accepts(s, 2, 0)
                });
                self.inv_drag = Some((2, first.into_iter().collect()));
            }
            _ => {}
        }
    }
    fn inventory_pick_release(&mut self) {
        use vise_client::container_screen::{Button, Release, Target};
        let (slot, outside) = self.inventory_target();
        let dragged = self.inv_drag.as_ref().is_some_and(|(_, s)| !s.is_empty());
        let release = self.inv_clicks.release(
            &Target::of(slot, outside),
            Button::Middle,
            self.carried.is_none(),
            dragged,
            self.has_shift_down(),
            true,
        );
        match (release, slot) {
            (Release::QuickCraft, _) => self.inventory_release(),
            (Release::Click(input), Some(slot)) if !outside => {
                self.inv_drag = None;
                self.menu_click(Some(slot), 2, input);
            }
            _ => self.inv_drag = None,
        }
    }
    fn inventory_release(&mut self) {
        use vise_client::menu::Input;
        let Some((ty, slots)) = self.inv_drag.take() else {
            return;
        };
        if slots.len() == 1 {
            if !self.menu_click(Some(slots[0]), ty, Input::Pickup) {
                self.click_inventory(ty == 1);
            }
        } else {
            let mask = |header: i32| header | (ty << 2);
            let mut ops = vec![(None, mask(0), Input::QuickCraft)];
            ops.extend(
                slots
                    .into_iter()
                    .map(|slot| (Some(slot), mask(1), Input::QuickCraft)),
            );
            ops.push((None, mask(2), Input::QuickCraft));
            if !self.menu_ops(&ops) {
                self.click_inventory(ty == 1);
            }
        }
        if let Some(key) = self.container {
            self.container_changed(key);
        }
    }
    fn click_inventory(&mut self, right: bool) {
        if self.mod_browser_press(right) || self.mount_click(right) {
            return;
        }
        if self.recipe_book_click(right) {
            return;
        }
        if self.station_click(right) {
            return;
        }
        if self.remote.is_some() {
            return self.remote_click(right);
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let batch = HudBatch::new(size);
        let Some(slot) = self.slot_at(&batch, self.cursor.0, self.cursor.1) else {
            return;
        };
        let held_shift = self.has_shift_down();
        if self.crafter_click(slot, held_shift, right) {
            return;
        }
        if held_shift && self.container.is_some() && self.container_quick_move(slot) {
            if let Some(key) = self.container {
                self.world.modified.insert((key.0 >> 4, key.2 >> 4));
            }
            return;
        }
        if let Some(c) = &self.carried
            && self.refuses(slot, c)
        {
            return;
        }
        let max = |items: &Items, s: &ItemStack| items.get(&s.item).map_or(64, |i| i.max_stack);
        let shift = held_shift;
        if slot >= 300
            && let Some(key) = self.container
        {
            self.world.modified.insert((key.0 >> 4, key.2 >> 4));
        }
        let is_furnace = matches!(self.container_kind(), Some(BlockEntity::Furnace(_)));
        if is_furnace && slot == 302 {
            let Some(result) = self.slot_mut(302).take() else {
                return;
            };
            match &mut self.carried {
                None => self.carried = Some(result),
                Some(c)
                    if c.item == result.item
                        && c.count + result.count <= max(world_items!(self), c) =>
                {
                    c.count += result.count
                }
                _ => *self.slot_mut(302) = Some(result),
            }
            return;
        }
        let furnace_input = (is_furnace && slot == 300).then(|| self.slot_mut(300).clone());
        if slot == 200 {
            let mut crafted = 0;
            while let Some((recipe, result)) = self.craft_recipe() {
                let made = (result.item.clone(), result.count);
                let cells = self.craft_size * self.craft_size;
                let ingredients = self.craft_grid[..cells].to_vec();
                let remainders = world_recipes!(self).remainders(
                    &recipe,
                    &self.craft_grid[..cells],
                    self.craft_size,
                );
                if shift {
                    if self.give(result).is_some() {
                        break;
                    }
                } else {
                    match &mut self.carried {
                        None => self.carried = Some(result),
                        Some(c)
                            if c.item == result.item
                                && c.count + result.count <= max(world_items!(self), c) =>
                        {
                            c.count += result.count
                        }
                        _ => return,
                    }
                }
                for cell in self.craft_grid.iter_mut() {
                    if let Some(s) = cell {
                        s.count -= 1;
                        if s.count == 0 {
                            *cell = None;
                        }
                    }
                }
                for (i, rest) in remainders.into_iter().enumerate() {
                    let Some(rest) = rest else { continue };
                    match &mut self.craft_grid[i] {
                        cell @ None => *cell = Some(rest),
                        Some(c) if c.same_item_same_components(&rest) => c.count += rest.count,
                        _ => self.give_or_drop(rest),
                    }
                }
                crafted += 1;
                self.stats.add("crafted", &made.0, made.1 as i32);
                self.award_crafted(&[(format!("minecraft:{recipe}"), ingredients)]);
                if !shift || crafted >= 64 {
                    break;
                }
            }
            return;
        }
        if shift
            && (slot < 36 || (500..504).contains(&slot) || (100..109).contains(&slot))
            && self.inventory_quick_move(slot)
        {
            return;
        }
        if (500..504).contains(&slot) {
            let wanted = ["head", "chest", "legs", "feet"][slot - 500];
            let fits = self.carried.as_ref().is_none_or(|c| {
                self.res
                    .items()
                    .get(&c.item)
                    .and_then(|i| i.equip_slot.as_deref())
                    == Some(wanted)
            });
            if !fits {
                return;
            }
            std::mem::swap(&mut self.carried, &mut self.armor[slot - 500]);
            self.apply_equipment();
            return;
        }
        let carried = self.carried.take();
        let current = self.slot_mut(slot).take();
        let items = self.items();
        let (new_slot, new_carried) = match (carried, current) {
            (None, Some(mut s)) if right => {
                let half = s.count.div_ceil(2);
                s.count -= half;
                let taken = ItemStack {
                    count: half,
                    ..s.clone()
                };
                ((s.count > 0).then_some(s), Some(taken))
            }
            (None, s) => (None, s),
            (Some(mut c), None) if right => {
                let one = ItemStack {
                    count: 1,
                    ..c.clone()
                };
                c.count -= 1;
                (Some(one), (c.count > 0).then_some(c))
            }
            (Some(c), None) => (Some(c), None),
            (Some(mut c), Some(mut s)) if c.same_item_same_components(&s) => {
                let room = max(items, &s).saturating_sub(s.count);
                let moved = if right {
                    1.min(room).min(c.count)
                } else {
                    room.min(c.count)
                };
                s.count += moved;
                c.count -= moved;
                (Some(s), (c.count > 0).then_some(c))
            }
            (Some(c), Some(s)) => (Some(c), Some(s)),
        };
        self.carried = new_carried;
        *self.slot_mut(slot) = new_slot;
        if let Some(before) = furnace_input
            && let Some(BlockEntity::Furnace(f)) = self
                .container
                .and_then(|k| self.world.block_entities.get_mut(&k))
        {
            f.input_clicked(world_recipes!(self), before);
        }
    }
    pub(super) fn camera_orientation(&self) -> glam::Quat {
        camera_rotation(self.camera_type, self.yaw, self.pitch)
    }
    fn entity_geometry(
        &mut self,
        camera: glam::DVec3,
        alpha: f64,
        mut out: Vec<Vertex>,
    ) -> Vec<Vertex> {
        self.entity_spans.borrow_mut().clear();
        self.prepare_entity_culling(camera);
        self.prepare_trims();
        self.mark_entity(out.len(), "minecraft:entity_flame");
        let cracks = self.cracks(camera);
        let fire_yaw = if self.camera_type == perspective::CameraType::ThirdPersonFront {
            self.yaw + 180.0
        } else {
            self.yaw
        };
        for m in &self.mobs {
            let blazing = m.species() == vise_client::mobs::Species::Blaze && m.extra.flag;
            if (m.is_on_fire() || blazing) && !m.removed {
                let at = m.previous.lerp(m.position, alpha);
                vise_client::fire_render::flames(
                    [at.x, at.y, at.z],
                    m.width(),
                    m.height(),
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        at.x.floor() as i32,
                        at.y.floor() as i32,
                        at.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        if self.own_body_in_view()
            && let Some(p) = &self.player
            && p.fire_ticks > 0
            && !p.spectator
        {
            let feet = self
                .previous_position
                .lerp(p.position, self.clock.camera_fraction());
            vise_client::fire_render::flames(
                [feet.x, feet.y, feet.z],
                p.dimensions.width,
                p.dimensions.height,
                camera,
                fire_yaw,
                &self.gui.atlas.sprites,
                self.world.light(
                    feet.x.floor() as i32,
                    feet.y.floor() as i32,
                    feet.z.floor() as i32,
                ),
                &mut out,
            );
        }
        for item in &self.dropped {
            if item.fire_ticks > 0
                && !self
                    .res
                    .items()
                    .resists_fire(&item.stack, "minecraft:in_fire")
            {
                let at = item.previous.lerp(item.position, alpha);
                vise_client::fire_render::flames(
                    [at.x, at.y, at.z],
                    0.25,
                    0.25,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        at.x.floor() as i32,
                        at.y.floor() as i32,
                        at.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        for stand in &self.armor_stands {
            if stand.fire_ticks > 0 && !stand.marker {
                let scale = if stand.small { 0.5 } else { 1.0 };
                vise_client::fire_render::flames(
                    [stand.position.x, stand.position.y, stand.position.z],
                    0.5 * scale,
                    1.975 * scale,
                    camera,
                    fire_yaw,
                    &self.gui.atlas.sprites,
                    self.world.light(
                        stand.position.x.floor() as i32,
                        stand.position.y.floor() as i32,
                        stand.position.z.floor() as i32,
                    ),
                    &mut out,
                );
            }
        }
        self.mark_entity(out.len(), "minecraft:item");
        self.dropped_item_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "");
        self.vault_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "");
        self.campfire_geometry(camera, &mut out);
        self.brushable_geometry(camera, &mut out);
        self.shelf_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:ominous_item_spawner");
        self.ominous_item_spawner_geometry(camera, alpha, &mut out);
        self.mob_geometry(camera, alpha, &mut out);
        self.mark_player(out.len(), !self.own_body_in_view());
        self.player_model_geometry(camera, self.clock.camera_fraction(), &mut out);
        self.mark_entity(out.len(), "minecraft:player");
        self.guest_model_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:evoker_fangs");
        self.fangs_geometry(camera, alpha, &mut out);
        self.block_item_display_geometry(camera, alpha, &mut out);
        let mut see_through = std::mem::take(&mut self.see_through_vertices);
        see_through.clear();
        self.name_tag_geometry(camera, alpha, &mut out, &mut see_through);
        self.text_display_geometry(camera, alpha, &mut out, &mut see_through);
        self.see_through_vertices = see_through;
        self.mark_translucent_entity(out.len(), "minecraft:entity_shadow");
        let shadow_sprite = self
            .settings
            .entity_shadows
            .then(|| self.gui.atlas.sprites.get("misc/shadow").copied())
            .flatten();
        if let Some(sprite) = shadow_sprite {
            let ambient = match self.dim {
                Dim::Nether => 0.1,
                Dim::End => 0.25,
                Dim::Overworld => 0.0,
            };
            let darken = self.sky_darken();
            let a = alpha;
            let lerp = |p: Vec3, q: Vec3| {
                glam::DVec3::new(
                    p.x + (q.x - p.x) * a,
                    p.y + (q.y - p.y) * a,
                    p.z + (q.z - p.z) * a,
                )
            };
            let mut casters: Vec<(glam::DVec3, &str, f32)> =
                vise_client::shadows::mob_casters(&self.mobs, a).collect();
            let fixed = |id: &'static str| (id, vise_client::shadows::shadow_params(id).0);
            let (item, item_radius) = fixed("minecraft:item");
            casters.extend(
                self.dropped
                    .iter()
                    .map(|i| (lerp(i.previous, i.position), item, item_radius)),
            );
            let (orb, orb_radius) = fixed("minecraft:experience_orb");
            casters.extend(
                self.orbs
                    .iter()
                    .map(|o| (lerp(o.previous, o.position), orb, orb_radius)),
            );
            for (pos, id, radius) in casters {
                let strength = vise_client::shadows::shadow_params(id).1;
                vise_client::shadows::shadow(
                    &self.world,
                    self.res.facts(),
                    &|state| {
                        self.res
                            .collision()
                            .cached_shapes()
                            .cached_shape(state)
                            .ok()
                            .flatten()
                            .and_then(|shape| shape.bounds())
                    },
                    sprite,
                    camera,
                    pos,
                    radius,
                    strength,
                    ambient,
                    darken,
                    &mut out,
                );
            }
        }
        if let Some(r) = &self.renderer {
            let lid = |p: [i32; 3]| self.chest_lid(p, alpha as f32);
            let anim = |p: [i32; 3]| self.block_anim(p, alpha as f32);
            let scene = vise_client::block_entity_render::Scene {
                camera,
                orientation: self.camera_orientation(),
                anim: &anim,
                texts: None,
                partial: alpha as f32,
                game_time: self.world.game_time,
                max_y: self.world.shape.max_y() - 1,
                world: &self.world,
                sprites: &self.gui.atlas.sprites,
                nether: self.dim == Dim::Nether,
                lid: &lid,
                sign: &|p| match self.world.block_entities.get(&(p[0], p[1], p[2])) {
                    Some(BlockEntity::Sign(sign)) => Some(sign),
                    _ => None,
                },
                banner: &|p| self.banner_layers_at(p),
                skull: &|p| self.skull_skin_at(p),
                glyph: &|c| self.gui.atlas.glyph_cell(c),
                glyphs: self.gui.atlas.font.epoch(),
            };
            let view = self.view_frustum;
            let off_screen = |state: u32| {
                let name = &*self.res.facts().get(BlockStateId(state)).name;
                matches!(
                    name,
                    "minecraft:beacon"
                        | "minecraft:end_gateway"
                        | "minecraft:structure_block"
                        | "minecraft:test_instance_block"
                )
            };
            let visible = r.block_entities().filter(|(p, state)| {
                if !self.res.block_entity_render().renders(*state) {
                    return false;
                }
                off_screen(*state)
                    || view.is_none_or(|v| {
                        let o =
                            (glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
                                - camera)
                                .as_vec3();
                        v.intersects(o - glam::Vec3::ONE, o + glam::Vec3::splat(2.0))
                    })
            });
            if self.entity_culler.is_some() || self.pack_ids.is_some() {
                for block in visible {
                    let start = out.len();
                    self.mark_block_entity(start, block.1);
                    self.res
                        .block_entity_render()
                        .render(&scene, std::iter::once(block), &mut out);
                    if !off_screen(block.1) && self.entity_culler.is_some() {
                        self.cull_entity_vertices(&mut out, start);
                    }
                }
            } else {
                self.res
                    .block_entity_render()
                    .render(&scene, visible, &mut out);
            }
            self.mark_stage(out.len(), "DESTROY");
            for &(pos, stage) in &cracks {
                let state = self.world.block(pos[0], pos[1], pos[2]);
                self.res
                    .block_entity_render()
                    .render_breaking(&scene, pos, state.0, stage, &mut out);
            }
        }
        self.mark_entity(out.len(), "minecraft:experience_orb");
        if let Some(r) = self
            .gui
            .atlas
            .sprites
            .get("entity/experience/experience_orb")
            .copied()
        {
            let (yaw, pitch) = (self.yaw.to_radians(), self.pitch.to_radians());
            let forward = glam::Vec3::new(
                -yaw.sin() * pitch.cos(),
                -pitch.sin(),
                yaw.cos() * pitch.cos(),
            );
            let right = forward.cross(glam::Vec3::Y).normalize_or_zero();
            let up = right.cross(forward).normalize_or_zero();
            let live = self.orbs.iter().map(|o| {
                let at = [
                    o.previous.x + (o.position.x - o.previous.x) * alpha,
                    o.previous.y + (o.position.y - o.previous.y) * alpha,
                    o.previous.z + (o.position.z - o.previous.z) * alpha,
                ];
                (o, at, alpha as f32)
            });
            let taken = self.pickups.live.iter().filter_map(|p| match &p.taken {
                pickups::Taken::Orb(o) => Some((&**o, pickups::position(p, alpha), 1.0)),
                _ => None,
            });
            for (o, at, partial) in live.chain(taken) {
                let p = glam::DVec3::new(at[0], at[1] + 0.1, at[2]) - camera;
                let icon = o.icon();
                let (du, dv) = ((r.u1 - r.u0) / 4.0, (r.v1 - r.v0) / 4.0);
                let (u0, v0) = (r.u0 + (icon % 4) as f32 * du, r.v0 + (icon / 4) as f32 * dv);
                let t = (o.age as f32 + partial) / 2.0;
                let red = ((t.sin() + 1.0) * 0.5 * 255.0) as u32;
                let blue = (((t + 4.188_79).sin() + 1.0) * 0.1 * 255.0) as u32;
                let color = 0x8000_0000 | blue << 16 | 255 << 8 | red;
                let light = self.world.light(
                    o.position.x.floor() as i32,
                    o.position.y.floor() as i32,
                    o.position.z.floor() as i32,
                );
                let packed = (u32::from(light >> 4).max(10) * 16)
                    | ((u32::from(light & 15).max(10) * 16) << 8);
                let c = p.as_vec3();
                let s = 0.15;
                for (k, uv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                    .iter()
                    .zip([[u0, v0 + dv], [u0 + du, v0 + dv], [u0 + du, v0], [u0, v0]])
                {
                    let v = c + right * (k.0 * s) + up * (k.1 * s);
                    out.push(Vertex {
                        normal: [0; 4],
                        pos: v.to_array(),
                        uv,
                        layer: 0,
                        color,
                        light: packed,

                        glint_uv: [0.0, 0.0],
                    });
                }
            }
        }
        self.mark_entity(out.len(), "minecraft:falling_block");
        for f in &self.falling {
            let Some((_, tint)) = self.block_face_sprite(f.state) else {
                continue;
            };
            let p = glam::DVec3::new(
                f.previous.x + (f.position.x - f.previous.x) * alpha - 0.5,
                f.previous.y + (f.position.y - f.previous.y) * alpha,
                f.previous.z + (f.position.z - f.previous.z) * alpha - 0.5,
            ) - camera;
            let o = p.as_vec3();
            let light = self.world.light(
                f.position.x.floor() as i32,
                f.position.y.floor() as i32 + 1,
                f.position.z.floor() as i32,
            );
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            let start = out.len();
            self.block_model_vertices(
                f.state,
                vise_core::random::position_seed(f.start_pos),
                glam::Mat4::from_translation(o),
                packed,
                false,
                &mut out,
            );
            let tint = vise_client::mobs::render::abgr(tint);
            for vertex in &mut out[start..] {
                vertex.color = vise_client::hud_layout::multiply(vertex.color, tint);
            }
        }
        self.mark_entity(out.len(), "");
        self.moving_piston_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:tnt");
        self.tnt_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:painting");
        self.painting_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:item_frame");
        self.frame_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:armor_stand");
        self.armor_stand_geometry(camera, alpha, &mut out);
        self.cushion_geometry(camera, &mut out);
        self.mark_entity(out.len(), "minecraft:oak_boat");
        self.boat_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:minecart");
        self.cart_geometry(camera, alpha, &mut out);
        self.mark_stage(out.len(), "NONE");
        self.beacon_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:fishing_bobber");
        self.fishing_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:leash_knot");
        self.leash_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:trident");
        self.trident_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:firework_rocket");
        self.rocket_geometry(camera, alpha, &mut out);
        self.mark_entity(out.len(), "minecraft:splash_potion");
        self.thrown_potion_geometry(camera, alpha, &mut out);
        self.mark_stage(out.len(), "PARTICLES");
        let world = &self.world;
        self.particles.render(
            camera,
            self.yaw,
            self.pitch,
            alpha as f32,
            &|b| world.light(b[0], b[1], b[2]),
            &mut out,
        );
        self.mark_stage(out.len(), "DESTROY");
        for &(pos, stage) in &cracks {
            let o = (glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]))
                - camera)
                .as_vec3();
            let state = self.world.block(pos[0], pos[1], pos[2]);
            if self.res.facts().get(state).render_shape
                == vise_client::blocks::RenderShape::Model
                && let Ok(models) = self
                    .pack
                    .render
                    .model_catalog
                    .collect(state, BlockPos::new(pos[0], pos[1], pos[2]))
            {
                let offset = self
                    .pack
                    .context
                    .tables
                    .offset(state, pos[0], pos[2])
                    .unwrap_or([0.0; 3]);
                let origin = o + glam::DVec3::from(offset).as_vec3();
                let translucent = models
                    .iter()
                    .any(|(_, model)| model.faces.iter().any(|f| f.translucent));
                for (_, model) in models {
                    vise_client::breaking::geometry(model, origin, translucent, stage, &mut out);
                }
            }
        }
        out
    }

    pub(crate) fn destroy_block_progress(&mut self, breaker: i32, pos: [i32; 3], progress: i32) {
        let now = self.world.game_time;
        self.destroying
            .retain(|_, c| now.saturating_sub(c.2) <= 400);
        if (0..10).contains(&progress) {
            self.destroying.insert(breaker, (pos, progress as u8, now));
        } else {
            self.destroying.remove(&breaker);
        }
    }

    fn cracks(&self, camera: glam::DVec3) -> Vec<([i32; 3], [f32; 2])> {
        const DESTROY_STAGES: [&str; 10] = [
            "block/destroy_stage_0",
            "block/destroy_stage_1",
            "block/destroy_stage_2",
            "block/destroy_stage_3",
            "block/destroy_stage_4",
            "block/destroy_stage_5",
            "block/destroy_stage_6",
            "block/destroy_stage_7",
            "block/destroy_stage_8",
            "block/destroy_stage_9",
        ];
        let mut stages: Vec<([i32; 3], u32)> = Vec::new();
        let mut add = |pos: [i32; 3], stage: u32| match stages.iter_mut().find(|c| c.0 == pos) {
            Some(c) => c.1 = c.1.max(stage),
            None => stages.push((pos, stage)),
        };
        if let Some(m) = &self.mining
            && m.progress > 0.0
        {
            add(m.pos, ((m.progress * 10.0) as u32).min(9));
        }
        let now = self.world.game_time;
        for (pos, stage, at) in self.destroying.values() {
            if now.saturating_sub(*at) <= 400 {
                add(*pos, u32::from(*stage));
            }
        }
        stages
            .into_iter()
            .filter(|(pos, _)| {
                let d = glam::DVec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]))
                    + 0.5
                    - camera;
                d.length_squared() <= 1024.0
            })
            .filter_map(|(pos, stage)| {
                let name = DESTROY_STAGES.get(stage as usize)?;
                let r = self.gui.atlas.sprites.get(*name)?;
                Some((pos, [r.u0, r.v0]))
            })
            .collect()
    }

    fn sky_darken(&self) -> i32 {
        if self.dim == Dim::Nether {
            return 11;
        }
        if self.dim == Dim::End {
            let sky = self
                .res
                .end_environment()
                .sample(self.world.day_time, None, None);
            return (15.0 - 15.0 * sky.sky_light_level).round() as i32;
        }
        let (rain, thunder) = self.world.weather.levels(1.0);
        let key = (self.world.day_time, rain.to_bits(), thunder.to_bits());
        if let Some((k, darken)) = self.sky_darken_memo.get()
            && k == key
        {
            return darken;
        }
        let darken = vise_client::environment::sky_darken(
            self.res.environment(),
            self.world.day_time,
            rain,
            thunder,
        );
        self.sky_darken_memo.set(Some((key, darken)));
        darken
    }
    fn mob_hit(&self) -> Option<(usize, f64)> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = if self.creative { 5.0 } else { 3.0 };
        let mut best: Option<(usize, f64)> = None;
        for (i, m) in self.mobs.iter().enumerate() {
            if !m.alive() || Some(m.id) == self.riding || Some(m.id) == self.remote_camera_id() {
                continue;
            }
            if let Some(t) = ray_box(eye, dir, reach, m.bounds())
                && best.is_none_or(|b| t < b.1)
            {
                best = Some((i, t));
            }
        }
        best
    }
    pub(super) fn melee_damage(
        &mut self,
        target_kind: &str,
        living: bool,
    ) -> (f32, f32, bool, bool) {
        let hand_damage = 1.0 + self.attack_item_info().map_or(0.0, |i| i.attack_damage);
        let spin = self.player.as_ref().map_or(0.0, |p| p.auto_spin_attack_dmg);
        let damage = if spin > 0.0 {
            spin
        } else {
            hand_damage + self.effect_attack_bonus()
        };
        let scale = self.attack_strength_scale();
        let enchant_bonus = self.enchanted_damage(target_kind, damage) - damage;
        let mut amount = damage * (0.2 + scale * scale * 0.8);
        amount += self.smash_damage_bonus(target_kind);
        let crit = living && self.critical_attack(scale);
        if crit {
            amount *= 1.5;
        }
        amount += enchant_bonus * scale;
        (amount, scale, crit, enchant_bonus * scale > 0.0)
    }
    fn attack_strength_scale(&self) -> f32 {
        let delay = vise_client::attack_strength::delay(self.main_hand_attack_speed());
        ((self.attack_ticks as f32 + 0.5) / delay).clamp(0.0, 1.0)
    }
    pub(super) fn sweep_attack_position(&mut self, scale: f32, crit: bool) -> Option<[f64; 3]> {
        let p = self.player.as_mut()?;
        if let Ok(speed) = p.attribute("minecraft:movement_speed") {
            p.speed = speed as f32;
        }
        let p = self.player.as_ref()?;
        let sword = self.held().is_some_and(|s| {
            self.res
                .items()
                .item_tags
                .get("minecraft:swords")
                .is_some_and(|tag| tag.contains(&*s.item))
        });
        if !p.is_sweep_attack(scale, crit, sword) {
            return None;
        }
        use vise_core::geometry::trig;
        let yaw = f64::from(self.yaw * trig::DEG_TO_RAD);
        Some([
            p.position.x - f64::from(trig::sin(yaw)),
            p.position.y + f64::from(p.dimensions.height) * 0.5,
            p.position.z + f64::from(trig::cos(yaw)),
        ])
    }
    pub(super) fn sweep_attack(
        &mut self,
        at: [f64; 3],
        from: Option<[f64; 3]>,
        target: Option<(Aabb, mace::Spared)>,
        scale: f32,
    ) {
        self.audio
            .play("minecraft:entity.player.attack.sweep", from, 1.0, 1.0);
        if let Some((area, mob)) = target {
            self.sweep_damage(area, mob, scale);
        }
        let arc = vise_client::particles::ParticleOptions::simple(
            vise_client::particles::ParticleType::SweepAttack,
        );
        self.particles.add(&arc, at, [0.0; 3]);
        let origin = from.unwrap_or(at);
        let spread = [at[0] - origin[0], 0.0, at[2] - origin[2]];
        self.lan_send_particles(self.dim.id(), &arc, None, at, spread, 0.0, 0);
    }
    pub(super) fn vehicle_attack_sounds(&mut self, area: Aabb, magic: Option<i32>) {
        self.attack_effects(area, None, mace::Spared::Nothing, magic);
    }
    pub(super) fn attack_effects(
        &mut self,
        area: Aabb,
        living: Option<i32>,
        spared: mace::Spared,
        magic: Option<i32>,
    ) {
        let scale = self.attack_strength_scale();
        let full = scale > 0.9;
        let player = self.player.as_ref();
        let from = player.map(|p| [p.position.x, p.position.y, p.position.z]);
        if full && player.is_some_and(|p| p.sprinting) {
            self.audio
                .play("minecraft:entity.player.attack.knockback", from, 1.0, 1.0);
        }
        let crit = living.is_some() && self.critical_attack(scale);
        let sweep = self.sweep_attack_position(scale, crit);
        if let Some(at) = sweep {
            self.sweep_attack(at, from, Some((area, spared)), scale);
        }
        if crit {
            self.audio
                .play("minecraft:entity.player.attack.crit", from, 1.0, 1.0);
            if let Some(id) = living {
                self.area_critical_hit(id, area, false);
            }
        } else if sweep.is_none() {
            let sound = if full {
                "minecraft:entity.player.attack.strong"
            } else {
                "minecraft:entity.player.attack.weak"
            };
            self.audio.play(sound, from, 1.0, 1.0);
        }
        if let Some(id) = magic {
            self.area_critical_hit(id, area, true);
        }
    }
    fn area_critical_hit(&mut self, id: i32, area: Aabb, magic: bool) {
        let (feet, size) = (area.bottom_center(), area.size());
        self.critical_hit(
            self.lan_acting,
            id,
            [feet.x, feet.y, feet.z],
            size.x,
            size.y,
            magic,
        );
    }
    pub(super) fn critical_attack(&self, scale: f32) -> bool {
        let player = self.player.as_ref();
        let falling = player.is_some_and(|p| {
            !p.ground.on_ground
                && p.fall_distance > 0.0
                && !p.fluids.was_touching_water
                && !p.sprinting
        });
        let restricted = self.potions.effects.get("blindness").is_some() || self.riding.is_some();
        scale > 0.9 && falling && !restricted
    }
    fn attack(&mut self) -> bool {
        if self.spectator() {
            return false;
        }
        if let Some((i, d)) = self.painting_hit()
            && self.mob_hit().is_none_or(|m| d < m.1)
            && self.pick().is_none_or(|h| {
                let eye = self.eye_position().unwrap_or_default();
                (0..3)
                    .map(|a| (h.point[a] - eye[a]).powi(2))
                    .sum::<f64>()
                    .sqrt()
                    > d
            })
        {
            self.break_painting(i, true);
            return true;
        }
        if self.attack_redirectable() {
            return true;
        }
        if self.attack_frame()
            || self.attack_armor_stand()
            || self.attack_cushion()
            || self.attack_boat()
            || self.attack_cart()
            || self.attack_dragon()
            || self.attack_guest()
        {
            return true;
        }
        let Some((index, distance)) = self.mob_hit() else {
            return false;
        };
        if let Some(hit) = self.pick() {
            let eye = self.eye_position().unwrap_or_default();
            let d = ((hit.point[0] - eye[0]).powi(2)
                + (hit.point[1] - eye[1]).powi(2)
                + (hit.point[2] - eye[2]).powi(2))
            .sqrt();
            if d < distance {
                return false;
            }
        }
        self.attack_mob(index);
        self.swing_arm();
        true
    }
    pub(super) fn attack_mob(&mut self, index: usize) {
        let target_kind = self.mobs[index].kind.id.clone();
        let target_kind = format!("minecraft:{}", target_kind.trim_start_matches("minecraft:"));
        let (amount, scale, crit, magic) = self.melee_damage(&target_kind, true);
        let sweep = self.sweep_attack_position(scale, crit);
        let (knockback, knockback_attack) = self.extra_knockback(&target_kind, scale);
        self.attack_ticks = 0;
        let player = self.player.as_ref();
        let from = player.map(|p| [p.position.x, p.position.y, p.position.z]);
        if knockback_attack {
            self.audio
                .play("minecraft:entity.player.attack.knockback", from, 1.0, 1.0);
        }
        self.combat.last_hurt_mob = Some((self.mobs[index].id, self.world.game_time));
        let old_health = self.mobs[index].health;
        let source = match from {
            Some(f) => self.attack_source(f),
            None => vise_client::mobs::Damage::of(vise_client::mobs::DamageKind::Generic),
        };
        let landed = self.hurt_by_player(index, amount, source);
        if landed {
            let victim = self.mob_entity(&self.mobs[index]);
            self.player_hit(
                &victim,
                creatures::damage_type(source.kind, true),
                None,
                amount,
            );
        }
        if let Some(p) = self.player.as_mut() {
            if p.attack_log.len() >= 64 {
                p.attack_log.remove(0);
            }
            p.attack_log.push(vise_client::health::AttackRecord {
                game_time: p.health.game_time(),
                target: target_kind.clone(),
                id: self.mobs[index].id,
                killed: landed && !self.mobs[index].alive(),
                amount,
                scale,
                crit,
                landed,
                dealt: old_health - self.mobs[index].health,
            });
        }
        if landed {
            if knockback > 0.0 {
                let (xd, zd) = self.extra_knockback_direction();
                self.mobs[index].knockback(f64::from(knockback), xd, zd);
                self.stop_sprint_after_extra_knockback();
            }
            let mob = &self.mobs[index];
            let actual = old_health - mob.health;
            let (feet, width, height) = (
                [mob.position.x, mob.position.y, mob.position.z],
                mob.kind.width,
                mob.kind.height,
            );
            self.stats
                .custom("damage_dealt", (actual * 10.0).round() as i32);
            if crit {
                self.critical_hit(
                    self.lan_acting,
                    self.mobs[index].id as i32,
                    feet,
                    width,
                    height,
                    false,
                );
            }
            if magic {
                self.critical_hit(
                    self.lan_acting,
                    self.mobs[index].id as i32,
                    feet,
                    width,
                    height,
                    true,
                );
            }
            if actual > 2.0 {
                let indicator = vise_client::particles::ParticleOptions::simple(
                    vise_client::particles::ParticleType::DamageIndicator,
                );
                self.particles.send_particles(
                    &indicator,
                    [feet[0], feet[1] + height * 0.5, feet[2]],
                    [0.1, 0.0, 0.1],
                    0.2,
                    (actual * 0.5) as u32,
                );
                self.lan_send_particles(
                    self.dim.id(),
                    &indicator,
                    None,
                    [feet[0], feet[1] + height * 0.5, feet[2]],
                    [0.1, 0.0, 0.1],
                    0.2,
                    (actual * 0.5) as u32 as i32,
                );
            }
            self.last_hurt_by_player
                .insert(self.mobs[index].id, self.lan_acting);
            let sound = if crit {
                "minecraft:entity.player.attack.crit"
            } else if scale > 0.9 {
                "minecraft:entity.player.attack.strong"
            } else {
                "minecraft:entity.player.attack.weak"
            };
            if let Some(at) = sweep {
                let target = (
                    self.mobs[index].bounds(),
                    mace::Spared::Mob(self.mobs[index].id),
                );
                self.sweep_attack(at, from, Some(target), scale);
            } else {
                self.audio.play(sound, from, 1.0, 1.0);
            }
            let queued = std::mem::take(&mut self.mobs[index].outbox);
            let mut infested = false;
            for event in queued {
                match event {
                    vise_client::mobs::MobEvent::Sound(s, at, volume, pitch) => {
                        self.audio
                            .play(&format!("minecraft:{s}"), Some(at), volume, pitch);
                    }
                    vise_client::mobs::MobEvent::Infested { .. } => infested = true,
                    other => {
                        self.mobs[index].outbox.push(other);
                    }
                }
            }
            if infested {
                let id = self.mobs[index].id;
                self.mob_infested(id);
            }
            self.weapon_hit(index);
            if let Some(p) = self.player.as_mut()
                && !self.creative
            {
                p.cause_food_exhaustion(0.1);
            }
        } else {
            self.audio
                .play("minecraft:entity.player.attack.nodamage", from, 1.0, 1.0);
        }
    }
    fn explode(&mut self, center: [f64; 3], radius: f32) -> Result<()> {
        let interaction = if !self.ui.rules.bool("mob_griefing") {
            explosion::BlockInteraction::Keep
        } else {
            self.destroy_type("mob_explosion_drop_decay")
        };
        self.server_explode(center, radius, interaction, false)
    }
    fn with_entities<'a>(
        &self,
        chunks: impl IntoIterator<Item = &'a Arc<vise_client::chunk::Chunk>>,
    ) -> Vec<Arc<vise_client::chunk::Chunk>> {
        vise_client::save::chunks_with_block_entities(
            chunks,
            &self.world.block_entities,
            &self.world.block_names,
        )
        .into_iter()
        .map(|mut c| {
            self.attach_save_data(&mut c);
            Arc::new(c)
        })
        .collect()
    }
    fn tick_mobs(&mut self) -> Result<()> {
        self.tick_creatures()?;
        self.position_rider();
        self.tick_stats();
        self.tick_scoreboard();
        self.load_bossbars();
        self.tick_functions();
        self.tick_worldborder();
        self.tick_subtitles();
        self.spawn_phantoms();
        self.tick_advancements();
        self.tick_player_model();
        Ok(())
    }
    fn celestial_geometry(
        &self,
        sky: &vise_client::environment::Sky,
        rain: f32,
    ) -> (Vec<Vertex>, Option<(glam::Mat4, f32)>) {
        let body_color =
            0x00FF_FFFF | (((1.0 - rain).clamp(0.0, 1.0) * 255.0).round() as u32) << 24;
        let mut out = Vec::new();
        let atlas = &self.gui.atlas;
        let frame = |angle: f32| {
            glam::Mat4::from_rotation_y((-90.0f32).to_radians())
                * glam::Mat4::from_rotation_x(angle.to_radians())
        };
        let mut body = |name: &str, angle: f32, size: f32, moon: bool| {
            let Some(r) = atlas.sprites.get(name).copied() else {
                return;
            };
            let m = frame(angle);
            let corners = [(-size, -size), (size, -size), (size, size), (-size, size)];
            let uv = if moon {
                [[r.u1, r.v1], [r.u0, r.v1], [r.u0, r.v0], [r.u1, r.v0]]
            } else {
                [[r.u0, r.v0], [r.u1, r.v0], [r.u1, r.v1], [r.u0, r.v1]]
            };
            for ((x, z), uv) in corners.iter().zip(uv) {
                let p = m.transform_point3(glam::Vec3::new(*x, 100.0, *z));
                out.push(Vertex {
                    normal: [0; 4],
                    pos: p.to_array(),
                    uv,
                    layer: 2,
                    color: body_color,
                    light: 0,

                    glint_uv: [0.0, 0.0],
                });
            }
        };
        body("environment/celestial/sun", sky.sun_angle, 30.0, false);
        let phases = [
            "full_moon",
            "waning_gibbous",
            "third_quarter",
            "waning_crescent",
            "new_moon",
            "waxing_crescent",
            "first_quarter",
            "waxing_gibbous",
        ];
        let phase = phases[(self.world.day_time / 24000).rem_euclid(8) as usize];
        body(
            &format!("environment/celestial/moon/{phase}"),
            sky.moon_angle,
            20.0,
            true,
        );
        let stars =
            (sky.star_brightness > 0.0).then(|| (frame(sky.star_angle), sky.star_brightness));
        (out, stars)
    }
    fn title_action(&mut self, event_loop: &ActiveEventLoop, action: Action) -> Result<()> {
        match action {
            Action::Quit => event_loop.exit(),
            Action::Options => self.open_options(true),
            Action::Accessibility => {
                self.open_options_at(true, vise_client::options_ui::Page::Accessibility)
            }
            Action::Language => {
                self.open_options_at(true, vise_client::options_ui::Page::Language)
            }
            Action::Play { folder } => self.open_world(&folder, None, None, false, None)?,
            Action::Create {
                folder,
                name,
                seed,
                creative,
                options,
            } => self.open_world(&folder, Some(&name), seed, creative, Some(&options))?,
            Action::Connect { address } => {
                self.connect_server(&address, &self.options.player_name())?
            }
            Action::DataPacks { dir, selected } => {
                self.keys.clear();
                self.screen = Screen::Options;
                self.options_from_title = true;
                let resources = self.vanilla_root();
                self.ui.options.open_data_packs(dir, selected, resources);
            }
            Action::None => {}
        }
        Ok(())
    }
    fn open_world(
        &mut self,
        folder: &str,
        name: Option<&str>,
        seed: Option<i64>,
        creative: bool,
        create: Option<&vise_client::title::WorldOptions>,
    ) -> Result<()> {
        world_loading::world_data_used();
        if self.defer_world_entry(PendingWorldEntry::Local {
            folder: folder.to_owned(),
            name: name.map(str::to_owned),
            seed,
            creative,
            create: create.cloned().map(Box::new),
        })? {
            return Ok(());
        }
        self.join_world_rest()?;
        if let Some(dir) = create.and_then(|o| o.pack_dir.as_deref()) {
            let world = std::env::current_dir()?.join("saves").join(folder);
            let packs = create.map_or(&[][..], |o| &o.data_packs[..]);
            if let Err(e) = vise_client::datapack::install(dir, &world, packs) {
                log::warn!("Vise: data packs for {folder}: {e}");
            }
            let _ = std::fs::remove_dir_all(dir);
        }
        if self
            .options
            .execution_role
            .starts_integrated_server(self.options.server_thread)
        {
            return self.open_integrated(folder, name, seed, creative, create);
        }
        let request = world_loading::WorldRequest::new(folder, name, seed, creative, create)?;
        if self.window.is_some() {
            self.perf.world_opening();
            if self.title.is_none() {
                self.title = Some(Title::new(
                    std::env::current_dir()?.join("saves"),
                    &self.options.resources,
                ));
            }
            self.world_loader = Some(world_loading::WorldLoader::start(
                request,
                self.res.clone(),
                self.pack.clone(),
                self.options.clone(),
                self.settings.clone(),
            )?);
            return Ok(());
        }
        self.load_world(request)
    }

    fn load_world(&mut self, request: world_loading::WorldRequest) -> Result<()> {
        self.join_world_rest()?;
        self.perf.world_opening();
        let prepared = request.prepare(&self.res, &self.pack, &self.options, &self.settings)?;
        self.finish_open_world(prepared)
    }

    fn finish_open_world(&mut self, prepared: world_loading::PreparedWorld) -> Result<()> {
        self.reset_tutorial();
        let world_loading::PreparedWorld {
            save_root,
            shapes,
            level,
            world,
            streaming,
            save,
            session_lock,
            rand_sequences,
            locator,
        } = prepared;
        let dim = Dim::from_id(&level.dimension);
        self.level_shapes = shapes;
        self.world_root = save_root.clone();
        vise_client::crash::set_detail(
            "World",
            format!("{} ({})", level.name, save_root.display()),
        );
        self.session_lock = Some(session_lock);
        self.end.set_locator(locator);
        self.share_spawn_conditions();
        self.score = Default::default();
        self.fns = Default::default();
        self.world_tables = self.build_world_tables();
        let _ = vise_client::structure_blocks::world_loot_data(
            &self.options.resources,
            &pack_files(&self.world_tables),
        );
        self.bosses = Default::default();
        reset_world_progress(&mut self.adv, &mut self.stats);
        self.prefetch_advancement_tree();
        self.restore_player_state = true;
        self.rand_sequences = rand_sequences;
        self.border = Default::default();
        self.set_mode_id(level.mode());
        self.previous_mode = level.previous_game_mode.map(game_mode::mode_from_id);
        let mut inventory = vec![None; 36];
        self.armor = [None, None, None, None];
        self.offhand = None;
        self.ender_items = if level.ender_items.len() == 27 {
            level.ender_items.clone()
        } else {
            vec![None; 27]
        };
        for (slot, id, count, damage) in &level.inventory {
            let stack = ItemStack {
                item: Arc::from(id.as_str()),
                count: *count,
                damage: *damage,
                extra: level.inventory_components.get(slot).cloned(),
            };
            if (100..104).contains(slot) {
                self.armor[usize::from(103 - slot)] = Some(stack);
            } else if *slot == 150 {
                self.offhand = Some(stack);
            } else if let Some(cell) = inventory.get_mut(usize::from(*slot)) {
                *cell = Some(stack);
            }
        }
        if self.creative && level.inventory.is_empty() && level.position.is_none() {
            for (i, name) in [
                "minecraft:grass_block",
                "minecraft:stone",
                "minecraft:oak_planks",
                "minecraft:glass",
                "minecraft:torch",
                "minecraft:oak_log",
                "minecraft:chest",
                "minecraft:furnace",
                "minecraft:diamond_pickaxe",
            ]
            .iter()
            .enumerate()
            {
                inventory[i] = Some(ItemStack {
                    item: Arc::from(*name),
                    count: 1,
                    damage: 0,
                    extra: None,
                });
            }
        }
        let (yaw, pitch) = self
            .options
            .look
            .unwrap_or((level.rotation[0], level.rotation[1]));
        self.yaw = yaw;
        self.pitch = pitch;
        self.selected = level.selected_slot.clamp(0, 8) as usize;
        self.streaming = streaming;
        self.world = world;
        self.inventory = inventory;
        self.save = save;
        self.pending_vehicle = level.root_vehicle;
        self.level = level;
        let index = &world_stations!(self).recipe_index;
        let book = &mut self.level.recipe_book;
        book.recipes.retain(|r| index.contains(r));
        book.to_be_displayed.retain(|r| index.contains(r));
        self.dim = dim;
        self.world_icon = Some(WorldIcon {
            path: save_root.join("icon.png"),
            last_attempt: None,
        });
        self.world_flat = vise_client::save::WorldGen::read(&save_root)
            .is_some_and(|w| w.preset == vise_client::save::WorldPreset::Flat);
        self.block_ticks = Ticks::new(self.level.seed as i32);
        self.tick_chunks.clear();
        self.clear_entities();
        self.travelling = None;
        self.player = None;
        self.sleeping = None;
        self.sleep_timer = 0;
        self.player_beds.clear();
        self.spawn = None;
        self.respawn_travel = None;
        self.respawn_config = self
            .level
            .respawn_elsewhere
            .as_ref()
            .map(respawn_anchor::RespawnConfig::from_saved);
        self.spawn_origin = None;
        self.spawn_spiral = Default::default();
        self.player_spawn = None;
        self.creatures.reset_trader();
        self.creatures.cat_tick = 0;
        self.portals = Default::default();
        self.experience = Experience {
            level: self.level.xp_level,
            progress: self.level.xp_progress,
            total: self.level.xp_total,
        };
        self.restore_effects();
        self.mining = None;
        self.container = None;
        let kinds = std::mem::take(&mut self.be_anims.kinds);
        self.be_anims = vise_client::block_entity_anim::BlockEntityAnims {
            kinds,
            ..Default::default()
        };
        self.carried = None;
        self.using = None;
        self.screen = Screen::Playing;
        self.title = None;
        self.clear_lod();
        self.pending_uploads.clear();
        if let Some(r) = self.renderer.as_mut() {
            r.clear_sections(self.world.layout());
        }
        self.apply_settings();
        self.streaming.sky_light = dim != Dim::Nether;
        self.streaming.nether_shading = dim == Dim::Nether;
        self.load_world_state();
        Ok(())
    }
    fn make_streaming(
        &self,
        world_root: &std::path::Path,
        dim: Dim,
        seed: i64,
    ) -> Result<(Streaming, Arc<SaveDir>)> {
        let (streaming, save, locator) = world_loading::make_streaming(
            &self.res,
            &self.pack,
            &self.options,
            &self.settings,
            world_root,
            dim,
            self.level_shapes.get(dim.id()),
            seed,
        )?;
        self.end.set_locator(locator);
        self.share_spawn_conditions();
        Ok((streaming, save))
    }
    fn enter_dimension(&mut self, target: Dim) -> Result<()> {
        let Some(player) = self.player.take() else {
            return Ok(());
        };
        let from = self.dim;
        let destination = self.switch_level(target, player.position)?;
        let at = player.position;
        self.trigger_dimension(from, target, [at.x, at.y, at.z]);
        self.travelling = Some((player, destination));
        self.end_travel = (target == Dim::End || from == Dim::End) && self.end_portal_travel;
        Ok(())
    }
    pub(super) fn switch_level(&mut self, target: Dim, from: Vec3) -> Result<Vec3> {
        self.save.save_forced_chunks(&self.streaming.forced)?;
        self.streaming.save(&mut self.world, true);
        let keys: Vec<_> = self.world.modified.drain().collect();
        let chunks = self.with_entities(keys.iter().filter_map(|k| self.world.chunks.get(k)));
        self.save
            .save(&chunks, self.pack.assets.registry, self.res.biomes())?;
        self.save_dimension_entities()?;
        let scale = match (self.dim, target) {
            (Dim::Overworld, Dim::Nether) => 1.0 / 8.0,
            (Dim::Nether, Dim::Overworld) => 8.0,
            (Dim::End, Dim::Nether) => 1.0 / 8.0,
            _ => 1.0,
        };
        let destination = self.end_destination(target).unwrap_or(Vec3::new(
            from.x * scale,
            from.y,
            from.z * scale,
        ));
        let (streaming, save) =
            self.make_streaming(&self.world_root.clone(), target, self.level.seed)?;
        let mut world = World::new(self.res.facts().air, self.level.seed);
        world.shape = self.level_shapes.get(target.id());
        world.fast_lava = target == Dim::Nether;
        world.game_time = self.world.game_time;
        world.day_time = self.world.day_time;
        world.weather = self.world.weather;
        world.health_rules = self.world.health_rules;
        world.block_log = self.world.block_log.as_ref().map(|_| Vec::new());
        world.light_log = self.world.light_log.as_ref().map(|_| Default::default());
        world.watched = std::mem::take(&mut self.world.watched);
        self.streaming = streaming;
        self.world = world;
        self.save = save;
        self.dim = target;
        self.level.dimension = target.id().to_owned();
        self.block_ticks = Ticks::new(self.level.seed as i32);
        self.tick_chunks.clear();
        self.falling.clear();
        self.tnts.clear();
        self.mobs.clear();
        self.creatures.projectiles.clear();
        self.dropped.clear();
        self.particles.clear();
        self.pickups.clear();
        self.orbs.clear();
        self.mining = None;
        self.clear_lod();
        self.pending_uploads.clear();
        if let Some(r) = self.renderer.as_mut() {
            r.clear_sections(self.world.layout());
        }
        self.apply_settings();
        Ok(destination)
    }
    fn portal_shape(&self, start: [i32; 3], x_axis: bool) -> Option<([i32; 3], i32, i32)> {
        let name = |p: [i32; 3]| {
            self.res
                .facts()
                .get(self.world.block(p[0], p[1], p[2]))
                .name
                .clone()
        };
        let empty = |p: [i32; 3]| {
            let n = name(p);
            matches!(
                &*n,
                "minecraft:air"
                    | "minecraft:cave_air"
                    | "minecraft:fire"
                    | "minecraft:soul_fire"
                    | "minecraft:nether_portal"
            )
        };
        let frame = |p: [i32; 3]| &*name(p) == "minecraft:obsidian";
        let d = if x_axis { [1, 0, 0] } else { [0, 0, 1] };
        let mut bottom = start;
        let mut n = 0;
        while empty([bottom[0], bottom[1] - 1, bottom[2]]) && n < 21 {
            bottom[1] -= 1;
            n += 1;
        }
        if !frame([bottom[0], bottom[1] - 1, bottom[2]]) {
            return None;
        }
        let mut left = bottom;
        n = 0;
        while empty([left[0] - d[0], left[1], left[2] - d[2]]) && n < 21 {
            left = [left[0] - d[0], left[1], left[2] - d[2]];
            n += 1;
        }
        if !frame([left[0] - d[0], left[1], left[2] - d[2]]) {
            return None;
        }
        let mut width = 0;
        while width <= 21 {
            let p = [left[0] + d[0] * width, left[1], left[2] + d[2] * width];
            if !empty(p) || !frame([p[0], p[1] - 1, p[2]]) {
                break;
            }
            width += 1;
        }
        if !(2..=21).contains(&width)
            || !frame([left[0] + d[0] * width, left[1], left[2] + d[2] * width])
        {
            return None;
        }
        let mut height = 0;
        'rows: while height <= 21 {
            let y = left[1] + height;
            if !frame([left[0] - d[0], y, left[2] - d[2]])
                || !frame([left[0] + d[0] * width, y, left[2] + d[2] * width])
            {
                break;
            }
            for i in 0..width {
                if !empty([left[0] + d[0] * i, y, left[2] + d[2] * i]) {
                    break 'rows;
                }
            }
            height += 1;
        }
        let top_ok =
            (0..width).all(|i| frame([left[0] + d[0] * i, left[1] + height, left[2] + d[2] * i]));
        ((3..=21).contains(&height) && top_ok).then_some((left, width, height))
    }
    fn try_light_portal(&mut self, at: [i32; 3]) -> Result<bool> {
        if !matches!(self.dim, Dim::Overworld | Dim::Nether) {
            return Ok(false);
        }
        for x_axis in [true, false] {
            if let Some((origin, width, height)) = self.portal_shape(at, x_axis) {
                let d = if x_axis { [1, 0, 0] } else { [0, 0, 1] };
                let registry = &self.pack.assets.registry;
                let Some(default) = registry.default_state("minecraft:nether_portal") else {
                    return Ok(false);
                };
                let mut portal = registry.state(default).cloned().ok_or("portal state")?;
                portal
                    .properties
                    .insert("axis".into(), if x_axis { "x" } else { "z" }.into());
                let id = registry.resolve(&portal).unwrap_or(default);
                for h in 0..height {
                    for w in 0..width {
                        self.world.set(
                            origin[0] + d[0] * w,
                            origin[1] + h,
                            origin[2] + d[2] * w,
                            id,
                        )?;
                    }
                }
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn place_after_travel(&mut self, player: PlayerMotion, target: Vec3) -> Result<()> {
        self.arrive_through_nether_portal(player, target)
    }
    pub(super) fn finish_perf(&mut self) {
        let s = &self.settings;
        let settings = vise_telemetry::payload::Settings {
            render_distance: s.render_distance.clamp(0, 255) as u8,
            simulation_distance: s.simulation_distance.clamp(0, 255) as u8,
            max_fps: if s.max_fps >= 260 {
                0
            } else {
                s.max_fps.min(u32::from(u16::MAX)) as u16
            },
            vsync: s.vsync,
            fullscreen: s.fullscreen,
            graphics: s.graphics_preset.clone(),
        };
        if let Some(window) = &self.window {
            vise_client::telemetry::note_display(window);
        }
        self.perf
            .finish(&vise_client::logging::game_home(), settings);
    }

    pub(super) fn save_for_exit(&mut self) -> Result<()> {
        while self.pending_save.is_some() {
            self.poll_closing_save()?;
            if self.pending_save.is_some() {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        if self.title.is_some() || self.saved_for_exit {
            return Ok(());
        }
        if let Some(server) = self.integrated.take() {
            server.stop()?;
            self.saved_for_exit = true;
            return Ok(());
        }
        if let Some(mut remote) = self.remote.take() {
            remote.session.disconnect();
            self.saved_for_exit = true;
            return Ok(());
        }
        self.close_container();
        if self.remote.is_none() {
            self.stats.custom("leave_game", 1);
        }
        self.lan_close(false);
        self.free_player_beds()?;
        self.save_game()?;
        self.saved_for_exit = true;
        Ok(())
    }
    fn close_world(&mut self) -> Result<()> {
        self.clear_waypoints();
        if self.external_server() {
            return self.leave_server(None);
        }
        self.begin_closing_save(false)
    }
    fn finish_close_world(&mut self) -> Result<()> {
        self.ambience.clear(&mut self.audio);
        self.music.stop_playing(None, &mut self.audio);
        self.audio.stop_all();
        if self.remote.is_some() {
            self.world_icon = None;
            return self.leave_server(None);
        }
        self.session_lock = None;
        self.world_icon = None;
        vise_client::crash::clear_detail("World");
        self.debug_charts.reset();
        self.boss_overlay.reset();
        self.item_activation.reset();
        self.bosses = Default::default();
        self.streaming.paused = true;
        self.world = World::new(self.res.facts().air, 0);
        self.player = None;
        self.sleeping = None;
        self.sleep_timer = 0;
        self.player_beds.clear();
        self.clear_entities();
        self.screen = Screen::Playing;
        self.clear_lod();
        self.pending_uploads.clear();
        if let Some(r) = self.renderer.as_mut() {
            r.clear_sections(self.world.layout());
        }
        self.title = Some(Title::new(
            std::env::current_dir()?.join("saves"),
            &self.options.resources,
        ));
        if let Some(title) = &mut self.title {
            title.last_server = self.settings.file.string("lastServer");
            title.skip_multiplayer_warning = self.settings.file.bool("skipMultiplayerWarning");
        }
        self.capture(false);
        Ok(())
    }
    fn clear_entities(&mut self) {
        self.mobs.clear();
        self.dropped.clear();
        self.orbs.clear();
        self.creatures.projectiles.clear();
        self.falling.clear();
        self.tnts.clear();
        self.boats.clear();
        self.carts.clear();
        self.armor_stands.clear();
        self.cushions.clear();
        self.item_frames.clear();
        self.paintings.clear();
        self.others.clear();
        self.dragon = None;
        self.crystals.clear();
        self.dragon_fireballs.clear();
        self.breath_clouds.clear();
        self.tridents.clear();
        self.rockets.clear();
        self.bolts.clear();
        self.particles.clear();
        self.pickups.clear();
        self.riding = None;
        self.persist = Default::default();
    }
    fn apply_equipment(&mut self) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        for (attribute, id) in std::mem::take(&mut self.applied_modifiers) {
            let _ = player.set_modifier(&attribute, &id, 0.0, "add_value", false);
        }
        let slots = ["head", "chest", "legs", "feet"];
        let mut equipped: Vec<(Option<&ItemStack>, &str)> = self
            .armor
            .iter()
            .zip(slots)
            .map(|(s, n)| (s.as_ref(), n))
            .collect();
        equipped.push((self.inventory[self.selected].as_ref(), "mainhand"));
        for (stack, slot) in equipped {
            let Some(info) = stack.and_then(|s| world_items!(self).get(&s.item)) else {
                continue;
            };
            for (attribute, id, amount, op, group) in &info.modifiers {
                let matches = group == slot
                    || group == "any"
                    || (group == "armor" && slot != "mainhand")
                    || (group == "hand" && slot == "mainhand");
                if !matches || slot == "mainhand" {
                    continue;
                }
                if player
                    .set_modifier(attribute, id, *amount, op, true)
                    .is_ok()
                {
                    self.applied_modifiers.push((attribute.clone(), id.clone()));
                }
            }
        }
        self.apply_enchantment_attributes();
    }
    fn wear_armor(&mut self, events: &[MotionEvent]) {
        for event in events {
            if let MotionEvent::Health(vise_client::health::HealthEvent::ArmorHurt {
                source,
                amount,
            }) = event
            {
                self.hurt_armor(*source, *amount);
            }
        }
    }
    fn hurt_armor(&mut self, source: vise_world::damage_source::DamageTypeId, damage: f32) {
        if damage <= 0.0 {
            return;
        }
        let amount = ((damage / 4.0) as u32).max(1);
        let name = self
            .motion
            .health
            .damage
            .definition(source)
            .map(|d| d.name.clone())
            .unwrap_or_default();
        let fire = self
            .motion
            .health
            .damage
            .in_tag(source, "minecraft:is_fire");
        let mut broke = false;
        let mut broken_items = Vec::new();
        let losses: Vec<u32> = (0..4)
            .map(|i| match self.armor[i].clone() {
                Some(s)
                    if self
                        .res
                        .items()
                        .get(&s.item)
                        .is_some_and(|info| info.equip_damage_on_hurt)
                        && !(fire && world_items!(self).resists_fire(&s, &name)) =>
                {
                    self.wear(&s, amount)
                }
                _ => 0,
            })
            .collect();
        for (slot, amount) in self.armor.iter_mut().zip(losses) {
            if let Some(stack) = slot.as_mut()
                && let Some(info) = world_items!(self).get(&stack.item)
                && info.max_damage > 0
            {
                stack.damage += amount;
                if stack.damage >= info.max_damage {
                    broken_items.push(stack.clone());
                    *slot = None;
                    broke = true;
                }
            }
        }
        if broke {
            if let Some(eye) = self.eye_position() {
                self.audio.play(
                    "minecraft:entity.item.break",
                    Some(eye),
                    0.8,
                    0.8 + self.world.random.next_float() * 0.4,
                );
            }
            for stack in &broken_items {
                self.eat_particles(stack, 5);
            }
            self.apply_equipment();
        }
    }
    fn pick_fluid(&self) -> Option<Hit> {
        let (origin, dir) = self.aim()?;
        let mut t = 0.0;
        let mut last = origin.map(|v| v.floor() as i32);
        let mut first = true;
        while t <= self.reach() {
            let p = [
                origin[0] + dir[0] * t,
                origin[1] + dir[1] * t,
                origin[2] + dir[2] * t,
            ];
            let b = p.map(|v| v.floor() as i32);
            if first || b != last {
                let state = self.world.block(b[0], b[1], b[2]);
                let f = self.res.facts().get(state);
                let face = [last[0] - b[0], last[1] - b[1], last[2] - b[2]];
                if f.fluid != Fluid::None
                    && f.fluid_level == 8
                    && !f.fluid_falling
                    && !f.waterlogged
                    && f.render_shape != vise_client::blocks::RenderShape::Model
                {
                    if !first {
                        return Some(Hit {
                            pos: b,
                            face,
                            point: p,
                        });
                    }
                    let above = self.world.block(b[0], b[1] + 1, b[2]);
                    let same_above = self.res.facts().get(above).fluid == f.fluid;
                    if let Some((face, point)) =
                        clip_start_source(origin, dir, self.reach(), b, same_above)
                    {
                        return Some(Hit {
                            pos: b,
                            face,
                            point,
                        });
                    }
                } else if !first && !self.res.facts().is_air(state) && f.fluid == Fluid::None {
                    return Some(Hit {
                        pos: b,
                        face,
                        point: p,
                    });
                }
                last = b;
                first = false;
            }
            t += 0.05;
        }
        None
    }
    pub(super) fn bucket_hit(&self) -> Option<Hit> {
        let origin = self.aim().map_or([0.0; 3], |(origin, _)| origin);
        let distance = |p: [f64; 3]| {
            (p[0] - origin[0]).powi(2) + (p[1] - origin[1]).powi(2) + (p[2] - origin[2]).powi(2)
        };
        match (self.pick(), self.pick_fluid()) {
            (Some(block), Some(fluid)) => Some(if distance(block.point) <= distance(fluid.point) {
                block
            } else {
                fluid
            }),
            (block, fluid) => block.or(fluid),
        }
    }
    pub(super) fn bucket_target(&self, content: BucketContent, hit: &Hit) -> [i32; 3] {
        let clicked = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if content == BucketContent::Powder {
            if self.res.facts().get(clicked).replaceable {
                return hit.pos;
            }
        } else if content.is_water() && self.res.behaviours().is_liquid_container(clicked) {
            return hit.pos;
        }
        [
            hit.pos[0] + hit.face[0],
            hit.pos[1] + hit.face[1],
            hit.pos[2] + hit.face[2],
        ]
    }
    fn use_on_cauldron(&mut self, pos: [i32; 3]) -> Result<bool> {
        use vise_client::cauldron::{self, Outcome};
        struct Hand<'g, 'a>(&'g mut Game<'a>);
        impl cauldron::Player for Hand<'_, '_> {
            fn creative(&self) -> bool {
                self.0.creative
            }
            fn hand(&mut self) -> &mut Option<ItemStack> {
                self.0.held_mut()
            }
            fn add(&mut self, stack: &mut ItemStack) -> bool {
                let before = stack.count;
                match self.0.give(stack.clone()) {
                    None => stack.count = 0,
                    Some(rest) => stack.count = rest.count,
                }
                stack.count < before
            }
            fn contains(&self, stack: &ItemStack) -> bool {
                self.0
                    .inventory
                    .iter()
                    .flatten()
                    .any(|have| have.same_item_same_components(stack))
            }
            fn drop_stack(&mut self, stack: ItemStack) {
                if let Some(eye) = self.0.eye_position() {
                    self.0.throw_from_eyes(stack, eye, false);
                }
            }
        }
        let registry = self.pack.assets.registry;
        let id = self.world.block(pos[0], pos[1], pos[2]);
        let Some(state) = registry
            .state(id)
            .filter(|s| s.name.ends_with("cauldron"))
            .cloned()
        else {
            return Ok(false);
        };
        let Some(held) = self.held() else {
            return Ok(false);
        };
        let removes_dye = cauldron::removes_dye(self.items(), &held.item);
        let above = self.world.block(pos[0], pos[1] + 1, pos[2]);
        let under_water = registry
            .state(above)
            .is_some_and(|s| s.name == "minecraft:water");
        let result = cauldron::interact(&state, under_water, removes_dye, &mut Hand(self));
        if result.outcome == Outcome::Pass {
            return Ok(false);
        }
        if let Some(stat) = result.stat {
            self.stats.custom(stat, 1);
        }
        if let Some(block) = result.block
            && let Ok(new) = registry.resolve(&block)
        {
            self.level_set(pos, new)?;
        }
        if let Some(sound) = result.sound {
            self.audio.play(
                sound,
                Some([
                    f64::from(pos[0]) + 0.5,
                    f64::from(pos[1]) + 0.5,
                    f64::from(pos[2]) + 0.5,
                ]),
                1.0,
                1.0,
            );
        }
        Ok(true)
    }
    pub(super) fn block_transform(
        &self,
        stack: &ItemStack,
        pos: [i32; 3],
        face: [i32; 3],
    ) -> Option<(BlockStateId, String, Option<String>, u32, String)> {
        use vise_client::block_transform::Around;
        struct Column<'w> {
            world: &'w World,
            registry: &'w vise_world::registry::BlockRegistry,
            pos: [i32; 3],
        }
        impl Around for Column<'_> {
            fn state(&self, o: [i32; 3]) -> vise_world::chunk::BlockState {
                let id =
                    self.world
                        .block(self.pos[0] + o[0], self.pos[1] + o[1], self.pos[2] + o[2]);
                self.registry.state(id).cloned().unwrap_or_else(|| {
                    vise_world::chunk::BlockState {
                        name: "minecraft:air".into(),
                        properties: Default::default(),
                    }
                })
            }
        }
        let id = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.block_transformer.clone())?;
        let shield = self
            .offhand
            .as_ref()
            .and_then(|s| self.items().get(&s.item))
            .is_some_and(|i| i.blocks_attacks.is_some());
        if shield && !self.keys.contains(&KeyCode::ShiftLeft) {
            return None;
        }
        let face_name = match face {
            [0, -1, 0] => "down",
            [0, 1, 0] => "up",
            [0, 0, -1] => "north",
            [0, 0, 1] => "south",
            [-1, 0, 0] => "west",
            _ => "east",
        };
        let registry = self.pack.assets.registry;
        let around = Column {
            world: &self.world,
            registry,
            pos,
        };
        let (_, t, state) =
            self.res
                .transformers()
                .apply(self.items(), registry, &id, face_name, &around)?;
        let new = registry.resolve(&state).ok()?;
        Some((
            new,
            t.sound.clone(),
            t.loot.clone(),
            t.damage,
            t.particle.clone(),
        ))
    }
    pub(super) fn transform_particle(&mut self, particle: &str, pos: [i32; 3]) {
        match particle {
            "scrape" => self.level_event_at(3005, pos, 0),
            "wax_on" => self.level_event_at(3003, pos, 0),
            "wax_off" => self.level_event_at(3004, pos, 0),
            _ => {}
        }
    }
    fn use_block_transformer(
        &mut self,
        stack: &ItemStack,
        pos: [i32; 3],
        face: [i32; 3],
    ) -> Result<bool> {
        let Some((new, sound, loot, damage, particle)) = self.block_transform(stack, pos, face)
        else {
            return Ok(false);
        };
        self.trigger_block("minecraft:item_used_on_block", pos, stack);
        self.level_set(pos, new)?;
        self.transform_particle(&particle, pos);
        if loot.as_deref() == Some("minecraft:till/rooted_dirt") {
            self.pop_resource(
                [pos[0] + face[0], pos[1] + face[1], pos[2] + face[2]],
                ItemStack::new("minecraft:hanging_roots", 1),
            );
        }
        self.audio.play(
            &sound,
            Some([
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ]),
            1.0,
            1.0,
        );
        self.stats.add("used", &stack.item, 1);
        self.damage_held(damage);
        self.post_block_game_event("minecraft:block_change", pos, self.sculk_actor(), Some(new));
        Ok(true)
    }
    fn set_state_named(&mut self, pos: [i32; 3], name: &str, keep: &[&str]) -> Result<bool> {
        let registry = &self.pack.assets.registry;
        let old = self.world.block(pos[0], pos[1], pos[2]);
        let Some(default) = registry.default_state(name) else {
            return Ok(false);
        };
        let mut state = registry.state(default).cloned().ok_or("state")?;
        if let Some(previous) = registry.state(old) {
            for k in keep {
                if let Some(v) = previous.properties.get(*k) {
                    state.properties.insert((*k).to_owned(), v.clone());
                }
            }
        }
        let id = registry.resolve(&state).unwrap_or(default);
        self.level_set(pos, id)?;
        Ok(true)
    }
    pub(super) fn cauldron_extinguish(&mut self, pos: BlockPos) {
        if self.remote.is_some() {
            return;
        }
        let at = [pos.x, pos.y, pos.z];
        let registry = self.pack.assets.registry;
        let Some(old) = registry
            .state(self.world.block(at[0], at[1], at[2]))
            .filter(|s| {
                matches!(
                    s.name.as_str(),
                    "minecraft:water_cauldron" | "minecraft:powder_snow_cauldron"
                )
            })
            .cloned()
        else {
            return;
        };
        let Ok(new) = registry.resolve(&vise_client::cauldron::extinguished(&old)) else {
            return;
        };
        if self.level_set(at, new).is_ok() {
            self.post_block_game_event("minecraft:block_change", at, None, Some(new));
        }
    }
    pub(super) fn damage_held(&mut self, amount: u32) {
        if self.creative {
            return;
        }
        let amount = match self.held().cloned() {
            Some(held) => self.wear(&held, amount),
            None => amount,
        };
        let max = self
            .held()
            .and_then(|s| self.items().get(&s.item))
            .map_or(0, |i| i.max_damage);
        let cell = self.held_mut();
        if let Some(stack) = cell.as_mut()
            && max > 0
        {
            stack.damage += amount;
            if stack.damage >= max {
                let broken = stack.item.clone();
                let gone = stack.clone();
                *cell = None;
                self.stats.add("broken", &broken, 1);
                if let Some(eye) = self.eye_position() {
                    self.audio.play(
                        "minecraft:entity.item.break",
                        Some(eye),
                        0.8,
                        0.8 + self.world.random.next_float() * 0.4,
                    );
                }
                self.eat_particles(&gone, 5);
            }
        }
    }
    fn hurt_held_without_breaking(&mut self, amount: u32) {
        if self.creative {
            return;
        }
        let Some(held) = self.held().cloned() else {
            return;
        };
        let amount = self.wear(&held, amount);
        let max = self.items().get(&held.item).map_or(0, |i| i.max_damage);
        if amount == 0 || max == 0 {
            return;
        }
        if let Some(stack) = self.held_mut().as_mut() {
            stack.damage = (stack.damage + amount).min(max - 1);
        }
    }
    fn use_tool(
        &mut self,
        stack: &ItemStack,
        allow_block_actions: bool,
    ) -> Result<vise_client::first_person::UseResult> {
        use vise_client::first_person::UseResult;
        let item = stack.item.trim_start_matches("minecraft:").to_owned();
        let at = |p: [i32; 3]| {
            [
                f64::from(p[0]) + 0.5,
                f64::from(p[1]) + 0.5,
                f64::from(p[2]) + 0.5,
            ]
        };
        if allow_block_actions
            && let Some(hit) = self.pick()
            && self.use_on_cauldron(hit.pos)?
        {
            return Ok(UseResult::Success { swing: true });
        }
        if allow_block_actions
            && let Some(hit) = self.pick()
            && let Some(placed) = self.use_on_campfire(hit.pos)?
        {
            return Ok(UseResult::Success { swing: placed });
        }
        let bucket = vise_client::items::bucket_contents(&stack.item);
        if matches!(self.guest_use, Some(GuestUse::OnBlock(_)))
            && (item == "bucket" || bucket.is_some())
        {
            return Ok(UseResult::Pass);
        }
        if item == "bucket" {
            let Some(hit) = self.bucket_hit() else {
                return Ok(UseResult::Pass);
            };
            if !self.may_build() {
                return Ok(UseResult::Fail);
            }
            if self.is_powder_snow(hit.pos) {
                self.pick_up_powder_snow(hit.pos)?;
                return Ok(UseResult::Success { swing: true });
            }
            let pos = BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
            let creative = self.creative;
            let Some(filled) =
                self.with_level(|b, level, ticks| b.bucket_pickup(level, ticks, pos, creative))
            else {
                return Ok(UseResult::Fail);
            };
            self.process_level_events()?;
            self.stats.add("used", "minecraft:bucket", 1);
            let sound = if filled == "minecraft:lava_bucket" {
                "minecraft:item.bucket.fill_lava"
            } else {
                "minecraft:item.bucket.fill"
            };
            self.audio.play(sound, Some(at(hit.pos)), 1.0, 1.0);
            self.post_block_game_event(
                "minecraft:fluid_pickup",
                hit.pos,
                Some(vise_client::mobs::Who::Player),
                None,
            );
            let filled = ItemStack::new(filled, 1);
            self.filled_result(filled.clone());
            self.trigger_item("minecraft:filled_bucket", &filled);
            return Ok(UseResult::Success { swing: true });
        }
        if !allow_block_actions && bucket.is_none() {
            return Ok(UseResult::Pass);
        }
        let Some(hit) = self.pick() else {
            return Ok(UseResult::Pass);
        };
        let clicked = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if let Some((content, sound, mob)) = bucket {
            if !self.may_build() {
                return Ok(UseResult::Fail);
            }
            let target = self.bucket_target(content, &hit);
            let pos = BlockPos::new(target[0], target[1], target[2]);
            let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
            let creative = self.creative;
            let emptied = self.with_level(|b, level, ticks| {
                b.bucket_empty(level, ticks, pos, content, sound, sneaking, creative)
            });
            self.process_level_events()?;
            if !emptied {
                return Ok(UseResult::Fail);
            }
            if let Some(kind) = mob {
                if let Some(id) = self.spawn_mob_at(kind, (target[0], target[1], target[2]), true) {
                    if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id) {
                        vise_client::entity_nbt::load_bucket_data(m, stack);
                    }
                    self.mob_placed(id)?;
                }
                self.post_block_game_event(
                    "minecraft:entity_place",
                    target,
                    self.sculk_actor(),
                    None,
                );
            }
            self.trigger_block("minecraft:placed_block", target, stack);
            if !self.creative {
                *self.held_mut() = Some(ItemStack::new("minecraft:bucket", 1));
            }
            return Ok(UseResult::Success { swing: true });
        }
        if self.use_block_transformer(stack, hit.pos, hit.face)? {
            return Ok(UseResult::Success { swing: true });
        }
        if item == "honeycomb" {
            let registry = self.pack.assets.registry;
            let old = registry.state(clicked).cloned();
            if let Some(waxed) = old.and_then(|old| self.res.transformers().waxed(registry, &old))
                && let Ok(state) = registry.resolve(&waxed)
            {
                self.trigger_block("minecraft:item_used_on_block", hit.pos, stack);
                self.level_set(hit.pos, state)?;
                self.level_event_at(3003, hit.pos, 0);
                self.audio.play(
                    "minecraft:item.honeycomb.wax_on",
                    Some(at(hit.pos)),
                    1.0,
                    1.0,
                );
                self.post_block_game_event(
                    "minecraft:block_change",
                    hit.pos,
                    self.sculk_actor(),
                    Some(state),
                );
                self.stats.add("used", &stack.item, 1);
                if !self.creative {
                    self.consume_held();
                }
                return Ok(UseResult::Success { swing: true });
            }
        }
        if item == "flint_and_steel" || item == "fire_charge" {
            if !self.may_build() {
                return Ok(UseResult::Fail);
            }
            let name = self.res.facts().get(clicked).name.clone();
            if self.can_light(clicked, &name) {
                let registry = self.pack.assets.registry;
                let mut state = registry
                    .state(clicked)
                    .cloned()
                    .ok_or("missing lit block state")?;
                state.properties.insert("lit".into(), "true".into());
                self.level_set(hit.pos, registry.resolve(&state)?)?;
                self.audio.play(
                    if item == "fire_charge" {
                        "minecraft:item.firecharge.use"
                    } else {
                        "minecraft:item.flintandsteel.use"
                    },
                    Some(at(hit.pos)),
                    1.0,
                    vise_client::items::fire_starter_pitch(
                        item == "fire_charge",
                        &mut self.world.random,
                    ),
                );
                if item == "fire_charge" {
                    self.consume_held();
                } else {
                    self.damage_held(1);
                }
                return Ok(UseResult::Success { swing: true });
            }
            let target = [
                hit.pos[0] + hit.face[0],
                hit.pos[1] + hit.face[1],
                hit.pos[2] + hit.face[2],
            ];
            if self.can_place_used_fire(target) {
                if !self.try_light_portal(target)? {
                    self.place_used_fire(target)?;
                }
                let sound = vise_client::items::fire_starter_sound(item == "fire_charge");
                let pitch = vise_client::items::fire_starter_pitch(
                    item == "fire_charge",
                    &mut self.world.random,
                );
                self.audio.play(sound, Some(at(target)), 1.0, pitch);
                if item == "fire_charge" {
                    self.consume_held();
                } else {
                    self.damage_held(1);
                }
                return Ok(UseResult::Success { swing: true });
            }
            return Ok(UseResult::Fail);
        }
        Ok(UseResult::Pass)
    }
    fn spawn_orbs(&mut self, at: [f64; 3], mut value: u32) {
        while value > 0 {
            let v = ExperienceOrb::split(value);
            value -= v;
            let merge_id = self.world.random.next_int_bound(40).unwrap_or(0);
            self.award_orb(at, v, merge_id);
        }
    }
    fn award_orb(&mut self, at: [f64; 3], value: u32, merge_id: i32) {
        let centre = Vec3::new(at[0], at[1], at[2]);
        let half = Vec3::new(0.5, 0.5, 0.5);
        let area = Aabb::new(centre.subtract(half), centre.add_vector(half));
        let target = self.remote.is_none().then(|| {
            self.orbs
                .iter_mut()
                .find(|o| o.can_merge(merge_id as u32, value) && o.bounds().intersects(area))
        });
        if let Some(Some(orb)) = target {
            orb.count += 1;
            orb.age = 0;
        } else {
            self.add_orb(at, value);
        }
    }
    fn add_orb(&mut self, at: [f64; 3], value: u32) {
        let mut r = dispenser::entity_random();
        let _yaw = r.next_float();
        let p = Vec3::new(at[0], at[1], at[2]);
        self.next_entity += 1;
        self.orbs.push(ExperienceOrb {
            id: self.next_entity,
            position: p,
            previous: p,
            velocity: Vec3::new(
                (r.next_double() * 0.2 - 0.1) * 2.0,
                r.next_double() * 0.2 * 2.0,
                (r.next_double() * 0.2 - 0.1) * 2.0,
            ),
            value,
            age: 0,
            on_ground: false,
            count: 1,
            ticks: 0,
        });
    }
    fn can_place_used_fire(&mut self, target: [i32; 3]) -> bool {
        let pos = BlockPos::new(target[0], target[1], target[2]);
        let direction = match ((self.yaw / 90.0 + 0.5).floor() as i32).rem_euclid(4) {
            0 => Direction::South,
            1 => Direction::West,
            2 => Direction::North,
            _ => Direction::East,
        };
        self.with_level(|b, level, ticks| b.fire_can_be_placed_at(level, ticks, pos, direction))
    }
    fn place_used_fire(&mut self, target: [i32; 3]) -> Result<()> {
        let pos = BlockPos::new(target[0], target[1], target[2]);
        let state = self.with_level(|b, level, ticks| b.fire_state_for(level, ticks, pos));
        self.level_set(target, state)
    }
    fn block_experience(&mut self, name: &str) -> u32 {
        let (min, max) = match name.trim_start_matches("minecraft:") {
            "coal_ore" | "deepslate_coal_ore" => (0, 2),
            "nether_gold_ore" => (0, 1),
            "lapis_ore" | "deepslate_lapis_ore" | "nether_quartz_ore" => (2, 5),
            "diamond_ore" | "deepslate_diamond_ore" | "emerald_ore" | "deepslate_emerald_ore" => {
                (3, 7)
            }
            "redstone_ore" | "deepslate_redstone_ore" => (1, 5),
            "sculk" => (1, 1),
            "sculk_catalyst" | "sculk_sensor" | "calibrated_sculk_sensor" | "sculk_shrieker" => {
                (5, 5)
            }
            "spawner" => {
                let r = &mut self.world.random;
                let a = r.next_int_bound(15).unwrap_or(0);
                return (15 + a + r.next_int_bound(15).unwrap_or(0)) as u32;
            }
            _ => (0, 0),
        };
        if max == 0 {
            return 0;
        }
        (min + self.world.random.next_int_bound(max - min + 1).unwrap_or(0)) as u32
    }
    fn tick_experience(&mut self) {
        if self.xp_delay > 0 {
            self.xp_delay -= 1;
        }
        let spectator = self.spectator();
        let player = self
            .player
            .as_ref()
            .filter(|p| !p.health.dead_or_dying() && !spectator);
        let mut followed: Vec<([f64; 3], [f64; 3])> = player
            .map(|p| {
                let feet = [p.position.x, p.position.y, p.position.z];
                let eye = f64::from(p.dimensions.eye_height);
                (feet, [feet[0], feet[1] + eye / 2.0, feet[2]])
            })
            .into_iter()
            .collect();
        if let Some(host) = self.lan.as_ref() {
            followed.extend(
                host.hub
                    .guests
                    .iter()
                    .filter(|g| g.placed && g.loaded && !g.dead && g.health > 0.0)
                    .filter(|g| !g.spectator())
                    .map(|g| {
                        let p = g.position;
                        (p, [p[0], p[1] + g.eye_height() / 2.0, p[2]])
                    }),
            );
        }
        let bounds = player.map(|p| {
            let w = f64::from(p.dimensions.width) / 2.0;
            (
                [
                    p.position.x - w - 1.0,
                    p.position.y - 0.5,
                    p.position.z - w - 1.0,
                ],
                [
                    p.position.x + w + 1.0,
                    p.position.y + f64::from(p.dimensions.height) + 0.5,
                    p.position.z + w + 1.0,
                ],
            )
        });
        let mut orbs = std::mem::take(&mut self.orbs);
        let mut lava_random = None;
        let mut next_float = || {
            lava_random
                .get_or_insert_with(dispenser::entity_random)
                .next_float()
        };
        orbs.retain_mut(|o| {
            let distance = |feet: &[f64; 3]| -> f64 {
                (feet[0] - o.position.x).powi(2)
                    + (feet[1] - o.position.y).powi(2)
                    + (feet[2] - o.position.z).powi(2)
            };
            let target = followed
                .iter()
                .filter(|(feet, _)| distance(feet) < 64.0)
                .min_by(|a, b| distance(&a.0).total_cmp(&distance(&b.0)))
                .map(|(_, target)| *target);
            o.tick(
                &self.world,
                self.res.facts(),
                self.collision,
                target,
                &mut next_float,
            )
        });
        if self.remote.is_none() {
            vise_client::entities::merge_orbs(&mut orbs);
        }
        let mut gained = 0;
        let mut taken = Vec::new();
        if let Some((min, max)) = bounds {
            orbs.retain_mut(|o| {
                let touching = o.position.x + 0.25 > min[0]
                    && o.position.x - 0.25 < max[0]
                    && o.position.y + 0.5 > min[1]
                    && o.position.y < max[1]
                    && o.position.z + 0.25 > min[2]
                    && o.position.z - 0.25 < max[2];
                if touching && self.xp_delay == 0 {
                    self.xp_delay = 2;
                    gained += o.value;
                    taken.push(o.clone());
                    o.count -= 1;
                    return o.count > 0;
                }
                true
            });
        }
        self.orbs = orbs;
        for o in taken {
            self.add_pickup(pickups::Taken::Orb(Box::new(o)), pickups::Collector::Player);
        }
        if gained > 0 {
            gained = self.mending(gained);
        }
        if gained > 0 {
            let at = self.eye_position();
            let pitch =
                (self.world.random.next_float() - self.world.random.next_float()) * 0.35 + 0.9;
            self.audio
                .play("minecraft:entity.experience_orb.pickup", at, 0.1, pitch);
            self.give_orb_points(gained);
        }
    }
    pub(super) fn award_furnace_recipes(&mut self, pos: (i32, i32, i32), at: [f64; 3]) {
        let used = match self.world.block_entities.get_mut(&pos) {
            Some(BlockEntity::Furnace(f)) => {
                let used = f.recipe_experience(world_recipes!(self));
                f.recipes_used.clear();
                used
            }
            _ => return,
        };
        for (count, experience) in used {
            let xp = vise_client::containers::create_experience(
                count,
                experience,
                &mut self.world.random,
            );
            self.spawn_orbs(at, xp);
        }
    }
    pub(super) fn give_orb_points(&mut self, gained: u32) {
        let at = self.eye_position();
        if let Some(level) = self.experience.give(gained as i32)
            && self.ticks > self.last_level_up + 100
        {
            self.last_level_up = self.ticks;
            let volume = if level > 30 { 1.0 } else { level as f32 / 30.0 };
            self.audio
                .play("minecraft:entity.player.levelup", at, volume * 0.75, 1.0);
        }
    }
    fn release_bow(&mut self, ticks: u32) {
        let f = ticks as f32 / 20.0;
        let power = ((f * f + f * 2.0) / 3.0).min(1.0);
        if power < 0.1 {
            return;
        }
        let arrow_slot = self.projectile_slot("minecraft:bow");
        if arrow_slot.is_none() && !self.creative {
            return;
        }
        let Some(eye) = self.eye_position() else {
            return;
        };
        let p = Vec3::new(eye[0], eye[1] - 0.1, eye[2]);
        let ammo = arrow_slot
            .and_then(|i| self.stack_in(i).cloned())
            .unwrap_or_else(|| ItemStack::new("minecraft:arrow", 1));
        let weapon = self.held().cloned();
        let (power_bonus, infinity) = self.bow_enchantments(&ammo.item);
        let effect = dispenser::tipped_effect(&ammo).or_else(|| {
            (&*ammo.item == "minecraft:spectral_arrow").then_some(("minecraft:glowing", 200))
        });
        let kind = vise_client::projectiles::ProjectileKind::Arrow {
            base_damage: 2.0 + power_bonus,
            critical: power >= 1.0,
            effect,
        };
        let mut arrow = vise_client::projectiles::Projectile::new(
            kind,
            p,
            Some(vise_client::mobs::Who::Player),
        );
        if let Some(a) = arrow.arrow.as_mut() {
            a.set_item(&vise_client::arrows::drawn_ammo(
                &ammo,
                self.creative || infinity,
            ));
            a.set_owner(Some(vise_client::mobs::Who::Player));
            a.fired_from_weapon = weapon;
        }
        if let Some(seconds) = self.bow_projectile_ignite()
            && let Some(a) = arrow.arrow.as_mut()
        {
            a.ignite_for_seconds(seconds);
        }
        let (velocity, on_ground) = self.player.as_ref().map_or((Vec3::ZERO, true), |pl| {
            (self.known_player_movement(), pl.ground.on_ground)
        });
        arrow.shoot_from_rotation(
            &mut self.world.random,
            self.pitch,
            self.yaw,
            0.0,
            power * 3.0,
            1.0,
            velocity,
            on_ground,
        );
        self.add_projectile(arrow);
        self.player_projectile_shot();
        let pitch = 1.0 / (self.world.random.next_float() * 0.4 + 1.2) + power * 0.5;
        self.audio
            .play("minecraft:entity.arrow.shoot", Some(eye), 1.0, pitch);
        if !self.creative
            && !infinity
            && let Some(i) = arrow_slot
        {
            let cell = self.stack_in_mut(i);
            if let Some(s) = cell.as_mut() {
                s.count -= 1;
                if s.count == 0 {
                    *cell = None;
                }
            }
        }
        self.damage_held(1);
    }
    fn first_person_arm(&self, left: bool, m: glam::Mat4, light: u32, out: &mut Vec<Vertex>) {
        let slim = self.player_model.as_ref().is_some_and(|m| m.slim);
        let Some(arm) = self.res.player_arm(slim, left).as_ref() else {
            return;
        };
        let sprites = &self.gui.atlas.sprites;
        let Some(r) = self
            .player_model
            .as_ref()
            .and_then(|m| m.skin.as_deref())
            .and_then(|skin| sprites.get(skin))
            .or_else(|| sprites.get("entity/player/wide/steve"))
            .copied()
        else {
            return;
        };
        let z_rot = if left { -0.1 } else { 0.1 };
        let pose = |p: &vise_client::mobs::Part| {
            let mut pose = p.pose;
            pose[3] = 0.0;
            pose[4] = 0.0;
            pose[5] = if p.name == arm.name { z_rot } else { 0.0 };
            pose
        };
        vise_client::mobs::render_model(arm, m, &pose, [r.u0, r.v0, r.u1, r.v1], light, 0, out);
    }
    fn hand_geometry(&mut self, alpha: f64) -> Vec<Vertex> {
        let mut out = Vec::new();
        if self.spectator()
            || self.ui.hide_gui
            || self.sleeping.is_some()
            || !self.camera_type.first_person()
            || self
                .player
                .as_ref()
                .is_none_or(|p| p.health.dead_or_dying())
            || self.screen == Screen::Dead
        {
            return out;
        }
        let a = alpha as f32;
        let swing = self.swing.animation(a);
        let attacks = Hand::BOTH.map(|h| {
            if self.swing.attack_hand() == h {
                swing
            } else {
                0.0
            }
        });
        let inverse_heights = self.heights.interpolated(a).map(|h| 1.0 - h);
        let view = vise_client::camera::view_matrix(self.yaw, self.pitch);
        let [sway_x, sway_y] = self.sway.rotation(a, self.pitch, self.yaw);
        let to_world = view.inverse()
            * glam::Mat4::from_rotation_x(sway_x.to_radians())
            * glam::Mat4::from_rotation_y(sway_y.to_radians());
        let eye = self.eye_position().unwrap_or_default();
        let light = self.world.light(
            eye[0].floor() as i32,
            eye[1].floor() as i32,
            eye[2].floor() as i32,
        );
        let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
        let shown = self.hands_to_render();
        let used_hand = self.used_hand();
        let spinning = self
            .player
            .as_ref()
            .is_some_and(|p| p.auto_spin_attack_ticks > 0);
        let items = self.visible_hand_items();
        let off_hand_empty = items[1].is_none();
        for (i, (hand, item)) in Hand::BOTH.into_iter().zip(items).enumerate() {
            if !shown[i] {
                continue;
            }
            let (attack, inverse_height) = (attacks[i], inverse_heights[i]);
            let left = self.settings.main_hand_left != (hand == Hand::Off);
            let Some(stack) = item else {
                if hand == Hand::Off || self.own_invisible() {
                    continue;
                }
                let m = to_world
                    * vise_client::first_person::player_arm_pose(left, inverse_height, attack);
                self.first_person_arm(left, m, packed, &mut out);
                continue;
            };
            if let Some(id) = maps::map_id(&stack) {
                use vise_client::first_person::{MapHold, map_arm_poses, one_handed_map_pose};
                let hold = MapHold::of(hand, off_hand_empty);
                if !self.own_invisible() {
                    for (arm, pose) in map_arm_poses(hold, left, self.pitch, inverse_height, attack)
                    {
                        self.first_person_arm(arm, to_world * pose, packed, &mut out);
                    }
                }
                match hold {
                    MapHold::TwoHanded => self.two_handed_map_geometry(
                        to_world,
                        attack,
                        inverse_height,
                        id,
                        packed,
                        &mut out,
                    ),
                    MapHold::OneHanded => self.held_map(
                        to_world * one_handed_map_pose(left, inverse_height, attack),
                        id,
                        packed,
                        &mut out,
                    ),
                }
                continue;
            }
            let mut owner = self.player_owner(Some(&stack));
            owner.using_this_item &= used_hand == Some(hand);
            if hand == Hand::Off {
                owner.is_selected = false;
                owner.is_carried = false;
            }
            owner.fishing_cast_with_this_item =
                self.main_hand_rod() && self.cast_rod() == Some(&stack);
            let using = owner.using_this_item && owner.use_item_remaining_ticks > 0;
            let ticks_used =
                owner.use_duration as f32 - (owner.use_item_remaining_ticks as f32 - a + 1.0);
            use vise_client::first_person::ItemPose;
            let spear = using
                .then(|| vise_client::spear::Kinetic::of(&stack.item))
                .flatten()
                .map(|kinetic| ItemPose::Spear {
                    kinetic,
                    time_held: ticks_used,
                    ticks_since_hit: self.kinetic_hit_ticks(a),
                });
            let pose = match &*stack.item {
                _ if let Some(pose) = spear => pose,
                "minecraft:bow" if using => ItemPose::Bow { ticks_used },
                "minecraft:crossbow" => {
                    let charged = stack.data().is_some_and(|d| !d.charged.is_empty());
                    ItemPose::Crossbow {
                        ticks_used: (using && !charged).then_some(ticks_used),
                        charge_duration: owner.crossbow_charge_duration as f32,
                        charged: charged && hand == Hand::Main,
                    }
                }
                "minecraft:trident" if using => ItemPose::Trident { ticks_used },
                "minecraft:brush" if using => ItemPose::Brush {
                    remaining: owner.use_item_remaining_ticks as f32,
                    partial: a,
                },
                _ if using => {
                    let components = self
                        .pack
                        .item_renderer
                        .models
                        .components(&vise_client::item_render::stack_view(&stack));
                    let animation = components
                        .get("minecraft:consumable")
                        .map(vise_client::first_person::consumable_animation);
                    if matches!(
                        animation,
                        Some(
                            vise_world::consumable::UseAnimation::Eat
                                | vise_world::consumable::UseAnimation::Drink
                        )
                    ) {
                        ItemPose::Eat {
                            remaining: owner.use_item_remaining_ticks as f32,
                            duration: owner.use_duration as f32,
                            partial: a,
                        }
                    } else {
                        ItemPose::Using
                    }
                }
                _ if spinning => ItemPose::SpinAttack,
                _ if self
                    .swing
                    .current
                    .is_some_and(|c| c.hand == hand && c.animation.kind == 2) =>
                {
                    ItemPose::Stab
                }
                _ => ItemPose::Swing,
            };
            let m = to_world
                * vise_client::first_person::item_pose(left, inverse_height, attack, pose);
            let (context, seed) = if left {
                (
                    vise_assets::item_model::DisplayContext::FirstPersonLeftHand,
                    3,
                )
            } else {
                (
                    vise_assets::item_model::DisplayContext::FirstPersonRightHand,
                    4,
                )
            };
            let state = self.resolve_item(&stack, context, Some(&owner), seed);
            let light = self.world.light(
                eye[0].floor() as i32,
                eye[1].floor() as i32,
                eye[2].floor() as i32,
            );
            self.submit_item_world(
                &state,
                m,
                vise_client::block_entity_render::vertex_light(light),
                &mut out,
            );
        }
        out
    }
    fn new_renderer(&self, window: Arc<Window>) -> Result<Renderer> {
        self.renderer_for(window, &self.pack, &self.gui)
    }
    fn renderer_for(
        &self,
        window: Arc<Window>,
        pack: &PackResources,
        gui: &UiResources,
    ) -> Result<Renderer> {
        build_renderer(
            window,
            self.options.offscreen,
            pack,
            gui,
            self.renderer.as_ref(),
            None,
            self.settings.vise.api,
        )
    }
    fn apply_settings(&mut self) {
        let abilities = self.chat_abilities();
        self.ui.chat.set_abilities(&self.gui.atlas.font, abilities);
        self.streaming.radius = self.settings.render_distance;
        let (s, st) = (&self.settings, &self.streaming);
        let remesh = (s.biome_blend, s.smooth_lighting, s.cutout_leaves)
            != (st.biome_blend, st.smooth_lighting, st.cutout_leaves);
        self.streaming.biome_blend = self.settings.biome_blend;
        self.streaming.smooth_lighting = self.settings.smooth_lighting;
        self.streaming.cutout_leaves = self.settings.cutout_leaves;
        if remesh {
            self.all_sections_changed();
        }
        self.audio.master = self.settings.master_volume;
        if self.settings.sound_device != self.audio.device() {
            let _ = self.audio.set_device(&self.settings.sound_device);
        }
        vise_client::hud::GUI_SCALE.store(
            self.settings.gui_scale,
            std::sync::atomic::Ordering::Relaxed,
        );
        self.apply_ui_settings();
    }
    fn all_sections_changed(&mut self) {
        for &(x, z) in self.world.chunks.keys() {
            for sy in 0..self.world.layout().height() / 16 {
                self.world.dirty_sections.insert((x, sy, z));
            }
        }
    }
    fn precipitation(&self, x: i32, y: i32, z: i32) -> Option<bool> {
        let chunk = self.world.chunks.get(&(x >> 4, z >> 4))?;
        self.res
            .biomes()
            .get(chunk.biome(x, y, z))
            .snows_at(x, y, z)
    }
    fn raining_at(&self, p: BlockPos) -> bool {
        self.world.weather.levels(1.0).0 > 0.2
            && self.world.light(p.x, p.y, p.z) >> 4 == 15
            && self.rain_height(p.x, p.z) <= p.y
            && self.precipitation(p.x, p.y, p.z) == Some(false)
    }
    fn rain_height(&self, x: i32, z: i32) -> i32 {
        render_tick::motion_blocking_height(&self.world, &self.pack.assets.properties, x, z)
    }
    fn tick_weather_effects(&mut self) {
        if self.flash > 0 {
            self.flash -= 1;
        }
    }
    fn weather_geometry(
        &mut self,
        camera: glam::DVec3,
        alpha: f64,
    ) -> Vec<vise_client::weather::Vertex> {
        use vise_client::weather;
        let (intensity, _) = self.world.weather.levels(alpha as f32);
        if intensity <= 0.0 {
            return Vec::new();
        }
        let atlas = &self.gui.atlas;
        let (Some(rain_tex), Some(snow_tex)) = (
            atlas.sprites.get("environment/rain").copied(),
            atlas.sprites.get("environment/snow").copied(),
        ) else {
            return Vec::new();
        };
        let (cx, cy, cz) = (
            camera.x.floor() as i32,
            camera.y.floor() as i32,
            camera.z.floor() as i32,
        );
        let radius = self.settings.weather_radius.clamp(0, 15);
        let eye = camera.to_array();
        let mut rain = Vec::new();
        let mut snow_vertices = Vec::new();
        for z in cz.saturating_sub(radius)..=cz.saturating_add(radius) {
            for x in cx.saturating_sub(radius)..=cx.saturating_add(radius) {
                let top = self.rain_height(x, z);
                let y0 = cy.saturating_sub(radius).max(top);
                let y1 = cy.saturating_add(radius).max(top);
                if y1 == y0 {
                    continue;
                }
                let Some(snow) = self.precipitation(x, cy, z) else {
                    continue;
                };
                let parameters = self.weather_parameters.get(x, z);
                let light = self.world.light(x, top.max(cy), z);
                let coords = i32::from(light & 15) << 4 | i32::from(light >> 4) << 20;
                let lc = if snow {
                    let (block, sky) = (coords >> 4 & 15, coords >> 20 & 15);
                    ((block * 3 + 15) / 4) << 4 | ((sky * 3 + 15) / 4) << 20
                } else {
                    coords
                };
                let column = weather::Column {
                    x,
                    z,
                    bottom_y: y0,
                    top_y: y1,
                    u_offset: 0.0,
                    v_offset: 0.0,
                    light_coords: lc,
                };
                let r = if snow { snow_tex } else { rain_tex };
                let vertices = weather::vertices(
                    &column,
                    parameters,
                    snow,
                    eye,
                    radius,
                    intensity,
                    [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0],
                );
                if snow {
                    snow_vertices.extend(vertices);
                } else {
                    rain.extend(vertices);
                }
            }
        }
        rain.extend(snow_vertices);
        rain
    }
    fn tell_sleeper(&mut self, key: &str, fallback: &str, overlay: bool) {
        if self.lan_acting.is_some() {
            self.guest_packets
                .push(vise_protocol::packets::play::Clientbound::SystemChat(
                    vise_protocol::packets::play::SystemChat {
                        content: vise_protocol::text::translatable(key, &[]),
                        overlay,
                    },
                ));
        } else if overlay {
            self.message = Some((self.translate(key, fallback), Instant::now()));
        } else {
            self.system_message(vise_client::text::Text::translate(key, &[]));
        }
    }
    fn try_sleep(&mut self, pos: [i32; 3]) {
        let Some((position, yaw)) = self.player.as_ref().map(|p| (p.position, p.yaw)) else {
            return;
        };
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let props = self
            .pack
            .assets
            .registry
            .state(state)
            .map(|s| s.properties.clone())
            .unwrap_or_default();
        let d = facing_offset(props.get("facing").map_or("north", String::as_str));
        let (head, foot) = if props.get("part").map(String::as_str) == Some("foot") {
            ([pos[0] + d[0], pos[1], pos[2] + d[2]], pos)
        } else {
            (pos, [pos[0] - d[0], pos[1], pos[2] - d[2]])
        };
        let head_state = self.world.block(head[0], head[1], head[2]);
        let occupied = self
            .pack
            .assets
            .registry
            .state(head_state)
            .is_some_and(|s| s.properties.get("occupied").is_some_and(|v| v == "true"));
        if occupied {
            if !self.kick_villager_out_of_bed(head) {
                self.tell_sleeper("block.minecraft.bed.occupied", "This bed is occupied", true);
            }
            return;
        }
        let reachable = |bed: [i32; 3]| {
            (position.x - f64::from(bed[0]) - 0.5).abs() <= 3.0
                && (position.y - f64::from(bed[1])).abs() <= 2.0
                && (position.z - f64::from(bed[2]) - 0.5).abs() <= 3.0
        };
        if !reachable(head) && !reachable(foot) {
            self.tell_sleeper(
                "block.minecraft.bed.too_far_away",
                "You may not rest now; the bed is too far away",
                true,
            );
            return;
        }
        let blocked = [head, foot].iter().any(|bed| {
            let f = self
                .res
                .facts()
                .get(self.world.block(bed[0], bed[1] + 1, bed[2]));
            f.full_collision && f.solid_render
        });
        if blocked {
            self.tell_sleeper(
                "block.minecraft.bed.obstructed",
                "This bed is obstructed",
                true,
            );
            return;
        }
        if self.bed_sets_spawn(state) {
            let config = respawn_anchor::RespawnConfig::wrapped(self.dim, head, yaw, false);
            let moved = self
                .respawn_config
                .is_none_or(|c| (c.dim, c.pos) != (config.dim, config.pos));
            self.respawn_config = Some(config);
            if moved {
                self.tell_sleeper("block.minecraft.set_spawn", "Respawn point set", false);
            }
            self.spawn = if self.lan_acting.is_some() {
                None
            } else {
                self.level
                    .world_spawn
                    .map(|[x, y, z]| {
                        Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5)
                    })
                    .or(self.spawn)
            };
        }
        if !(self.dim == Dim::Overworld && self.sky_darken() >= 4) {
            self.tell_sleeper(
                "block.minecraft.bed.no_sleep",
                "You can sleep only at night or during thunderstorms",
                true,
            );
            return;
        }
        let monsters = !self.creative
            && self.mobs.iter().any(|m| {
                vise_client::mobs::ai::is_monster(m.species())
                    && (m.species() != vise_client::mobs::Species::ZombifiedPiglin
                        || (m.extra.anger > 0 && m.ai.target.is_some()))
                    && m.alive()
                    && (m.position.x - position.x).abs() < 8.0
                    && (m.position.y - position.y).abs() < 5.0
                    && (m.position.z - position.z).abs() < 8.0
            });
        if monsters {
            self.tell_sleeper(
                "block.minecraft.bed.not_safe",
                "You may not rest now; there are monsters nearby",
                true,
            );
            return;
        }
        if self.ui.rules.int("players_sleeping_percentage") > 100 {
            self.tell_sleeper(
                "sleep.not_possible",
                "No amount of rest can pass this night",
                true,
            );
        }
        self.stop_mining();
        self.left = false;
        self.right = false;
        self.using = None;
        self.sleeping = Some(head);
        self.sleep_timer = 0;
        self.occupy_bed(head);
        self.stats.custom("sleep_in_bed", 1);
        self.trigger_player("minecraft:slept_in_bed");
        self.stats.reset("custom", "minecraft:time_since_rest");
    }
    fn wake_up(&mut self) {
        if self.remote.is_some() {
            self.remote_stop_sleeping();
            return;
        }
        self.sleeping = None;
        self.sleep_timer = 100;
    }
    fn tick_sleep(&mut self) {
        self.sleep_timer = tick_sleep_timer(self.sleep_timer, self.sleeping.is_some());
        if let Some(head) = self.sleeping {
            let bed = self.world.block(head[0], head[1], head[2]);
            let bed_gone = !self.res.facts().get(bed).name.ends_with("_bed");
            if (self.screen != Screen::Chat && self.keys.contains(&KeyCode::ShiftLeft))
                || bed_gone
                || self
                    .player
                    .as_ref()
                    .is_none_or(|p| p.health.dead_or_dying())
            {
                self.wake_up();
                return;
            }
            if let Some(p) = self.player.as_mut() {
                p.position = Vec3::new(
                    f64::from(head[0]) + 0.5,
                    f64::from(head[1]) + 0.5625 + 0.125,
                    f64::from(head[2]) + 0.5,
                );
                p.velocity = Vec3::ZERO;
            }
        }
        if self.enough_slept() {
            if self.ui.rules.bool("advance_time") {
                let t = self.world.day_time;
                self.world.day_time = t + 24000 - t.rem_euclid(24000);
            }
            if self.ui.rules.bool("advance_weather") && self.world.weather.levels(1.0).0 > 0.2 {
                self.world.weather.raining = false;
                self.world.weather.thundering = false;
                self.world.weather.rain_time = 0;
                self.world.weather.thunder_time = 0;
            }
            if self.sleeping.is_some() {
                self.wake_up();
            }
            self.wake_guests();
            self.sleep_seen.1 = 0;
        }
    }
    fn creative_origin(batch: &HudBatch) -> (f32, f32) {
        batch.centered_origin(Creative::WIDTH, Creative::HEIGHT)
    }
    fn creative_tab_title(&self, tab: &vise_client::creative::Tab) -> String {
        vise_client::creative::item_group_key(&tab.id)
            .map_or_else(|| tab.title.clone(), |key| self.translate(key, &tab.title))
    }
    fn tooltip(&self, batch: &mut HudBatch, text: &str, x: f32, y: f32) {
        let atlas = &self.gui.atlas;
        let w = HudBatch::text_width(atlas, text);
        let (x, y) = (x + 12.0, y - 12.0);
        batch.fill(atlas, x - 3.0, y - 4.0, w + 6.0, 16.0, rgba(16, 0, 16, 240));
        batch.fill(atlas, x - 3.0, y - 3.0, w + 6.0, 1.0, rgba(80, 0, 255, 80));
        batch.fill(atlas, x - 3.0, y + 10.0, w + 6.0, 1.0, rgba(40, 0, 127, 80));
        batch.text(atlas, text, x, y, u32::MAX, true);
    }
    fn creative_tooltip_tabs(
        &self,
        stack: &ItemStack,
        grid: bool,
    ) -> Vec<(vise_client::text::Text, u32)> {
        let screen = &self.creative_screen;
        let tab = screen.tab();
        let category = !tab.search && !tab.inventory;
        if !vise_client::creative::tooltip_lists_tabs(category, grid) {
            return Vec::new();
        }
        let blue = rgba(85, 85, 255, 255);
        let dark_purple = rgba(170, 0, 170, 255);
        let mut lines: Vec<(vise_client::text::Text, u32)> = screen
            .tabs_containing(stack)
            .into_iter()
            .map(|i| {
                let title = self.creative_tab_title(&screen.tabs[i]);
                (vise_client::text::Text::literal(title), blue)
            })
            .collect();
        if tab.search && grid {
            for tag in screen.search_tag_lines(stack) {
                lines.push((vise_client::text::Text::literal(tag), dark_purple));
            }
        }
        lines
    }
    fn draw_creative(&mut self, batch: &mut HudBatch) {
        let (ox, oy) = Self::creative_origin(batch);
        let selected = self.creative_screen.selected;
        let tabs = self.creative_screen.tab_headers();
        let tab = &tabs[selected.min(tabs.len() - 1)];
        let tab_sprite = |t: &vise_client::creative::Tab, on: bool| {
            format!(
                "gui/sprites/container/creative_inventory/tab_{}_{}_{}",
                if t.top { "top" } else { "bottom" },
                if on { "selected" } else { "unselected" },
                t.column + 1
            )
        };
        batch.fill(
            &self.gui.atlas,
            0.0,
            0.0,
            batch.width,
            batch.height,
            rgba(16, 16, 16, 160),
        );
        for (i, t) in tabs.iter().enumerate() {
            if i != selected {
                let (tx, ty) = Creative::tab_position(t);
                batch.sprite(&self.gui.atlas, &tab_sprite(t, false), ox + tx, oy + ty);
                let mouse = (self.cursor.0 - ox, self.cursor.1 - oy);
                if Creative::tab_wants_hand(t, false, mouse) {
                    vise_client::widgets::request_cursor(
                        vise_client::widgets::Cursor::PointingHand,
                    );
                }
            }
        }
        let background = if tab.search {
            "gui/container/creative_inventory/tab_item_search"
        } else if tab.inventory {
            "gui/container/creative_inventory/tab_inventory"
        } else {
            "gui/container/creative_inventory/tab_items"
        };
        if let Some(mut r) = self.gui.atlas.sprites.get(background).copied() {
            r.u1 = r.u0 + (r.u1 - r.u0) * Creative::WIDTH / r.w as f32;
            r.v1 = r.v0 + (r.v1 - r.v0) * Creative::HEIGHT / r.h as f32;
            batch.rect_uv(ox, oy, Creative::WIDTH, Creative::HEIGHT, r, u32::MAX);
        }
        let (tx, ty) = Creative::tab_position(tab);
        batch.sprite(&self.gui.atlas, &tab_sprite(tab, true), ox + tx, oy + ty);
        for t in &tabs {
            let (tx, ty) = Creative::tab_position(t);
            let stack = ItemStack {
                item: Arc::from(t.icon.as_str()),
                count: 1,
                damage: 0,
                extra: None,
            };
            self.draw_item(
                batch,
                &stack,
                ox + tx + 5.0,
                oy + ty + if t.top { 9.0 } else { 7.0 },
            );
        }
        let atlas = &self.gui.atlas;
        if !tab.inventory {
            let title = self.creative_tab_title(tab);
            batch.text(
                atlas,
                &title,
                ox + 8.0,
                oy + 6.0,
                rgba(64, 64, 64, 255),
                false,
            );
        }
        if tab.search {
            let search = &mut self.creative_screen.search;
            search.x = ox + 82.0;
            search.y = oy + 6.0;
            search.width = 80.0;
            search.bordered = false;
            search.focused = true;
            search.text_color = u32::MAX;
            search.max_length = clipboard::CREATIVE_SEARCH_CHARS;
            search.draw(batch, atlas, None);
        }
        let mut hovered: Option<(ItemStack, bool)> = None;
        if tab.inventory {
            self.draw_player_preview(
                batch,
                ox + 73.0,
                oy + 6.0,
                ox + 105.0,
                oy + 49.0,
                20.0,
                0.0625,
            );
            self.draw_inventory_slots(batch, ox, oy);
            if Self::creative_trash_hovered(batch, self.cursor) {
                let (x, y) = Creative::TRASH_POSITION;
                batch.fill(
                    &self.gui.atlas,
                    ox + x,
                    oy + y,
                    16.0,
                    16.0,
                    rgba(255, 255, 255, 128),
                );
            }
            if let Some(slot) = self.slot_at(batch, self.cursor.0, self.cursor.1) {
                let (x, y) = self.slot_position(slot);
                batch.fill(
                    &self.gui.atlas,
                    ox + x,
                    oy + y,
                    16.0,
                    16.0,
                    rgba(255, 255, 255, 128),
                );
            }
        } else {
            let items = self.creative_screen.items(&self.pack.names);
            let first = self.creative_screen.first_row(&self.pack.names);
            for r in 0..5 {
                for c in 0..9 {
                    let Some(stack) = items.get((first + r) * 9 + c).filter(|s| s.count > 0) else {
                        continue;
                    };
                    let (x, y) = (ox + 9.0 + c as f32 * 18.0, oy + 18.0 + r as f32 * 18.0);
                    self.draw_item(batch, stack, x, y);
                    if self.cursor.0 >= x
                        && self.cursor.0 < x + 16.0
                        && self.cursor.1 >= y
                        && self.cursor.1 < y + 16.0
                    {
                        batch.fill(&self.gui.atlas, x, y, 16.0, 16.0, rgba(255, 255, 255, 128));
                        hovered = Some((stack.clone(), true));
                    }
                }
            }
            let drag = self.drag_view();
            for c in 0..9 {
                let (x, y) = (ox + 9.0 + c as f32 * 18.0, oy + 112.0);
                self.draw_slot_item(batch, drag.as_ref(), c, &self.inventory[c], x, y);
                if self.cursor.0 >= x
                    && self.cursor.0 < x + 16.0
                    && self.cursor.1 >= y
                    && self.cursor.1 < y + 16.0
                {
                    batch.fill(&self.gui.atlas, x, y, 16.0, 16.0, rgba(255, 255, 255, 128));
                    hovered = self.inventory[c].clone().map(|s| (s, false));
                }
            }
            let scrollable = self.creative_screen.rows(&self.pack.names) > 5;
            let scroller = if scrollable {
                "gui/sprites/container/creative_inventory/scroller"
            } else {
                "gui/sprites/container/creative_inventory/scroller_disabled"
            };
            batch.sprite(
                &self.gui.atlas,
                scroller,
                ox + Creative::SCROLL_X,
                oy + Creative::SCROLL_Y + self.creative_screen.scroller_offset(),
            );
        }
        for t in &tabs {
            let (tx, ty) = Creative::tab_position(t);
            if self.cursor.0 >= ox + tx
                && self.cursor.0 < ox + tx + 26.0
                && self.cursor.1 >= oy + ty
                && self.cursor.1 < oy + ty + 32.0
            {
                self.tooltip(
                    batch,
                    &self.creative_tab_title(t),
                    self.cursor.0,
                    self.cursor.1,
                );
            }
        }
        self.draw_carried(batch);
        if tab.inventory {
            if Self::creative_trash_hovered(batch, self.cursor) {
                let destroy = self.translate("inventory.binSlot", "Destroy Item");
                self.tooltip(batch, &destroy, self.cursor.0, self.cursor.1);
            } else if self.carried.is_none()
                && let Some(slot) = self.slot_at(batch, self.cursor.0, self.cursor.1)
                && let Some(stack) = self.slot_view(slot)
            {
                let tabs = self.creative_tooltip_tabs(&stack, false);
                self.draw_stack_tooltip_with(
                    batch,
                    &stack,
                    false,
                    self.cursor.0,
                    self.cursor.1,
                    tabs,
                );
            }
        } else if self.carried.is_none()
            && let Some((item, grid)) = hovered
        {
            let tabs = self.creative_tooltip_tabs(&item, grid);
            self.draw_stack_tooltip_with(batch, &item, grid, self.cursor.0, self.cursor.1, tabs);
        }
        self.draw_inventory_effects(batch, (ox, oy), Creative::WIDTH as i32);
    }
    fn creative_trash_hovered(batch: &HudBatch, cursor: (f32, f32)) -> bool {
        let (ox, oy) = Self::creative_origin(batch);
        let (x, y) = Creative::TRASH_POSITION;
        cursor.0 >= ox + x
            && cursor.0 < ox + x + 16.0
            && cursor.1 >= oy + y
            && cursor.1 < oy + y + 16.0
    }
    fn click_creative(&mut self, right: bool) {
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let batch = HudBatch::new(size);
        let (ox, oy) = Self::creative_origin(&batch);
        let (cx, cy) = self.cursor;
        let tabs = self.creative_screen.tabs.clone();
        for (i, t) in tabs.iter().enumerate() {
            let (tx, ty) = Creative::tab_position(t);
            if cx >= ox + tx && cx < ox + tx + 26.0 && cy >= oy + ty && cy < oy + ty + 32.0 {
                select_creative_tab(&mut self.creative_screen, &mut self.inv_clicks, i);
                self.refresh_saved_hotbars();
                self.inv_drag = None;
                self.last_inv_click = None;
                return;
            }
        }
        if self.creative_inventory_open() {
            if Self::creative_trash_hovered(&batch, self.cursor) {
                if self.has_shift_down() {
                    let before = self.player_equipment();
                    self.inventory.fill(None);
                    self.armor.fill(None);
                    self.offhand = None;
                    self.craft_grid[..4].fill(None);
                    self.player_equipment_changed(&before);
                    self.apply_equipment();
                } else {
                    self.carried = None;
                }
                self.inv_drag = None;
                self.inv_clicks = Default::default();
            } else if self.slot_at(&batch, cx, cy).is_some() || self.clicked_outside(&batch) {
                self.inventory_press(right);
            }
            return;
        }
        if !right && self.creative_search_click(ox, oy) {
            return;
        }
        if !right
            && self
                .creative_screen
                .press_scrollbar(cx - ox, cy - oy, &self.pack.names)
        {
            return;
        }
        if self.slot_at(&batch, cx, cy).is_some() || self.clicked_outside(&batch) {
            self.inventory_press(right);
        }
    }
    fn creative_search_click(&mut self, ox: f32, oy: f32) -> bool {
        if !self.creative_screen.tab().search {
            return false;
        }
        let (x, y, w, h) = (ox + 82.0, oy + 6.0, 80.0, 9.0);
        let (mx, my) = self.cursor;
        if mx < x || my < y || mx >= x + w || my >= y + h {
            return false;
        }
        let shift = self.has_shift_down();
        let search = &mut self.creative_screen.search;
        search.x = x;
        search.y = y;
        search.width = w;
        search.click(&self.gui.atlas, mx, shift, false);
        true
    }
    pub(super) fn sync_level(&mut self) {
        self.level.game_rules = self.ui.rules.by_name();
        self.level.selected_slot = self.selected as i32;
        let mode = self.mode_id();
        let vehicle = self.root_vehicle_uuid(self.riding);
        let level = &mut self.level;
        level.creative = self.creative;
        level.game_mode = mode;
        level.previous_game_mode = self.previous_mode.map(game_mode::mode_id_of);
        level.difficulty =
            vise_client::title::DIFFICULTIES[usize::from(self.ui.difficulty) & 3].into();
        level.time = self.world.game_time;
        level.weather = self.world.weather;
        level.day_time = self.world.day_time;
        level.spawn = self.spawn.map(|s| [s.x, s.y, s.z]);
        level.respawn_elsewhere = self.respawn_config.map(|c| c.saved());
        level.rotation = [self.yaw, self.pitch];
        level.xp_level = self.experience.level;
        level.xp_progress = self.experience.progress;
        level.xp_total = self.experience.total;
        level.root_vehicle = vehicle.or(self.pending_vehicle);
        if let Some(p) = &self.player {
            level.position = Some([p.position.x, p.position.y, p.position.z]);
            level.health = p.health.health();
            level.absorption = p.health.absorption();
            let extra = level
                .player_extra
                .get_or_insert_with(|| vise_world::nbt::Tag::Compound(Default::default()));
            if let vise_world::nbt::Tag::Compound(map) = extra {
                map.insert(
                    "Fire".into(),
                    vise_world::nbt::Tag::Short(
                        p.fire_ticks.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16,
                    ),
                );
                map.insert("Air".into(), vise_world::nbt::Tag::Short(p.air as i16));
            }
            level.fall_distance = p.fall_distance;

            level.food = p.health.food.food;
            level.saturation = p.health.food.saturation;
            level.flying = p.flying;
        }
        level.inventory = self
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                s.as_ref()
                    .map(|s| (i as u8, s.item.to_string(), s.count, s.damage))
            })
            .collect();
        level.inventory_components = self
            .inventory
            .iter()
            .enumerate()
            .filter_map(|(i, s)| Some((i as u8, s.as_ref()?.extra.clone()?)))
            .collect();
        if let Some(s) = &self.offhand {
            level
                .inventory
                .push((150, s.item.to_string(), s.count, s.damage));
            if let Some(extra) = s.extra.clone() {
                level.inventory_components.insert(150, extra);
            }
        }
        for (i, s) in self.armor.iter().enumerate() {
            if let Some(s) = s {
                level
                    .inventory
                    .push((103 - i as u8, s.item.to_string(), s.count, s.damage));
                if let Some(extra) = s.extra.clone() {
                    level.inventory_components.insert(103 - i as u8, extra);
                }
            }
        }
        level.dimension = self.dim.id().to_owned();
        if let Some(key) = self.ender_open
            && let Some(BlockEntity::Chest(items)) = self.world.block_entities.get(&key)
        {
            self.ender_items = items.clone();
        }
        level.ender_items = self.ender_items.clone();
        level.active_effects = self.potions.effects.saved();
    }
    fn save_game(&mut self) -> Result<()> {
        self.save_game_with(true)
    }
    fn save_game_with(&mut self, wait: bool) -> Result<()> {
        if self.remote.is_some() {
            return Ok(());
        }
        self.save_level(wait)?;
        self.save_parked_levels(wait)?;
        self.save_game_metadata()
    }
    fn save_game_metadata(&mut self) -> Result<()> {
        self.save_maps();
        self.save_random_sequences();
        self.save_fight();
        self.save_wandering_trader();
        self.save_portal_arrivals();
        self.save_stats();
        self.save_advancements();
        self.save_scoreboard();
        self.save_bossbars();
        self.save_schedule();
        self.save_warden_tracker();
        self.sync_level();
        self.store_owner();
        self.level.write(&self.world_root)?;
        self.save_world_state();
        self.message = Some(("Saved the game".into(), Instant::now()));
        Ok(())
    }
    fn save_level(&mut self, wait: bool) -> Result<()> {
        self.save.save_forced_chunks(&self.streaming.forced)?;
        self.streaming.save(&mut self.world, wait);
        let keys: Vec<_> = self.world.modified.drain().collect();
        let chunks = self.with_entities(keys.iter().filter_map(|k| self.world.chunks.get(k)));
        self.save
            .save_later(&chunks, self.pack.assets.registry, self.res.biomes())?;
        self.save_entities(false)?;
        self.save_raids();
        if wait {
            self.save.flush()?;
        }
        Ok(())
    }
    fn tick_world_icon(&mut self) {
        let Some(icon) = self.world_icon.as_ref() else {
            return;
        };
        if icon
            .last_attempt
            .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        if icon.path.is_file() {
            self.world_icon = None;
            return;
        }
        if self.streaming.pending() != 0
            || !self.pending_uploads.is_empty()
            || self.remote_backlog() != 0
        {
            return;
        }
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        if renderer.drawn_sections <= 10 {
            return;
        }
        if renderer.screenshot.is_some() {
            return;
        }
        let path = icon.path.clone();
        if let Some(icon) = self.world_icon.as_mut() {
            icon.last_attempt = Some(Instant::now());
        }
        renderer.screenshot = Some(vise_client::gfx::ScreenshotRequest {
            path,
            size: Some([64, 64]),
        });
    }
    fn render(&mut self) -> Result<bool> {
        self.glint_frame = self.capture_glint_frame();
        if self.renderer.as_mut().is_some_and(|r| r.display_changed()) {
            self.apply_ui_settings();
        }
        if self.integrated_starting.is_some() {
            if let Some(renderer) = &mut self.renderer {
                let mut hud = HudBatch::new([renderer.config.width, renderer.config.height]);
                let (w, h) = (hud.width, hud.height);
                vise_client::ui::menu_background(
                    &mut hud,
                    &self.gui.atlas,
                    0.0,
                    0.0,
                    w,
                    h,
                    false,
                );
                vise_client::level_loading::draw(&mut hud, &self.gui.atlas, None, None);
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        if let Some(loader) = &self.world_loader {
            if let Some(renderer) = &mut self.renderer {
                let mut hud = HudBatch::new([renderer.config.width, renderer.config.height]);
                vise_client::level_loading::draw_progress(
                    &mut hud,
                    &self.gui.atlas,
                    if loader.cancelled {
                        "Cancelling world"
                    } else {
                        "Opening world"
                    },
                    0.0,
                );
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        if self.pending_entry.is_some() {
            if let (Some(renderer), Some(loader)) = (&mut self.renderer, &self.rest_loader) {
                let mut hud = HudBatch::new([renderer.config.width, renderer.config.height]);
                vise_client::level_loading::draw_progress(
                    &mut hud,
                    &self.gui.atlas,
                    loader.progress.line(),
                    loader.progress.fraction(),
                );
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        if let Some(reload) = &self.pack_reload {
            if let Some(renderer) = &mut self.renderer {
                let mut hud = HudBatch::new([renderer.config.width, renderer.config.height]);
                vise_client::level_loading::draw_progress(
                    &mut hud,
                    &self.gui.atlas,
                    reload.progress.line(),
                    reload.progress.fraction(),
                );
                return renderer.render(&loading_frame(&hud));
            }
            return Ok(false);
        }
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return Ok(false);
        };
        let alpha = self.clock.fraction();
        let camera_alpha = self.clock.camera_fraction();
        let view = self.camera_view(camera_alpha as f32);
        let (camera, underwater, view_yaw, view_pitch) = match view {
            Some((c, yaw, pitch)) => {
                let s =
                    self.world
                        .block(c.x.floor() as i32, c.y.floor() as i32, c.z.floor() as i32);
                (c, self.res.facts().get(s).fluid == Fluid::Water, yaw, pitch)
            }
            None => (
                glam::DVec3::new(0.5, 100.0, 0.5),
                false,
                self.yaw,
                self.pitch,
            ),
        };
        self.audio
            .set_listener([camera.x, camera.y, camera.z], view_yaw, view_pitch);
        let block = [
            camera.x.floor() as i32,
            camera.y.floor() as i32,
            camera.z.floor() as i32,
        ];
        let cave_culling = self.settings.vise.cave_culling
            && !self
                .res
                .facts()
                .get(self.world.block(block[0], block[1], block[2]))
                .solid_render;
        if cave_culling
            && self.title.is_none()
            && let Some(renderer) = self.renderer.as_mut()
        {
            vise_client::spans::time("occlusion", || {
                renderer.start_occlusion(camera, (self.streaming.radius * 16) as f32)
            });
        }
        let biome = self
            .world
            .chunks
            .get(&(block[0] >> 4, block[2] >> 4))
            .map(|c| c.biome(block[0], block[1], block[2]))
            .unwrap_or(0);
        let environment = self.dim_environment();
        let biomes = self.res.biomes();
        let weights = {
            let chunks = &self.world.chunks;
            let plains = biomes.id("minecraft:plains");
            vise_client::biome::gaussian_weights(camera.to_array(), |qx, qy, qz| {
                chunks
                    .get(&(qx >> 2, qz >> 2))
                    .map_or(plains, |c| c.biome(qx * 4, qy * 4, qz * 4))
            })
        };
        let blend = |pick: fn(&vise_client::biome::BiomeInfo) -> Option<u32>, base: [f32; 3]| {
            vise_client::biome::blend_rgb(&weights, |b| {
                pick(biomes.get(b)).map_or(base, vise_client::biome::rgb)
            })
        };
        let biome_sky = blend(
            |b| b.sky,
            environment.base_rgb("minecraft:visual/sky_color"),
        );
        let biome_fog = blend(
            |b| b.fog,
            environment.base_rgb("minecraft:visual/fog_color"),
        );
        let water_fog = blend(|b| Some(b.water_fog), [0.0; 3]);
        let mut sky = environment.sample(self.world.day_time, Some(biome_sky), Some(biome_fog));
        let (rain, thunder) = if self.dim == Dim::Overworld {
            self.world.weather.levels(alpha as f32)
        } else {
            (0.0, 0.0)
        };
        vise_client::environment::apply_weather(&mut sky, rain, thunder);
        self.end_flash_sky(&mut sky, alpha);
        if self.flash > 0 && !self.settings.file.bool("hideLightningFlashes") {
            let flash = [0.8, 0.8, 1.0];
            sky.sky_color =
                std::array::from_fn(|i| sky.sky_color[i] + (flash[i] - sky.sky_color[i]) * 0.22);
            sky.sky_light_factor = 1.0;
        }
        let mut lines = self.fishing_line(alpha, size);
        let mut on_top = Vec::new();
        if self.show_hitboxes || self.show_chunk_borders {
            let g = self.debug_gizmos(alpha as f32);
            lines.extend(g.lines);
            on_top = g.on_top;
        }
        let view = vise_client::camera::view_matrix(view_yaw, view_pitch);
        let at = camera.to_array();
        let mut crosshair = vise_client::gizmos::Gizmos::default();
        if self.debug_crosshair() && self.camera_type.first_person() && !self.ui.hide_gui {
            let scale = HudBatch::new(size).scale;
            crosshair.debug_crosshair(at, view_pitch, view_yaw, scale);
        }
        let debug_lines = vise_client::gizmos::camera_relative(&lines, at, view);
        let debug_lines_on_top = vise_client::gizmos::camera_relative(&on_top, at, view);
        let debug_crosshair = vise_client::gizmos::camera_relative(&crosshair.on_top, at, view);
        let mut outline_after_terrain = false;
        let adventure = self.mode == vise_client::command::args::GameMode::Adventure;
        let outline = if !self.ui.hide_gui && (self.may_build() || adventure) {
            self.pick()
                .filter(|h| {
                    self.may_build()
                        || !self.block_action_restricted(h.pos)
                        || self.can_place_on_in_adventure(h.pos)
                })
                .map(|h| {
                    let state = self.world.block(h.pos[0], h.pos[1], h.pos[2]);
                    let f = self.res.facts().get(state);
                    outline_after_terrain = self
                        .pack
                        .render
                        .model_catalog
                        .collect(state, BlockPos::new(h.pos[0], h.pos[1], h.pos[2]))
                        .is_ok_and(|models| {
                            models
                                .iter()
                                .any(|(_, model)| model.faces.iter().any(|f| f.translucent))
                        });
                    let o = if f.outline_offset {
                        self.pack
                            .context
                            .tables
                            .offset(state, h.pos[0], h.pos[2])
                            .unwrap_or([0.0; 3])
                    } else {
                        [0.0; 3]
                    };
                    let p = h.pos.map(f64::from);
                    if let Some(b) = self.context_outline(h.pos, state) {
                        return vise_client::blocks::cube_edges()
                            .iter()
                            .map(|e| {
                                std::array::from_fn(|i| {
                                    let k = i % 3;
                                    p[k] + b[k] + e[i] * (b[k + 3] - b[k])
                                })
                            })
                            .collect();
                    }
                    f.outline_edges
                        .iter()
                        .map(|e| std::array::from_fn(|i| p[i % 3] + o[i % 3] + e[i]))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut hud = vise_client::spans::time("hud", || self.hud(size));
        let title = self.title.is_some()
            || self.loading_reason() == Some(vise_client::level_loading::Reason::Other)
            || self.remote_loading();
        let (yaw, pitch) = if title {
            (
                self.panorama_rotation.update(
                    self.started.elapsed().as_secs_f32(),
                    self.settings.file.double("panoramaScrollSpeed") as f32,
                ),
                10.0,
            )
        } else {
            (view_yaw, view_pitch)
        };
        let fov_degrees = if title {
            85.0
        } else {
            self.fov_degrees(camera_alpha as f32, camera.to_array())
        };
        self.locator_camera =
            (!title).then_some((camera.to_array(), view_yaw, view_pitch, fov_degrees));
        let bob = if title {
            glam::Mat4::IDENTITY
        } else {
            self.bob_matrix(camera_alpha as f32)
        };
        let spin = if title {
            glam::Mat4::IDENTITY
        } else {
            self.spinning_matrix(camera_alpha as f32)
        };
        let terrain_distance = self.terrain_distance();
        let distance = (terrain_distance * 16) as f32;
        let depth_far =
            vise_client::camera::depth_far(terrain_distance, self.settings.cloud_range);
        self.view_frustum = self
            .renderer
            .as_ref()
            .map(|r| r.camera_frustum(fov_degrees, yaw, pitch, bob * spin, depth_far));
        let mut entities = std::mem::take(&mut self.entity_vertices);
        entities.clear();
        self.see_through_vertices.clear();
        self.glowing_vertices.get_mut().clear();
        if !title {
            entities = vise_client::spans::time("entity_geometry", || {
                self.entity_geometry(camera, alpha, entities)
            });
        }
        let bolts = if title {
            Vec::new()
        } else {
            self.bolt_instances(camera)
        };
        let mut additive = Vec::new();
        if !title && self.dim == Dim::End {
            let mut dragon = Vec::new();
            self.dragon_geometry(camera, alpha, &mut dragon, &mut additive);
            self.dragon_fireball_geometry(camera, alpha, &mut dragon);
            entities.extend(dragon);
        }
        let (celestial, stars) = if title {
            (Vec::new(), None)
        } else if self.dim == Dim::Overworld
            && self.potions.effects.get("darkness").is_none()
            && self.potions.effects.get("blindness").is_none()
        {
            self.celestial_geometry(&sky, rain)
        } else if self.dim == Dim::End && self.settings.vise.sky {
            (self.end_flash_geometry(), None)
        } else {
            (Vec::new(), None)
        };
        let mut hand = if title || self.scoping() {
            Vec::new()
        } else {
            self.hand_geometry(camera_alpha)
        };
        if !title {
            let view = bob * vise_client::camera::view_matrix(yaw, pitch);
            let aspect = size[0] as f32 / size[1].max(1) as f32;
            self.item_activation_geometry(camera_alpha as f32, view, aspect, &mut hand);
        }
        let screen = if title {
            Vec::new()
        } else {
            self.screen_quads(alpha as f32, size)
        };
        if !title {
            self.end_geometry(camera, &mut entities);
        }
        let weather = if !title && self.dim == Dim::Overworld && self.settings.vise.weather {
            vise_client::spans::time("weather", || self.weather_geometry(camera, alpha))
        } else {
            Vec::new()
        };
        let border = (!title)
            .then(|| self.border_draw(camera.to_array(), alpha as f32))
            .flatten();
        let see_through = std::mem::take(&mut self.see_through_vertices);
        if title {
            self.dynamic_lights.clear();
        } else {
            self.gather_dynamic_lights(camera, alpha);
        }
        let pack_inputs = if title {
            Default::default()
        } else {
            self.shader_pack_inputs(camera, alpha as f32)
        };
        hud.patches.append(&mut self.map_patches);
        let entity_spans = std::mem::take(self.entity_spans.get_mut());
        let glowing = std::mem::take(self.glowing_vertices.get_mut());
        let frame = Frame {
            weather: &weather,
            weather_time: vise_client::weather::Time::new(self.world.game_time, alpha as f32),
            nether_lighting: self.dim == Dim::Nether,
            camera,
            yaw,
            pitch,
            fov_degrees,
            bob,
            spin,
            sky,
            distance,
            detail_distance: (self.streaming.radius * 16) as f32,
            depth_far,
            underwater,
            title,
            sky_blocked: self.potions.effects.get("blindness").is_some()
                || self.potions.effects.get("darkness").is_some()
                || matches!(
                    vise_client::fog::fog_type(
                        &self.world,
                        self.res.facts(),
                        [camera.x, camera.y, camera.z],
                        self.forward(),
                    ),
                    vise_client::fog::FogType::Lava | vise_client::fog::FogType::PowderSnow
                ),
            skybox: self.dim == Dim::Overworld && self.settings.vise.sky,
            end_sky: self.dim == Dim::End && self.settings.vise.sky,
            end_sky_sprite: self
                .gui
                .atlas
                .sprites
                .get("environment/end_sky")
                .map(|r| [r.u0, r.v0, r.u1, r.v1]),
            environment_fog: (self.dim == Dim::Nether).then_some((sky.fog_start, sky.fog_end)),
            cave_culling,
            water_fog,
            fog: Some(self.frame_fog(camera, &sky, biome, water_fog)),
            dark_disc: self.dark_disc(camera),
            outline,
            outline_after_terrain,
            outline_high_contrast: self.settings.file.bool("highContrastBlockOutline"),
            debug_lines: &debug_lines,
            debug_lines_on_top: &debug_lines_on_top,
            debug_crosshair: &debug_crosshair,
            hud: &hud,
            glint_alpha: self.glint_frame.alpha,
            glint_ticks: self.glint_frame.scroll_ticks(),
            clouds: (!title
                && self.dim == Dim::Overworld
                && vise_client::clouds::visible(&self.settings.clouds, sky.cloud_color[3]))
            .then(|| self.gui.atlas.sprites.get("vise/white"))
            .flatten()
            .map(|white| vise_client::clouds::Input {
                texture: &self.pack.clouds,
                uv: [(white.u0 + white.u1) / 2.0, (white.v0 + white.v1) / 2.0],
                camera,
                time: self.world.game_time,
                partial_tick: alpha,
                range: self.settings.cloud_range,
                fast: self.settings.clouds == "fast",
                color: sky.cloud_color,
            }),
            entities: &entities,
            entity_spans: &entity_spans,
            glowing: &glowing,
            dynamic_lights: &self.dynamic_lights,
            lightning: vise_client::gfx::lightning::Input {
                bolts: &bolts,
                uv: vise_client::gfx::lightning::white_uv(&self.gui.atlas.font.white()),
            },
            additive: &additive,
            see_through: &see_through,
            celestial: if self.settings.vise.sky || title {
                &celestial
            } else {
                &[]
            },
            stars: stars.filter(|_| self.settings.vise.sky || title),
            hand: &hand,
            hand_fov_degrees: self.hud_fov_degrees(camera_alpha as f32, camera.to_array()),
            screen: &screen,
            pack: pack_inputs,
            border,
            game_time: self.world.game_time as f32,
            shader_game_time: ((self.world.game_time % 24000) as f32 + alpha as f32) / 24000.0,
            crumbling_sprite: self
                .gui
                .atlas
                .sprites
                .get("block/destroy_stage_0")
                .map_or([0.0; 4], |r| [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0]),
            end_portal_sprites: ["environment/end_sky", "entity/end_portal/end_portal"].map(|n| {
                self.gui
                    .atlas
                    .sprites
                    .get(n)
                    .map_or([0.0; 4], |r| [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0])
            }),
            ticks: self.ticks,
            darkness: self.darkness_lighting(alpha as f32),
            night_vision: self.night_vision_intensity(alpha as f32),
            block_flicker: self.fog.block_light_flicker,
            boss_darken: self
                .boss_overlay
                .darken_amount(self.clock.fraction() as f32),
            menu_blur: self.menu_blur(),
        };
        let result = match self.renderer.as_mut() {
            Some(renderer) => {
                renderer.set_pass_timing(self.debug);
                vise_client::spans::time("renderer", || renderer.render(&frame))
            }
            None => Err("renderer".into()),
        };
        self.entity_vertices = entities;
        *self.entity_spans.get_mut() = entity_spans;
        *self.glowing_vertices.get_mut() = glowing;
        self.see_through_vertices = see_through;
        result
    }
    fn loading_reason(&self) -> Option<vise_client::level_loading::Reason> {
        use vise_client::level_loading::Reason;
        if self.title.is_some() || self.remote.is_some() || self.player.is_some() {
            return None;
        }
        Some(
            if self.travelling.is_none() || self.respawn_travel.is_some() {
                Reason::Other
            } else if self.end_travel {
                Reason::EndPortal
            } else {
                Reason::NetherPortal
            },
        )
    }
    fn level_load_progress(&self) -> f32 {
        let (cx, cz) = self.streaming.center;
        let ready = (-3..=3)
            .flat_map(|dz| (-3..=3).map(move |dx| (cx + dx, cz + dz)))
            .filter(|&(x, z)| self.streaming.is_lit(x, z))
            .count();
        vise_client::level_loading::progress(ready)
    }
    fn draw_level_loading(
        &self,
        batch: &mut HudBatch,
        reason: vise_client::level_loading::Reason,
    ) {
        use vise_client::level_loading::{self, Reason, STATUS_RADIUS};
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        match reason {
            Reason::Other => {
                vise_client::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, false)
            }
            Reason::NetherPortal => {
                let overlay = hud_overlays::portal_overlay(w as i32, h as i32, 1.0);
                hud_layout::paint(batch, atlas, &[overlay]);
            }
            Reason::EndPortal => batch.fill(atlas, 0.0, 0.0, w, h, rgba(20, 20, 20, 255)),
        }
        let (cx, cz) = self.streaming.center;
        let status = |x: i32, z: i32| {
            self.streaming.latest_status(
                &self.world,
                cx + x - STATUS_RADIUS,
                cz + z - STATUS_RADIUS,
            )
        };
        level_loading::draw(batch, atlas, Some(&status), Some(self.loading_progress));
    }
    fn menu_blur(&self) -> Option<f32> {
        let loading = self.loading_reason() == Some(vise_client::level_loading::Reason::Other)
            || self.remote_loading();
        let blurred = loading
            || self.title.as_ref().is_some_and(|t| {
                matches!(self.screen, Screen::Options | Screen::Win)
                    || !matches!(t.mode, vise_client::title::Mode::Main)
            })
            || (self.title.is_none()
                && self.player.is_some()
                && (self.screen.blurs_background(self.pause_menu)
                    || self.dialog_open()
                    || (self.screen == Screen::Win && self.win_screen_pauses())));
        let radius = self.settings.menu_blur as f32;
        (blurred && radius >= 1.0).then_some(radius)
    }
    fn border_draw(
        &mut self,
        camera: [f64; 3],
        partial: f32,
    ) -> Option<vise_client::gfx::BorderDraw> {
        use vise_client::border_render::{self, BorderRenderer};
        let distance = f64::from(self.streaming.radius * 16);
        border_render::extract(
            &self.border,
            partial,
            camera,
            distance,
            &mut self.border_state,
        );
        let depth_far =
            vise_client::camera::depth_far(self.terrain_distance(), self.settings.cloud_range);
        if !self
            .border_renderer
            .prepare(&self.border_state, camera, distance, depth_far)
        {
            return None;
        }
        let millis = self.started.elapsed().as_millis() as i64;
        Some(vise_client::gfx::BorderDraw {
            vertices: self.border_renderer.vertices,
            rebuilt: self.border_renderer.take_rebuilt(),
            transforms: self
                .border_renderer
                .transforms(&self.border_state, camera, millis),
            sides: BorderRenderer::sides(&self.border_state, camera, distance),
        })
    }
    fn key_pressed(&mut self, key: KeyCode) -> Result<()> {
        if self.sleeping.is_some()
            && self.screen == Screen::Playing
            && !matches!(
                key,
                KeyCode::F1 | KeyCode::F2 | KeyCode::F11 | KeyCode::KeyT | KeyCode::Slash
            )
        {
            if matches!(key, KeyCode::Escape | KeyCode::ShiftLeft) {
                self.wake_up();
            }
            return Ok(());
        }
        if self.sort_key_press(key) {
            return Ok(());
        }
        if self.remote.is_some() && self.remote_key(key)? {
            return Ok(());
        }
        match key {
            KeyCode::Escape => match self.screen {
                Screen::Options => self.options_key(KeyCode::Escape, None)?,
                Screen::Playing => {
                    self.screen = Screen::Paused;
                    self.pause_menu = true;
                    self.capture(false);
                }
                Screen::Creative => {
                    self.carried = None;
                    self.screen = Screen::Playing;
                    self.capture(true);
                }
                Screen::Paused | Screen::Inventory => {
                    self.close_container();
                    self.screen = Screen::Playing;
                    self.pause_menu = true;
                    self.capture(true);
                }
                Screen::GameModeSwitcher => self.close_gamemode_switcher(false),
                Screen::Dead => {}
                Screen::Stats | Screen::Friends => self.screen = Screen::Paused,
                Screen::ServerLinks => self.screen = Screen::Paused,
                Screen::ConfirmLink | Screen::DialogWarning => self.screen = Screen::ServerLinks,
                Screen::Advancements => self.close_advancements(),
                Screen::WorldOptions => self.world_options_escape(),
                Screen::CommandBlock => self.close_command_block(),
                Screen::Chat => self.close_chat(),
                Screen::Win => self.close_win_screen()?,
            },
            KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::Digit3
            | KeyCode::Digit4
            | KeyCode::Digit5
            | KeyCode::Digit6
            | KeyCode::Digit7
            | KeyCode::Digit8
            | KeyCode::Digit9
            | KeyCode::KeyF
            | KeyCode::KeyQ
                if (self.screen == Screen::Inventory
                    || (self.screen == Screen::Creative && !self.creative_screen.tab().search))
                    && self.inventory_hotkey(key) => {}
            _ if self.screen == Screen::Creative
                && self.creative_screen.tab().search
                && self.search_tab_hotbar_key(key) => {}
            KeyCode::KeyT if self.screen == Screen::Inventory => self.recipe_focus_search(),
            KeyCode::KeyT if self.screen == Screen::Playing => self.open_chat_method(""),
            KeyCode::KeyL if self.screen == Screen::Playing => self.open_advancements(false),
            KeyCode::KeyL if self.screen == Screen::Advancements => self.close_advancements(),
            KeyCode::Slash if self.screen == Screen::Playing => self.open_chat_method("/"),
            KeyCode::F1 if self.screen == Screen::Playing => self.toggle_hide_gui(),
            KeyCode::F11 => self.toggle_fullscreen(),
            KeyCode::F5 if self.screen == Screen::Playing => self.cycle_camera(),
            KeyCode::KeyE
                if self.screen == Screen::Creative && self.creative_screen.tab().search => {}
            KeyCode::KeyE => match self.screen {
                Screen::Playing if self.spectator() => {}
                Screen::Playing => {
                    if !self.open_ridden_mount_menu() {
                        if self.tutorial.open_inventory() {
                            self.persist_tutorial_step();
                        }
                        if self.creative {
                            self.screen = Screen::Creative;
                            self.refresh_creative_tabs();
                            self.creative_screen.reopen();
                            self.refresh_saved_hotbars();
                        } else {
                            self.screen = Screen::Inventory;
                        }
                        self.capture(false);
                    }
                }
                Screen::Creative => {
                    if let Some(c) = self.carried.take() {
                        let _ = self.give(c);
                    }
                    self.screen = Screen::Playing;
                    self.capture(true);
                }
                Screen::Inventory => {
                    self.close_container();
                    self.screen = Screen::Playing;
                    self.capture(true);
                }
                _ => {}
            },
            KeyCode::KeyF if self.screen == Screen::Playing && self.remote.is_some() => {
                if !self.spectator()
                    && let Some(r) = self.remote.as_mut()
                {
                    r.pending_keys.push(key);
                }
            }
            KeyCode::KeyF if self.screen == Screen::Playing => {
                std::mem::swap(&mut self.offhand, &mut self.inventory[self.selected]);
                self.hand_stack_replaced(self.selected);
                self.hand_stack_replaced(shield::OFFHAND_SLOT);
                let (offhand, main) = (self.offhand.clone(), self.inventory[self.selected].clone());
                self.player_on_equip("offhand", main.as_ref(), offhand.as_ref());
                self.player_on_equip("mainhand", offhand.as_ref(), main.as_ref());
                self.using = None;
            }
            KeyCode::KeyQ if self.screen == Screen::Playing => {
                let all = drop_whole_stack(&self.physical_keys);
                self.drop_held(all);
            }
            KeyCode::Digit1
            | KeyCode::Digit2
            | KeyCode::Digit3
            | KeyCode::Digit4
            | KeyCode::Digit5
            | KeyCode::Digit6
            | KeyCode::Digit7
            | KeyCode::Digit8
            | KeyCode::Digit9
                if self.screen == Screen::Playing =>
            {
                let slot = match key {
                    KeyCode::Digit1 => 0,
                    KeyCode::Digit2 => 1,
                    KeyCode::Digit3 => 2,
                    KeyCode::Digit4 => 3,
                    KeyCode::Digit5 => 4,
                    KeyCode::Digit6 => 5,
                    KeyCode::Digit7 => 6,
                    KeyCode::Digit8 => 7,
                    _ => 8,
                };
                let load = self.mapped_key_down("key.loadToolbarActivator");
                if self.spectator() {
                    self.spectator_hotbar_key(slot as i32);
                } else if self.creative
                    && (load || self.mapped_key_down("key.saveToolbarActivator"))
                {
                    self.hotbar_load_or_save(slot, load);
                } else {
                    self.selected = slot;
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn play_mouse(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
        if self.sleeping.is_some() {
            return Ok(());
        }
        if button == MouseButton::Middle && self.spectator() {
            if pressed {
                self.spectator_action_key();
            }
            return Ok(());
        }
        if self.remote.is_some() {
            return self.remote_mouse(button, pressed);
        }
        match button {
            MouseButton::Left => {
                if pressed {
                    self.attack_clicks += 1;
                }
                self.toggles
                    .button(2, pressed, false, self.settings.toggle_attack);
                self.left = self.toggles.is_down(2);
            }
            MouseButton::Right => {
                self.toggles
                    .button(3, pressed, false, self.settings.toggle_use);
                self.right = self.toggles.is_down(3);
                if pressed && self.using.is_none() {
                    self.use_with_hand()?;
                    self.use_delay = 4;
                }
            }
            MouseButton::Middle if pressed => {
                if let Some(entity) = self.pick_entity_hit() {
                    if let Some(stack) = entity.and_then(|id| self.entity_pick_result(id)) {
                        let infinite = self.creative;
                        vise_client::pick_item::pick(
                            &mut self.inventory[..36],
                            &mut self.selected,
                            &stack,
                            infinite,
                        );
                    }
                } else if let Some(hit) = self.pick() {
                    let include_data = self.creative && self.has_control_down();
                    if let Some(stack) = self.picked_stack(hit.pos, include_data) {
                        let infinite = self.creative;
                        vise_client::pick_item::pick(
                            &mut self.inventory[..36],
                            &mut self.selected,
                            &stack,
                            infinite,
                        );
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn far_terrain_choice(&self) -> (u32, bool) {
        let g = &self.settings.vise;
        let reduced = g.far_detail == vise_client::graphics::FarDetail::Reduced;
        (g.lod_distance, reduced)
    }
    fn lod_distance(&self) -> u32 {
        let (distance, _) = self.far_terrain_choice();
        match &self.renderer {
            Some(renderer) => {
                vise_client::graphics::lod_distance_capped(distance, &renderer.info)
            }
            None => distance.min(1024),
        }
    }
    fn terrain_distance(&self) -> i32 {
        if self.lod.is_some() {
            self.streaming.radius.max(self.lod_distance() as i32)
        } else {
            self.streaming.radius
        }
    }
    fn clear_lod(&mut self) {
        self.lod = None;
        self.lod_source_attempted = false;
        self.lod_observe.clear();
        self.far_beacons.clear();
        self.lod_covered_at = None;
        if let Some(renderer) = &mut self.renderer {
            renderer.clear_lods();
        }
    }
    fn prepare_lod(&mut self) {
        let enabled = self.far_terrain_choice().0 > 0
            && self.renderer.is_some()
            && self.player.is_some()
            && self.title.is_none()
            && (self.remote.is_none() || self.integrated.is_some());
        if !enabled {
            if self.lod.is_some() || self.lod_source_attempted {
                self.clear_lod();
            }
            return;
        }
        if self.lod_source_attempted {
            return;
        }
        self.lod_source_attempted = true;
        let source = match &self.integrated {
            Some(server) => server.call(|game| game.streaming.lod_source()),
            None => Some(self.streaming.lod_source()),
        };
        if let Some(source) = source {
            self.lod = Some(vise_client::lod::Lod::new(
                source,
                self.pack.context.clone(),
            ));
            self.lod_observe.extend(self.world.chunks.keys().copied());
        }
    }
    fn update_world(&mut self) {
        if let Err(e) = self.poll_remote() {
            log::warn!("Vise: {e}");
        }
        self.prepare_lod();
        if self.lod.is_some() {
            for &((x, _, z), _, _) in &self.world.light_edits {
                self.lod_observe.insert((x >> 4, z >> 4));
            }
        }
        if let Some(p) = &self.player {
            self.streaming.center = (
                (p.position.x.floor() as i32) >> 4,
                (p.position.z.floor() as i32) >> 4,
            );
        }
        self.streaming.loading =
            self.options.execution_role.is_client() && self.loading_reason().is_some();
        self.tick_pregen();
        let areas = self.streaming.extra.len();
        let waiting = std::mem::take(&mut self.spawn_wait);
        self.streaming
            .extra
            .extend(waiting.into_iter().map(|chunk| (chunk, 0)));
        let (uploads, unloaded) =
            vise_client::spans::time("streaming", || self.streaming.update(&mut self.world));
        self.streaming.extra.truncate(areas);
        self.spawn_fresh_chunks();
        if let Some(lod) = &mut self.lod {
            self.lod_observe
                .extend(uploads.iter().map(|(key, _)| (key.0, key.2)));
            for chunk in &unloaded {
                self.lod_observe.remove(&(chunk.x, chunk.z));
            }
            vise_client::spans::time("lod_observe", || {
                for chunk in &unloaded {
                    lod.observe(chunk.clone(), self.res.facts());
                }
                while lod.observing() < vise_client::lod::OBSERVE_AHEAD
                    && let Some(&key) = self.lod_observe.iter().next()
                {
                    self.lod_observe.remove(&key);
                    if let Some(chunk) = self.world.chunks.get(&key) {
                        lod.observe(chunk.clone(), self.res.facts());
                    }
                }
            });
        }
        let dirty: Vec<_> = unloaded
            .iter()
            .filter(|c| self.world.modified.remove(&(c.x, c.z)))
            .cloned()
            .collect();
        let modified = vise_client::spans::time("with_entities", || self.with_entities(&dirty));
        vise_client::spans::time("unload_entities", || self.unload_entities(&unloaded));
        if !unloaded.is_empty() {
            let gone: vise_client::fast_hash::FxHashSet<(i32, i32)> =
                unloaded.iter().map(|c| (c.x, c.z)).collect();
            self.world
                .block_entities
                .retain(|p, _| !gone.contains(&(p.0 >> 4, p.2 >> 4)));
            self.world
                .block_names
                .retain(|p, _| !gone.contains(&(p.0 >> 4, p.2 >> 4)));
        }
        if !modified.is_empty()
            && let Err(e) = vise_client::spans::time("save_chunks", || {
                self.save
                    .save_later(&modified, self.pack.assets.registry, self.res.biomes())
            })
        {
            log::warn!("Vise: failed to save unloaded chunks: {e}");
        }
        let (far_distance, far_reduced) = self.far_terrain_choice();
        if let Some(renderer) = self.renderer.as_mut() {
            let g = &self.settings.vise;
            let budgeted = g.chunk_updates > 0 || g.upload_budget_kib > 0;
            if !budgeted && self.pending_uploads.is_empty() {
                vise_client::spans::time("upload", || {
                    for (key, mesh) in &uploads {
                        renderer.upload(*key, mesh);
                    }
                });
                self.streaming.free(uploads);
            } else {
                let pending = &mut self.pending_uploads;
                let fresh: vise_client::fast_hash::FxHashSet<(i32, i32, i32)> =
                    uploads.iter().map(|(k, _)| *k).collect();
                let chunks = &self.world.chunks;
                let loaded = |k: &(i32, i32, i32)| chunks.contains_key(&(k.0, k.2));
                let mut spent: Vec<_> = pending
                    .extract_if(.., |(k, _)| fresh.contains(k) || !loaded(k))
                    .collect();
                for upload in uploads {
                    if loaded(&upload.0) {
                        pending.push(upload);
                    } else {
                        spent.push(upload);
                    }
                }
                let centre = self.streaming.center;
                let far = |k: &(i32, i32, i32)| (k.0 - centre.0).pow(2) + (k.2 - centre.1).pow(2);
                pending.sort_unstable_by_key(|(k, _)| std::cmp::Reverse(far(k)));
                let (limit, budget) = if budgeted {
                    (
                        g.chunk_updates as usize,
                        u64::from(g.upload_budget_kib) * 1024,
                    )
                } else {
                    (0, 0)
                };
                let (mut count, mut bytes) = (0, 0u64);
                while let Some((_, mesh)) = pending.last() {
                    let size = ((mesh.solid.len() + mesh.translucent.len())
                        * std::mem::size_of::<vise_client::mesher::TerrainVertex>())
                        as u64;
                    if (limit > 0 && count >= limit)
                        || (budget > 0 && bytes > 0 && bytes + size > budget)
                    {
                        break;
                    }
                    let (key, mesh) = pending.pop().expect("checked above");
                    renderer.upload(key, &mesh);
                    spent.push((key, mesh));
                    count += 1;
                    bytes += size;
                }
                self.streaming.free(spent);
            }
            renderer.remove_columns(unloaded.iter().map(|c| (c.x, c.z)));
            if let (Some(lod), Some(player)) = (&mut self.lod, &self.player) {
                let lod_distance =
                    vise_client::graphics::lod_distance_capped(far_distance, &renderer.info);
                lod.set_near_loading(self.streaming.busy());
                lod.set_reduced_detail(far_reduced);
                let at = (
                    renderer.lod_detail_revision(),
                    self.streaming.center,
                    self.streaming.radius,
                );
                let covered = (self.lod_covered_at != Some(at)).then(|| {
                    self.lod_covered_at = Some(at);
                    vise_client::spans::time("lod_covered_set", || {
                        renderer
                            .lod_detail_columns()
                            .filter(|&(x, z)| {
                                vise_client::streaming::in_view_distance(at.1, at.2, x, z)
                            })
                            .collect()
                    })
                });
                let meshes = vise_client::spans::time("lod_update", || {
                    lod.update(
                        [player.position.x, player.position.y, player.position.z],
                        self.yaw,
                        self.streaming.radius,
                        lod_distance,
                        covered,
                    )
                });
                renderer.queue_lods(meshes, lod.removed(), lod.take_cutout());
                vise_client::spans::time("lod_upload", || {
                    renderer.upload_lods(std::time::Duration::from_millis(2))
                });
            }
        }
    }
}
impl ApplicationHandler for Game<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let fullscreen = self.settings.fullscreen || self.options.fullscreen;
        let size = self.options.window_size.map(f64::from);
        self.fullscreen = vise_client::window::FullscreenState::new(fullscreen);
        let attributes = Window::default_attributes()
            .with_title("Vise")
            .with_window_icon(vise_client::window::icon());
        #[cfg(target_os = "linux")]
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes, "vise", "vise",
        );
        let window = match event_loop.create_window(
            attributes
                .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1]))
                .with_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None))),
        ) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(e.into());
                event_loop.exit();
                return;
            }
        };
        vise_client::telemetry::note_display(&window);
        match self.new_renderer(window.clone()) {
            Ok(r) => {
                self.renderer = Some(r);
                self.world_renderer_ready = true;
            }
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
                return;
            }
        }
        self.window = Some(window);
        self.apply_settings();
        self.previous = Instant::now();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        follow_keymap(&mut event);
        if let WindowEvent::KeyboardInput { event, .. } = &event
            && let PhysicalKey::Code(key) = event.physical_key
        {
            if event.state == ElementState::Pressed {
                self.physical_keys.insert(key);
            } else {
                self.physical_keys.remove(&key);
            }
        }
        if let WindowEvent::MouseInput { state, button, .. } = &event {
            if *state == ElementState::Pressed {
                self.physical_buttons.insert(*button);
            } else {
                self.physical_buttons.remove(button);
            }
        }
        if let WindowEvent::Focused(false) = &event {
            self.physical_keys.clear();
            self.physical_buttons.clear();
        }
        if let WindowEvent::ModifiersChanged(m) = &event {
            self.modifiers = m.state();
        }
        if let WindowEvent::KeyboardInput { event, .. } = &event
            && event.state == ElementState::Pressed
            && let PhysicalKey::Code(key) = event.physical_key
            && vise_client::keybinds::key_name(key)
                == Some(self.settings.keys.get("key.fullscreen"))
            && !(self.screen == Screen::Options && self.options_waiting_for_key())
        {
            self.last_input = Instant::now();
            if !event.repeat {
                self.toggle_fullscreen();
            }
            return;
        }
        if let WindowEvent::KeyboardInput { event, .. } = &event
            && event.state == ElementState::Pressed
            && let PhysicalKey::Code(key) = event.physical_key
        {
            self.last_input = Instant::now();
            if self.screenshot_shortcut(key, event.repeat) {
                return;
            }
        }
        if let WindowEvent::KeyboardInput {
            event: key_event, ..
        } = &event
            && let PhysicalKey::Code(key) = key_event.physical_key
        {
            use vise_client::debug_keys::KeyAction;
            let action = match (key_event.state, key_event.repeat) {
                (ElementState::Released, _) => KeyAction::Release,
                (_, false) => KeyAction::Press,
                (_, true) => KeyAction::Repeat,
            };
            if self.debug_key(key, action) {
                return;
            }
        }
        if matches!(
            event,
            WindowEvent::KeyboardInput { .. }
                | WindowEvent::MouseInput { .. }
                | WindowEvent::MouseWheel { .. }
                | WindowEvent::CursorMoved { .. }
        ) {
            self.last_input = Instant::now();
        }
        if (self.pending_entry.is_some() || self.world_loader.is_some())
            && matches!(&event, WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.physical_key == PhysicalKey::Code(KeyCode::Escape))
        {
            self.pending_entry = None;
            if let Some(loader) = self.world_loader.take() {
                self.world_loader = Some(if loader.cancelled {
                    loader
                } else {
                    loader.cancel()
                });
            }
            self.perf.cancel_world_opening();
            return;
        }
        if (self.pending_entry.is_some()
            || self.world_loader.is_some()
            || self.integrated_starting.is_some())
            && !matches!(
                &event,
                WindowEvent::CloseRequested
                    | WindowEvent::Resized(_)
                    | WindowEvent::RedrawRequested
            )
        {
            return;
        }
        if self.pack_reload.is_some()
            && !matches!(
                &event,
                WindowEvent::CloseRequested
                    | WindowEvent::Focused(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::RedrawRequested
            )
        {
            return;
        }
        if self.pending_save.is_some() {
            match event {
                WindowEvent::CloseRequested => {
                    self.pending_save.as_mut().unwrap().quit = true;
                }
                WindowEvent::Focused(focused) => {
                    self.focused = focused;
                    if !focused {
                        self.capture(false);
                    }
                }
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width, size.height);
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Err(error) = self.render_closing_save() {
                        self.error = Some(error);
                        event_loop.exit();
                    }
                }
                _ => {}
            }
            return;
        }
        let result: Result<()> = (|| {
            match event {
                WindowEvent::CloseRequested => {
                    self.begin_closing_save(true)?;
                }
                WindowEvent::Resized(size) => {
                    if let Some(r) = &mut self.renderer {
                        r.resize(size.width, size.height);
                    }
                    self.observe_fullscreen();
                }
                WindowEvent::Focused(focused) => {
                    self.focused = focused;
                    self.focus_lost = (!focused).then(Instant::now);
                    if !focused && self.control.is_none() {
                        self.keys.clear();
                        self.left = false;
                        self.right = false;
                        self.capture(false);
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    if let Some(r) = &self.renderer {
                        let b = HudBatch::new([r.config.width, r.config.height]);
                        self.cursor = (position.x as f32 / b.scale, position.y as f32 / b.scale);
                        if self.dialog_open() {
                            self.dialog_drag();
                        }
                        if self.screen == Screen::Advancements {
                            self.advancements_drag();
                        }
                        if self.screen == Screen::Stats {
                            self.stats_drag();
                        }
                        if self.screen == Screen::WorldOptions {
                            self.world_options_drag();
                        }
                        if self.screen == Screen::Creative {
                            let (_, oy) = Self::creative_origin(&b);
                            self.creative_screen.drag_scrollbar(self.cursor.1 - oy);
                        }
                        if self.screen == Screen::Inventory {
                            self.station_drag();
                        }
                        if matches!(self.screen, Screen::Inventory | Screen::Creative) {
                            self.inventory_drag_move();
                        }
                        if let Some(title) = &mut self.title {
                            title.drag(self.cursor, (b.width, b.height));
                            title.drag_field(&self.gui.atlas, self.cursor.0);
                        }
                        if self.dragging && self.screen == Screen::Options {
                            let _ = b;
                            self.options_drag()?;
                        }
                    }
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    let (raw_x, raw_y) = match delta {
                        MouseScrollDelta::LineDelta(x, y) => (f64::from(x), f64::from(y)),
                        MouseScrollDelta::PixelDelta(p) => (p.x / 30.0, p.y / 30.0),
                    };
                    let discrete = self.settings.discrete_scroll;
                    let wheel_sensitivity = f64::from(self.settings.mouse_wheel_sensitivity);
                    let scaled = |v: f64| {
                        (if discrete {
                            vise_client::mouse::java_signum(v)
                        } else {
                            v
                        }) * wheel_sensitivity
                    };
                    let d = scaled(raw_y) as f32;
                    if self.dialog_open() {
                        self.dialog_scroll(d);
                    } else if self.code_of_conduct.is_some() {
                        self.code_of_conduct_scroll(d);
                    } else if let Some(title) = &mut self.title
                        && self.screen != Screen::Win
                    {
                        let size = self
                            .window
                            .as_ref()
                            .map(|w| w.inner_size())
                            .unwrap_or_default();
                        let batch = HudBatch::new([size.width, size.height]);
                        title.scroll_worlds(d, self.cursor, (batch.width, batch.height));
                    }
                    if self.screen == Screen::Playing
                        && self.sleeping.is_none()
                        && self.player.is_some()
                        && !self.native_wheel_event(d)
                        && let Some((wheel, (_, vertical_steps))) =
                            self.mouse
                                .on_scroll(raw_x, raw_y, discrete, wheel_sensitivity)
                    {
                        if self.spectator() && self.spectator_gui.is_menu_active() {
                            self.spectator_scroll(-wheel);
                        } else if self.spectator() {
                            if let Some(player) = &mut self.player {
                                player.flying_speed = vise_client::mouse::spectator_flying_speed(
                                    player.flying_speed,
                                    vertical_steps,
                                );
                            }
                        } else {
                            self.selected =
                                vise_client::mouse::ScrollWheelHandler::next_selection(
                                    f64::from(wheel),
                                    self.selected as i32,
                                    9,
                                ) as usize;
                        }
                    }
                    if self.screen == Screen::Chat && d != 0.0 {
                        self.chat_scroll(d);
                    }
                    if self.screen == Screen::CommandBlock && d != 0.0 {
                        self.command_block_scroll(d);
                    }
                    if self.screen == Screen::Options && d != 0.0 {
                        self.options_scroll(d)?;
                    }
                    let bundle = bundles::takes_bundle_wheel(self.screen)
                        && d != 0.0
                        && self.bundle_scroll(scaled(raw_x), scaled(raw_y));
                    if self.screen == Screen::Creative && d != 0.0 && !bundle {
                        self.creative_screen.wheel(f64::from(d), &self.pack.names);
                    }
                    if self.screen == Screen::Inventory
                        && d != 0.0
                        && !bundle
                        && !self.tweak_scroll(d, self.inventory_target().0)
                    {
                        self.station_scroll(d);
                    }
                    if self.screen == Screen::Stats && d != 0.0 {
                        self.stats_scroll(d);
                    }
                    if self.screen == Screen::WorldOptions && d != 0.0 {
                        self.world_options_scroll(d);
                    }
                    if self.screen == Screen::ServerLinks && d != 0.0 {
                        self.server_links_scroll(d);
                    }
                    if self.screen == Screen::Advancements {
                        self.advancements_scroll(scaled(raw_x) as f32, d);
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if self.link_prompt.is_some() => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.link_prompt_click();
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if self.code_of_conduct.is_some() => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.code_of_conduct_click()?;
                    }
                }
                WindowEvent::MouseInput { state, button, .. }
                    if self.server_packs.prompt.is_some() =>
                {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.pack_prompt_click()?;
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if self.dialog_open() => {
                    if button == MouseButton::Left {
                        if state == ElementState::Pressed {
                            self.dialog_click()?;
                        } else {
                            self.dialog_release();
                        }
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if self.remote_loading() => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        self.connect_screen_click()?;
                    }
                }
                WindowEvent::MouseInput { .. } if self.screen == Screen::Win => {}
                WindowEvent::MouseInput { state, button, .. } if self.screen == Screen::Options => {
                    if self.options_waiting_for_key() && state == ElementState::Pressed {
                        self.options_bind_mouse(button)?;
                    } else if matches!(button, MouseButton::Left | MouseButton::Right) {
                        self.options_mouse(
                            state == ElementState::Pressed,
                            button == MouseButton::Right,
                        )?;
                    }
                }
                WindowEvent::MouseInput {
                    state: ElementState::Released,
                    button: MouseButton::Left,
                    ..
                } if self.title.is_some() && self.screen != Screen::Options => {
                    if let Some(title) = &mut self.title {
                        title.release();
                        title.release_field();
                    }
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } if self.title.is_some() && state == ElementState::Pressed => {
                    let size = self
                        .renderer
                        .as_ref()
                        .map(|r| [r.config.width, r.config.height])
                        .unwrap_or([1, 1]);
                    let b = HudBatch::new(size);
                    let atlas = &self.gui.atlas;
                    let shift = self.has_shift_down();
                    let (action, sound) = self.title.as_mut().map_or((Action::None, false), |t| {
                        let clicked = t.click(self.cursor.0, self.cursor.1, (b.width, b.height));
                        t.press_field(atlas, self.cursor, (b.width, b.height), shift);
                        let rules = t.press_rules(atlas, self.cursor, (b.width, b.height));
                        (clicked.0, clicked.1 || rules)
                    });
                    self.save_title_options();
                    if sound {
                        self.audio.play_button_click();
                    }
                    self.title_action(event_loop, action)?;
                }
                WindowEvent::MouseInput { .. } if self.title.is_some() => {}
                WindowEvent::MouseInput { state, button, .. } if self.book.is_some() => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let size = self
                            .renderer
                            .as_ref()
                            .map(|r| [r.config.width, r.config.height])
                            .unwrap_or([1, 1]);
                        self.book_click(size);
                    }
                }
                WindowEvent::MouseInput { state, button, .. } if self.sign_edit.is_some() => {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let size = self
                            .renderer
                            .as_ref()
                            .map(|r| [r.config.width, r.config.height])
                            .unwrap_or([1, 1]);
                        self.sign_edit_click(size);
                    }
                }
                WindowEvent::MouseInput { state, button, .. } => {
                    let pressed = state == ElementState::Pressed;
                    if matches!(self.screen, Screen::Inventory | Screen::Creative)
                        && let Some(key) = self
                            .settings
                            .keys
                            .container_mouse_key(button, self.creative)
                    {
                        if pressed && self.carried.is_none() {
                            self.inventory_hotkey(key);
                        }
                        return Ok(());
                    }
                    if pressed && self.sort_mouse_press(button) {
                        return Ok(());
                    }
                    match self.screen {
                        Screen::Playing => {
                            if pressed && !self.captured {
                                self.capture(true);
                                return Ok(());
                            }
                            self.extras_mouse(button, pressed);
                            if pressed
                                && self.settings.keys.get("key.smoothCamera")
                                    == vise_client::keybinds::mouse_name(button)
                            {
                                self.smooth_camera = !self.smooth_camera;
                            }
                            for mapped in self.settings.keys.translate_mouse_all(button) {
                                match mapped {
                                    Err(b) => self.play_mouse(b, pressed)?,
                                    Ok(k) if pressed => {
                                        self.keys.insert(k);
                                        self.key_pressed(k)?;
                                    }
                                    Ok(k) => {
                                        self.keys.remove(&k);
                                    }
                                }
                            }
                        }
                        Screen::Inventory | Screen::Creative
                            if !matches!(button, MouseButton::Left | MouseButton::Right) =>
                        {
                            if self.settings.keys.get("key.pickItem")
                                == vise_client::keybinds::mouse_name(button)
                            {
                                if pressed {
                                    self.inventory_pick_press();
                                } else {
                                    self.inventory_pick_release();
                                }
                                if let Some(key) = self.container {
                                    self.container_changed(key);
                                }
                            }
                        }
                        Screen::Inventory if pressed => {
                            self.inventory_press(button == MouseButton::Right);
                            if let Some(key) = self.container {
                                self.container_changed(key);
                            }
                        }
                        Screen::Inventory => {
                            self.inventory_mouse_release(button == MouseButton::Right);
                            self.station_release();
                        }
                        Screen::Chat if pressed && button == MouseButton::Left => self.chat_click(),
                        Screen::Creative if pressed => {
                            self.click_creative(button == MouseButton::Right);
                        }
                        Screen::Creative => {
                            if button == MouseButton::Left {
                                self.creative_screen.release();
                            }
                            self.inventory_mouse_release(button == MouseButton::Right);
                        }
                        Screen::Paused
                            if pressed && button == MouseButton::Left && self.pause_menu =>
                        {
                            self.pause_click()?
                        }
                        Screen::WorldOptions if pressed && button == MouseButton::Left => {
                            self.world_options_click()
                        }
                        Screen::ServerLinks if pressed && button == MouseButton::Left => {
                            self.server_links_click()?
                        }
                        Screen::ConfirmLink if pressed && button == MouseButton::Left => {
                            self.confirm_link_click()
                        }
                        Screen::DialogWarning if pressed && button == MouseButton::Left => {
                            self.dialog_warning_click()?
                        }
                        Screen::WorldOptions if button == MouseButton::Left => {
                            self.world_options_release()
                        }
                        Screen::Friends if pressed && button == MouseButton::Left => {
                            self.friends_screen_click()
                        }
                        Screen::CommandBlock if pressed && button == MouseButton::Left => {
                            self.command_block_click()
                        }
                        Screen::Dead if pressed && button == MouseButton::Left => {
                            self.death_click()?
                        }
                        Screen::Advancements if button == MouseButton::Left => {
                            let size = self
                                .renderer
                                .as_ref()
                                .map_or([1, 1], |r| [r.config.width, r.config.height]);
                            self.advancements_mouse(pressed, size);
                        }
                        Screen::Stats if pressed && button == MouseButton::Left => {
                            let size = self
                                .renderer
                                .as_ref()
                                .map_or([1, 1], |r| [r.config.width, r.config.height]);
                            self.stats_click(size);
                        }
                        Screen::Stats if button == MouseButton::Left => self.stats_release(),
                        _ => {}
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.screen == Screen::Dead && self.ui.death_confirm.is_some() =>
                {
                    if event.state == ElementState::Pressed
                        && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                        && self
                            .ui
                            .death_confirm
                            .is_some_and(|t| t.elapsed().as_millis() >= 1000)
                    {
                        self.death_confirm_answer(false)?;
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if self.link_prompt.is_some() => {
                    if event.state == ElementState::Pressed
                        && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                    {
                        self.answer_link_prompt(false);
                    }
                }
                WindowEvent::KeyboardInput { .. } if self.code_of_conduct.is_some() => {}
                WindowEvent::KeyboardInput { event, .. } if self.server_packs.prompt.is_some() => {
                    if event.state == ElementState::Pressed
                        && event.physical_key == PhysicalKey::Code(KeyCode::Escape)
                    {
                        self.answer_pack_prompt(false)?;
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if self.dialog_open() => {
                    let shortcut = self.edit_shortcut(&event);
                    if event.state == ElementState::Pressed
                        && let PhysicalKey::Code(key) = event.physical_key
                    {
                        self.dialog_key(key, shortcut, event.text.as_deref())?
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.screen == Screen::Chat && self.title.is_none() =>
                {
                    if let PhysicalKey::Code(key) = event.physical_key {
                        if event.state == ElementState::Pressed {
                            self.keys.insert(key);
                            self.chat_key(&event);
                        } else {
                            self.keys.remove(&key);
                        }
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if self.sign_edit.is_some() => {
                    self.sign_edit_key(&event)
                }
                WindowEvent::KeyboardInput { event, .. } if self.book.is_some() => {
                    self.book_key(&event)
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.screen == Screen::Creative
                        && self.creative_screen.tab().search
                        && event.state == ElementState::Pressed
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
                {
                    if let PhysicalKey::Code(key) = event.physical_key
                        && self.search_tab_hotbar_key(key)
                    {
                    } else if let Some(shortcut) = self.edit_shortcut(&event) {
                        self.creative_search_shortcut(shortcut);
                    } else if let PhysicalKey::Code(key) = event.physical_key {
                        let before = self.creative_screen.search.value.clone();
                        let (ctrl, shift) = (self.has_control_down(), self.has_shift_down());
                        if self.creative_screen.search.key(key, ctrl, shift) {
                            if self.creative_screen.search.value != before {
                                self.creative_screen.scroll = 0.0;
                            }
                        } else {
                            self.creative_search_edit(event.text.as_deref());
                        }
                    } else {
                        self.creative_search_edit(event.text.as_deref());
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.recipe_search_focused()
                        && event.state == ElementState::Pressed
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
                {
                    self.recipe_key(&event);
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.mod_browser_typing()
                        && event.state == ElementState::Pressed
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
                {
                    self.mod_browser_key(&event);
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.station_typing()
                        && event.state == ElementState::Pressed
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
                {
                    if let Some(shortcut) = self.edit_shortcut(&event) {
                        self.station_shortcut(shortcut);
                    } else if let PhysicalKey::Code(key) = event.physical_key
                        && self.station_key(key)
                    {
                    } else {
                        let backspace = event.physical_key == PhysicalKey::Code(KeyCode::Backspace);
                        self.station_text(event.text.as_deref(), backspace);
                    }
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.screen == Screen::WorldOptions
                        && !matches!(event.physical_key, PhysicalKey::Code(KeyCode::Escape)) =>
                {
                    self.world_options_key(&event)
                }
                WindowEvent::KeyboardInput { event, .. } if self.screen == Screen::Win => {
                    self.win_key(&event)?
                }
                WindowEvent::KeyboardInput { event, .. } if self.screen == Screen::CommandBlock => {
                    self.command_block_key(&event)
                }
                WindowEvent::KeyboardInput { event, .. }
                    if self.screen == Screen::Options && event.state == ElementState::Pressed =>
                {
                    if let Some(shortcut) = self.edit_shortcut(&event)
                        && self.options_shortcut(shortcut)
                    {
                        return Ok(());
                    }
                    if let PhysicalKey::Code(key) = event.physical_key {
                        self.options_key(key, event.text.as_deref())?;
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if self.title.is_some() => {
                    if event.state == ElementState::Pressed {
                        if let Some(shortcut) = self.edit_shortcut(&event) {
                            self.title_shortcut(shortcut);
                            return Ok(());
                        }
                        match event.physical_key {
                            PhysicalKey::Code(
                                code @ (KeyCode::Enter | KeyCode::NumpadEnter | KeyCode::Space),
                            ) if code != KeyCode::Space
                                || self.title.as_ref().is_some_and(|t| t.list_focused()) =>
                            {
                                let action = self
                                    .title
                                    .as_mut()
                                    .map_or(Action::None, |t| t.confirm_key());
                                self.save_title_options();
                                if matches!(action, Action::Play { .. }) {
                                    self.audio.play_button_click();
                                }
                                self.title_action(event_loop, action)?;
                            }
                            PhysicalKey::Code(
                                key @ (KeyCode::Backspace
                                | KeyCode::Delete
                                | KeyCode::ArrowLeft
                                | KeyCode::ArrowRight
                                | KeyCode::Home
                                | KeyCode::End),
                            ) if self.title.as_mut().is_some_and(|t| t.has_caret_field()) => {
                                let (ctrl, shift) =
                                    (self.has_control_down(), self.has_shift_down());
                                if let Some(t) = self.title.as_mut() {
                                    t.field_key(key, ctrl, shift);
                                }
                            }
                            PhysicalKey::Code(KeyCode::Backspace) => self.title_edit(None, true),
                            PhysicalKey::Code(KeyCode::Tab) => {
                                if let Some(t) = self.title.as_mut() {
                                    t.tab();
                                }
                            }
                            PhysicalKey::Code(KeyCode::Escape) => {
                                if let Some(t) = self.title.as_mut() {
                                    t.escape();
                                }
                                self.save_title_options();
                            }
                            PhysicalKey::Code(KeyCode::F5) => {
                                if let Some(t) = self.title.as_mut() {
                                    t.refresh_servers();
                                }
                            }
                            _ => self.title_edit(event.text.as_deref(), false),
                        }
                    }
                }
                WindowEvent::KeyboardInput { event, .. } => {
                    let PhysicalKey::Code(key) = event.physical_key else {
                        return Ok(());
                    };
                    self.native_key_event(key, event.state == ElementState::Pressed, event.repeat);
                    if self.screen == Screen::Playing
                        && event.state == ElementState::Pressed
                        && !event.repeat
                        && vise_client::keybinds::key_name(key)
                            .is_some_and(|n| self.settings.keys.get("key.smoothCamera") == n)
                    {
                        self.smooth_camera = !self.smooth_camera;
                    }
                    for mapped in self.settings.keys.translate_all(key) {
                        match mapped {
                            Ok(k) => {
                                self.mapped_key(
                                    k,
                                    event.state == ElementState::Pressed,
                                    event.repeat,
                                )?;
                            }
                            Err(button) => {
                                let action = match (event.state, event.repeat) {
                                    (ElementState::Released, _) => KeyAction::Release,
                                    (_, false) => KeyAction::Press,
                                    (_, true) => KeyAction::Repeat,
                                };
                                if let Some(pressed) =
                                    keyboard_button(self.screen == Screen::Playing, action)
                                {
                                    self.play_mouse(button, pressed)?;
                                }
                            }
                        }
                    }
                }
                WindowEvent::RedrawRequested => {
                    self.observe_fullscreen();
                    let now = Instant::now();
                    let frame_cpu = self
                        .options
                        .spikes
                        .and_then(|_| vise_client::spans::thread_cpu());
                    let elapsed = now.duration_since(self.previous);
                    self.frame_ticks = elapsed.as_secs_f32() * 20.0;
                    self.previous = now;
                    if self.screen == Screen::Playing
                        && self.title.is_none()
                        && self.sign_edit.is_none()
                        && self.book.is_none()
                    {
                        let turn = |k| f32::from(u8::from(self.keys.contains(&k)));
                        let secs = elapsed.as_secs_f32().min(0.1);
                        let yaw = (turn(KeyCode::ArrowRight) - turn(KeyCode::ArrowLeft)) * 90.0;
                        let pitch = (turn(KeyCode::ArrowDown) - turn(KeyCode::ArrowUp)) * 90.0;
                        if yaw != 0.0 || pitch != 0.0 {
                            self.turn_player(
                                f64::from(yaw * secs / 0.15),
                                f64::from(pitch * secs / 0.15),
                            );
                        }
                    }
                    if self.title.is_none() {
                        vise_client::spans::time("update_world", || self.update_world());
                        vise_client::spans::time("spawn_player", || self.spawn_player())?;
                    }
                    let updated = Instant::now();
                    self.track_toggle_screens();
                    vise_client::heartbeat::set_lan(self.is_published());
                    self.update_discord_presence();
                    let steps = self.clock.advance(elapsed);
                    let in_frame = self.remote.is_none();
                    let mut spent = Duration::ZERO;
                    for step in 0..steps {
                        if in_frame && !self.clock.tick_fits(spent) {
                            if let Some(warning) = self.clock.drop_ticks(steps - step) {
                                log::warn!("{warning}");
                            }
                            break;
                        }
                        let started = Instant::now();
                        if self.title.is_none() && !self.pause {
                            let tick = Instant::now();
                            vise_client::spans::time("tick", || self.tick())?;
                            if self.remote.is_none() {
                                self.perf.tick(tick.elapsed().as_secs_f32() * 1000.0);
                                self.debug_charts.log_tick(tick, tick.elapsed());
                            }
                            self.tick_ambience();
                            self.tick_fov();
                        }
                        self.tick_win_screen()?;
                        self.tick_chat_queue();
                        self.tick_music();
                        self.tick_sound_engine();
                        self.clock.record(started.elapsed());
                        spent += started.elapsed();
                        if let Some(report) = self.clock.take_sprint_report() {
                            self.sprint_report(&report);
                        }
                    }
                    self.extras_frame(elapsed);
                    self.extras_retry()?;
                    self.apply_mouse_look();
                    self.pad_look(elapsed.as_secs_f32());
                    self.advance_flight();
                    self.tick_world_icon();
                    let ticked = Instant::now();
                    let rendered = self.render()?;
                    self.apply_cursor_icon();
                    if let Some(threshold) = self.options.spikes
                        && let Some(r) = self.renderer.as_ref()
                    {
                        let cpu = now.elapsed().saturating_sub(r.last_wait).as_secs_f32() * 1000.0;
                        let spans = vise_client::spans::take();
                        if cpu > threshold {
                            let worked = vise_client::spans::thread_cpu()
                                .zip(frame_cpu)
                                .map_or_else(String::new, |(end, start)| {
                                    format!(", cpu {:.1} ms", (end - start).as_secs_f32() * 1000.0)
                                });
                            log::info!(
                                "Vise: slow frame {cpu:.1} ms{worked} ({steps} ticks): {}",
                                vise_client::spans::describe(&spans)
                            );
                        }
                    }
                    if let Some(r) = self.renderer.as_mut() {
                        let cpu = now.elapsed().saturating_sub(r.last_wait);
                        if rendered {
                            let ms = |d: Duration| d.as_secs_f32() * 1000.0;
                            self.frame_graph.record(
                                ms(elapsed),
                                ms(cpu),
                                r.gpu_times.last().copied(),
                            );
                            self.frame_stats.record_cpu(ms(cpu));
                            let render = r.last_encode;
                            self.frame_stats.record_phases([
                                ms(updated - now),
                                ms(ticked - updated),
                                ms(ticked.elapsed().saturating_sub(r.last_wait + render)),
                                ms(render),
                            ]);
                        }
                        self.frame_stats.record_gpu(&r.gpu_times);
                        r.gpu_times.clear();
                    }
                    self.limit_frame();
                    self.pause = self.pauses_game();
                    if let Some(server) = &self.integrated {
                        server.set_paused(self.pause);
                        server.set_distances(
                            self.settings.render_distance,
                            self.settings.simulation_distance,
                            self.settings.entity_distance,
                        );
                    }
                    self.clock.end_frame(self.pause);
                    let mut consumed = false;
                    let finished = self
                        .renderer
                        .as_mut()
                        .and_then(|r| r.screenshot_result.take());
                    if let Some(result) = &finished
                        && self.ui.screenshot_message.is_some()
                    {
                        self.finish_screenshot(result);
                        consumed = true;
                    }
                    if let Some(result) = &finished
                        && let Some(reply) = self.pending_screenshot.take()
                    {
                        let _ = reply.send(
                            match result {
                                Ok([w, h]) => {
                                    serde_json::json!({ "ok": true, "width": w, "height": h })
                                }
                                Err(e) => serde_json::json!({ "error": e }),
                            }
                            .to_string(),
                        );
                        consumed = true;
                    }
                    if !consumed && let Some(Err(error)) = finished {
                        log::warn!("Vise: couldn't save the world icon: {error}");
                    }
                    if rendered {
                        self.frame_stats.record(elapsed.as_secs_f32() * 1000.0);
                        self.debug_charts.frame_times.log(elapsed.as_nanos() as i64);
                        if self.frames == 0 {
                            log::info!(
                                "Vise: first frame after {} ms",
                                self.started.elapsed().as_millis()
                            );
                        }
                        if self.title.is_some() {
                            self.perf.title_shown();
                        } else if self.screen == Screen::Playing && self.player.is_some() {
                            self.perf.frame(elapsed.as_secs_f32() * 1000.0);
                            self.world_frames += 1;
                        }
                        self.frames += 1;
                        self.fps_frames += 1;
                        vise_client::crash::debug_crash_tick();
                        if self.fps_at.elapsed() >= Duration::from_secs(1) {
                            self.fps = self.fps_frames as f32 / self.fps_at.elapsed().as_secs_f32();
                            self.fps_frames = 0;
                            self.fps_at = Instant::now();
                        }
                        if self.options.frames.is_some_and(|f| self.frames >= f)
                            || self
                                .options
                                .world_frames
                                .is_some_and(|f| self.world_frames >= f)
                        {
                            event_loop.exit();
                        }
                    }
                }
                _ => {}
            }

            Ok(())
        })();
        if let Err(e) = result {
            self.error = Some(e);
            event_loop.exit();
        }
    }
    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if self.pending_save.is_none()
            && self.captured
            && self.focused
            && self.screen == Screen::Playing
            && let DeviceEvent::MouseMotion { delta: (x, y) } = event
        {
            self.mouse.on_move(0.0, 0.0, x, y, true, true);
            self.last_input = Instant::now();
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if vise_threads::stop_requests() > 0
            && self.pending_save.as_ref().is_none_or(|save| !save.quit)
        {
            if let Err(error) = self.begin_closing_save(true) {
                self.error = Some(error);
                event_loop.exit();
                return;
            }
        }
        if self.pending_save.is_some() {
            match self.poll_closing_save() {
                Ok(true) => event_loop.exit(),
                Ok(false) => {
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                    event_loop.set_control_flow(ControlFlow::WaitUntil(
                        Instant::now() + Duration::from_millis(4),
                    ));
                }
                Err(error) => {
                    self.error = Some(error);
                    event_loop.exit();
                }
            }
            return;
        }
        if let Err(error) = self.resume_world_entry() {
            self.error = Some(error);
            event_loop.exit();
            return;
        }
        if let Some(folder) = self.title.as_mut().and_then(|t| t.take_open_request())
            && let Err(error) = self.open_world(&folder, None, None, false, None)
        {
            self.error = Some(error);
            event_loop.exit();
            return;
        }
        self.tick_server_packs();
        self.poll_control(event_loop);
        if self.pending_save.is_some() {
            return;
        }
        self.poll_pads(event_loop);
        self.pause_if_inactive();
        if self.options.frames.is_some() && self.started.elapsed() > self.options.timeout {
            self.error = Some("frame-limited run timed out".into());
            event_loop.exit();
            return;
        }
        if let Some(w) = &self.window {
            if self.options.offscreen {
                let id = w.id();
                self.window_event(event_loop, id, WindowEvent::RedrawRequested);
            } else {
                w.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.pad_close_output();
        if let Err(e) = self.save_for_exit() {
            log::warn!("Vise: failed to save the world on exit: {e}");
        }
        self.finish_perf();
    }
}
fn load_clouds(root: &std::path::Path) -> (u32, u32, Vec<bool>) {
    vise_client::hud::decode(&root.join("assets/minecraft/textures/environment/clouds.png"))
        .map(|(w, h, px)| {
            (
                w,
                h,
                px.as_chunks::<4>().0.iter().map(|p| p[3] >= 10).collect(),
            )
        })
        .unwrap_or((1, 1, vec![false]))
}
fn load_forcefield(root: &std::path::Path) -> (u32, u32, Vec<u8>) {
    vise_client::hud::decode(&root.join("assets/minecraft/textures/misc/forcefield.png"))
        .ok()
        .filter(|(w, h, _)| w == h && w.is_power_of_two())
        .unwrap_or((1, 1, vec![0; 4]))
}
impl<'a> Game<'a> {
    fn new(
        res: &'a Arc<Resources>,
        pack: PackResources,
        gui: UiResources,
        options: Options,
        settings: Settings,
        audio: Audio,
        prepared: Option<GameCpuResources>,
    ) -> Result<Self> {
        let root = options.resources.clone();
        let registry = pack.assets.registry;
        let mut streaming = Streaming::new(
            Arc::new(TestTerrain::new(registry, res.biomes())?),
            pack.context.clone(),
            1,
        );
        streaming.paused = true;
        let level = Level::default();
        let save = Arc::new(SaveDir::new(
            std::env::current_dir()?
                .join("saves")
                .join(".vise-title"),
            vise_client::save::LevelShape::OVERWORLD,
        ));
        let prepared = prepared
            .map(Ok)
            .unwrap_or_else(|| GameCpuResources::load(&root))?;
        let world = World::new(res.facts().air, 0);
        let tutorial = tutorial::Tutorial::new(tutorial::Step::from_option(
            settings.file.int("tutorialStep"),
        ));
        let creative = options.creative;
        let inventory = vec![None; 36];
        let now = Instant::now();
        let look = options.look;
        let particles = vise_client::particles::ParticleEngine::new(
            &root,
            &gui.atlas.sprites,
            &gui.atlas.translucent,
        );
        Ok(Game {
            res: res.clone(),
            rest_loader: None,
            pending_entry: None,
            world_loader: None,
            pending_pack_reload: false,
            pack_reload: None,
            server_packs: Default::default(),
            link_prompt: None,
            code_of_conduct: None,
            dialogs: Default::default(),
            integrated_starting: None,
            world_renderer_ready: false,
            collision: res.collision(),
            motion: res.motion(),
            pack,
            gui,
            fog: Default::default(),
            screen_fx: Default::default(),
            item_activation: Default::default(),
            weather_parameters: Default::default(),
            panorama_rotation: Default::default(),
            world,
            streaming,
            lod: None,
            lod_source_attempted: false,
            far_beacons: HashMap::new(),
            lod_observe: Default::default(),
            lod_covered_at: None,
            options,
            player: None,
            spawn: None,
            clock: FixedStep::default(),
            previous_position: Vec3::ZERO,
            eye: 1.62,
            previous_eye: 1.62,
            yaw: look.map_or(level.rotation[0], |l| l.0),
            pitch: look.map_or(level.rotation[1], |l| l.1),
            inventory,
            selected: 0,
            carried: None,
            craft_grid: vec![None; 9],
            craft_size: 2,
            craft_table: None,
            inv_drag: None,
            tweaks: Default::default(),
            bundle_mouse: Default::default(),
            inv_clicks: Default::default(),
            last_inv_click: None,
            last_quick_moved: None,
            container: None,
            station: None,
            potions: Default::default(),
            scratch_slot: None,
            mining: None,
            destroying: HashMap::new(),
            destroy_delay: 0,
            use_delay: 0,
            creative,
            mode: if creative {
                vise_client::command::args::GameMode::Creative
            } else {
                vise_client::command::args::GameMode::Survival
            },
            keys: HashSet::new(),
            physical_keys: HashSet::new(),
            physical_buttons: HashSet::new(),
            left: false,
            right: false,
            cursor: (0.0, 0.0),
            cursor_icon: vise_client::widgets::Cursor::Arrow,
            captured: false,
            keyboard_mouse: true,
            view_at: [0, 0],
            focused: true,
            focus_lost: None,
            modifiers: winit::keyboard::ModifiersState::empty(),
            fullscreen: vise_client::window::FullscreenState::new(false),
            mouse: MouseLook::default(),
            agent_pointer: false,
            smooth_camera: false,
            toggles: Default::default(),
            was_playing: false,
            pause: false,
            debug: false,
            screen: Screen::Playing,
            window: None,
            renderer: None,
            entity_vertices: Vec::new(),
            dynamic_lights: Vec::new(),
            luminance: None,
            see_through_vertices: Vec::new(),
            entity_spans: Default::default(),
            glowing_vertices: Default::default(),
            pack_ids: None,
            health_hud: hud_layout::HealthHud::default(),
            started: now,
            spectator_gui: Default::default(),
            previous: now,
            last_input: now,
            pad: Default::default(),
            frames: 0,
            glint_frame: Default::default(),
            item_level_cache: Default::default(),
            world_frames: 0,
            fps: 0.0,
            fps_frames: 0,
            fps_at: now,
            ticks: 0,
            error: None,
            message: None,
            save,
            level,
            session_lock: None,
            audio,
            music: {
                let mut music = vise_client::music::MusicManager::default();
                music.frequency = vise_client::music::MusicFrequency::parse(
                    &settings.file.name("musicFrequency"),
                );
                music
            },
            ambience: Default::default(),
            sound_paused: false,
            cart_sounds: HashSet::new(),
            riding_cart_sounds: None,
            was_fall_flying: false,
            bee_sounds: HashMap::new(),
            guardian_sounds: HashSet::new(),
            dropped: Vec::new(),
            next_entity: 0,
            particles,
            pickups: Default::default(),
            sign_edit: None,
            book: None,
            book_last_click: None,
            lectern_pulses: Vec::new(),
            loading_progress: 0.0,
            spawn_origin: None,
            spawn_spiral: Default::default(),
            player_spawn: None,
            spawn_wait: Vec::new(),
            mobs: Vec::new(),
            creatures: Default::default(),
            sculk: Default::default(),
            raiding: Default::default(),
            attack_ticks: 100,
            attack_item: None,
            last_hurt_by_player: HashMap::new(),
            love_cause: HashMap::new(),
            died: HashSet::new(),
            be_anims: {
                let mut anims = vise_client::block_entity_anim::BlockEntityAnims::default();
                anims.classify(registry);
                anims
            },
            using: None,
            view: Default::default(),
            limiter: vise_client::window::FramerateLimiter::new(0),
            frame_clock: Default::default(),
            scope_scale: 0.5,
            frame_ticks: 0.0,
            title: None,
            armor: [None, None, None, None],
            offhand: None,
            hotbar_pop: [0; 10],
            hand: Hand::Main,
            shield_cooldown: 0,
            cooldowns: Default::default(),
            command_source: None,
            titles: Default::default(),
            tool_highlight: Default::default(),
            saving_indicator: Default::default(),
            autosaving: false,
            applied_modifiers: Vec::new(),
            experience: Experience::default(),
            orbs: Vec::new(),
            xp_delay: 0,
            last_level_up: 0,
            swing: Default::default(),
            swings_started: 0,
            heights: Default::default(),
            sway: Default::default(),
            visible_hands: [(0, None), (0, None)],
            hands_replaced: [false; 2],
            clicks: Default::default(),
            attack_clicks: 0,
            hand_strength: Default::default(),
            settings,
            tutorial,
            tutorial_toasts: Vec::new(),
            options_from_title: false,
            dragging: false,
            avatar: Default::default(),
            flash: 0,
            rain_sound_time: 0,
            lightning_seen: [0; 2],
            sleeping: None,
            sleep_timer: 0,
            player_beds: Vec::new(),
            sleep_seen: (0, 0),
            dim: Dim::Overworld,
            world_root: PathBuf::new(),
            world_flat: false,
            travelling: None,
            server_trips: Vec::new(),
            level_teleports: Vec::new(),
            levels: Default::default(),
            level_pass: Default::default(),
            portals: Default::default(),
            end_travel: false,
            respawn_config: None,
            respawn_travel: None,
            end_portal_travel: false,
            win: None,
            end: Default::default(),
            block_ticks: Ticks::new(0),
            tick_chunks: Default::default(),
            tickets: Default::default(),
            random_tick_sections: Default::default(),
            level_events: Vec::new(),
            falling: Vec::new(),
            paintings: Vec::new(),
            painting_variants: prepared.painting_variants,
            item_frames: Vec::new(),
            frame_model: prepared.frame_model,
            glow_frame_model: prepared.glow_frame_model,
            map_frame_model: prepared.map_frame_model,
            glow_map_frame_model: prepared.glow_map_frame_model,
            armor_stands: Vec::new(),
            cushions: Vec::new(),
            others: Vec::new(),
            armor_stand_assets: prepared.armor_stand_assets,
            waypoint_styles: prepared.waypoint_styles,
            waypoints: Default::default(),
            locator_camera: None,
            experience_display: Default::default(),
            boats: Vec::new(),
            boat_models: prepared.boat_models,
            crystal_model: prepared.crystal_model,
            cushion_model: prepared.cushion_model,
            carts: Vec::new(),
            jukebox_sounds: HashMap::new(),
            bolts: Vec::new(),
            maps: maps::Maps::default(),
            fight: dragon::Fight::default(),
            dragon: None,
            crystals: Vec::new(),
            dragon_fireballs: Vec::new(),
            breath_clouds: Vec::new(),
            dragon_model: prepared.dragon_model,
            map_patches: Vec::new(),
            text_displays: Default::default(),
            remote_knots: Default::default(),
            knot_ids: Default::default(),
            text_display_vertices: 0,
            skin_downloads: RefCell::new(vise_client::skins::Downloader::new(
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("assets/skins"),
            )),
            skin_profiles: RefCell::new(vise_client::skins::Resolver::new()),
            skin_slots: vise_client::skins::SkinSlots::new(vise_client::hud::SKIN_SLOTS),
            trim_slots: Vec::new(),
            cart_models: prepared.cart_models,
            riding: None,
            pending_vehicle: None,
            ender_items: vec![None; 27],
            ender_open: None,
            pending: Default::default(),
            spill: Default::default(),
            recipe_ui: Default::default(),
            mount_menu: Default::default(),
            hook: None,
            remote_hooks: Vec::new(),
            tridents: Vec::new(),
            rockets: Vec::new(),
            ominous_spawners: Vec::new(),
            creaking: Default::default(),
            spawners: Default::default(),
            spawn_scratch: Default::default(),
            trident_model: prepared.trident_model,
            arrow_model: prepared.arrow_model,
            tnts: Vec::new(),
            hoppers: HashMap::new(),
            item_columns: None,
            creative_screen: prepared.creative_screen,
            control: None,
            frame_stats: Default::default(),
            perf: Default::default(),
            pending_screenshot: None,
            pending_bench: None,
            flight: None,
            ui_wait: Default::default(),
            events: Default::default(),
            agent: agent::Agent::default(),
            view_frustum: None,
            entity_culler: None,
            pack_shadow_casters: (false, false),
            pending_uploads: Vec::new(),
            frame_graph: Default::default(),
            extras: Default::default(),
            mods: Default::default(),
            recipe_tables: Default::default(),
            panel: Default::default(),
            native_shaders: Default::default(),
            native_lifecycle: Default::default(),
            sky_darken_memo: Default::default(),
            persist: Default::default(),
            remote: None,
            lan: None,
            integrated: None,
            level_shapes: Default::default(),
            serving: None,
            world_options: None,
            options_to_world_options: false,
            command_block_screen: None,
            friends: None,
            debug_keys: Default::default(),
            debug_charts: Default::default(),
            show_hitboxes: false,
            show_chunk_borders: false,
            pause_menu: true,
            previous_mode: None,
            gamemode_switcher: None,
            clipboard: Default::default(),
            text_selection: Default::default(),
            saved_for_exit: false,
            pending_save: None,
            world_icon: None,
            forced_hit: None,
            tick_hit: None,
            guest_use: None,
            lan_events: Vec::new(),
            lan_songs: Vec::new(),
            lan_cushion_breaks: Vec::new(),
            lan_block_events: Vec::new(),
            lan_entity_events: Vec::new(),
            lan_critical_hits: Vec::new(),
            lan_kinetic_hits: Vec::new(),
            stabbed: HashMap::new(),
            known_movement: Vec3::ZERO,
            kinetic_feedback: HashMap::new(),
            lan_global_events: Vec::new(),
            lid_openers: HashMap::new(),
            lan_acting: None,
            lan_break_pos: None,
            guest_packets: Vec::new(),
            guest_cooldowns: Vec::new(),
            guest_entity_events: Vec::new(),
            guest_player_sounds: Vec::new(),
            lan_acting_name: None,
            lan_acting_uuid: None,
            lan_acting_permission: None,
            chat_capture: None,
            admin_feedback: Vec::new(),
            console_broadcast: None,
            world_tables: None,
            cmds: Default::default(),
            ui: chat::UiState::new(),
            combat: Default::default(),
            guest_combat: Default::default(),
            mount: Default::default(),
            stats: Default::default(),
            score: Default::default(),
            fns: Default::default(),
            bosses: Default::default(),
            boss_overlay: Default::default(),
            exec: Default::default(),
            rand_sequences: Default::default(),
            brushing: Default::default(),
            border: Default::default(),
            border_renderer: Default::default(),
            border_state: Default::default(),
            tab_list: Default::default(),
            subtitles: Default::default(),
            adv: Default::default(),
            restore_player_state: true,
            stats_ui: Default::default(),
            adv_ui: Default::default(),
            links_ui: Default::default(),
            camera_type: Default::default(),
            fangs: Default::default(),
            player_model: None,
        })
    }
}

struct GameCpuResources {
    painting_variants: paintings::PaintingVariants,
    frame_model: frames::FrameModel,
    glow_frame_model: frames::FrameModel,
    map_frame_model: frames::FrameModel,
    glow_map_frame_model: frames::FrameModel,
    armor_stand_assets: armor_stands::ArmorStandAssets,
    waypoint_styles: vise_client::locator_bar::WaypointStyles,
    boat_models: boats::BoatModels,
    cart_models: minecarts::CartModels,
    crystal_model: Option<vise_client::mobs::Part>,
    cushion_model: Option<vise_client::mobs::Part>,
    dragon_model: Option<vise_client::mobs::Part>,
    arrow_model: Option<vise_client::mobs::Part>,
    trident_model: Option<vise_client::mobs::Part>,
    creative_screen: Creative,
}
impl GameCpuResources {
    fn load(root: &std::path::Path) -> Result<Self> {
        let document = vise_client::mobs::EntityModelDocument::load(root).ok();
        let model = |key| document.as_ref().and_then(|document| document.model(key));
        Ok(Self {
            painting_variants: paintings::PaintingVariants::load(root),
            frame_model: frames::bake_frame(root, "item_frame"),
            glow_frame_model: frames::bake_frame(root, "glow_item_frame"),
            map_frame_model: frames::bake_frame(root, "item_frame_map"),
            glow_map_frame_model: frames::bake_frame(root, "glow_item_frame_map"),
            armor_stand_assets: armor_stands::ArmorStandAssets::load(root, document.as_ref()),
            waypoint_styles: vise_client::locator_bar::WaypointStyles::load(root),
            boat_models: boats::BoatModels::load(document.as_ref()),
            cart_models: minecarts::CartModels::load(document.as_ref()),
            crystal_model: model("minecraft:end_crystal#main"),
            cushion_model: model("minecraft:cushion#main"),
            dragon_model: model("minecraft:ender_dragon#main"),
            arrow_model: model("minecraft:arrow#main"),
            trident_model: model("minecraft:trident#main"),
            creative_screen: Creative::load(root)?,
        })
    }
}

struct Loaded {
    game_cpu: Option<GameCpuResources>,
    renderer_cpu: Option<vise_client::gfx::PreparedTextures>,
    root: PathBuf,
    pack_error: Option<String>,
    resources: Arc<Resources>,
    rest_loader: Option<WorldRestLoader>,
    pack: PackResources,
    gui: Option<UiResources>,
}

enum LoaderMsg {
    Ui(UiResources, PathBuf),
    Done(std::result::Result<Loaded, String>),
}

enum PendingWorldEntry {
    Local {
        folder: String,
        name: Option<String>,
        seed: Option<i64>,
        creative: bool,
        create: Option<Box<vise_client::title::WorldOptions>>,
    },
    Server {
        address: String,
        name: String,
        cookies: Option<vise_client::net::Cookies>,
    },
}

struct WorldRestLoader {
    progress: Progress,
    handle: JoinHandle<std::result::Result<(), String>>,
}

impl WorldRestLoader {
    fn ready(&self) -> Result<bool> {
        Ok(self.handle.is_finished())
    }

    fn finish(self) -> Result<()> {
        self.handle
            .join()
            .map_err(|_| "world resource loader panicked")?
            .map_err(Into::into)
    }
}

impl Game<'_> {
    fn defer_world_entry(&mut self, entry: PendingWorldEntry) -> Result<bool> {
        self.poll_world_rest()?;
        if self.rest_loader.is_none() && self.world_loader.is_none() {
            return Ok(false);
        }
        self.perf.world_opening();
        if self.title.is_none() {
            self.title = Some(Title::new(
                std::env::current_dir()?.join("saves"),
                &self.options.resources,
            ));
        }
        self.pending_entry = Some(entry);
        log::info!("Vise: world entry waiting for world resources");
        Ok(true)
    }

    fn resume_world_entry(&mut self) -> Result<()> {
        self.poll_integrated_start()?;
        self.poll_world_rest()?;
        if let Some(loader) = &self.world_loader {
            if !loader.ready() {
                return Ok(());
            }
            let loader = self
                .world_loader
                .take()
                .ok_or("missing world initialization worker")?;
            let cancelled = loader.cancelled;
            let prepared = loader.finish();
            if !cancelled {
                self.finish_open_world(prepared?)?;
            }
        }
        if self.rest_loader.is_some() {
            return Ok(());
        }
        let on_world_screens = self.title.as_ref().is_some_and(|title| {
            !matches!(
                title.mode,
                vise_client::title::Mode::Main | vise_client::title::Mode::Multiplayer(_)
            )
        });
        world_loading::preload_world_data(
            on_world_screens,
            &self.options.resources,
            self.pack.assets.registry,
        );
        if std::mem::take(&mut self.pending_pack_reload) {
            self.reload_packs();
        }
        match self.pending_entry.take() {
            Some(PendingWorldEntry::Local {
                folder,
                name,
                seed,
                creative,
                create,
            }) => {
                self.open_world(&folder, name.as_deref(), seed, creative, create.as_deref())?;
            }
            Some(PendingWorldEntry::Server {
                address,
                name,
                cookies,
            }) => {
                self.connect_server_with_cookies(&address, &name, cookies)?;
            }
            None => {}
        }
        Ok(())
    }

    fn poll_world_rest(&mut self) -> Result<()> {
        if let Some(loader) = &self.rest_loader
            && loader.ready()?
        {
            self.rest_loader
                .take()
                .ok_or("missing world loader")?
                .finish()?;
        }
        Ok(())
    }

    fn items(&self) -> &Items {
        match &self.world_tables {
            Some(t) => &t.items,
            None => self.res.items(),
        }
    }
    fn recipes(&self) -> &Recipes {
        match &self.world_tables {
            Some(t) => &t.recipes,
            None => self.res.recipes(),
        }
    }
    fn stations(&self) -> &vise_client::stations::StationData {
        match &self.world_tables {
            Some(t) => &t.stations,
            None => self.res.stations(),
        }
    }
    fn pack_files(&self) -> vise_world::loot::PackOverlay<'_> {
        pack_files(&self.world_tables)
    }
    fn build_world_tables(&self) -> Option<WorldTables> {
        let files = functions::content_overlay(
            &self.world_root,
            &self.options.resources,
            self.cmds.vanilla_disabled,
        );
        if files.is_empty() {
            return None;
        }
        let overlay = vise_world::loot::PackOverlay { files: &files };
        let root = &self.options.resources;
        let items = match Items::load(root, self.pack.assets.registry, self.res.facts(), &overlay) {
            Ok(items) => items,
            Err(e) => {
                log::warn!("Vise: the world's data packs are not applied: {e}");
                return None;
            }
        };
        let recipes = match Recipes::load(root, &items, &overlay) {
            Ok(recipes) => recipes,
            Err(e) => {
                log::warn!("Vise: the world's data packs are not applied: {e}");
                return None;
            }
        };
        let stations = vise_client::stations::StationData::load(root, &items, &overlay);
        Some(WorldTables {
            files,
            items,
            recipes,
            stations,
        })
    }
    pub(super) fn reload_world_tables(&mut self) {
        if self.remote.is_some() {
            return;
        }
        self.world_tables = self.build_world_tables();
        vise_client::structure_blocks::forget_loot_data();
        lan::forget_sync_data();
        let overlay = pack_files(&self.world_tables);
        self.adv.tree = Arc::new(vise_client::advancements::Tree::load_with(
            &self.options.resources.join("data"),
            &overlay,
        ));
        self.adv.reset_tree();
    }

    fn join_world_rest(&mut self) -> Result<()> {
        if self.rest_loader.is_some() {
            let waited = Instant::now();
            log::info!("Vise: waiting for world resources");
            while self.rest_loader.is_some() {
                self.poll_world_rest()?;
                if self.rest_loader.is_some() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            log::info!(
                "Vise: world resources joined after {} ms",
                waited.elapsed().as_millis()
            );
        }
        if self.res.rest.get().is_none() {
            return Err("world resources are unavailable".into());
        }
        Ok(())
    }
}

const HOTBAR_DIGITS: [KeyCode; 9] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
    KeyCode::Digit7,
    KeyCode::Digit8,
    KeyCode::Digit9,
];
const NUMERIC_KEYS: [KeyCode; 11] = [
    KeyCode::Digit0,
    KeyCode::Numpad0,
    KeyCode::Numpad1,
    KeyCode::Numpad2,
    KeyCode::Numpad3,
    KeyCode::Numpad4,
    KeyCode::Numpad5,
    KeyCode::Numpad6,
    KeyCode::Numpad7,
    KeyCode::Numpad8,
    KeyCode::Numpad9,
];

const STAGE_LINES: [&str; 10] = [
    "Loading menu",
    "Loading assets",
    "Loading block facts",
    "Loading biomes",
    "Loading pack tables",
    "Loading collision",
    "Loading items",
    "Loading recipes",
    "Loading world resources",
    "World resources ready",
];

#[derive(Clone)]
struct Progress {
    value: Arc<AtomicU32>,
    lines: &'static [&'static str],
}
impl Progress {
    fn new() -> Self {
        Self::with(&STAGE_LINES)
    }
    fn with(lines: &'static [&'static str]) -> Self {
        Self {
            value: Arc::new(AtomicU32::new(0)),
            lines,
        }
    }
    fn set(&self, stage: u32) {
        self.value.store(stage, Ordering::Relaxed);
    }
    fn fraction(&self) -> f32 {
        self.value.load(Ordering::Relaxed) as f32 / (self.lines.len() - 1) as f32
    }
    fn line(&self) -> &'static str {
        self.lines[(self.value.load(Ordering::Relaxed) as usize).min(self.lines.len() - 1)]
    }
}

fn synchronous_startup(options: &Options) -> bool {
    options.frames.is_some()
        || options.world_frames.is_some()
        || options.benchmark.is_some()
        || options.control.is_some()
        || !options.execution_role.is_client()
        || std::env::var_os("VISE_DISPENSE_FIXTURE").is_some()
        || std::env::var_os("VISE_SCULK_FIXTURE").is_some()
        || std::env::var_os("VISE_COMMAND_SCRIPT").is_some()
        || std::env::var_os("VISE_NET_REPORT").is_some()
}

fn load_resources(
    mut root: PathBuf,
    vanilla_root: &std::path::Path,
    settings: &Settings,
    started: Instant,
    menu: bool,
    headless: bool,
    progress: &Progress,
    publish: &mut dyn FnMut(UiResources, &std::path::Path),
) -> Result<Loaded> {
    let requested_root = root.clone();
    let threads = settings.vise.worker_threads();
    let mut loaded_gui = None;
    let mut ui_ready = false;
    let mut pack_error = None;
    let load_ui = |root: &std::path::Path| {
        if headless {
            UiResources::server(root)
        } else {
            UiResources::load(root, vanilla_root)
        }
    };
    let ui = match load_ui(&root) {
        Ok(ui) => Some(ui),
        Err(e) if root != vanilla_root => {
            log::warn!("Vise: resource packs failed to load ({e}); using the default resources");
            pack_error = Some(e.to_string());
            root = vanilla_root.to_path_buf();
            None
        }
        Err(_) => None,
    };
    if let Some(ui) = ui {
        if menu {
            vise_client::telemetry::mark_ui_ready();
            log::info!(
                "Vise: UI bundle ready after {} ms",
                started.elapsed().as_millis()
            );
            publish(ui, &root);
        } else {
            loaded_gui = Some(ui);
        }
        ui_ready = true;
    }
    progress.set(0);
    let reload = |assets: &Assets, root: &std::path::Path| {
        if headless {
            assets.reload_for_server(root)
        } else {
            assets.reload(root)
        }
    };
    let (assets, render) = match Assets::load(&root).and_then(|assets| {
        let render = reload(&assets, &root)?;
        Ok((assets, render))
    }) {
        Ok(bundle) => bundle,
        Err(e) if root != vanilla_root => {
            log::warn!("Vise: resource packs failed to load ({e}); using the default resources");
            pack_error = Some(e.to_string());
            root = vanilla_root.to_path_buf();
            let assets = Assets::load(vanilla_root)?;
            let render = reload(&assets, vanilla_root)?;
            (assets, render)
        }
        Err(e) => {
            return Err(format!(
                "{e}\nPrepare resources with the launcher, or: vise-prepare --prepare"
            )
            .into());
        }
    };
    let assets = Arc::new(assets);
    let render = Arc::new(render);
    if !ui_ready || root != requested_root {
        let ui = load_ui(&root)?;
        if menu {
            vise_client::telemetry::mark_ui_ready();
            log::info!(
                "Vise: UI bundle ready after {} ms",
                started.elapsed().as_millis()
            );
            publish(ui, &root);
        } else {
            loaded_gui = Some(ui);
        }
    }
    log::info!("load assets: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(1);
    let behaviour_json: serde_json::Value = serde_json::from_reader(std::io::BufReader::new(
        std::fs::File::open(root.join("block-behaviours.json"))?,
    ))?;
    let registry = assets.registry;
    let load_facts = || {
        BlockFacts::load_with_projection(&root, registry, &behaviour_json)
            .map_err(|e| e.to_string())
    };
    let load_biomes = || Biomes::load(&root).map_err(|e| e.to_string());
    let load_environment = || Environment::load(&root).map_err(|e| e.to_string());
    let (facts, biomes, environment) = if threads > 1 {
        std::thread::scope(|scope| {
            let facts = scope.spawn(load_facts);
            let biomes = scope.spawn(load_biomes);
            let environment = scope.spawn(load_environment);
            (
                facts.join().unwrap(),
                biomes.join().unwrap(),
                environment.join().unwrap(),
            )
        })
    } else {
        (load_facts(), load_biomes(), load_environment())
    };
    let (facts, gameplay_projection) =
        facts.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let biomes = Arc::new(biomes.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?);
    let environment = environment.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    log::info!("load facts: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(2);
    log::info!("load biomes+env: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(3);
    let light = Arc::new(LightTables::load(&root, assets.registry, &facts)?);
    let pack = PackResources::load(&root, assets.clone(), render, &facts, biomes.clone(), light)?;
    log::info!("load pack tables: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(4);
    if !menu {
        vise_client::ui::load_metadata(&root);
        vise_client::lang::set(vise_client::lang::Language::load(
            &[&root],
            &settings.language,
        ));
    }
    let registry_owner = assets.clone();
    let collision = collisions::load(&root, registry_owner.registry)?;
    let motion = MotionResources::load_with_facts(&root, &registry_owner, &facts)?;
    log::info!(
        "load collision+motion: {:.2}s",
        started.elapsed().as_secs_f32()
    );
    progress.set(5);
    let resources = Resources {
        core: WorldCore {
            facts,
            biomes: biomes.clone(),
            environment,
            collision,
            motion,
            threads,
        },
        rest: std::sync::OnceLock::new(),
    };
    let resources = Arc::new(resources);
    if menu {
        vise_client::telemetry::mark_core_ready();
        log::info!(
            "Vise: core-ready after {} ms",
            started.elapsed().as_millis()
        );
    }
    let prepared_at = Instant::now();
    let game_cpu = menu.then(|| GameCpuResources::load(&root)).transpose()?;
    let renderer_cpu = menu.then(|| prepare_renderer_textures(&pack));
    if menu {
        log::info!(
            "Vise: handoff CPU resources prepared on worker in {} ms",
            prepared_at.elapsed().as_millis()
        );
    }
    let visibility = pack.context.tables.visibility.clone();
    let rest_loader = if menu {
        let resources = resources.clone();
        let root = root.clone();
        let rest_progress = progress.clone();
        let progress = progress.clone();
        let handle = std::thread::Builder::new()
            .name("world-rest-loader".into())
            .spawn(move || {
                let rest = load_world_rest(
                    &root,
                    &resources,
                    &assets,
                    &behaviour_json,
                    &gameplay_projection,
                    visibility,
                    &progress,
                    started,
                )
                .map_err(|error| error.to_string())?;
                resources
                    .rest
                    .set(rest)
                    .map_err(|_| "world resources already loaded".to_owned())?;
                progress.set(9);
                vise_client::telemetry::mark_rest_ready();
                log::info!(
                    "Vise: rest-ready after {} ms",
                    started.elapsed().as_millis()
                );
                Ok(())
            })?;
        Some(WorldRestLoader {
            progress: rest_progress,
            handle,
        })
    } else {
        let rest = load_world_rest(
            &root,
            &resources,
            &assets,
            &behaviour_json,
            &gameplay_projection,
            visibility,
            progress,
            started,
        )?;
        resources
            .rest
            .set(rest)
            .map_err(|_| "world resources already loaded")?;
        None
    };
    Ok(Loaded {
        game_cpu,
        renderer_cpu,
        root,
        pack_error,
        resources,
        rest_loader,
        pack,
        gui: loaded_gui,
    })
}

fn load_world_rest(
    root: &std::path::Path,
    resources: &Resources,
    assets: &Assets,
    behaviour_json: &serde_json::Value,
    gameplay_projection: &[vise_client::blocks::GameplayProjection],
    visibility: vise_world::block_render::FaceVisibility,
    progress: &Progress,
    started: Instant,
) -> Result<WorldRest> {
    let facts = resources.facts();
    let biomes = resources.biomes();
    let threads = resources.threads();
    let registry_owner = assets;
    progress.set(6);
    let mob_data = vise_client::mobs::MobData::load(root, facts);
    vise_client::mobs::trial_spawner::load_registry(root);

    let items = Items::load(
        root,
        assets.registry,
        facts,
        &vise_world::loot::PackOverlay::NONE,
    )?;
    log::info!("load items: {:.2}s", started.elapsed().as_secs_f32());
    progress.set(7);
    let recipes = Recipes::load(root, &items, &vise_world::loot::PackOverlay::NONE)?;
    let transformers = vise_client::block_transform::Transformers::load(root, assets.registry)?;
    log::info!(
        "Vise: resources loaded in {:.1}s; {} block states, {} items, {} biomes; {} worker threads",
        started.elapsed().as_secs_f32(),
        facts.states.len(),
        items.items.len(),
        biomes.list.len(),
        threads
    );
    progress.set(8);
    let stations = vise_client::stations::StationData::load(
        root,
        &items,
        &vise_world::loot::PackOverlay::NONE,
    );
    let animate =
        vise_client::animate_tick::AnimateTables::load(root, registry_owner.registry, biomes)?;
    let fog = vise_client::fog::FogTables::load(root, biomes);
    Ok(WorldRest {
        stations,
        items,
        recipes,
        transformers,
        providers: Providers::load(root),
        block_entity_render:
            vise_client::block_entity_render::BlockEntityRenderer::load_with_visibility(
                root,
                registry_owner.registry,
                visibility,
            )?,
        animate,
        fog,
        music: vise_client::music::MusicTables::load(root),
        ambient: vise_client::ambience::AmbientTables::load(root),
        nether_environment: Environment::load_dimension(root, "the_nether", false)?,
        end_environment: Environment::load_dimension(root, "the_end", false)?,
        behaviours: Behaviours::load_with_gameplay(
            registry_owner.registry,
            root,
            behaviour_json,
            gameplay_projection,
        )?,
        mob_types: MobTypes::load(root)?
            .types
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect(),
        mob_data,
        player_type: perspective::player_type(root),
        player_arm: player_arms(root, "minecraft:player#main"),
        player_arm_slim: player_arms(root, "minecraft:player_slim#main"),
        spin_attack: vise_client::mobs::load_model(root, "minecraft:spin_attack#main"),
    })
}

fn prepare_renderer_textures(pack: &PackResources) -> vise_client::gfx::PreparedTextures {
    let assets = &pack.render;
    vise_client::gfx::PreparedTextures::new(
        &assets.pixels,
        assets.sprite_size,
        assets.texture_layers,
        &assets.sprite_mips,
        (&pack.forcefield.2, pack.forcefield.0),
    )
}

fn build_renderer(
    window: Arc<Window>,
    offscreen: bool,
    pack: &PackResources,
    gui: &UiResources,
    previous: Option<&Renderer>,
    prepared: Option<vise_client::gfx::PreparedTextures>,
    api: vise_client::graphics::GraphicsApi,
) -> Result<Renderer> {
    let assets = &pack.render;
    let atlas = &gui.atlas;
    atlas.font.reset_cache();
    let mut renderer = pollster::block_on(Renderer::new(
        window,
        offscreen,
        &assets.pixels,
        assets.sprite_size,
        assets.sprite_minimum_size,
        assets.texture_layers,
        &assets.sprite_mips,
        &assets.animations,
        (&atlas.pixels, atlas.width, atlas.height),
        (&atlas.glint.0, atlas.glint.1, atlas.glint.2),
        (
            &atlas.glint_armor.0,
            atlas.glint_armor.1,
            atlas.glint_armor.2,
        ),
        (&pack.forcefield.2, pack.forcefield.0),
        &gui.panorama,
        prepared,
        previous,
        api,
    ))?;
    renderer.hud.sprite_animations = atlas
        .sprite_animations
        .iter()
        .map(|(rect, animation)| (*rect, animation.clone(), (usize::MAX, 0, 0)))
        .collect();
    Ok(renderer)
}

fn loading_renderer(
    window: Arc<Window>,
    offscreen: bool,
    gui: Option<&UiResources>,
    previous: Option<&Renderer>,
    api: vise_client::graphics::GraphicsApi,
) -> Result<Renderer> {
    static FORCEFIELD: [u8; 4] = [0; 4];
    static EMPTY_HUD: [u8; 4] = [0, 0, 0, 255];
    static EMPTY_GLINT: [u8; 4] = [0; 4];
    let empty_panorama = vise_client::gfx::panorama::Faces {
        size: 1,
        pixels: [0u8; 4].repeat(6),
    };
    let panorama = gui.map_or(&empty_panorama, |gui| gui.panorama.as_ref());
    let atlas = gui.map(|gui| &gui.atlas);
    let (pixels, width, height) = match atlas {
        Some(atlas) => (atlas.pixels.as_slice(), atlas.width, atlas.height),
        None => (EMPTY_HUD.as_slice(), 1, 1),
    };
    let (glint_pixels, glint_width, glint_height) = match atlas {
        Some(atlas) => (atlas.glint.0.as_slice(), atlas.glint.1, atlas.glint.2),
        None => (EMPTY_GLINT.as_slice(), 1, 1),
    };
    if let Some(atlas) = atlas {
        atlas.font.reset_cache();
    }
    let mut renderer = pollster::block_on(Renderer::new(
        window,
        offscreen,
        &[255, 255, 255, 255],
        1,
        1,
        1,
        &[],
        &[],
        (pixels, width, height),
        (glint_pixels, glint_width, glint_height),
        (glint_pixels, glint_width, glint_height),
        (&FORCEFIELD[..], 1),
        panorama,
        None,
        previous,
        api,
    ))?;
    if let Some(atlas) = atlas {
        renderer.hud.sprite_animations = atlas
            .sprite_animations
            .iter()
            .map(|(rect, animation)| (*rect, animation.clone(), (usize::MAX, 0, 0)))
            .collect();
    }
    Ok(renderer)
}

fn loading_frame(hud: &HudBatch) -> Frame<'_> {
    Frame {
        weather: &[],
        weather_time: vise_client::weather::Time::new(0, 0.0),
        camera: glam::DVec3::new(0.5, 100.0, 0.5),
        yaw: 0.0,
        pitch: 10.0,
        fov_degrees: 85.0,
        bob: glam::Mat4::IDENTITY,
        spin: glam::Mat4::IDENTITY,
        sky: vise_client::environment::Sky::default(),
        distance: 12.0,
        detail_distance: 12.0,
        depth_far: 1000.0,
        underwater: false,
        cave_culling: false,
        title: true,
        sky_blocked: false,
        skybox: false,
        end_sky: false,
        end_sky_sprite: None,
        environment_fog: None,
        water_fog: [0.0; 3],
        fog: None,
        dark_disc: false,
        outline: Vec::new(),
        outline_after_terrain: false,
        outline_high_contrast: false,
        debug_lines: &[],
        debug_lines_on_top: &[],
        debug_crosshair: &[],
        hud,
        glint_alpha: 0.75,
        glint_ticks: 0.0,
        nether_lighting: false,
        entities: &[],
        entity_spans: &[],
        glowing: &[],
        dynamic_lights: &[],
        clouds: None,
        lightning: Default::default(),
        additive: &[],
        see_through: &[],
        celestial: &[],
        stars: None,
        hand: &[],
        hand_fov_degrees: 70.0,
        screen: &[],
        pack: Default::default(),
        border: None,
        game_time: 0.0,
        shader_game_time: 0.0,
        end_portal_sprites: [[0.0; 4]; 2],
        crumbling_sprite: [0.0; 4],
        ticks: 0,
        darkness: (0.0, 0.0),
        night_vision: 0.0,
        block_flicker: 0.0,
        boss_darken: 0.0,
        menu_blur: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn start_game<'a>(
    res: &'a Arc<Resources>,
    pack: PackResources,
    gui: UiResources,
    options: Options,
    pack_error: Option<String>,
    audio: Audio,
    vanilla_root: PathBuf,
    started: Instant,
    rest_loader: Option<WorldRestLoader>,
    settings: Option<Settings>,
    prepared: Option<GameCpuResources>,
) -> Result<Option<Game<'a>>> {
    let handoff = prepared.is_some();
    let mut settings = match settings {
        Some(settings) => settings,
        None => Settings::load(&std::env::current_dir()?.join("options.txt")),
    };
    if let Some(d) = options.distance_override {
        settings.render_distance = d;
    }
    let mut game = Game::new(res, pack, gui, options, settings, audio, prepared)?;
    game.rest_loader = rest_loader;
    game.ui.vanilla_root = vanilla_root;
    if let Some(error) = pack_error {
        game.settings.resource_packs.clear();
        game.settings.save(&Game::options_path());
        game.open_options_at(true, vise_client::options_ui::Page::Packs);
        game.ui.options.resource_error = Some(error);
    }
    if game.options.benchmark.is_some() {
        game.settings.vsync = false;
        game.settings.max_fps = 260;
        game.settings.inactivity_fps_limit = "minimized".into();
        game.options.no_pause = true;
    }
    game.apply_settings();
    if game.options.benchmark.is_none() {
        game.mods.dev = game.options.dev_mods.clone();
        game.start_mods();
    }
    game.start_agent_control()?;
    if let Some(scenario) = game.options.benchmark.clone() {
        let control = game
            .control
            .get_or_insert_with(vise_client::control::Server::local);
        vise_client::benchmark::start(
            control.sender(),
            vise_client::benchmark::Setup {
                scenario,
                started,
                app: vise_bench::App {
                    name: "vise-client".into(),
                    version: vise_client::crash::version(),
                    commit: option_env!("VISE_COMMIT").map(str::to_owned),
                    mods: Vec::new(),
                },
                simulation_distance: game.settings.simulation_distance as u32,
                vsync: false,
                fps_cap: None,
                offscreen: game.options.offscreen,
                game_dir: std::env::current_dir()?,
            },
        );
    }
    if let Ok(spec) = std::env::var("VISE_DISPENSE_FIXTURE") {
        game.run_component_harness(integrated::ComponentHarness::Dispenser(spec))?;
        return Ok(None);
    }
    if let Ok(spec) = std::env::var("VISE_SCULK_FIXTURE") {
        game.run_component_harness(integrated::ComponentHarness::Sculk(spec))?;
        return Ok(None);
    }
    if game.options.execution_role == execution_role::ExecutionRole::DedicatedServer {
        game.run_dedicated()?;
        return Ok(None);
    }
    match game.options.world.clone() {
        _ if game.options.server.is_some() => {
            let (address, name) = (
                game.options.server.clone().unwrap_or_default(),
                game.options.player_name(),
            );
            game.connect_server(&address, &name)?;
            if let Some(report) = std::env::var_os("VISE_NET_REPORT") {
                game.run_net_harness(report.as_ref())?;
                return Ok(None);
            }
        }
        Some(folder) => {
            let (seed, creative) = (game.options.seed, game.options.creative);
            if std::env::var_os("VISE_COMMAND_SCRIPT").is_some() {
                game.open_integrated(&folder, None, Some(seed), creative, None)?;
            } else {
                game.title = Some(Title::new(
                    std::env::current_dir()?.join("saves"),
                    &game.options.resources,
                ));
                game.pending_entry = Some(PendingWorldEntry::Local {
                    folder,
                    name: None,
                    seed: Some(seed),
                    creative,
                    create: None,
                });
            }
        }
        None if handoff => {}
        None => {
            let mut title = Title::new(
                std::env::current_dir()?.join("saves"),
                &game.options.resources,
            );
            title.last_server = game.settings.file.string("lastServer");
            title.skip_multiplayer_warning = game.settings.file.bool("skipMultiplayerWarning");
            game.title = Some(title);
        }
    }
    if std::env::var_os("VISE_COMMAND_SCRIPT").is_some()
        && std::env::var_os("VISE_COMMAND_OUTPUT").is_some()
    {
        game.wait_command_harness()?;
        return Ok(None);
    }
    Ok(Some(game))
}

fn write_report(game: &Game<'_>) -> Result<()> {
    if let Some(path) = &game.options.report {
        let report = serde_json::json!({
            "frames": game.frames,
            "elapsed_seconds": game.started.elapsed().as_secs_f64(),
            "chunks": game.world.chunks.len(),
            "generated": game.streaming.generated_total,
            "meshed_sections": game.streaming.meshed_total,
            "player": game.player.as_ref().map(|p| [p.position.x, p.position.y, p.position.z]),
            "error": game.error.as_ref().map(|e| e.to_string()),
        });
        std::fs::write(path, serde_json::to_vec_pretty(&report)?)?;
    }
    Ok(())
}

fn leave_fullscreen_after(
    result: &std::result::Result<(), EventLoopError>,
    settings: &mut Settings,
) {
    if matches!(result, Err(EventLoopError::ExitFailure(_))) && settings.fullscreen {
        settings.fullscreen = false;
        settings.save(&Game::options_path());
        log::warn!(
            "the window system closed the game in fullscreen; the next start opens windowed"
        );
    }
}

fn run_event_loop(mut game: Game<'_>) -> Result<()> {
    let result = EventLoop::new()?.run_app(&mut game);
    leave_fullscreen_after(&result, &mut game.settings);
    result?;
    vise_client::discord_presence::stop();
    write_report(&game)?;
    if let Some(e) = game.error.take() {
        return Err(e);
    }
    Ok(())
}

struct Loading {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    gui: Option<UiResources>,
    settings: Settings,
    fullscreen: vise_client::window::FullscreenState,
    options: Option<Options>,
    vanilla_root: PathBuf,
    pack_error: Option<String>,
    started: Instant,
    panorama_rotation: vise_client::gfx::panorama::Rotation,
    progress: Progress,
    receiver: Receiver<LoaderMsg>,
    loaded: Option<std::result::Result<Loaded, String>>,
    delivered: bool,
    handle: Option<JoinHandle<()>>,
    shown: bool,
    menu: startup_menu::StartupMenu,
    splash: Option<(UiResources, vise_client::loading_splash::LoadingSplash)>,
    error: Option<Box<dyn std::error::Error>>,
}
impl Loading {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let fullscreen =
            self.settings.fullscreen || self.options.as_ref().is_some_and(|o| o.fullscreen);
        let size = self
            .options
            .as_ref()
            .map_or([1280, 720], |o| o.window_size)
            .map(f64::from);
        self.fullscreen = vise_client::window::FullscreenState::new(fullscreen);
        let attributes = Window::default_attributes()
            .with_title("Vise")
            .with_window_icon(vise_client::window::icon());
        #[cfg(target_os = "linux")]
        let attributes = winit::platform::wayland::WindowAttributesExtWayland::with_name(
            attributes, "vise", "vise",
        );
        let window = match event_loop.create_window(
            attributes
                .with_inner_size(winit::dpi::LogicalSize::new(size[0], size[1]))
                .with_fullscreen(fullscreen.then_some(winit::window::Fullscreen::Borderless(None))),
        ) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.error = Some(e.into());
                event_loop.exit();
                return;
            }
        };
        vise_client::telemetry::note_display(&window);
        let offscreen = self.options.as_ref().is_some_and(|o| o.offscreen);
        if self.gui.is_none() && !offscreen {
            self.splash = vise_client::loading_splash::atlas().map(|atlas| {
                (
                    UiResources {
                        atlas,
                        panorama: Arc::new(vise_client::gfx::panorama::Faces {
                            size: 1,
                            pixels: [0u8; 4].repeat(6),
                        }),
                    },
                    vise_client::loading_splash::LoadingSplash::new(),
                )
            });
        }
        match loading_renderer(
            window.clone(),
            offscreen,
            self.gui.as_ref().or(self.splash.as_ref().map(|s| &s.0)),
            None,
            self.settings.vise.api,
        ) {
            Ok(r) => self.renderer = Some(r),
            Err(e) => {
                self.error = Some(e);
                event_loop.exit();
                return;
            }
        }
        vise_client::hud::GUI_SCALE.store(self.settings.gui_scale, Ordering::Relaxed);
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, mut event: WindowEvent) {
        if self.window.as_ref().is_none_or(|w| w.id() != id) {
            return;
        }
        follow_keymap(&mut event);
        if let Err(error) = self.menu_event(event_loop, event) {
            self.error = Some(error);
            event_loop.exit();
        }
    }
    fn set_ui(&mut self, gui: UiResources, root: PathBuf) {
        log::info!(
            "Vise: loading the title artwork after {} ms",
            self.started.elapsed().as_millis()
        );
        self.menu.set_root(root, &self.settings);
        self.gui = Some(gui);
        vise_client::ui_theme::reset_atlas();
        self.apply_menu_settings();
        let Some(window) = self.window.clone() else {
            return;
        };
        let offscreen = self.options.as_ref().is_some_and(|o| o.offscreen);
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
        }
        match loading_renderer(
            window,
            offscreen,
            self.gui.as_ref(),
            self.renderer.as_ref(),
            self.settings.vise.api,
        ) {
            Ok(r) => {
                vise_client::hud::GUI_SCALE.store(self.settings.gui_scale, Ordering::Relaxed);
                self.renderer = Some(r);
            }
            Err(e) => self.error = Some(e),
        }
        self.splash = None;
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.error.is_some() {
            event_loop.exit();
            return;
        }
        if vise_threads::stop_requests() > 0 {
            event_loop.exit();
            return;
        }
        if let Err(error) = self.poll_menu_resources() {
            self.error = Some(error);
            event_loop.exit();
            return;
        }
        if let Some(w) = &self.window {
            if self.options.as_ref().is_some_and(|o| o.offscreen) {
                self.draw();
            } else {
                w.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }
    fn draw(&mut self) {
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let mut batch = HudBatch::new(size);
        self.draw_menu(&mut batch);
        if self.gui.is_none()
            && let Some((gui, splash)) = &mut self.splash
        {
            let mut ui = vise_client::ui_theme::Ui::new(&gui.atlas, &mut batch, (0.0, 0.0));
            splash.draw(
                &mut ui,
                self.progress.fraction(),
                self.progress.line(),
                self.started.elapsed().as_secs_f32(),
            );
            ui.finish();
        }
        let mut frame = loading_frame(&batch);
        frame.yaw = self.panorama_rotation.update(
            self.started.elapsed().as_secs_f32(),
            self.settings.file.double("panoramaScrollSpeed") as f32,
        );
        if let Some(renderer) = &mut self.renderer {
            match renderer.render(&frame) {
                Ok(true) if self.gui.is_some() && self.menu.pending.is_none() => {
                    self.menu.perf.title_shown()
                }
                Ok(false) if self.gui.is_none() => vise_client::ui_theme::reset_atlas(),
                Ok(_) => {}
                Err(e) => log::warn!("Vise: loading screen frame failed: {e}"),
            }
        }
        if !self.shown {
            self.shown = true;
            log::info!(
                "Vise: loading screen shown after {} ms",
                self.started.elapsed().as_millis()
            );
        }
    }
    fn finish(&mut self, loaded: Loaded) -> Result<Option<Game<'static>>> {
        let handoff_at = Instant::now();
        let mut options = self.options.take().ok_or("loading already finished")?;
        options.resources = loaded.root.clone();
        options.world = None;
        options.server = None;
        let gui = self.gui.take().ok_or("missing menu bundle")?;
        let window = self.window.clone().ok_or("window closed")?;
        if let Some(renderer) = &mut self.renderer {
            renderer.release_surface();
        }
        let renderer_at = Instant::now();
        let renderer = build_renderer(
            window.clone(),
            options.offscreen,
            &loaded.pack,
            &gui,
            self.renderer.as_ref(),
            loaded.renderer_cpu,
            self.settings.vise.api,
        )?;
        log::info!(
            "Vise: world renderer ready after {} ms",
            self.started.elapsed().as_millis()
        );
        let renderer_ms = renderer_at.elapsed().as_millis();
        let audio = if !options.execution_role.is_client() {
            Audio::silent(&options.resources)
        } else {
            Audio::with_device(&options.resources, &self.settings.sound_device)
        };
        let pack_error = loaded.pack_error.or_else(|| self.pack_error.take());
        let res: &'static Arc<Resources> = Box::leak(Box::new(loaded.resources));
        let game_at = Instant::now();
        let Some(mut game) = start_game(
            res,
            loaded.pack,
            gui,
            options,
            pack_error,
            audio,
            self.vanilla_root.clone(),
            self.started,
            loaded.rest_loader,
            Some(self.settings.clone()),
            loaded.game_cpu,
        )?
        else {
            return Ok(None);
        };
        let game_ms = game_at.elapsed().as_millis();
        game.window = Some(window);
        game.renderer = Some(renderer);
        game.world_renderer_ready = true;
        game.fullscreen = self.fullscreen;
        game.panorama_rotation = self.panorama_rotation;
        self.menu.handoff(&mut game);
        game.apply_settings();
        game.previous = Instant::now();
        log::info!(
            "Vise: startup handoff {} ms (renderer {} ms, start_game {} ms)",
            handoff_at.elapsed().as_millis(),
            renderer_ms,
            game_ms
        );
        Ok(Some(game))
    }
}

enum App {
    Loading(Box<Loading>),
    Ready(Box<Game<'static>>),
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self {
            App::Loading(loading) => loading.resumed(event_loop),
            App::Ready(game) => game.resumed(event_loop),
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match self {
            App::Loading(loading) => loading.window_event(event_loop, id, event),
            App::Ready(game) => game.window_event(event_loop, id, event),
        }
    }
    fn device_event(&mut self, event_loop: &ActiveEventLoop, id: DeviceId, event: DeviceEvent) {
        if let App::Ready(game) = self {
            game.device_event(event_loop, id, event);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        enum Step {
            Waiting,
            Ready(Box<Game<'static>>),
            Done,
            Failed(Box<dyn std::error::Error>),
        }
        let step = match self {
            App::Ready(game) => {
                game.about_to_wait(event_loop);
                return;
            }
            App::Loading(loading) => {
                loading.about_to_wait(event_loop);
                if loading.error.is_some() {
                    event_loop.exit();
                    return;
                }
                match if loading.menu.reload_packs {
                    None
                } else {
                    loading.loaded.take()
                } {
                    None => Step::Waiting,
                    Some(Err(e)) => Step::Failed(e.into()),
                    Some(Ok(loaded)) => match loading.finish(loaded) {
                        Ok(Some(game)) => Step::Ready(Box::new(game)),
                        Ok(None) => Step::Done,
                        Err(e) => Step::Failed(e),
                    },
                }
            }
        };
        match step {
            Step::Waiting => {}
            Step::Ready(game) => *self = App::Ready(game),
            Step::Done => event_loop.exit(),
            Step::Failed(e) => {
                if let App::Loading(loading) = self {
                    loading.error = Some(e);
                }
                event_loop.exit();
            }
        }
    }
    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        if let App::Ready(game) = self {
            game.exiting(event_loop);
        }
    }
}

fn missing_resources(root: &std::path::Path) -> Option<String> {
    if root.join("vise-resources.json").is_file() {
        return None;
    }
    let shown = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    Some(format!(
        "Vise's prepared game files are missing: no vise-resources.json in {}\n\
         Vise is started from its launcher: download it at {} (Download). It needs your \
         own Minecraft: Java Edition; the launcher signs you in and prepares the game files.\n\
         Developers: vise-prepare --prepare, or pass --resources <folder>",
        shown.display(),
        vise_client::no_resources::DOWNLOAD_URL
    ))
}

fn run_loading(
    options: Options,
    settings: Settings,
    vanilla_root: PathBuf,
    pack_error: Option<String>,
    started: Instant,
) -> Result<()> {
    let progress = Progress::new();
    let (receiver, loader) =
        startup_menu::spawn_loader(&options, &settings, &vanilla_root, started, &progress)?;
    let menu = startup_menu::StartupMenu::new(&options, pack_error.as_deref())?;
    let loading = Loading {
        window: None,
        renderer: None,
        gui: None,
        settings,
        fullscreen: vise_client::window::FullscreenState::new(false),
        options: Some(options),
        vanilla_root,
        pack_error,
        started,
        panorama_rotation: Default::default(),
        progress,
        receiver,
        loaded: None,
        delivered: false,
        handle: Some(loader),
        shown: false,
        menu,
        splash: None,
        error: None,
    };
    let mut app = App::Loading(Box::new(loading));
    let result = EventLoop::new()?.run_app(&mut app);
    match &mut app {
        App::Ready(game) => leave_fullscreen_after(&result, &mut game.settings),
        App::Loading(loading) => leave_fullscreen_after(&result, &mut loading.settings),
    }
    result?;
    vise_client::discord_presence::stop();
    match app {
        App::Ready(mut game) => {
            write_report(&game)?;
            if let Some(e) = game.error.take() {
                return Err(e);
            }
        }
        App::Loading(loading) => {
            if let Some(e) = loading.error {
                return Err(e);
            }
        }
    }
    Ok(())
}

pub use launch_api::{UsageError, exit_code};

pub fn run() -> Result<()> {
    let Some(mut options) = Options::read()? else {
        return Ok(());
    };
    if options.execution_role == execution_role::ExecutionRole::DedicatedServer
        && !dedicated::preflight(&mut options)?
    {
        return Ok(());
    }
    vise_threads::put_helpers_behind();
    if options.execution_role.is_client() {
        vise_threads::favour_current();
    }
    if options.execution_role.is_client() {
        vise_threads::catch_stop_signals();
    }
    let started = Instant::now();
    vise_client::spans::enable(options.spikes.is_some());
    let vanilla_root = options.resources.clone();
    if let Some(error) = missing_resources(&vanilla_root) {
        if !options.offscreen && !synchronous_startup(&options) {
            log::error!("Vise: {error}");
            let settings = Settings::load(&std::env::current_dir()?.join("options.txt"));
            return no_resources::show(&vanilla_root, &settings);
        }
        return Err(error.into());
    }
    if !options.offscreen && !synchronous_startup(&options) {
        vise_client::heartbeat::start();
    }
    let early_settings = Settings::load(&std::env::current_dir()?.join("options.txt"));
    if options.dedicated.is_none() {
        let automated = options.benchmark.is_some()
            || options.offscreen
            || options.frames.is_some()
            || options.world_frames.is_some();
        vise_client::discord_presence::start(early_settings.extras.discord_presence, automated);
    }
    Behaviours::set_cache_dir(std::env::current_dir()?.join(".vise-cache"));
    if !synchronous_startup(&options) {
        return run_loading(options, early_settings, vanilla_root, None, started);
    }
    let mut pack_error = None;
    options.resources = packs::startup_root(&vanilla_root, &early_settings.resource_packs, &[])
        .unwrap_or_else(|e| {
            log::warn!("Vise: resource packs not applied: {e}");
            pack_error = Some(e.to_string());
            vanilla_root.clone()
        });
    options.resources = mods::content_root(
        &options.resources,
        &early_settings.mods.enabled,
        &options.dev_mods,
    );
    let loaded = load_resources(
        options.resources.clone(),
        &vanilla_root,
        &early_settings,
        started,
        false,
        options.dedicated.is_some(),
        &Progress::new(),
        &mut |_, _| {},
    )?;
    let gui = loaded.gui.ok_or("missing menu bundle")?;
    options.resources = loaded.root.clone();
    let audio = if !options.execution_role.is_client() {
        Audio::silent(&options.resources)
    } else {
        Audio::with_device(&options.resources, &early_settings.sound_device)
    };
    let resources = loaded.resources;
    let game = start_game(
        &resources,
        loaded.pack,
        gui,
        options,
        loaded.pack_error.or(pack_error),
        audio,
        vanilla_root,
        started,
        loaded.rest_loader,
        None,
        None,
    )?;
    if let Some(game) = game {
        run_event_loop(game)?;
    }
    Ok(())
}

fn player_arms(root: &std::path::Path, key: &str) -> [Option<vise_client::mobs::Part>; 2] {
    let model = vise_client::mobs::load_model(root, key);
    ["right_arm", "left_arm"].map(|name| {
        model
            .as_ref()
            .and_then(|m| m.children.iter().find(|c| c.name == name).cloned())
    })
}

fn tick_sleep_timer(timer: u32, sleeping: bool) -> u32 {
    if sleeping {
        (timer + 1).min(100)
    } else if (1..109).contains(&timer) {
        timer + 1
    } else {
        0
    }
}

fn keyboard_button(playing: bool, action: KeyAction) -> Option<bool> {
    playing.then_some(action != KeyAction::Release)
}

fn clip_start_source(
    eye: [f64; 3],
    dir: [f64; 3],
    reach: f64,
    cell: [i32; 3],
    same_above: bool,
) -> Option<([i32; 3], [f64; 3])> {
    let height = if same_above {
        1.0
    } else {
        f64::from(8.0f32 / 9.0f32)
    };
    let probe_y = eye[1] + dir[1] * reach * 1.0e-7;
    if probe_y - f64::from(cell[1]) < height {
        let normals: [[i32; 3]; 6] = [
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
            [-1, 0, 0],
            [1, 0, 0],
        ];
        let mut nearest = normals[2];
        let mut best = f64::NEG_INFINITY;
        for n in normals {
            let dot =
                f64::from(n[0]) * dir[0] + f64::from(n[1]) * dir[1] + f64::from(n[2]) * dir[2];
            if dot > best {
                best = dot;
                nearest = n;
            }
        }
        return Some((nearest.map(|v| -v), eye));
    }
    if dir[1] >= 0.0 {
        return None;
    }
    let t = (f64::from(cell[1]) + height - eye[1]) / dir[1];
    if t > reach {
        return None;
    }
    let point = [
        eye[0] + dir[0] * t,
        eye[1] + dir[1] * t,
        eye[2] + dir[2] * t,
    ];
    (point[0].floor() as i32 == cell[0] && point[2].floor() as i32 == cell[2])
        .then_some(([0, 1, 0], point))
}

fn reset_world_progress(adv: &mut advancements::AdvState, stats: &mut stats::Stats) {
    *adv = Default::default();
    *stats = Default::default();
}

#[cfg(test)]
mod startup_cpu_tests {
    use super::*;

    #[test]
    fn a_folder_without_prepared_files_is_named_before_loading() {
        let root = std::env::temp_dir().join(format!("vise-unprepared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let missing = missing_resources(&root.join(".local/vise-client/26.3"))
            .expect("no manifest is reported");
        assert!(missing.contains("vise-resources.json"), "{missing}");
        assert!(missing.contains("vise-unprepared-"), "{missing}");
        assert!(missing.contains("launcher"), "{missing}");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("vise-resources.json"), "{}").unwrap();
        assert_eq!(missing_resources(&root), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn worker_prepares_owned_models_and_inventory_before_handoff() {
        let root = std::env::temp_dir().join(format!("vise-startup-cpu-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("entity-models.json"),
            r#"{
            "minecraft:end_crystal#main": {}, "minecraft:ender_dragon#main": {},
            "minecraft:arrow#main": {}, "minecraft:trident#main": {},
            "minecraft:armor_stand#main": {}, "minecraft:cushion#main": {}
        }"#,
        )
        .unwrap();
        std::fs::write(
            root.join("creative-tabs.json"),
            r#"[{
            "id":"minecraft:building_blocks", "items":["minecraft:stone"]
        }]"#,
        )
        .unwrap();
        let worker_root = root.clone();
        let prepared = std::thread::spawn(move || {
            GameCpuResources::load(&worker_root).map_err(|error| error.to_string())
        })
        .join()
        .unwrap()
        .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(prepared.crystal_model.is_some());
        assert!(prepared.cushion_model.is_some());
        assert!(prepared.dragon_model.is_some());
        assert!(prepared.arrow_model.is_some());
        assert!(prepared.trident_model.is_some());
        assert!(prepared.armor_stand_assets.main.is_some());
        assert_eq!(
            prepared.creative_screen.tabs[0].items[0].item.as_ref(),
            "minecraft:stone"
        );
        assert!(GameCpuResources::load(&root).is_err());
    }
}

#[cfg(test)]
mod runtime_reset_tests {
    use super::*;
    #[test]
    fn world_entry_invalidates_loaded_progress() {
        let mut adv = advancements::AdvState::default();
        let mut stats = stats::Stats::default();
        adv.loaded = true;
        stats.loaded = true;
        adv.progress
            .criteria
            .insert("minecraft:story/root".into(), Default::default());
        stats.add("mined", "minecraft:stone", 4);
        reset_world_progress(&mut adv, &mut stats);
        assert!(!adv.loaded && !stats.loaded);
        assert!(adv.progress.criteria.is_empty());
        assert!(stats.values.is_empty());
    }
}

#[cfg(test)]
mod entity_model_loading_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use vise_client::container_screen::{Button, Clicks, Release, Target};
    use vise_client::keymapping::{KeyAction, SPRINT, ToggleKeys};

    #[test]
    fn a_held_key_bound_to_a_button_repeats_its_clicks() {
        assert_eq!(keyboard_button(true, KeyAction::Press), Some(true));
        assert_eq!(keyboard_button(true, KeyAction::Repeat), Some(true));
        assert_eq!(keyboard_button(true, KeyAction::Release), Some(false));
        for action in [KeyAction::Press, KeyAction::Repeat, KeyAction::Release] {
            assert_eq!(keyboard_button(false, action), None);
        }
    }
    #[test]
    fn sleep_fade_lasts_ten_ticks_after_waking() {
        let alpha = |timer| match hud_overlays::sleep_overlay(320, 240, timer).first() {
            Some(hud_layout::GuiDraw::Fill { color, .. }) => color >> 24,
            None => 0,
            _ => panic!("sleep overlay must be a full-screen fill"),
        };
        assert_eq!(tick_sleep_timer(0, false), 0);
        let mut timer = 0;
        for _ in 0..50 {
            timer = tick_sleep_timer(timer, true);
        }
        assert_eq!(alpha(timer as i32), 110);
        for _ in 0..100 {
            timer = tick_sleep_timer(timer, true);
        }
        assert_eq!(timer, 100);
        assert_eq!(alpha(timer as i32), 220);
        for _ in 0..5 {
            timer = tick_sleep_timer(timer, false);
        }
        assert_eq!(alpha(timer as i32), 110);
        for _ in 0..5 {
            timer = tick_sleep_timer(timer, false);
        }
        assert_eq!(timer, 0);
        assert!(hud_overlays::sleep_overlay(320, 240, timer as i32).is_empty());
    }

    #[test]
    fn world_rest_poll_does_not_wait_for_a_blocked_worker() {
        let (release, wait) = std::sync::mpsc::channel();
        let loader = WorldRestLoader {
            progress: Progress::new(),
            handle: std::thread::spawn(move || {
                wait.recv().unwrap();
                Ok(())
            }),
        };
        assert!(!loader.ready().unwrap());
        release.send(()).unwrap();
        loader.finish().unwrap();
    }

    #[test]
    fn world_rest_poll_waits_for_a_slow_worker() {
        let (release, wait) = std::sync::mpsc::channel();
        let loader = WorldRestLoader {
            progress: Progress::new(),
            handle: std::thread::spawn(move || {
                wait.recv().unwrap();
                Ok(())
            }),
        };
        assert!(!loader.ready().unwrap());
        release.send(()).unwrap();
        while !loader.ready().unwrap() {
            std::thread::sleep(Duration::from_millis(1));
        }
        loader.finish().unwrap();
    }

    #[test]
    fn world_rest_worker_failure_reaches_the_client() {
        let loader = WorldRestLoader {
            progress: Progress::new(),
            handle: std::thread::spawn(|| Err("missing world table".to_owned())),
        };
        assert_eq!(
            loader.finish().unwrap_err().to_string(),
            "missing world table"
        );
    }

    #[test]
    fn world_rest_worker_panic_reaches_the_client() {
        let loader = WorldRestLoader {
            progress: Progress::new(),
            handle: std::thread::spawn(|| panic!("broken loader")),
        };
        assert_eq!(
            loader.finish().unwrap_err().to_string(),
            "world resource loader panicked"
        );
    }

    #[test]
    fn shared_resources_can_cross_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Resources>();
        shared::<PackResources>();
    }

    #[test]
    fn selecting_creative_tabs_does_not_drop_the_carried_stack_on_release() {
        let mut creative = Creative::from_json(&serde_json::json!({
            "schema": 2,
            "tabs": [
                {"type": "CATEGORY", "items": ["{count:1,id:\"minecraft:stone\"}"]},
                {"type": "INVENTORY"}
            ]
        }))
        .unwrap();
        for button in [Button::Left, Button::Right] {
            let mut clicks = Clicks::default();
            for tab in [1, 1, 0, 1] {
                select_creative_tab(&mut creative, &mut clicks, tab);
                assert_eq!(creative.selected, tab);
                assert_eq!(creative.tab().inventory, tab == 1);
                assert_eq!(
                    clicks.release(
                        &Target::<usize>::Outside,
                        button,
                        false,
                        false,
                        false,
                        false
                    ),
                    Release::Nothing
                );
            }
        }
    }

    #[test]
    fn a_ray_starting_inside_a_source_hits_it_at_the_start() {
        let eye = [0.5, 65.62, 0.5];
        let down = [0.0, -1.0, 0.0];
        assert_eq!(
            clip_start_source(eye, down, 4.5, [0, 65, 0], true),
            Some(([0, 1, 0], eye))
        );
        assert_eq!(
            clip_start_source(eye, [0.0, 1.0, 0.0], 4.5, [0, 65, 0], false),
            Some(([0, -1, 0], eye))
        );
        let ahead = [0.0, -0.2f64, 0.98f64.sqrt()];
        assert_eq!(
            clip_start_source(eye, ahead, 4.5, [0, 65, 0], true),
            Some(([0, 0, -1], eye))
        );
    }

    #[test]
    fn a_ray_starting_above_a_source_surface_enters_through_the_top() {
        let eye = [0.5, 65.95, 0.5];
        let hit = clip_start_source(eye, [0.0, -1.0, 0.0], 4.5, [0, 65, 0], false);
        let (face, point) = hit.unwrap();
        assert_eq!(face, [0, 1, 0]);
        assert!((point[1] - (65.0 + f64::from(8.0f32 / 9.0f32))).abs() < 1e-12);
        assert_eq!(
            clip_start_source(eye, [1.0, 0.0, 0.0], 4.5, [0, 65, 0], false),
            None
        );
        let shallow = [0.995f64.sqrt(), -0.0707, 0.0];
        assert_eq!(
            clip_start_source(eye, shallow, 4.5, [0, 65, 0], false),
            None
        );
    }

    #[test]
    fn stack_drop_requires_physical_ctrl_even_with_sprint_latched() {
        for sprint in [false, true] {
            let mut toggles = ToggleKeys::default();
            if sprint {
                toggles.key(SPRINT, KeyAction::Press, false, true);
                toggles.key(SPRINT, KeyAction::Release, false, true);
            }
            assert_eq!(toggles.is_down(SPRINT), sprint);

            for held in 0..4 {
                let mut physical_keys = HashSet::from([KeyCode::ShiftLeft, KeyCode::KeyQ]);
                if held & 1 != 0 {
                    physical_keys.insert(KeyCode::ControlLeft);
                }
                if held & 2 != 0 {
                    physical_keys.insert(KeyCode::ControlRight);
                }
                assert_eq!(drop_whole_stack(&physical_keys), held != 0);
                physical_keys.remove(&KeyCode::ControlLeft);
                physical_keys.remove(&KeyCode::ControlRight);
                assert!(!drop_whole_stack(&physical_keys));
                assert_eq!(toggles.is_down(SPRINT), sprint);
            }
        }
    }
}
