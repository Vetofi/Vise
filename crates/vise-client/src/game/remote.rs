use super::remote_chunks::{ChunkDecoder, ChunkFormat, Next};
use super::{Dim, Game, Mining, Screen};
use vise_client::first_person::{Hand, SwingAnimation};
use vise_client::{
    Result,
    blocks::Fluid,
    chunk::Chunk,
    containers::{BlockEntity, Furnace, FurnaceKind},
    entities::{Experience, ExperienceOrb, ItemEntity},
    hud::HudBatch,
    items::ItemStack,
    menu::Input,
    mobs::{Mob, MobType, Who},
    motion::{MotionWorld, PlayerMotion},
    net::{Event, Session},
    save::SaveDir,
    sprint::Keys,
    streaming::{ChunkGenerator, Proto, Streaming},
    tab_list::SPECTATOR,
    world::World,
};
use vise_core::{geometry::Vec3, random::RandomSource};
use vise_protocol::{
    entity_data::{ITEM_ENTITY_ITEM, Value},
    item::{Hashed, Stack},
    packets::{
        Empty,
        play::{self, Clientbound as C, Serverbound as S},
    },
    registry::{Builtin, DimensionType, Dynamic, ceil_log2},
    secure_chat::Signature,
    text::{Language, plain},
};
use vise_world::{
    level::InteractionResult,
    nbt::Tag,
    registry::BlockStateId,
    stepped_interpolation::{Path, PosRot, Start, SteppedInterpolation},
};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{event::MouseButton, keyboard::KeyCode};

fn remote_text(tag: &Tag) -> vise_client::text::Text {
    use vise_client::scoreboard::{comp_text, tag_to_json};
    comp_text(&tag_to_json(tag))
}

fn menu_title(tag: &Tag) -> vise_client::text::Text {
    remote_text(tag).resolve_legacy()
}

type QueuedChat = (
    vise_client::text::Text,
    vise_client::chat::ChatSource,
    Option<Signature>,
);

struct NoTerrain;
impl ChunkGenerator for NoTerrain {
    type Worker = ();
    fn worker(&self) {}
    fn proto(&self, _: &mut (), _: i32, _: i32) -> Proto {
        Box::new(())
    }
    fn finish(&self, _: Proto) -> Chunk {
        Chunk::empty(0, 0)
    }
}

struct Tracked {
    kind: String,
    position: [f64; 3],
}
#[derive(Clone, Copy)]
struct Lerp {
    target: [f64; 3],
    rotation: Option<(f32, f32)>,
    steps: u32,
}

fn place_interpolated(m: &mut Mob, at: PosRot) {
    m.position = Vec3::new(at.position[0], at.position[1], at.position[2]);
    m.yaw += wrap_degrees(at.yaw - m.yaw);
    m.head_pitch = (at.pitch % 360.0).clamp(-90.0, 90.0);
}

enum Received {
    Event(Event),
    Chunk(Arc<ChunkFormat>),
}

pub(super) struct Remote {
    pub session: Session,
    pub brand: String,
    pub quiet_commands: bool,
    pub(super) builtin: Arc<Builtin>,
    lang: Arc<Language>,
    pub(super) registries: Arc<Dynamic>,
    joined: bool,
    pub(super) logged_in: bool,
    status: String,
    pub(super) entity_id: i32,
    pub(super) camera: Option<i32>,
    pub(super) use_item: Option<Arc<str>>,
    pub(super) dimension: DimensionType,
    pub(super) chunk_format: Arc<ChunkFormat>,
    decoder: ChunkDecoder,
    pub(super) view_distance: i32,
    sequence: i32,
    position_sender: vise_client::player_sync::PositionSender,
    last_input: u8,
    was_sprinting: bool,
    carried: Option<usize>,
    abilities: Option<play::PlayerAbilities>,
    loaded_sent: bool,
    sent_info: vise_protocol::packets::configuration::ClientInformation,
    predictions: HashMap<[i32; 3], (i32, i32)>,
    pub(super) menu: Option<(i32, String)>,
    pub(super) menu_title: Option<vise_client::text::Text>,
    state_id: i32,
    use_target: Option<[i32; 3]>,
    entities: HashMap<i32, Tracked>,
    lerps: HashMap<i32, Lerp>,
    stepped: HashMap<i32, SteppedInterpolation>,
    head_lerps: HashMap<i32, (f32, u32)>,
    pub(super) ghast_dismount: Option<u32>,
    pub(super) first_passenger: bool,
    pending_items: HashMap<i32, ([f64; 3], Vec3)>,
    health: (f32, i32, f32),
    flash_on_set_health: bool,
    game_mode: i32,
    pub chat: Vec<(String, Instant)>,
    pub chat_received: u64,
    chat_queue: VecDeque<QueuedChat>,
    signatures: vise_protocol::secure_chat::SignatureCache,
    chat_previous: Option<Instant>,
    pub commands: Option<vise_client::command::Dispatcher>,
    custom_completions: std::collections::BTreeSet<String>,
    players: HashMap<u128, String>,
    tab_players: HashMap<u128, vise_client::tab_list::TabPlayer>,
    skin_textures: HashMap<u128, vise_client::skins::Textures>,
    skins: HashMap<u128, String>,
    listed: std::collections::HashSet<u128>,
    online_mode: bool,
    level_players: HashMap<i32, u128>,
    level_player_names: HashMap<i32, String>,
    level_player_textures: HashMap<i32, vise_client::skins::Textures>,
    glowing: std::collections::HashSet<i32>,
    pub(super) own_invisible: bool,
    profile: u128,
    pub(super) profile_name: String,
    destroying: Option<([i32; 3], u8, f32)>,
    destroying_item: String,
    destroy_delay: u32,
    input: vise_client::interaction_input::InteractionInput,
    pub(super) pending_keys: Vec<KeyCode>,
    swap_answer: [bool; 2],
    stop_mining: bool,
    placing: Option<([i32; 3], u64)>,
    pub(super) flat: bool,
    death_message: Option<vise_client::text::Text>,
    show_death_screen: bool,
    player_type: Option<Arc<MobType>>,
    synced: Vec<Option<ItemStack>>,
    decode_errors: u32,
    clock: Option<i32>,
    clock_rate: f32,
    clock_partial: f32,
    clock_game_time: i64,
    chunk_center: (i32, i32),
    backlog: VecDeque<Received>,
    backlog_since: Option<Instant>,
    transfer: Option<String>,
    pub(super) server_links: Vec<vise_protocol::packets::configuration::ServerLink>,
    pub(super) debug_query: super::debug_keys::DebugQuery,
}

const CHUNK_PACKETS_PER_FRAME: Duration = Duration::from_millis(1);
const BACKLOG_LIMIT: Duration = Duration::from_millis(50);
const MOUNT_MENU: &str = "vise:mount";

fn face_direction(face: [i32; 3]) -> u8 {
    match face {
        [0, -1, 0] => 0,
        [0, 1, 0] => 1,
        [0, 0, -1] => 2,
        [0, 0, 1] => 3,
        [-1, 0, 0] => 4,
        [1, 0, 0] => 5,
        _ => 1,
    }
}

fn number_format(f: &play::NumberFormat) -> vise_client::scoreboard::NumberFormat {
    use vise_client::scoreboard::{NumberFormat, tag_to_json};
    match f {
        play::NumberFormat::Blank => NumberFormat::Blank,
        play::NumberFormat::Styled(t) => NumberFormat::Styled(tag_to_json(t)),
        play::NumberFormat::Fixed(t) => NumberFormat::Fixed(tag_to_json(t)),
    }
}

impl Game<'_> {
    pub(super) fn remote_sound_name(
        &self,
        sound: &(i32, Option<(String, Option<f32>)>),
    ) -> Option<String> {
        match &sound.1 {
            Some((location, _)) => Some(location.clone()),
            None => self.remote.as_ref().and_then(|r| {
                r.builtin
                    .get("minecraft:sound_event")?
                    .name(sound.0)
                    .map(str::to_owned)
            }),
        }
    }
    pub(super) fn connect_server(&mut self, address: &str, name: &str) -> Result<()> {
        self.connect_server_with_cookies(address, name, None)
    }
    pub(super) fn connect_server_with_cookies(
        &mut self,
        address: &str,
        name: &str,
        cookies: Option<vise_client::net::Cookies>,
    ) -> Result<()> {
        super::world_loading::preload_server_loot(&self.options.resources);
        if self.defer_world_entry(super::PendingWorldEntry::Server {
            address: address.to_owned(),
            name: name.to_owned(),
            cookies: cookies.clone(),
        })? {
            return Ok(());
        }
        self.join_world_rest()?;
        if let Err(error) = self.join_server(address, name, None, cookies) {
            log::warn!("Vise: could not join server: {error}");
            if self.title.is_none() {
                self.title = Some(vise_client::title::Title::new(
                    std::env::current_dir()?.join("saves"),
                    &self.options.resources,
                ));
            }
            let skip = self.settings.file.bool("skipMultiplayerWarning");
            if let Some(title) = &mut self.title {
                title.skip_multiplayer_warning = skip;
                title.failed(error);
            }
            self.capture(false);
        }
        Ok(())
    }
    pub(super) fn connect_local(
        &mut self,
        pipe: vise_protocol::connection::Pipe,
        name: &str,
    ) -> Result<()> {
        self.join_server("local", name, Some(pipe), None)
    }
    fn join_server(
        &mut self,
        address: &str,
        name: &str,
        local: Option<vise_protocol::connection::Pipe>,
        cookies: Option<vise_client::net::Cookies>,
    ) -> Result<()> {
        self.reset_tutorial();
        self.perf.world_opening();
        let root = self.options.resources.clone();
        let builtin = Builtin::find(&root)
            .and_then(|p| Builtin::load(&p).ok())
            .ok_or("Resources are incomplete: registries.json is missing or invalid. Click \"Prepare again\" in the launcher (or run vise-prepare), then try again.")?;
        let lang = Arc::new(vise_client::net::load_language(&root));
        let view = client_view_distance(self.settings.render_distance);
        if local.is_none() {
            vise_client::crash::set_detail(
                "Server",
                vise_client::net::shown_address(address),
            );
        }
        let auth = vise_client::auth::current();
        let name = auth.as_ref().map_or(name, |a| a.name.as_str());
        let user = auth.as_ref().map_or_else(
            || {
                (
                    name.to_owned(),
                    vise_protocol::crypto::offline_uuid(name),
                )
            },
            |a| (a.name.clone(), a.uuid),
        );
        self.server_packs_connect(local.is_none().then_some(address), user);
        let session = match local {
            Some(pipe) => Session::connect_local(pipe, name, view, lang.clone(), root.clone()),
            None => {
                self.extras_connecting(address, name);
                match cookies {
                    Some(cookies) => Session::connect_transferred(
                        address,
                        name,
                        auth.clone(),
                        view,
                        lang.clone(),
                        cookies,
                        root.clone(),
                    ),
                    None => Session::connect(
                        address,
                        name,
                        auth.clone(),
                        view,
                        lang.clone(),
                        root.clone(),
                    ),
                }
            }
        };
        let player_type = super::perspective::player_type(&root);
        let mut streaming = Streaming::new(
            Arc::new(NoTerrain),
            self.pack.context.clone(),
            self.res.threads(),
        );
        streaming.remote = true;
        streaming.radius = self.settings.render_distance;
        self.streaming = streaming;
        self.world = World::new(self.res.facts().air, 0);
        self.save = Arc::new(SaveDir::new(
            std::env::temp_dir().join("vise-remote-unused"),
            vise_client::save::LevelShape::OVERWORLD,
        ));
        self.inventory = vec![None; 36];
        self.armor = [None, None, None, None];
        self.craft_grid = vec![None; 9];
        self.craft_size = 2;
        self.player = None;
        self.spawn = None;
        self.mobs.clear();
        self.clear_remote_objects();
        self.clear_remote_projectiles();
        self.creatures.projectiles.clear();
        self.dropped.clear();
        self.particles.clear();
        self.pickups.clear();
        self.orbs.clear();
        self.falling.clear();
        self.travelling = None;
        self.mining = None;
        self.container = None;
        self.carried = None;
        self.using = None;
        self.clicks.miss_time = 0;
        self.potions = Default::default();
        self.set_mode_id(0);
        self.experience = Experience {
            level: 0,
            progress: 0.0,
            total: 0,
        };
        self.ui.registries = Default::default();
        self.build_registries();
        self.ui.registries.permission = 0;
        self.ui.chat.clear();
        self.screen = Screen::Playing;
        self.title = None;
        self.dim = Dim::Overworld;
        self.clear_lod();
        self.pending_uploads.clear();
        if let Some(r) = self.renderer.as_mut() {
            r.clear_sections(self.world.layout());
        }
        self.apply_settings();
        self.clear_waypoints();
        self.boss_overlay.reset();
        self.remote = Some(Remote {
            session,
            brand: "unknown".into(),
            quiet_commands: false,
            builtin: Arc::new(builtin),
            lang,
            registries: Arc::new(Dynamic::default()),
            joined: false,
            logged_in: false,
            status: "Connecting to the server...".into(),
            entity_id: 0,
            camera: None,
            use_item: None,
            dimension: DimensionType::vanilla("minecraft:overworld"),
            chunk_format: Arc::new(ChunkFormat::new(
                &DimensionType::vanilla("minecraft:overworld"),
                Vec::new(),
                7,
            )),
            decoder: ChunkDecoder::new(),
            view_distance: self.settings.render_distance,
            sequence: 0,
            position_sender: Default::default(),
            last_input: 0,
            was_sprinting: false,
            carried: None,
            abilities: None,
            loaded_sent: false,
            sent_info: self.settings.client_information(view),
            predictions: HashMap::new(),
            menu: None,
            menu_title: None,
            state_id: 0,
            use_target: None,
            entities: HashMap::new(),
            lerps: HashMap::new(),
            stepped: HashMap::new(),
            head_lerps: HashMap::new(),
            ghast_dismount: None,
            first_passenger: false,
            pending_items: HashMap::new(),
            health: (20.0, 20, 5.0),
            flash_on_set_health: false,
            game_mode: 0,
            chat: Vec::new(),
            chat_received: 0,
            chat_queue: VecDeque::new(),
            signatures: Default::default(),
            chat_previous: None,
            commands: None,
            custom_completions: Default::default(),
            players: HashMap::new(),
            tab_players: HashMap::new(),
            skin_textures: HashMap::new(),
            skins: HashMap::new(),
            listed: Default::default(),
            online_mode: false,
            level_players: HashMap::new(),
            level_player_names: HashMap::new(),
            level_player_textures: HashMap::new(),
            glowing: Default::default(),
            own_invisible: false,
            profile: 0,
            profile_name: String::new(),
            destroying: None,
            destroying_item: String::new(),
            destroy_delay: 0,
            input: Default::default(),
            pending_keys: Vec::new(),
            swap_answer: [false; 2],
            stop_mining: false,
            placing: None,
            flat: false,
            death_message: None,
            show_death_screen: true,
            player_type,
            synced: vec![None; CREATIVE_SYNCED],
            decode_errors: 0,
            clock: None,
            clock_rate: 1.0,
            clock_partial: 0.0,
            clock_game_time: 0,
            chunk_center: (0, 0),
            backlog: VecDeque::new(),
            backlog_since: None,
            server_links: Vec::new(),
            debug_query: Default::default(),
            transfer: None,
        });
        self.clock = vise_client::clock::FixedStep::client();
        self.adv = Default::default();
        self.stats = Default::default();
        Ok(())
    }

    pub(super) fn leave_server(&mut self, reason: Option<String>) -> Result<()> {
        if reason.is_none() && self.integrated.as_ref().is_some_and(|s| !s.stopped()) {
            return self.begin_closing_save(false);
        }
        self.finish_perf();
        if reason.is_none() {
            self.extras_left();
        }
        self.clear_waypoints();
        self.hud_disconnected();
        self.titles.overlay_time = 0;
        self.item_activation.reset();
        self.world_icon = None;
        if let Some(mut r) = self.remote.take() {
            r.session.disconnect();
        }
        self.gui.atlas.object_skins.clear();
        self.server_packs_disconnect();
        self.code_of_conduct = None;
        self.reset_tutorial();
        self.close_dialogs();
        self.keys.clear();
        self.left = false;
        self.right = false;
        let single_player = match self.integrated.take() {
            Some(server) => {
                server.stop()?;
                true
            }
            None => false,
        };
        vise_client::crash::clear_detail("Server");
        self.clock = Default::default();
        self.adv = Default::default();
        self.stats = Default::default();
        self.score.board = Default::default();
        self.streaming = Streaming::new(Arc::new(NoTerrain), self.pack.context.clone(), 1);
        self.streaming.paused = true;
        self.world = World::new(self.res.facts().air, 0);
        self.player = None;
        self.sleeping = None;
        self.sleep_timer = 0;
        self.mobs.clear();
        self.clear_remote_objects();
        self.clear_remote_projectiles();
        self.dropped.clear();
        self.particles.clear();
        self.pickups.clear();
        self.score.board = Default::default();
        self.tab_list = Default::default();
        self.border = Default::default();
        self.screen = Screen::Playing;
        self.container = None;
        self.clear_lod();
        self.pending_uploads.clear();
        if let Some(r) = self.renderer.as_mut() {
            r.clear_sections(self.world.layout());
        }
        let saves = std::env::current_dir()?.join("saves");
        let mut title = vise_client::title::Title::new(saves.clone(), &self.options.resources);
        title.last_server = self.settings.file.string("lastServer");
        title.skip_multiplayer_warning = self.settings.file.bool("skipMultiplayerWarning");
        if single_player && reason.is_none() {
            self.title = Some(title);
            self.capture(false);
            return Ok(());
        }
        title.mode = vise_client::title::Mode::Multiplayer(match reason {
            Some(reason) => vise_client::multiplayer::Screen::Disconnected {
                reason,
                retry: None,
            },
            None => vise_client::multiplayer::Screen::list(&saves),
        });
        self.title = Some(title);
        self.capture(false);
        Ok(())
    }
    fn hud_disconnected(&mut self) {
        self.tab_list.reset();
        self.boss_overlay.reset();
        self.adv.toasts.clear();
        self.debug_charts.reset();
        self.titles.clear();
        self.titles.reset_times();
    }

    pub(super) fn send(&self, packet: S) {
        if let Some(r) = &self.remote {
            r.session.send(packet);
        }
    }
    fn next_sequence(&mut self) -> i32 {
        let r = self.remote.as_mut().expect("remote");
        r.sequence += 1;
        r.sequence
    }
    fn item_name(&self, id: i32) -> Arc<str> {
        let r = self.remote.as_ref().expect("remote");
        Arc::from(
            r.builtin
                .get("minecraft:item")
                .and_then(|i| i.name(id))
                .unwrap_or("minecraft:air"),
        )
    }
    pub(super) fn remote_effect_network_id(&self, effect: &str) -> Option<i32> {
        let r = self.remote.as_ref()?;
        r.builtin
            .get("minecraft:mob_effect")?
            .id(&format!("minecraft:{effect}"))
    }
    pub(super) fn remote_effect_id(&self, effect: i32) -> Option<&'static str> {
        let r = self.remote.as_ref()?;
        let name = r.builtin.get("minecraft:mob_effect")?.name(effect)?;
        vise_client::brewing::effect(name).map(|d| d.id)
    }
    fn remote_effect(&mut self, p: &play::UpdateMobEffect) {
        if self.remote.as_ref().is_none_or(|r| r.entity_id != p.id) {
            return;
        }
        let Some(id) = self.remote_effect_id(p.effect) else {
            return;
        };
        let mut e = vise_client::effects::EffectInstance::new(id, p.duration, p.amplifier);
        e.ambient = p.flags & 1 != 0;
        e.visible = p.flags & 2 != 0;
        let list = &mut self.potions.effects.list;
        match list.iter_mut().find(|x| x.id == id) {
            Some(existing) => *existing = e,
            None => {
                if id == "darkness" {
                    self.sculk.darkness = super::sculk::darkness_blend_added(&e, p.flags & 8 != 0);
                }
                list.push(e);
            }
        }
    }
    fn item_id(&self, name: &str) -> i32 {
        let r = self.remote.as_ref().expect("remote");
        r.builtin
            .get("minecraft:item")
            .and_then(|i| i.id(name))
            .unwrap_or(0)
    }
    pub(super) fn to_stack(&self, s: &Option<Stack>) -> Option<ItemStack> {
        let r = self.remote.as_ref()?;
        Some(vise_client::item_sync::from_network(
            s.as_ref()?,
            &r.builtin,
            &r.registries,
        ))
    }
    fn to_network(&self, s: &Option<ItemStack>) -> Option<Stack> {
        let r = self.remote.as_ref()?;
        if let Some((stack, patch)) = s.as_ref().and_then(|s| Some((s, s.wire_patch()?))) {
            let mut out = Stack::new(self.item_id(&stack.item), stack.count as i32);
            out.wire = vise_protocol::item::WirePatch(Some(patch));
            return Some(out);
        }
        vise_client::lan::guest::stack_to_network(
            |name| self.item_id(name),
            |registry, name| {
                r.builtin
                    .get(registry)
                    .and_then(|ids| ids.id(name))
                    .or_else(|| r.registries.id(registry, name))
            },
            s,
        )
    }
    pub(super) fn chat_line(
        &mut self,
        content: vise_client::text::Text,
        source: vise_client::chat::ChatSource,
    ) -> bool {
        self.signed_chat_line(content, source, None)
    }
    fn signed_chat_line(
        &mut self,
        content: vise_client::text::Text,
        source: vise_client::chat::ChatSource,
        signature: Option<Signature>,
    ) -> bool {
        if !self.ui.chat.shows(source) {
            return false;
        }
        let text = content.plain();
        let focused = self.screen == Screen::Chat;
        if source == vise_client::chat::ChatSource::Player {
            self.ui
                .chat
                .add_signed(&self.gui.atlas, content, signature, self.ticks, focused);
        } else {
            let indicator = Some(vise_client::chat::SYSTEM_INDICATOR);
            self.ui.chat.add(
                &self.gui.atlas,
                content,
                indicator,
                source,
                self.ticks,
                focused,
            );
        }
        if let Some(r) = self.remote.as_mut() {
            r.chat.push((text, Instant::now()));
            r.chat_received += 1;
            if r.chat.len() > 100 {
                r.chat.remove(0);
            }
        }
        true
    }

    pub(super) fn handle_player_chat(
        &mut self,
        line: vise_client::text::Text,
        signature: Option<Signature>,
    ) {
        let source = vise_client::chat::ChatSource::Player;
        if self.chat_delay_ms() > 0 {
            if let Some(r) = self.remote.as_mut() {
                r.chat_queue.push_back((line, source, signature));
            }
        } else {
            self.accept_chat(line, source, signature);
        }
    }

    fn handle_delete_chat(&mut self, packed: &(i32, Option<Vec<u8>>)) -> Result<()> {
        let Some(r) = self.remote.as_mut() else {
            return Ok(());
        };
        let Some(signature) = r.signatures.unpack(packed) else {
            let reason =
                vise_client::text::Text::translate("multiplayer.disconnect.invalid_packet", &[]);
            return self.leave_server(Some(reason.plain()));
        };
        let queued = r.chat_queue.len();
        r.chat_queue
            .retain(|(_, _, s)| s.as_ref() != Some(&signature));
        if r.chat_queue.len() == queued {
            self.ui
                .chat
                .delete_message(&self.gui.atlas, &signature, self.ticks);
        }
        Ok(())
    }

    fn accept_chat(
        &mut self,
        line: vise_client::text::Text,
        source: vise_client::chat::ChatSource,
        signature: Option<Signature>,
    ) -> bool {
        let shown = self.signed_chat_line(line, source, signature);
        if shown && let Some(r) = self.remote.as_mut() {
            r.chat_previous = Some(Instant::now());
        }
        shown
    }

    fn chat_delay_ms(&self) -> i64 {
        (self.settings.chat_delay * 1000.0) as i64
    }

    pub(super) fn tick_chat_queue(&mut self) {
        let delay = self.chat_delay_ms();
        if self.pause {
            if delay > 0
                && let Some(r) = self.remote.as_mut()
            {
                r.chat_previous = r.chat_previous.map(|t| t + Duration::from_millis(50));
            }
            return;
        }
        if delay == 0 {
            if self
                .remote
                .as_ref()
                .is_some_and(|r| !r.chat_queue.is_empty())
            {
                self.flush_chat_queue();
            }
            return;
        }
        let ready = self.remote.as_ref().is_some_and(|r| {
            r.chat_previous
                .is_none_or(|t| t.elapsed() >= Duration::from_millis(delay as u64))
        });
        if !ready {
            return;
        }
        while let Some((line, source, signature)) =
            self.remote.as_mut().and_then(|r| r.chat_queue.pop_front())
        {
            if self.accept_chat(line, source, signature) {
                break;
            }
        }
    }

    fn flush_chat_queue(&mut self) {
        let Some(queue) = self
            .remote
            .as_mut()
            .map(|r| std::mem::take(&mut r.chat_queue))
        else {
            return;
        };
        for (line, source, signature) in queue {
            self.signed_chat_line(line, source, signature);
        }
        if let Some(r) = self.remote.as_mut() {
            r.chat_previous = None;
        }
    }

    pub(super) fn remote_carts(&self) -> Option<Vec<serde_json::Value>> {
        let r = self.remote.as_ref()?;
        let mut carts: Vec<&Tracked> = r
            .entities
            .values()
            .filter(|t| t.kind.ends_with("minecart"))
            .collect();
        carts.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Some(
            carts
                .into_iter()
                .map(|t| serde_json::json!({"kind": t.kind, "position": t.position, "velocity": [0.0, 0.0, 0.0]}))
                .collect(),
        )
    }

    pub(super) fn remote_custom_completions(&self) -> std::collections::BTreeSet<String> {
        self.remote
            .as_ref()
            .map(|r| r.custom_completions.clone())
            .unwrap_or_default()
    }
    pub(super) fn remote_online_names(&self) -> Option<Vec<String>> {
        let r = self.remote.as_ref()?;
        let mut names: Vec<String> = r.players.values().cloned().collect();
        names.sort();
        Some(names)
    }
    pub(super) fn remote_menu_id(&self) -> i32 {
        self.remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .map_or(0, |m| m.0)
    }
    pub(super) fn remote_chat(&self, n: usize) -> Option<Vec<String>> {
        let r = self.remote.as_ref()?;
        Some(r.chat.iter().rev().take(n).map(|l| l.0.clone()).collect())
    }

    pub(super) fn remote_backlog(&self) -> usize {
        self.remote.as_ref().map_or(0, |r| r.backlog.len())
    }
    pub(super) fn poll_stationary_client(&mut self) -> Result<()> {
        self.poll_remote()?;
        let loaded = self.player.as_ref().is_some_and(|p| {
            self.streaming.is_lit(
                (p.position.x.floor() as i32) >> 4,
                (p.position.z.floor() as i32) >> 4,
            )
        });
        if loaded && self.remote.as_ref().is_some_and(|r| !r.loaded_sent) {
            self.send(S::PlayerLoaded(Empty));
            self.remote.as_mut().unwrap().loaded_sent = true;
        }
        Ok(())
    }

    pub(super) fn poll_remote(&mut self) -> Result<()> {
        let Some(r) = self.remote.as_mut() else {
            return Ok(());
        };
        let mut events = std::mem::take(&mut r.backlog);
        for event in r.session.poll() {
            events.push_back(match event {
                Event::Packet(p) if matches!(*p, C::LevelChunkWithLight(_)) => {
                    match r.decoder.submit(r.chunk_format.clone(), p) {
                        None => Received::Chunk(r.chunk_format.clone()),
                        Some(p) => Received::Event(Event::Packet(p)),
                    }
                }
                event => Received::Event(event),
            });
        }
        let overdue = r
            .backlog_since
            .is_some_and(|since| since.elapsed() >= BACKLOG_LIMIT);
        let mut chunks = Duration::ZERO;
        while let Some(item) = events.pop_front() {
            let chunk = matches!(item, Received::Chunk(_));
            if chunk && !overdue && chunks >= CHUNK_PACKETS_PER_FRAME {
                events.push_front(item);
                break;
            }
            let started = chunk.then(Instant::now);
            let event = match item {
                Received::Event(event) => event,
                Received::Chunk(format) => {
                    let Some(next) = self.remote.as_ref().map(|r| r.decoder.next()) else {
                        break;
                    };
                    let (packet, column) = match next {
                        Next::Ready(decoded) => decoded,
                        Next::Decoding => {
                            events.push_front(Received::Chunk(format));
                            break;
                        }
                        Next::Lost => {
                            log::warn!("Vise: the chunk decoder stopped; a column is missing");
                            continue;
                        }
                    };
                    if let C::LevelChunkWithLight(p) = *packet {
                        self.remote_chunk(&p, Some((&format, column)));
                    }
                    chunks += started.map_or(Duration::ZERO, |t| t.elapsed());
                    continue;
                }
            };
            match event {
                Event::Status(s) => {
                    if let Some(r) = self.remote.as_mut() {
                        r.status = s;
                    }
                }
                Event::Joined {
                    registries,
                    name,
                    uuid,
                } => {
                    self.set_registries(registries);
                    self.extras_joined();
                    self.close_dialogs();
                    if let Some(r) = self.remote.as_mut() {
                        r.joined = true;
                        r.profile = uuid;
                        r.profile_name = name.clone();
                        r.status = vise_client::lang::get_or(
                            "multiplayer.downloadingTerrain",
                            "Loading terrain...",
                        );
                    }
                }
                Event::Brand(brand) => {
                    if let Some(remote) = &mut self.remote {
                        remote.brand = brand;
                    }
                }
                Event::QuietCommands => {
                    if let Some(remote) = &mut self.remote {
                        remote.quiet_commands = true;
                    }
                }
                Event::Reconfigured(registries) => {
                    self.hud_disconnected();
                    self.score.board = Default::default();
                    if let Some(r) = self.remote.as_mut() {
                        r.players.clear();
                        r.tab_players.clear();
                        r.listed.clear();
                        r.signatures = Default::default();
                        r.custom_completions.clear();
                    }
                    self.ui.registries.custom_completions.clear();
                    self.set_registries(registries);
                }
                Event::Transfer { host, port } => {
                    if let Some(r) = self.remote.as_mut()
                        && r.session.address != "local"
                    {
                        r.transfer = Some(format!("{host}:{port}"));
                    }
                }
                Event::PackPush(p) => self.handle_pack_push(p),
                Event::PackPop(id) => self.handle_pack_pop(id),
                Event::ServerLinks(links) => {
                    if let Some(r) = self.remote.as_mut() {
                        r.server_links = super::server_links::trusted(links);
                    }
                }
                Event::ShowDialog(dialog) => self.show_dialog(&dialog),
                Event::ClearDialog => self.clear_dialog(),
                Event::CodeOfConduct(text) => self.handle_code_of_conduct(text),
                Event::DecodeError(e) => {
                    if let Some(r) = self.remote.as_mut() {
                        r.decode_errors += 1;
                        if r.decode_errors <= 20 {
                            log::warn!("Vise: skipped undecodable packet {e}");
                        }
                    }
                }
                Event::Disconnected {
                    reason,
                    from_server,
                } => {
                    log::info!("Vise: disconnected: {reason}");
                    let transfer = self.remote.as_mut().and_then(|r| {
                        r.transfer.take().map(|address| {
                            (address, r.session.name.clone(), r.session.cookies.clone())
                        })
                    });
                    if let Some((address, name, cookies)) = transfer {
                        self.leave_server(None)?;
                        self.connect_server_with_cookies(&address, &name, Some(cookies))?;
                        return Ok(());
                    }
                    let retry = self.reconnect_after(&reason, from_server);
                    self.leave_server(Some(reason))?;
                    self.offer_reconnect(retry);
                    return Ok(());
                }
                Event::Packet(packet) => self.handle_packet(*packet)?,
            }
            chunks += started.map_or(Duration::ZERO, |t| t.elapsed());
            if self.remote.is_none() {
                break;
            }
        }
        if let Some(r) = self.remote.as_mut() {
            r.backlog_since = if events.is_empty() {
                None
            } else {
                Some(r.backlog_since.unwrap_or_else(Instant::now))
            };
            r.backlog = events;
        }
        self.send_suggestion_request();
        Ok(())
    }
    fn send_suggestion_request(&mut self) {
        if self.remote.is_none() {
            return;
        }
        let Some((id, command)) = self
            .ui
            .chat_screen
            .as_mut()
            .and_then(|s| s.take_request())
            .or_else(|| self.command_block_suggestion_request())
        else {
            return;
        };
        let mut w = vise_protocol::buf::Writer::new();
        w.varint(id);
        w.string(&command);
        self.send(S::Unknown(
            vise_protocol::ids::play::serverbound::COMMAND_SUGGESTION,
            w.data,
        ));
    }
    fn handle_command_suggestions(&mut self, data: &[u8]) {
        let lang = self
            .remote
            .as_ref()
            .map(|r| r.lang.clone())
            .unwrap_or_default();
        let mut r = vise_protocol::buf::Reader::new(data);
        let decoded = (|| {
            let id = r.varint()?;
            let start = r.varint()?;
            let length = r.varint()?;
            let n = r.count(usize::MAX)?;
            let mut list = Vec::with_capacity(n.min(1000));
            for _ in 0..n {
                let text = r.string(32767)?;
                let tooltip = if r.bool()? {
                    Some(plain(&r.nbt()?, &lang))
                } else {
                    None
                };
                list.push((text, tooltip));
            }
            vise_protocol::Result::Ok((id, start, length, list))
        })();
        if let Ok((id, start, length, list)) = decoded {
            let range = (start.max(0) as usize, length.max(0) as usize);
            self.chat_server_suggestions(id, range, list.clone());
            self.command_block_server_suggestions(id, range, list);
        }
    }
    fn set_registries(&mut self, registries: Arc<Dynamic>) {
        let biomes: Vec<u16> = registries
            .registries
            .get("minecraft:worldgen/biome")
            .map(|list| {
                list.iter()
                    .map(|(name, _)| self.res.biomes().id(name))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(r) = self.remote.as_mut() {
            let bits = ceil_log2(biomes.len());
            let column = r.chunk_format.layout();
            r.chunk_format =
                Arc::new(ChunkFormat::new(&r.dimension, biomes, bits).in_column(column));
            r.registries = registries;
        }
    }
    fn enter_remote_dimension(&mut self, spawn: &play::SpawnInfo, keep_world: bool) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let dimension = r.registries.dimension(spawn.dimension_type);
        let shape = vise_client::chunk::LevelShape::new(
            dimension.min_y,
            dimension.height,
            dimension.logical_height,
            dimension.has_skylight,
        )
        .unwrap_or_else(|_| super::Dim::from_id(&dimension.name).shape());
        let same = r.dimension.name == dimension.name && keep_world;
        r.chunk_format = Arc::new(
            r.chunk_format
                .with_dimension(&dimension)
                .in_column(shape.column()),
        );
        r.dimension = dimension;
        r.game_mode = spawn.game_type;
        r.flat = spawn.flat;
        r.loaded_sent = false;
        r.position_sender = Default::default();
        r.predictions.clear();
        self.set_mode_id(spawn.game_type);
        self.dim = Dim::from_id(&spawn.dimension);
        self.level.last_death = spawn
            .last_death
            .as_ref()
            .map(|d| (d.dimension.clone(), d.pos));
        if !same {
            self.stop_sounds_for_new_level();
            let mut streaming = Streaming::new(
                Arc::new(NoTerrain),
                self.pack.context.clone(),
                self.res.threads(),
            );
            streaming.remote = true;
            streaming.radius = self.streaming.radius;
            self.streaming = streaming;
            let (time, day, weather) = (
                self.world.game_time,
                self.world.day_time,
                self.world.weather,
            );
            let old = std::mem::take(&mut self.world.chunks);
            let count = old.len();
            vise_client::chunk::free_columns(old, count);
            self.world = World::new(self.res.facts().air, spawn.seed);
            self.world.shape = shape;
            self.world.game_time = time;
            self.world.day_time = day;
            self.world.weather = weather;
            self.mobs.clear();
            self.clear_remote_objects();
            self.clear_remote_projectiles();
            self.dropped.clear();
            self.particles.clear();
            self.pickups.clear();
            if let Some(r) = self.remote.as_mut() {
                r.entities.clear();
                r.lerps.clear();
                r.stepped.clear();
                r.head_lerps.clear();
                r.pending_items.clear();
                r.level_players.clear();
                r.level_player_names.clear();
                r.level_player_textures.clear();
                r.chunk_center = (0, 0);
            }
            self.clear_lod();
            self.pending_uploads.clear();
            if let Some(renderer) = self.renderer.as_mut() {
                renderer.clear_sections(self.world.layout());
            }
        }
        let has_sky = self
            .remote
            .as_ref()
            .is_none_or(|r| r.dimension.has_skylight);
        self.streaming.sky_light = has_sky;
        self.streaming.nether_shading = self.dim == Dim::Nether;
        self.streaming.biome_zoom = vise_core::biome::BiomeZoom::new(spawn.seed);
        self.player = None;
        self.portals.player.cooldown = spawn.portal_cooldown;
        self.apply_settings();
    }

    pub(super) fn remote_chunk_in_range(&self, x: i32, z: i32) -> bool {
        self.remote.as_ref().is_none_or(|r| {
            let range = r.view_distance.max(2) + 3;
            (x - r.chunk_center.0).abs() <= range && (z - r.chunk_center.1).abs() <= range
        })
    }
    fn drop_remote_chunks_out_of_range(&mut self) {
        let gone: vise_client::fast_hash::FxHashSet<(i32, i32)> = self
            .world
            .chunks
            .keys()
            .copied()
            .filter(|&(x, z)| !self.remote_chunk_in_range(x, z))
            .collect();
        if gone.is_empty() {
            return;
        }
        let mut freed = Vec::with_capacity(gone.len());
        for &(x, z) in &gone {
            freed.extend(self.streaming.forget(&mut self.world, x, z));
        }
        if let Some(renderer) = self.renderer.as_mut() {
            renderer.remove_columns(gone.iter().copied());
        }
        vise_client::chunk::free_columns(freed, gone.len());
        self.world
            .block_entities
            .retain(|k, _| !gone.contains(&(k.0 >> 4, k.2 >> 4)));
    }
    pub(super) fn remote_block_entity_data(&mut self, pos: [i32; 3], kind: i32, tag: &Tag) {
        let key = (pos[0], pos[1], pos[2]);
        let Some(name) = self
            .remote
            .as_ref()
            .and_then(|r| r.builtin.get("minecraft:block_entity_type")?.name(kind))
        else {
            return;
        };
        if name == "minecraft:decorated_pot" {
            if let Tag::Compound(fields) = tag
                && !fields.is_empty()
            {
                let mut full = fields.clone();
                full.insert("id".into(), Tag::String(name.into()));
                for (k, v) in ["x", "y", "z"].into_iter().zip(pos) {
                    full.insert(k.into(), Tag::Int(v));
                }
                super::spill::put_saved_entity(&mut self.world, pos, Tag::Compound(full));
                let state = self.world.block(pos[0], pos[1], pos[2]);
                let _ = self.world.set(pos[0], pos[1], pos[2], state);
            }
            return;
        }
        let Some(entity) = self.world.block_entities.get(&key) else {
            return;
        };
        let Tag::Compound(fields) = tag else {
            return;
        };
        if entity.id() != name || fields.is_empty() {
            return;
        }
        let mut full = fields.clone();
        full.insert("id".into(), Tag::String(name.into()));
        for (k, v) in ["x", "y", "z"].into_iter().zip(pos) {
            full.insert(k.into(), Tag::Int(v));
        }
        if let Some((_, mut loaded)) =
            vise_client::save::decode_block_entity(&Tag::Compound(full))
        {
            match (self.world.block_entities.get(&key), &mut loaded) {
                (Some(BlockEntity::Vault(old)), BlockEntity::Vault(new)) => {
                    new.spin = old.spin;
                    new.o_spin = old.o_spin;
                }
                (Some(BlockEntity::TrialSpawner(old)), BlockEntity::TrialSpawner(new)) => {
                    new.spin = old.spin;
                    new.o_spin = old.o_spin;
                }
                _ => {}
            }
            let command_block = matches!(loaded, BlockEntity::CommandBlock(_));
            self.world.block_entities.insert(key, loaded);
            self.update_spawner_display(key);
            let state = self.world.block(pos[0], pos[1], pos[2]);
            let _ = self.world.set(pos[0], pos[1], pos[2], state);
            if command_block {
                self.command_block_data_arrived(key);
            }
        }
    }
    fn remote_animate(&mut self, id: i32, action: u8) {
        use vise_client::particles::{ParticleOptions, ParticleType};
        let me = self.remote.as_ref().is_some_and(|r| r.entity_id == id);
        let body = if me {
            self.player.as_ref().map(|p| {
                let d = p.dimensions;
                (
                    [p.position.x, p.position.y, p.position.z],
                    f64::from(d.width),
                    f64::from(d.height),
                )
            })
        } else {
            self.mobs.iter().find(|m| m.id == id as u32).map(|m| {
                (
                    [m.position.x, m.position.y, m.position.z],
                    m.kind.width,
                    m.kind.height,
                )
            })
        };
        let Some((feet, width, height)) = body else {
            return;
        };
        let particle = match action {
            0 => {
                if me {
                    self.sleeping = None;
                    self.sleep_timer = 100;
                }
                return;
            }
            1 => ParticleType::Crit,
            2 => ParticleType::EnchantedHit,
            _ => return,
        };
        self.particles
            .tracking_emitter(ParticleOptions::simple(particle), feet, width, height);
    }
    fn remote_server_data(&mut self, icon: Option<&[u8]>) {
        let (Some(r), Some(png)) = (self.remote.as_ref(), icon) else {
            return;
        };
        if self.integrated.is_some() {
            return;
        }
        if let Ok(dir) = std::env::current_dir() {
            vise_client::multiplayer::save_server_icon(
                &dir.join("saves"),
                &r.session.address,
                png,
            );
        }
    }
    pub(super) fn sync_block_entity(&mut self, pos: (i32, i32, i32), state: BlockStateId) {
        let name = self.res.facts().get(state).name.clone();
        match BlockEntity::for_block(&name) {
            Some(entity) => {
                let same = self
                    .world
                    .block_entities
                    .get(&pos)
                    .is_some_and(|e| e.id() == entity.id());
                if !same {
                    self.world.block_entities.insert(pos, entity);
                }
            }
            None => {
                self.world.block_entities.remove(&pos);
            }
        }
    }
    fn server_block(&mut self, pos: [i32; 3], state: i32) {
        if let Some((_, known)) = self
            .remote
            .as_mut()
            .and_then(|r| r.predictions.get_mut(&pos))
        {
            *known = state;
            return;
        }
        self.set_block(pos, state);
    }
    fn end_predictions(&mut self, sequence: i32) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let settled: Vec<_> = r
            .predictions
            .iter()
            .filter(|(_, (s, _))| *s <= sequence)
            .map(|(&pos, &(_, state))| (pos, state))
            .collect();
        for (pos, state) in settled {
            if let Some(r) = self.remote.as_mut() {
                r.predictions.remove(&pos);
            }
            if self.world.block(pos[0], pos[1], pos[2]).0 as i32 != state {
                self.set_block(pos, state);
            }
        }
    }
    fn set_block(&mut self, pos: [i32; 3], state: i32) {
        let state = BlockStateId(state.max(0) as u32);
        let previous = self.world.block(pos[0], pos[1], pos[2]);
        if self.world.loaded(pos[0], pos[2])
            && self.world.set(pos[0], pos[1], pos[2], state).is_ok()
        {
            self.sync_block_entity((pos[0], pos[1], pos[2]), state);
            let facts = self.res.facts();
            let moving = |s: BlockStateId| &*facts.get(s).name == "minecraft:moving_piston";
            if moving(previous) && !moving(state) {
                let at = vise_core::position::BlockPos::new(pos[0], pos[1], pos[2]);
                self.block_ticks.moving_pistons.retain(|(p, _)| *p != at);
                self.sync_moving_pistons();
            }
        }
        if let Some((target, _)) = self.remote.as_ref().and_then(|r| r.placing)
            && target == pos
            && !self.res.facts().is_air(state)
        {
            let s = self.res.facts().get(state).sounds.clone();
            let at = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ];
            self.audio
                .play(&s.place, Some(at), (s.volume + 1.0) / 2.0, s.pitch * 0.8);
            if let Some(r) = self.remote.as_mut() {
                r.placing = None;
            }
        }
    }
    fn spawn_entity(&mut self, id: i32, kind: &str, position: [f64; 3], rotation: [f32; 3]) {
        let [yaw, pitch, head] = rotation;
        let at = Vec3::new(position[0], position[1], position[2]);
        let mob_type = if kind == "minecraft:player" {
            self.remote.as_ref().and_then(|r| r.player_type.clone())
        } else {
            self.res.mob_types().get(kind).cloned()
        };
        if kind == "minecraft:ender_dragon" {
            self.spawn_remote_dragon(id, position, yaw);
        }
        if kind == "minecraft:experience_orb" {
            self.orbs.retain(|o| o.id != id as u32);
            self.orbs.push(ExperienceOrb {
                id: id as u32,
                position: at,
                previous: at,
                velocity: Vec3::ZERO,
                value: 1,
                age: 0,
                on_ground: false,
                count: 1,
                ticks: 0,
            });
        }
        if let Some(t) = mob_type {
            let mut mob = Mob::new(id as u32, t, at, head);
            super::remote_mob_data::client_defaults(&mut mob);
            (mob.yaw, mob.previous_yaw) = (yaw, yaw);
            (mob.head_pitch, mob.previous_head_pitch) = (pitch, pitch);
            mob.persistent = true;
            self.mobs.retain(|m| m.id != id as u32);
            self.mobs.push(mob);
            if let Some(r) = self.remote.as_mut() {
                r.stepped.remove(&id);
            }
        }
        if let Some(r) = self.remote.as_mut() {
            if kind == "minecraft:item" {
                r.pending_items.insert(id, (position, Vec3::ZERO));
            }
            r.entities.insert(
                id,
                Tracked {
                    kind: kind.to_owned(),
                    position,
                },
            );
        }
    }
    fn move_entity(
        &mut self,
        id: i32,
        path: Option<Path>,
        rotation: Option<(f32, f32)>,
        on_ground: bool,
        far_snaps: bool,
    ) {
        let dragon = self.dragon.as_ref().is_some_and(|d| d.id as i32 == id);
        let driven = self.driven_mount().is_some_and(|m| m.id as i32 == id);
        let mob = self
            .mobs
            .iter_mut()
            .find(|m| m.id == id as u32)
            .filter(|_| !dragon);
        let tracked = self
            .remote
            .as_ref()
            .and_then(|r| r.entities.get(&id))
            .map(|t| t.position);
        let position = path
            .as_ref()
            .map(Path::end)
            .or(tracked)
            .or_else(|| {
                mob.as_ref()
                    .map(|m| [m.position.x, m.position.y, m.position.z])
            })
            .unwrap_or([0.0; 3]);
        if let Some(r) = self.remote.as_mut() {
            if let Some(t) = r.entities.get_mut(&id) {
                t.position = position;
            }
            if driven {
                r.stepped.remove(&id);
                return;
            }
            if let Some(m) = mob {
                let entity = PosRot {
                    position: [m.position.x, m.position.y, m.position.z],
                    yaw: m.yaw,
                    pitch: m.head_pitch,
                };
                let far = far_snaps && {
                    let d = [0, 1, 2].map(|i| position[i] - entity.position[i]);
                    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] > 4096.0
                };
                let handler = r.stepped.entry(id).or_insert_with(|| {
                    let interval = vise_client::lan::tracking::settings(&m.kind.id).1;
                    SteppedInterpolation::new(interval as i32)
                });
                let snap = if far {
                    handler.cancel();
                    let (yaw, pitch) = rotation.unwrap_or((m.yaw, m.head_pitch));
                    Some(PosRot {
                        position,
                        yaw,
                        pitch,
                    })
                } else {
                    match handler.interpolate_to(entity, path, rotation) {
                        Start::Snap(at) => Some(at),
                        Start::Interpolating | Start::Unchanged => None,
                    }
                };
                if let Some(at) = snap {
                    place_interpolated(m, at);
                    m.previous = m.position;
                }
                m.on_ground = on_ground;
                return;
            }
            if let Some(p) = self
                .creatures
                .projectiles
                .iter_mut()
                .find(|p| p.id == id as u32)
            {
                if path.is_some() {
                    p.position = Vec3::new(position[0], position[1], position[2]);
                }
                if let Some((yaw, pitch)) = rotation {
                    p.yaw = yaw;
                    p.pitch = pitch;
                }
                return;
            }
            if path.is_some()
                && (self.potions.move_remote_thrown(
                    id as u32,
                    Vec3::new(position[0], position[1], position[2]),
                ) || self.end.move_remote_eye(id as u32, position))
            {
                return;
            }
            if path.is_some() {
                if let Some(h) = self.hook.as_mut().filter(|h| h.id() == id as u32) {
                    h.move_to(position, on_ground);
                    return;
                }
                if let Some(h) = self.remote_hooks.iter_mut().find(|h| h.id() == id as u32) {
                    h.move_to(position, on_ground);
                    return;
                }
            }
            if let Some(t) = self.tridents.iter_mut().find(|t| t.id == id as u32) {
                if path.is_some() {
                    t.position = Vec3::new(position[0], position[1], position[2]);
                }
                if let Some((yaw, pitch)) = rotation {
                    t.yaw = yaw;
                    t.pitch = pitch;
                }
                return;
            }
            if path.is_some()
                && let Some(f) = self.rockets.iter_mut().find(|f| f.id == id as u32)
            {
                f.position = Vec3::new(position[0], position[1], position[2]);
                return;
            }
            if let Some(b) = self.dragon_fireballs.iter_mut().find(|b| b.id == id as u32) {
                if path.is_some() {
                    b.position = Vec3::new(position[0], position[1], position[2]);
                }
                return;
            }
            if let Some(p) = r.pending_items.get_mut(&id) {
                p.0 = position;
            }
            let at = Vec3::new(position[0], position[1], position[2]);
            if let Some(d) = self.dropped.iter_mut().find(|d| d.id == id as u32) {
                d.position = at;
                d.on_ground = on_ground;
                return;
            }
            if let Some(f) = self.falling.iter_mut().find(|f| f.id == id as u32) {
                f.position = at;
                return;
            }
            if let Some(t) = self.tnts.iter_mut().find(|t| t.id == id as u32) {
                t.position = at;
                t.on_ground = on_ground;
                return;
            }
            let rotation = rotation.or_else(|| r.lerps.get(&id).and_then(|l| l.rotation));
            r.lerps.insert(
                id,
                Lerp {
                    target: position,
                    rotation,
                    steps: 3,
                },
            );
        }
        self.turn_remote_object(id, rotation.map(|(yaw, _)| yaw), on_ground);
        self.move_remote_text_display(id, position, rotation);
        self.turn_remote_projectile(id, rotation);
    }
    fn remove_entity(&mut self, id: i32) {
        self.remote_set_passengers(id, &[]);
        self.mobs.retain(|m| m.id != id as u32);
        self.orbs.retain(|o| o.id != id as u32);
        self.dropped.retain(|d| d.id != id as u32);
        self.remove_remote_dragon(id);
        self.remove_remote_object(id as u32);
        self.remove_remote_projectile(id as u32);
        self.persist.uuids.remove(&(id as u32));
        if let Some(r) = self.remote.as_mut() {
            r.glowing.remove(&id);
            r.entities.remove(&id);
            r.lerps.remove(&id);
            r.stepped.remove(&id);
            r.head_lerps.remove(&id);
            r.pending_items.remove(&id);
        }
    }
    fn menu_slot(menu: &str, slot: i32) -> Option<usize> {
        let s = usize::try_from(slot).ok()?;
        let slots = |n: usize| -> Option<usize> {
            match s {
                _ if s < n => Some(300 + s),
                _ if s < n + 27 => Some(9 + s - n),
                _ if s < n + 36 => Some(s - n - 27),
                _ => None,
            }
        };
        let rows = |n: usize| slots(n * 9);
        match menu {
            "inventory" => match s {
                0 => Some(200),
                1..=4 => Some(100 + s - 1),
                5..=8 => Some(500 + s - 5),
                9..=35 => Some(s),
                36..=44 => Some(s - 36),
                45 => Some(504),
                _ => None,
            },
            "minecraft:crafting" => match s {
                0 => Some(200),
                1..=9 => Some(100 + s - 1),
                10..=36 => Some(9 + s - 10),
                37..=45 => Some(s - 37),
                _ => None,
            },
            "minecraft:merchant" => match s {
                0 | 1 => Some(super::stations::INPUT + s),
                2 => Some(super::stations::RESULT),
                3..=29 => Some(9 + s - 3),
                30..=38 => Some(s - 30),
                _ => None,
            },
            "minecraft:furnace" | "minecraft:blast_furnace" | "minecraft:smoker" => match s {
                0..=2 => Some(300 + s),
                3..=29 => Some(9 + s - 3),
                30..=38 => Some(s - 30),
                _ => None,
            },
            "minecraft:generic_9x1" => rows(1),
            "minecraft:generic_9x2" => rows(2),
            "minecraft:generic_9x3" | "minecraft:shulker_box" => rows(3),
            "minecraft:generic_9x4" => rows(4),
            "minecraft:generic_9x5" => rows(5),
            "minecraft:generic_9x6" => rows(6),
            "minecraft:hopper" => slots(5),
            "minecraft:generic_3x3" => slots(9),
            "minecraft:brewing_stand" => slots(5),
            "minecraft:crafter_3x3" => slots(9),
            m if m.starts_with(MOUNT_MENU) => {
                let n = m[MOUNT_MENU.len()..].parse::<usize>().ok()? * 3;
                match s {
                    0 => Some(super::mount_menu::SADDLE),
                    1 => Some(super::mount_menu::ARMOR),
                    _ if s < 2 + n => Some(super::mount_menu::CHEST + s - 2),
                    _ if s < 2 + n + 27 => Some(9 + s - 2 - n),
                    _ if s < 2 + n + 36 => Some(s - 2 - n - 27),
                    _ => None,
                }
            }
            m => {
                let kind = Self::remote_station_kind(m)?;
                let inputs = kind.inputs();
                let own = inputs + usize::from(kind.has_result());
                match s {
                    _ if s < inputs => Some(super::stations::INPUT + s),
                    _ if s < own => Some(super::stations::RESULT),
                    _ if s < own + 27 => Some(9 + s - own),
                    _ if s < own + 36 => Some(s - own - 27),
                    _ => None,
                }
            }
        }
    }
    pub(super) fn menu_slot_index(menu: &str, internal: usize) -> Option<usize> {
        (0..)
            .map_while(|slot| Self::menu_slot(menu, slot))
            .position(|slot| slot == internal)
    }
    pub(super) fn remote_station_kind(menu: &str) -> Option<super::stations::Kind> {
        use super::stations::Kind;
        Some(match menu {
            "minecraft:loom" => Kind::Loom,
            "minecraft:stonecutter" => Kind::Stonecutter,
            "minecraft:grindstone" => Kind::Grindstone,
            "minecraft:anvil" => Kind::Anvil,
            "minecraft:smithing" => Kind::Smithing,
            "minecraft:enchantment" => Kind::Enchanting,
            "minecraft:beacon" => Kind::Beacon,
            "minecraft:cartography_table" => Kind::Cartography,
            _ => return None,
        })
    }
    fn open_menu_name(&self, container: i32) -> String {
        match self.remote.as_ref().and_then(|r| r.menu.as_ref()) {
            Some((id, name)) if *id == container && container != 0 => name.clone(),
            _ => "inventory".into(),
        }
    }
    fn set_menu_slot(&mut self, container: i32, slot: i32, item: Option<ItemStack>) {
        let menu = self.open_menu_name(container);
        if menu == "minecraft:lectern" {
            if slot == 0 {
                self.remote_lectern_book(item);
            }
            return;
        }
        if container != 0 && menu == "inventory" {
            return;
        }
        let Some(internal) = Self::menu_slot(&menu, slot) else {
            return;
        };
        if internal == 200 {
            return;
        }
        let slot = self.slot_mut(internal);
        let changed = !super::hand::matches_for_swap(slot.as_ref(), item.as_ref());
        *slot = item;
        if internal < 36 {
            self.remote_hand_slot_set(internal, changed);
        } else if internal == 504 {
            self.remote_hand_slot_set(super::shield::OFFHAND_SLOT, changed);
        }
        if internal >= super::stations::INPUT && self.station.is_some() {
            self.remote_station_slot_changed(internal);
        }
    }

    fn remote_mob_attributes(&mut self, id: i32, values: &[play::AttributeSnapshot]) {
        let catalog = &self.motion.attributes;
        let value = |name: &str| {
            let attribute = catalog.id(name)?;
            let s = values
                .iter()
                .find(|s| s.attribute == attribute.registry_id())?;
            Some(
                catalog
                    .attribute(attribute)
                    .definition
                    .sanitize(snapshot_value(s)),
            )
        };
        let (max_health, scale) = (value("minecraft:max_health"), value("minecraft:scale"));
        let (speed, jump) = (
            value("minecraft:movement_speed"),
            value("minecraft:jump_strength"),
        );
        let Some(m) = self.mobs.iter_mut().find(|m| m.id == id as u32) else {
            return;
        };
        if vise_client::mobs::equine::moved_by_rider(m.species()) {
            if let Some(speed) = speed {
                vise_client::mobs::finalize::set_base(
                    m,
                    vise_client::mobs::finalize::Attr::MovementSpeed,
                    speed,
                );
            }
            if let Some(jump) = jump {
                m.extra.vector[0] = jump;
            }
        }
        if let Some(max_health) = max_health {
            vise_client::mobs::finalize::set_base(
                m,
                vise_client::mobs::finalize::Attr::MaxHealth,
                max_health,
            );
        }
        if let Some(scale) = scale {
            m.extra.model_scale = scale as f32;
        }
    }

    fn handle_packet(&mut self, packet: C) -> Result<()> {
        match packet {
            C::Ping(p) => self.send(S::Pong(p)),
            C::PongResponse(p) => {
                let delay = self.debug_millis() - p.time;
                self.debug_charts.ping.log(delay);
            }
            C::Login(p) => {
                self.level.hardcore = p.hardcore;
                self.selected = 0;
                self.potions.effects.clear();
                if let Some(r) = self.remote.as_mut() {
                    r.carried = Some(0);
                    r.own_invisible = false;
                    r.online_mode = p.online_mode;
                    r.entity_id = p.player_id;
                    r.camera = None;
                    r.logged_in = true;
                    r.view_distance = p.chunk_radius;
                    r.show_death_screen = p.show_death_screen;
                }
                self.streaming.radius =
                    self.settings.render_distance.min(p.chunk_radius + 1).max(2);
                self.enter_remote_dimension(&p.spawn, false);
                let info = self
                    .settings
                    .client_information(client_view_distance(self.settings.render_distance));
                if let Some(r) = self.remote.as_mut() {
                    r.sent_info = info.clone();
                }
                self.send(S::ClientInformation(info));
            }
            C::Respawn(p) => {
                self.potions.effects.clear();
                self.using = None;
                if p.keep & 2 == 0 {
                    self.riding = None;
                    if let Some(r) = self.remote.as_mut() {
                        r.own_invisible = false;
                    }
                }
                let dimension_changed = self.dim != Dim::from_id(&p.spawn.dimension);
                self.enter_remote_dimension(&p.spawn, true);
                if dimension_changed {
                    let (music, _) = self.situational_music();
                    self.music.stop_playing(music.as_ref(), &mut self.audio);
                }
                self.screen = Screen::Playing;
                if let Some(r) = self.remote.as_mut() {
                    r.death_message = None;
                    r.health = (20.0, 20, 5.0);
                    r.camera = None;
                }
                self.capture(true);
            }
            C::PlayerPosition(p) => self.remote_teleport(&p)?,
            C::PlayerRotation(p) => {
                self.yaw = if p.relative_yaw {
                    self.yaw + p.yaw
                } else {
                    p.yaw
                };
                self.pitch = (if p.relative_pitch {
                    self.pitch + p.pitch
                } else {
                    p.pitch
                })
                .clamp(-90.0, 90.0);
                self.snap_player_model_pitch();
            }
            C::PlayerLookAt(p) => self.remote_look_at(&p),
            C::SetCamera(p) => self.remote_set_camera(p.camera_id),
            C::LevelChunkWithLight(p) => self.remote_chunk(&p, None),
            C::LightUpdate(p) => self.remote_light_update(&p),
            C::ChunksBiomes(p) => self.remote_chunks_biomes(&p),
            C::ForgetLevelChunk(p) => {
                let [x, z] = p.pos;
                self.streaming.forget(&mut self.world, x, z);
                self.world
                    .block_entities
                    .retain(|k, _| (k.0 >> 4, k.2 >> 4) != (x, z));
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.remove_columns([(x, z)]);
                }
            }
            C::ChunkBatchFinished(_) => {
                self.send(S::ChunkBatchReceived(play::ChunkBatchReceived {
                    chunks_per_tick: 64.0,
                }));
            }
            C::BlockUpdate(p) => self.server_block(p.pos, p.state),
            C::BlockDestruction(p) => {
                self.destroy_block_progress(p.id, p.pos, i32::from(p.progress));
            }
            C::SectionBlocksUpdate(p) => {
                let [sx, sy, sz] = p.section;
                for change in p.changes {
                    let local = (change & 0xFFF) as i32;
                    let pos = [
                        sx * 16 + (local >> 8 & 15),
                        sy * 16 + (local & 15),
                        sz * 16 + (local >> 4 & 15),
                    ];
                    self.server_block(pos, (change >> 12) as i32);
                }
            }
            C::BlockChangedAck(p) => self.end_predictions(p.sequence),
            C::BlockEvent(p) => {
                let pos = vise_core::position::BlockPos::new(p.pos[0], p.pos[1], p.pos[2]);
                let triggered = self.with_level(|b, level, ticks| {
                    b.client_block_event(level, ticks, pos, i32::from(p.b0), i32::from(p.b1))
                });
                self.sync_moving_pistons();
                self.process_level_events()?;
                if !triggered {
                    let state = self.world.block(p.pos[0], p.pos[1], p.pos[2]);
                    let block = &self.res.facts().get(state).name;
                    let named = self.remote.as_ref().and_then(|r| {
                        r.builtin
                            .get("minecraft:block")?
                            .name(p.block)
                            .map(str::to_owned)
                    });
                    if named.as_deref() == Some(&**block) {
                        self.block_entity_event(p.pos, i32::from(p.b0), i32::from(p.b1));
                    }
                }
            }
            C::LevelEvent(p) if p.global => self.global_level_event_sound(p.kind, p.pos),
            C::LevelEvent(p) => self.level_event_at(p.kind, p.pos, p.data),
            C::LevelParticles(p) => self.remote_level_particles(&p),
            C::Explode(p) => self.remote_explode(&p),
            C::Sound(p) => {
                if let Some(name) = self.remote_sound_name(&p.sound) {
                    let at = [
                        f64::from(p.x) / 8.0,
                        f64::from(p.y) / 8.0,
                        f64::from(p.z) / 8.0,
                    ];
                    match usize::try_from(p.source)
                        .ok()
                        .filter(|s| *s < vise_client::settings::SOUND_SOURCES.len())
                    {
                        Some(source) => {
                            self.audio
                                .play_on(source, &name, Some(at), p.volume, p.pitch)
                        }
                        None => self.audio.play(&name, Some(at), p.volume, p.pitch),
                    }
                }
            }
            C::StopSound(p) => self
                .audio
                .stop_sounds(p.sound.as_deref(), p.source.map(|s| s as usize)),
            C::AddEntity(p) => {
                let kind = self
                    .remote
                    .as_ref()
                    .and_then(|r| {
                        r.builtin
                            .get("minecraft:entity_type")?
                            .name(p.kind)
                            .map(str::to_owned)
                    })
                    .unwrap_or_default();
                let rotation =
                    [p.yaw, p.pitch, p.head_yaw].map(vise_protocol::buf::unpack_degrees);
                if let Some(r) = self.remote.as_mut() {
                    r.level_player_textures.remove(&p.id);
                    if kind == "minecraft:player" {
                        r.level_players.insert(p.id, p.uuid);
                        match r.players.get(&p.uuid) {
                            Some(name) => r.level_player_names.insert(p.id, name.clone()),
                            None => r.level_player_names.remove(&p.id),
                        };
                    } else {
                        r.level_players.remove(&p.id);
                        r.level_player_names.remove(&p.id);
                    }
                }
                self.persist.uuids.insert(
                    p.id as u32,
                    vise_client::lan::playerdata::uuid_ints(p.uuid),
                );
                self.spawn_entity(p.id, &kind, p.position, rotation);
                self.remote_motion(p.id, p.velocity);
                if !self.spawn_remote_object(p.id, &kind, p.position, rotation[0], p.data) {
                    self.spawn_remote_projectile(
                        p.id,
                        &kind,
                        p.position,
                        (rotation[0], rotation[1]),
                        p.velocity,
                        p.data,
                    );
                }
                self.move_remote_text_display(p.id, p.position, Some((rotation[0], rotation[1])));
            }
            C::RemoveEntities(p) => {
                for id in p.ids {
                    if let Some(r) = self.remote.as_mut() {
                        r.level_players.remove(&id);
                        r.level_player_names.remove(&id);
                        r.level_player_textures.remove(&id);
                    }
                    self.remove_entity(id);
                }
            }
            C::MoveEntityPos(play::MoveEntityPos(m))
            | C::MoveEntityPosRot(play::MoveEntityPosRot(m))
            | C::MoveEntityRot(play::MoveEntityRot(m)) => {
                let base = self
                    .remote
                    .as_ref()
                    .and_then(|r| r.entities.get(&m.id))
                    .map(|t| t.position);
                let Some(base) = base else { return Ok(()) };
                let path = m.delta.as_ref().map(|d| match d {
                    play::VecDelta::Linear(_) => Path::Linear(d.end(base)),
                    play::VecDelta::Stepped(steps) => {
                        let mut at = base;
                        Path::Stepped(
                            steps
                                .iter()
                                .map(|(delta, ticks)| {
                                    at = play::VecDelta::apply(at, *delta);
                                    (at, *ticks)
                                })
                                .collect(),
                        )
                    }
                });
                let rotation = m.rotation.map(|(y, x)| {
                    (
                        vise_protocol::buf::unpack_degrees(y),
                        vise_protocol::buf::unpack_degrees(x),
                    )
                });
                self.move_entity(m.id, path, rotation, m.on_ground, false);
            }
            C::EntityPositionSync(p) => {
                let path = match p.path {
                    play::PositionPath::Linear(at) => Path::Linear(at),
                    play::PositionPath::Stepped(steps) => Path::Stepped(steps),
                };
                self.move_entity(p.id, Some(path), Some((p.yaw, p.pitch)), p.on_ground, true)
            }
            C::TeleportEntity(p) => {
                let base = self
                    .remote
                    .as_ref()
                    .and_then(|r| r.entities.get(&p.id))
                    .map_or([0.0; 3], |t| t.position);
                let rel = |bit: i32, i: usize| {
                    if p.relatives & bit != 0 {
                        base[i] + p.position[i]
                    } else {
                        p.position[i]
                    }
                };
                let position = [
                    rel(play::relative::X, 0),
                    rel(play::relative::Y, 1),
                    rel(play::relative::Z, 2),
                ];
                self.move_entity(
                    p.id,
                    Some(Path::Linear(position)),
                    Some((p.yaw, p.pitch)),
                    p.on_ground,
                    false,
                );
                if self.driven_mount().is_some_and(|m| m.id as i32 == p.id) {
                    self.remote_move_vehicle(position, p.yaw);
                }
            }
            C::SetPassengers(p) => self.remote_set_passengers(p.vehicle, &p.passengers),
            C::SetEntityLink(p) => {
                let me = self.remote.as_ref().map(|r| r.entity_id);
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.source as u32) {
                    use vise_client::mobs::leash::Holder;
                    m.extra.leash = match p.dest {
                        0 => None,
                        d if Some(d) == me => Some(Holder::Player),
                        d => match self.remote_knots.get(&(d as u32)) {
                            Some(&block) => Some(Holder::Knot(block)),
                            None => Some(Holder::Mob(d as u32)),
                        },
                    };
                }
            }
            C::MoveVehicle(p) => self.remote_move_vehicle(p.position, p.yaw),
            C::RotateHead(p) => {
                if let Some(r) = self.remote.as_mut() {
                    let yaw = vise_protocol::buf::unpack_degrees(p.head_yaw);
                    r.head_lerps.insert(p.id, (yaw, 3));
                }
            }
            C::SetEntityData(p) => {
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    for (index, _, value) in &p.values {
                        match (index, value) {
                            (0, Value::Byte(flags)) => {
                                m.invisible = flags & 0x20 != 0;
                                m.glowing = flags & 0x40 != 0;
                            }
                            (2, Value::OptText(name)) => {
                                let text = name.as_ref().map(vise_client::entity_nbt::name_text);
                                m.custom_name =
                                    text.as_ref().map(vise_client::text::Text::plain);
                                m.custom_name_text = text;
                            }
                            (2, Value::Text(name)) => {
                                let text = vise_client::entity_nbt::name_text(name);
                                m.custom_name = Some(text.plain());
                                m.custom_name_text = Some(text);
                            }
                            (3, Value::Bool(visible)) => m.name_visible = *visible,
                            _ => {}
                        }
                    }
                }
                if let Some((_, _, Value::Byte(flags))) = p.values.iter().find(|v| v.0 == 0)
                    && let Some(r) = self.remote.as_mut()
                {
                    if flags & 0x40 != 0 {
                        r.glowing.insert(p.id);
                    } else {
                        r.glowing.remove(&p.id);
                    }
                }
                self.remote_mob_data(p.id, &p.values);
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                    && m.species() == vise_client::mobs::Species::Creeper
                {
                    for (index, _, value) in &p.values {
                        match (index, value) {
                            (16, Value::Int(dir)) => m.extra.counter = *dir,
                            (17, Value::Bool(powered)) => m.extra.charged = *powered,
                            (18, Value::Bool(ignited)) => m.extra.flag = *ignited,
                            _ => {}
                        }
                    }
                }
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    for (index, _, value) in &p.values {
                        match (index, value) {
                            (7, Value::Int(ticks)) => m.ticks_frozen = *ticks,
                            (19, Value::Bool(cold))
                                if m.species() == vise_client::mobs::Species::Strider =>
                            {
                                m.extra.flag = *cold;
                            }
                            _ => {}
                        }
                    }
                }
                let mannequin = self
                    .mobs
                    .iter()
                    .any(|m| m.id == p.id as u32 && &*m.kind.id == "minecraft:mannequin");
                let other = mannequin
                    || self.remote.as_ref().is_some_and(|r| {
                        r.level_players.contains_key(&p.id) && r.entity_id != p.id
                    });
                if other {
                    for (index, _, value) in &p.values {
                        if let (14, Value::OptPos(bed)) = (index, value) {
                            let code = bed.map_or(0, |b| self.bed_facing(b));
                            if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                                m.extra.ints[1] = code;
                            }
                        }
                        if let (15, Value::Id(arm)) = (index, value)
                            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                        {
                            m.extra.main_hand_left = *arm == 0;
                        }
                        if let (16, Value::Byte(bits)) = (index, value)
                            && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                        {
                            m.extra.hidden_model_parts = !(*bits as u8) & 0x7F;
                        }
                    }
                }
                if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                    for (index, _, value) in &p.values {
                        match (index, value) {
                            (14, Value::OptPos(bed)) => {
                                if bed.is_some() && self.sleeping.is_none() {
                                    self.sleep_timer = 0;
                                }
                                self.sleeping = *bed;
                            }
                            (0, Value::Byte(flags)) => {
                                if let Some(pl) = self.player.as_mut() {
                                    pl.sprinting = flags & 0x08 != 0;
                                }
                                if let Some(r) = self.remote.as_mut() {
                                    r.own_invisible = flags & 0x20 != 0;
                                }
                            }
                            (1, Value::Int(air)) => {
                                if let Some(pl) = self.player.as_mut() {
                                    pl.air = *air;
                                }
                            }
                            (17, Value::Float(amount)) => {
                                if let Some(pl) = self.player.as_mut() {
                                    let _ = pl.internal_set_absorption(*amount);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if (mannequin
                    || self
                        .remote
                        .as_ref()
                        .is_some_and(|r| r.level_players.contains_key(&p.id)))
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                {
                    for (index, _, value) in &p.values {
                        if let (0, Value::Byte(flags)) = (index, value) {
                            m.extra.crouching = flags & 2 != 0;
                            m.extra.sprinting = flags & 8 != 0;
                        }
                    }
                }
                for (index, _, value) in &p.values {
                    if *index == ITEM_ENTITY_ITEM
                        && let Value::Item(stack) = value
                    {
                        self.remote_item_data(p.id, stack);
                    }
                    if *index == 8
                        && let Value::Int(v) = value
                        && let Some(o) = self.orbs.iter_mut().find(|o| o.id == p.id as u32)
                    {
                        o.value = (*v).max(0) as u32;
                    }
                }
                self.remote_object_data(p.id, &p.values);
                self.remote_projectile_data(p.id, &p.values);
                self.remote_dragon_data(p.id, &p.values);
            }
            C::SetEquipment(p) => {
                self.remote_object_equipment(p.id, &p.slots);
                let items: Vec<(u8, Option<Arc<str>>, Option<ItemStack>)> = p
                    .slots
                    .iter()
                    .map(|(slot, stack)| {
                        (
                            *slot,
                            stack.as_ref().map(|s| self.item_name(s.item)),
                            self.to_stack(stack),
                        )
                    })
                    .collect();
                let charged = p.slots.iter().find(|(slot, _)| *slot == 0).map(|(_, s)| {
                    s.as_ref().is_some_and(|s| {
                        matches!(
                            s.component(vise_protocol::item::component::CHARGED_PROJECTILES),
                            Some(vise_protocol::item::Component::Stacks(list))
                                if !list.is_empty()
                        )
                    })
                });
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    if let Some(charged) = charged
                        && &*m.kind.id == "minecraft:player"
                    {
                        m.extra.charged = charged;
                    }
                    let body = vise_client::mobs::drops::head_is_body(m.species());
                    for (slot, item, stack) in items {
                        let e = &mut m.extra;
                        match slot {
                            0 => e.set_main_hand(item),
                            1 => e.off_hand = item,
                            2 => {
                                e.feet = item;
                                e.armor[3] = stack;
                            }
                            3 => {
                                e.legs = item;
                                e.armor[2] = stack;
                            }
                            4 => {
                                e.chest = item;
                                e.armor[1] = stack;
                            }
                            5 if !body => {
                                e.head = item;
                                e.armor[0] = stack;
                            }
                            6 if body => {
                                e.body_damage = stack.as_ref().map_or(0, |s| s.damage as i32);
                                e.head = item;
                            }
                            7 => m.set_saddle(stack),
                            _ => {}
                        }
                    }
                }
            }
            C::TakeItemEntity(p) => {
                let orb = self.orbs.iter().any(|o| o.id == p.item as u32);
                let item = self.dropped.iter().find(|d| d.id == p.item as u32);
                let at = match item {
                    Some(d) => Some([d.position.x, d.position.y, d.position.z]),
                    None if self
                        .remote
                        .as_ref()
                        .is_some_and(|r| r.entity_id == p.player) =>
                    {
                        self.eye_position()
                    }
                    None => None,
                };
                if !orb && let Some(at) = at {
                    let pitch = ((self.world.random.next_float() - self.world.random.next_float())
                        * 0.7
                        + 1.0)
                        * 2.0;
                    self.audio
                        .play("minecraft:entity.item.pickup", Some(at), 0.2, pitch);
                }
                let collector = if self
                    .remote
                    .as_ref()
                    .is_some_and(|r| r.entity_id == p.player)
                {
                    super::pickups::Collector::Player
                } else {
                    super::pickups::Collector::Mob(p.player as u32)
                };
                if let Some(item) = self.dropped.iter().find(|d| d.id == p.item as u32).cloned() {
                    self.add_pickup(super::pickups::Taken::Item(Box::new(item)), collector);
                } else if let Some(orb) = self.orbs.iter().find(|o| o.id == p.item as u32).cloned()
                {
                    self.add_pickup(super::pickups::Taken::Orb(Box::new(orb)), collector);
                }
                if !orb {
                    self.remove_entity(p.item);
                }
            }
            C::SetEntityMotion(p) if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) => {
                if let Some(pl) = self.player.as_mut() {
                    pl.velocity = Vec3::new(p.velocity[0], p.velocity[1], p.velocity[2]);
                }
            }
            C::SetEntityMotion(p) => self.remote_motion(p.id, p.velocity),
            C::DamageEvent(p) => {
                if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                    if let Some(pl) = self.player.as_mut() {
                        pl.health.animate_hurt();
                    }
                } else if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    m.limb_speed = 1.5;
                    m.hurt_time = 10;
                } else {
                    self.remote_dragon_hurt(p.id);
                }
            }
            C::HurtAnimation(p) => {
                if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                    if let Some(pl) = self.player.as_mut() {
                        pl.health.animate_hurt_dir(p.yaw);
                    }
                } else if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    m.hurt_time = 10;
                } else {
                    self.remote_dragon_hurt(p.id);
                }
            }
            C::EntityEvent(p) => {
                if p.event == super::fireworks::FIREWORK_EXPLODE_EVENT {
                    self.remote_firework_explosion(p.id);
                }
                self.remote_fangs_event(p.id as u32, p.event);
                if p.event == 0 {
                    self.remote_arrow_puff(p.id as u32);
                }
                if p.event == 2 {
                    self.remote_kinetic_hit(p.id);
                }
                if (24..=28).contains(&p.event)
                    && self.remote.as_ref().is_some_and(|r| r.entity_id == p.id)
                {
                    self.ui.registries.permission = (p.event - 24) as u8;
                    self.remote_world_options_permission(p.event >= 26);
                }
                if p.event == 69 {
                    self.remote_ravager_roar(p.id);
                }
                if p.event == 21 {
                    self.start_guardian_sound(p.id as u32);
                }
                if p.event == 45 {
                    self.fox_eat_particles(p.id as u32);
                }
                if p.event == 1
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                    && m.species() == vise_client::mobs::Species::Rabbit
                {
                    vise_client::mobs::rabbit::hop_event(m);
                }
                if p.event == 19
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                {
                    vise_client::mobs::aquatic::squid_cycle_event(m);
                }
                if p.event == 55
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                {
                    std::mem::swap(&mut m.extra.main_hand, &mut m.extra.off_hand);
                }
                if let Some(slot) = vise_client::mobs::item_break::equipment_break_slot(p.event)
                {
                    self.remote_equipment_break(p.id, slot);
                }
                if p.event == 3
                    && let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32)
                {
                    m.health = 0.0;
                }
                if p.event == 3
                    && self.remote.as_ref().is_some_and(|r| r.entity_id == p.id)
                    && let Some(pl) = self.player.as_ref()
                {
                    let at = [pl.position.x, pl.position.y, pl.position.z];
                    self.audio
                        .play("minecraft:entity.player.death", Some(at), 1.0, 1.0);
                }
                if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    vise_client::mobs::sync::entity_event(m, p.event as u8);
                }
                if p.event == 60
                    && let Some(m) = self.mobs.iter().find(|m| m.id == p.id as u32)
                {
                    let (at, w, h) = (m.position, f64::from(m.width()), f64::from(m.height()));
                    self.death_poof([at.x, at.y, at.z], w, h);
                }
                if p.event == super::totem::TOTEM_EVENT {
                    self.remote_totem(p.id);
                }
                if p.event == 20 && self.mobs.iter().any(|m| m.id == p.id as u32) {
                    self.poof_particles(p.id as u32);
                }
                if matches!(p.event, 6 | 7 | 12 | 13 | 14 | 18) {
                    self.entity_event_particles(p.id as u32, p.event);
                }
                if p.event == 3 {
                    let crumbs = self
                        .creatures
                        .projectiles
                        .iter()
                        .find(|pr| pr.id == p.id as u32)
                        .and_then(|pr| {
                            let item = match pr.kind {
                                vise_client::projectiles::ProjectileKind::Snowball => {
                                    "minecraft:snowball"
                                }
                                vise_client::projectiles::ProjectileKind::Egg => "minecraft:egg",
                                _ => return None,
                            };
                            Some(([pr.position.x, pr.position.y, pr.position.z], item))
                        });
                    if let Some((at, item)) = crumbs {
                        self.item_burst(at, item);
                    }
                }
                if p.event == super::combat::DROWN_EVENT {
                    self.remote_drown(p.id);
                }
                if p.event == 53 {
                    let at = if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                        self.player
                            .as_ref()
                            .map(|q| [q.position.x, q.position.y, q.position.z])
                    } else {
                        self.mobs
                            .iter()
                            .find(|m| m.id == p.id as u32)
                            .map(|m| [m.position.x, m.position.y, m.position.z])
                    };
                    if let Some(at) = at {
                        self.honey_slide_particles(at);
                    }
                }
                if p.event == super::consume::USE_ITEM_COMPLETE_EVENT
                    && self.remote.as_ref().is_some_and(|r| r.entity_id == p.id)
                {
                    self.remote_complete_using();
                }
            }
            C::Animate(p) => self.remote_animate(p.id, p.action),
            C::SwingAnimation(p) => {
                let hand = if p.hand == 1 { Hand::Off } else { Hand::Main };
                let animation = vise_client::first_person::SwingAnimation {
                    kind: p.kind.clamp(0, 255) as u8,
                    duration: p.duration,
                };
                if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) {
                    self.swing_hand(hand, animation);
                } else if let Some(m) = self.mobs.iter_mut().find(|m| m.id == p.id as u32) {
                    m.swing_hand_modified(hand == Hand::Off, animation);
                }
            }
            C::UpdateAttributes(p) if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id) => {
                if let Some(pl) = self.player.as_mut() {
                    vise_client::lan::attributes::apply(&mut pl.attributes, &p.values);
                }
                self.apply_remote_health();
            }
            C::UpdateAttributes(p) => self.remote_mob_attributes(p.id, &p.values),
            C::UpdateMobEffect(p) => self.remote_effect(&p),
            C::RemoveMobEffect(p) => {
                if self.remote.as_ref().is_some_and(|r| r.entity_id == p.id)
                    && let Some(id) = self.remote_effect_id(p.effect)
                {
                    self.potions.effects.list.retain(|e| e.id != id);
                }
            }
            C::ChangeDifficulty(p) => {
                self.ui.difficulty = p.difficulty.clamp(0, 3) as u8;
                self.level.difficulty_locked = p.locked;
                self.remote_world_options_difficulty(self.ui.difficulty, p.locked);
            }
            C::GameRuleValues(p) => self.remote_world_options_rules(p.entries),
            C::SetHealth(p) => {
                let flash = self.remote.as_mut().is_some_and(|r| {
                    r.health = (p.health, p.food, p.saturation);
                    std::mem::replace(&mut r.flash_on_set_health, true)
                });
                if flash && let Some(pl) = self.player.as_mut() {
                    pl.health.flash_health_change(p.health);
                }
                self.apply_remote_health();
                let shown = self.remote.as_ref().is_none_or(|r| r.show_death_screen);
                if p.health <= 0.0 && shown && self.screen != Screen::Dead && self.player.is_some()
                {
                    self.screen = Screen::Dead;
                    self.ui.death_time = 0;
                    self.ui.death_confirm = None;
                    self.capture(false);
                }
            }
            C::PlayerCombatKill(p) => {
                let message = remote_text(&p.message);
                let me = self.remote.as_ref().is_some_and(|r| r.entity_id == p.id);
                if let Some(r) = self.remote.as_mut() {
                    r.death_message = Some(message);
                }
                if me && self.screen != Screen::Dead && self.player.is_some() {
                    if self.remote.as_ref().is_none_or(|r| r.show_death_screen) {
                        self.screen = Screen::Dead;
                        self.ui.death_time = 0;
                        self.ui.death_confirm = None;
                        self.capture(false);
                    } else {
                        self.respawn_player()?;
                    }
                }
            }
            C::TrackedWaypoint(p) => self.handle_waypoint(&p),
            C::SetExperience(p) => {
                self.experience = Experience {
                    level: p.level,
                    progress: p.progress,
                    total: p.total,
                }
            }
            C::SetTime(p) => {
                self.world.game_time = p.game_time;
                self.remote_clock_tick();
                let clock = {
                    let r = self.remote.as_ref();
                    let names =
                        r.and_then(|r| r.registries.registries.get("minecraft:world_clock"));
                    let wanted = r.map(|r| r.dimension.name.clone()).unwrap_or_default();
                    p.clocks
                        .iter()
                        .find(|c| {
                            names
                                .and_then(|n| n.get(c.clock as usize))
                                .is_some_and(|(n, _)| *n == wanted)
                        })
                        .or_else(|| p.clocks.first())
                        .cloned()
                };
                if let Some(c) = clock {
                    self.world.day_time = c.total_ticks;
                    if let Some(r) = self.remote.as_mut() {
                        r.clock = Some(c.clock);
                        r.clock_rate = c.rate;
                        r.clock_partial = c.partial_tick;
                    }
                }
            }
            C::GameEvent(p) => match p.event {
                10 => {
                    self.creatures.elder_apparition = super::mob_blocks::APPARITION_TICKS;
                    if p.param == 1.0
                        && let Some(at) = self.player.as_ref().map(|m| m.position)
                    {
                        self.audio.play(
                            "minecraft:entity.elder_guardian.curse",
                            Some([at.x, at.y, at.z]),
                            1.0,
                            1.0,
                        );
                    }
                }
                4 if self.screen != Screen::Win => {
                    self.open_win_screen(true, super::win::Finish::RemoteRespawn);
                }
                1 => {
                    self.world.weather.raining = true;
                    self.world.weather.rain_level = (0.0, 0.0);
                }
                2 => {
                    self.world.weather.raining = false;
                    self.world.weather.rain_level = (1.0, 1.0);
                }
                3 => {
                    let mode = p.param as i32;
                    self.set_mode_id(mode);
                    if let Some(r) = self.remote.as_mut() {
                        r.game_mode = mode;
                    }
                }
                11 => {
                    if let Some(r) = self.remote.as_mut() {
                        r.show_death_screen = p.param == 0.0;
                    }
                }
                7 => {
                    self.world.weather.rain_level = (p.param, p.param);
                    self.world.weather.raining = p.param > 0.0;
                }
                8 => {
                    self.world.weather.thunder_level = (p.param, p.param);
                    self.world.weather.thundering = p.param > 0.0;
                }
                _ => {}
            },
            C::PlayerAbilities(p) => {
                self.creative = p.flags & 8 != 0;
                if let Some(r) = self.remote.as_mut() {
                    r.abilities = Some(p);
                }
                self.apply_remote_abilities();
            }
            C::SetHeldSlot(p) => {
                if (0..9).contains(&p.slot) {
                    self.selected = p.slot as usize;
                }
            }
            C::ContainerSetContent(p) => {
                self.remote_menu_state(p.container, p.state);
                for (slot, item) in p.items.iter().enumerate() {
                    let stack = self.to_stack(item);
                    self.set_menu_slot(p.container, slot as i32, stack);
                }
                self.carried = self.to_stack(&p.carried);
                self.remote_synced();
            }
            C::ContainerSetSlot(p) => {
                self.remote_menu_state(p.container, p.state);
                let stack = self.to_stack(&p.item);
                self.tutorial_get_item(stack.as_ref());
                if p.container == 0
                    && (36..=45).contains(&p.slot)
                    && let Some(new) = &stack
                {
                    let (pop, old) = match p.slot {
                        45 => (9, self.offhand.as_ref()),
                        s => (s as usize - 36, self.inventory[s as usize - 36].as_ref()),
                    };
                    if old.is_none_or(|old| old.count < new.count) {
                        self.hotbar_pop[pop] = 5;
                    }
                }
                let before = self.creative_stacks();
                self.set_menu_slot(p.container, i32::from(p.slot), stack);
                self.remote_synced_since(&before);
            }
            C::SetCursorItem(p) => {
                let stack = self.to_stack(&p.item);
                self.tutorial_get_item(stack.as_ref());
                self.carried = stack;
            }
            C::SetPlayerInventory(p) => {
                let stack = self.to_stack(&p.item);
                self.tutorial_get_item(stack.as_ref());
                let before = self.creative_stacks();
                let held = match p.slot {
                    0..=35 => self.inventory[p.slot as usize].as_ref(),
                    40 => self.offhand.as_ref(),
                    _ => None,
                };
                let changed = !super::hand::matches_for_swap(held, stack.as_ref());
                if let Ok(slot) = usize::try_from(p.slot) {
                    self.remote_hand_slot_set(slot, changed);
                }
                match p.slot {
                    0..=35 => self.inventory[p.slot as usize] = stack,
                    36..=39 => self.armor[3 - (p.slot - 36) as usize] = stack,
                    40 => self.offhand = stack,
                    _ => {}
                }
                self.remote_synced_since(&before);
            }
            C::ContainerSetData(p) if self.open_menu_name(p.container) == "minecraft:lectern" => {
                self.remote_lectern_page(i32::from(p.value));
            }
            C::ContainerSetData(p) if self.remote_station_open(p.container) => {
                self.remote_station_data(usize::try_from(p.id).unwrap_or(usize::MAX), p.value);
            }
            C::ContainerSetData(p) => {
                let v = i32::from(p.value);
                match self
                    .container
                    .and_then(|key| self.world.block_entities.get_mut(&key))
                {
                    Some(BlockEntity::Furnace(f)) => match p.id {
                        0 => f.lit_remaining = v,
                        1 => f.lit_total = v,
                        2 => f.cooking_timer = v,
                        3 => f.cooking_total = v,
                        _ => {}
                    },
                    Some(BlockEntity::Crafter(c)) => match usize::try_from(p.id) {
                        Ok(k @ 0..9) => c.disabled[k] = v == 1,
                        Ok(9) => c.triggered = v == 1,
                        _ => {}
                    },
                    Some(BlockEntity::Brewing(b)) => match p.id {
                        0 => b.brew_time = v,
                        1 => b.fuel = v,
                        2 => b.total_brew_time = v,
                        3 => b.total_fuel = v,
                        _ => {}
                    },
                    _ => {}
                }
            }
            C::OpenScreen(p) => self.remote_open_screen(p),
            C::MountScreenOpen(p) => self.remote_open_mount(p),
            C::RecipeBookAdd(p) => self.remote_recipe_book_add(p),
            C::RecipeBookRemove(p) => self.remote_recipe_book_remove(p),
            C::RecipeBookSettings(p) => self.remote_recipe_book_settings(p),
            C::PlaceGhostRecipe(p) => self.remote_ghost_recipe(p),
            C::MerchantOffers(p) => self.remote_merchant_offers(p),
            C::ContainerClose(_) if self.remote_lectern_closed() => {}
            C::ContainerClose(_) => {
                if self.screen == Screen::Inventory {
                    self.remote_close_local();
                    self.screen = Screen::Playing;
                    self.capture(true);
                }
            }
            C::TagQuery(p) => self.answer_tag_query(p.transaction, p.tag),
            C::Unknown(id, data)
                if id == vise_protocol::ids::play::clientbound::COMMAND_SUGGESTIONS =>
            {
                self.handle_command_suggestions(&data);
            }
            C::CustomChatCompletions(p) => {
                if let Some(r) = self.remote.as_mut() {
                    vise_client::command::modify_custom_completions(
                        &mut r.custom_completions,
                        vise_client::command::CustomCompletionAction::from_id(p.action),
                        &p.entries,
                    );
                    self.ui.registries.custom_completions = r.custom_completions.clone();
                }
            }
            C::Commands(p) => {
                let tree = vise_client::command::network::from_packet(
                    &p,
                    vise_client::command::vanilla::dispatcher(),
                );
                if let Some(r) = self.remote.as_mut() {
                    r.commands = Some(tree);
                }
            }
            C::Cooldown(p) => {
                let group: Arc<str> = Arc::from(p.group.as_str());
                if p.duration == 0 {
                    self.cooldowns.remove(&group);
                } else {
                    self.cooldowns.add(group, p.duration);
                }
            }
            C::SystemChat(p) => {
                let text = remote_text(&p.content);
                if p.overlay {
                    self.titles.set_overlay(text, false);
                } else {
                    self.chat_line(text, vise_client::chat::ChatSource::Server);
                }
            }
            C::PlayerChat(p) => {
                use vise_client::scoreboard::tag_to_json;
                let Some(r) = self.remote.as_mut() else {
                    return Ok(());
                };
                let last_seen: Option<Vec<Signature>> =
                    p.last_seen.iter().map(|s| r.signatures.unpack(s)).collect();
                let Some(last_seen) = last_seen else {
                    log::error!(
                        "Message from player with ID {:032x} referenced unrecognized signature id",
                        p.sender
                    );
                    let reason = vise_client::text::Text::translate(
                        "multiplayer.disconnect.invalid_packet",
                        &[],
                    );
                    return self.leave_server(Some(reason.plain()));
                };
                let signature = p
                    .signature
                    .as_deref()
                    .and_then(vise_protocol::secure_chat::signature);
                r.signatures.push(&last_seen, signature.as_ref());
                let content = p
                    .unsigned_content
                    .as_ref()
                    .map_or_else(|| serde_json::Value::from(p.content.as_str()), tag_to_json);
                let line = self.format_chat(&p.chat_type, content);
                self.handle_player_chat(line, signature);
            }
            C::DisguisedChat(p) => {
                let content = vise_client::scoreboard::tag_to_json(&p.message);
                let line = self.format_chat(&p.chat_type, content);
                self.handle_player_chat(line, None);
            }
            C::DeleteChat(p) => self.handle_delete_chat(&p.signature)?,
            C::PlayerInfoUpdate(p) => {
                if let Some(r) = self.remote.as_mut() {
                    if p.actions & 1 != 0 {
                        for e in &p.entries {
                            if let Some(name) = &e.name {
                                r.tab_players.entry(e.uuid).or_insert_with(|| {
                                    vise_client::tab_list::TabPlayer::new(e.uuid, name.clone())
                                });
                                if let Some(t) = e.properties.iter().find(|p| p.name == "textures")
                                {
                                    use vise_client::skins::{self, Signature};
                                    let signature = if t.signature.is_some() {
                                        Signature::Signed
                                    } else {
                                        Signature::Unsigned
                                    };
                                    let textures =
                                        skins::unpack(&t.value, signature, &skins::TEXTURE_URIS);
                                    r.skin_textures.entry(e.uuid).or_insert(textures);
                                }
                            }
                        }
                    }
                    for e in p.entries {
                        if let Some(t) = r.tab_players.get_mut(&e.uuid) {
                            if let Some(mode) = e.game_mode {
                                t.game_mode = mode;
                            }
                            if let Some(latency) = e.latency {
                                t.latency = latency;
                            }
                            if let Some(name) = &e.display_name {
                                t.display_name =
                                    name.as_ref().map(vise_client::scoreboard::tag_to_json);
                            }
                            if let Some(order) = e.list_order {
                                t.list_order = order;
                            }
                            if let Some(hat) = e.show_hat {
                                t.show_hat = hat;
                            }
                            match e.listed {
                                Some(true) => {
                                    r.listed.insert(e.uuid);
                                }
                                Some(false) => {
                                    r.listed.remove(&e.uuid);
                                }
                                None => {}
                            }
                        }
                        if let Some(name) = e.name {
                            r.players.insert(e.uuid, name);
                        }
                    }
                }
            }
            C::PlayerInfoRemove(p) => {
                if let Some(r) = self.remote.as_mut() {
                    for u in p.uuids {
                        r.players.remove(&u);
                        r.tab_players.remove(&u);
                        r.skin_textures.remove(&u);
                        r.listed.remove(&u);
                    }
                }
            }
            C::TabList(p) => {
                let text = |t: &Tag| {
                    let json = vise_client::scoreboard::tag_to_json(t);
                    let text = vise_client::scoreboard::comp_text(&json);
                    text.spans
                        .iter()
                        .any(|span| !span.text.is_empty())
                        .then_some(text)
                };
                self.tab_list.header = text(&p.header);
                self.tab_list.footer = text(&p.footer);
            }
            C::InitializeBorder(p) => {
                let b = &mut self.border;
                b.set_center([p.center_x, p.center_z]);
                if p.lerp_time > 0 {
                    b.lerp_between(p.old_size, p.new_size, p.lerp_time);
                } else {
                    b.set_size(p.new_size);
                }
                b.set_absolute_max_size(p.absolute_max_size);
                b.warning_distance = p.warning_blocks;
                b.warning_time = p.warning_time;
            }
            C::SetBorderCenter(p) => self.border.set_center([p.x, p.z]),
            C::SetBorderLerpSize(p) => {
                self.border
                    .lerp_between(p.old_size, p.new_size, p.lerp_time)
            }
            C::SetBorderSize(p) => self.border.set_size(p.size),
            C::SetBorderWarningDelay(p) => self.border.warning_time = p.delay,
            C::SetBorderWarningDistance(p) => self.border.warning_distance = p.blocks,
            C::SetObjective(p) => self.remote_objective(p),
            C::SetDisplayObjective(p) => {
                let slot = match p.slot {
                    0 => "list".to_owned(),
                    2 => "below_name".to_owned(),
                    n @ 3..=18 => format!(
                        "sidebar.team.{}",
                        vise_client::text::COLORS[(n - 3) as usize].1
                    ),
                    _ => "sidebar".to_owned(),
                };
                let board = &mut self.score.board;
                let objective = board.objective(&p.objective).map(|o| o.name.clone());
                board.set_display_objective(&slot, objective.as_deref());
            }
            C::SetScore(p) => {
                let board = &mut self.score.board;
                if board.objective(&p.objective).is_some() {
                    let s = board.score_mut(&p.owner, &p.objective);
                    s.value = p.score;
                    s.display = p
                        .display
                        .as_ref()
                        .map(vise_client::scoreboard::tag_to_json);
                    s.number_format = p.number_format.as_ref().map(number_format);
                }
            }
            C::ResetScore(p) => match &p.objective {
                Some(o) => self.score.board.reset_one(&p.owner, o),
                None => self.score.board.reset_all(&p.owner),
            },
            C::SetPlayerTeam(p) => self.remote_team(p),
            C::SetChunkCacheRadius(p) => {
                self.streaming.radius = self.settings.render_distance.min(p.distance + 1).max(2);
                if let Some(r) = self.remote.as_mut() {
                    r.view_distance = p.distance;
                }
                self.drop_remote_chunks_out_of_range();
            }
            C::SetChunkCacheCenter(p) => {
                if let Some(r) = self.remote.as_mut() {
                    r.chunk_center = (p.x, p.z);
                }
                self.drop_remote_chunks_out_of_range();
            }
            C::BlockEntityData(p) => self.remote_block_entity_data(p.pos, p.kind, &p.tag),
            C::OpenSignEditor(p) => self.remote_open_sign_editor(p),
            C::OpenBook(p) => self.remote_open_book(p.hand),
            C::SetDefaultSpawnPosition(p) => {
                let [x, y, z] = p.pos.pos;
                self.spawn = (p.pos.dimension == "minecraft:overworld")
                    .then(|| Vec3::new(f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5));
            }
            C::ServerData(p) => self.remote_server_data(p.icon.as_deref()),
            C::MapItemData(p) => self.remote_map_item_data(&p),
            C::TickingState(p) => {
                self.clock.set_rate(p.rate);
                self.clock.set_frozen(p.frozen);
            }
            C::TickingStep(p) => self.clock.set_frozen_ticks_to_run(p.steps),
            C::UpdateAdvancements(p) => {
                let icons = p
                    .added
                    .iter()
                    .map(|a| {
                        a.display
                            .as_ref()
                            .map_or_else(String::new, |d| self.item_name(d.icon.item).to_string())
                    })
                    .collect();
                self.remote_advancements(p, icons);
            }
            C::AwardStats(p) => {
                let Some(r) = self.remote.as_ref() else {
                    return Ok(());
                };
                let mut known = Vec::new();
                for (ty, value, count) in p.stats {
                    let Some(ty) = r
                        .builtin
                        .get("minecraft:stat_type")
                        .and_then(|t| t.name(ty))
                    else {
                        continue;
                    };
                    let values = match ty {
                        "minecraft:mined" => "minecraft:block",
                        "minecraft:killed" | "minecraft:killed_by" => "minecraft:entity_type",
                        "minecraft:custom" => "minecraft:custom_stat",
                        _ => "minecraft:item",
                    };
                    if let Some(key) = r.builtin.get(values).and_then(|v| v.name(value)) {
                        known.push((ty.to_owned(), key.to_owned(), count));
                    }
                }
                for (ty, key, count) in known {
                    self.stats.values.entry(ty).or_default().insert(key, count);
                }
            }
            C::BossEvent(p) => self.apply_boss_event(&p),
            C::SetActionBarText(p) => self.titles.set_overlay(remote_text(&p.text), false),
            C::SetTitleText(p) => self.titles.set_title(remote_text(&p.text)),
            C::SetSubtitleText(p) => self.titles.set_subtitle(remote_text(&p.text)),
            C::SetTitlesAnimation(p) => self.titles.set_times(p.fade_in, p.stay, p.fade_out),
            C::ShowDialog(p) => self.show_play_dialog(&p.dialog),
            C::ClearDialog(_) => self.clear_dialog(),
            C::ClearTitles(p) => {
                self.titles.clear();
                if p.value {
                    self.titles.reset_times();
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn remote_objective(&mut self, p: play::SetObjective) {
        use vise_client::scoreboard::{RenderType, tag_to_json};
        let board = &mut self.score.board;
        match (p.method, p.display) {
            (1, _) => board.remove_objective(&p.name),
            (method, Some((name, render, format))) => {
                let render = if render == 1 {
                    RenderType::Hearts
                } else {
                    RenderType::Integer
                };
                let format = format.as_ref().map(number_format);
                if method == 0 {
                    board.add_objective(
                        &p.name,
                        "dummy",
                        tag_to_json(&name),
                        render,
                        false,
                        format,
                    );
                } else if let Some(o) = board.objective_mut(&p.name) {
                    o.display_name = tag_to_json(&name);
                    o.render_type = render;
                    o.number_format = format;
                }
            }
            _ => {}
        }
    }
    fn remote_team(&mut self, p: play::SetPlayerTeam) {
        use vise_client::scoreboard::{CollisionRule, Visibility, tag_to_json};
        let board = &mut self.score.board;
        if p.method == 1 {
            board.remove_team(&p.name);
            return;
        }
        if p.method == 0 {
            board.add_team(&p.name);
        }
        if let Some(params) = &p.parameters
            && let Some(team) = board.team_mut(&p.name)
        {
            team.display_name = tag_to_json(&params.display_name);
            team.prefix = tag_to_json(&params.prefix);
            team.suffix = tag_to_json(&params.suffix);
            team.color = params
                .color
                .and_then(|c| vise_client::text::COLORS.get(c as usize))
                .map(|c| c.1.to_owned());
            team.allow_friendly_fire = params.options & 1 != 0;
            team.see_friendly_invisibles = params.options & 2 != 0;
            team.nametag_visibility = Visibility::ALL
                .get(params.name_tag_visibility as usize)
                .copied()
                .unwrap_or(Visibility::Always);
            team.collision_rule = CollisionRule::ALL
                .get(params.collision_rule as usize)
                .copied()
                .unwrap_or(CollisionRule::Always);
        }
        match p.method {
            0 | 3 => {
                for player in &p.players {
                    board.add_to_team(player, &p.name);
                }
            }
            4 => {
                for player in &p.players {
                    board.remove_from_team(player);
                }
            }
            _ => {}
        }
    }
    pub(super) fn remote_tab_players(
        &mut self,
    ) -> Option<(Vec<vise_client::tab_list::TabPlayer>, bool, Vec<u128>)> {
        self.update_remote_skins();
        let mut players: Vec<_> = {
            let r = self.remote.as_ref()?;
            r.listed
                .iter()
                .filter_map(|u| r.tab_players.get(u).cloned())
                .collect()
        };
        for p in &mut players {
            p.skin = self
                .remote
                .as_ref()
                .and_then(|r| r.skins.get(&p.uuid).cloned())
                .unwrap_or_else(|| vise_client::tab_list::default_skin(p.uuid).to_owned());
            self.gui
                .atlas
                .set_object_skin(format!("id:{:032x}", p.uuid), p.skin.clone());
            self.gui.atlas.set_object_skin(
                format!("name:{}", p.name.to_ascii_lowercase()),
                p.skin.clone(),
            );
        }
        let r = self.remote.as_ref()?;
        let upside_down = players
            .iter()
            .filter(|p| {
                (p.name == "Dinnerbone" || p.name == "Grumm")
                    && (p.uuid == r.profile || r.level_players.values().any(|u| *u == p.uuid))
            })
            .map(|p| p.uuid)
            .collect();
        Some((players, r.online_mode, upside_down))
    }

    pub(super) fn resolve_inline_profiles(&mut self, profiles: &[serde_json::Value]) {
        use vise_client::skins;
        for profile in profiles {
            let skin = skins::head_skin(
                profile,
                &mut self.skin_profiles.borrow_mut(),
                &mut self.skin_downloads.borrow_mut(),
                |location| self.gui.atlas.sprites.contains_key(location),
            );
            let id = skins::parse_profile(profile).id;
            let name = profile
                .as_str()
                .or_else(|| profile.get("name").and_then(serde_json::Value::as_str));
            let keys = [
                Some(format!("id:{id:032x}")),
                name.map(|n| format!("name:{}", n.to_ascii_lowercase())),
            ];
            for key in keys.iter().flatten() {
                if let Some(listed) = self.gui.atlas.object_skins.get(key)
                    && listed.starts_with("skins/")
                {
                    let listed = listed.clone();
                    self.again_skin(&listed);
                }
            }
            self.gui
                .atlas
                .set_object_skin(format!("id:{id:032x}"), skin.clone());
            if let Some(name) = name {
                self.gui
                    .atlas
                    .set_object_skin(format!("name:{}", name.to_ascii_lowercase()), skin);
            }
        }
    }
    fn own_textures(&self) -> Option<vise_client::skins::Textures> {
        use vise_client::skins::{self, NameOrId, Resolve};
        let auth = vise_client::auth::current()?;
        let state = self
            .skin_profiles
            .borrow_mut()
            .state(&NameOrId::Id(auth.uuid));
        let Resolve::Done(Some(profile)) = state else {
            return None;
        };
        let (value, signature) = profile.textures()?;
        Some(skins::unpack(&value, signature, &skins::TEXTURE_URIS))
    }
    fn update_remote_skins(&mut self) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let mut entries = skin_entries(
            &r.level_players,
            &r.skin_textures,
            &mut r.level_player_textures,
        );
        let r = &*r;
        let local = r.profile;
        if !entries.iter().any(|(u, t)| *u == local && t.skin.is_some())
            && let Some(own) = self.own_textures()
        {
            entries.retain(|(u, _)| *u != local);
            entries.push((local, own));
        }
        let mut resolved: Vec<(u128, String)> = Vec::new();
        let mut slim = std::collections::HashSet::new();
        let mut capes: HashMap<u128, String> = HashMap::new();
        for (uuid, textures) in entries {
            let skin = {
                let mut downloads = self.skin_downloads.borrow_mut();
                vise_client::skins::lookup(&textures, uuid != local, |kind, url| {
                    downloads.state(kind, url)
                })
            };
            if let Some(skin) = skin {
                if textures.slim {
                    slim.insert(uuid);
                }
                if let Some(cape) = vise_client::skins::cape(&textures) {
                    capes.insert(uuid, cape);
                }
                resolved.push((uuid, skin));
            }
        }
        if !resolved.iter().any(|(u, _)| *u == local)
            && let Some((rgba, is_slim)) = vise_client::skins::offline_skin()
        {
            const OFFLINE: &str = "vise/offline_skin";
            if !self.skin_slots.contains(OFFLINE) {
                self.store_skin(OFFLINE.into(), rgba.clone());
            }
            if *is_slim {
                slim.insert(local);
            }
            resolved.push((local, OFFLINE.into()));
        }
        if !capes.contains_key(&local)
            && let Some(rgba) = vise_client::skins::offline_cape()
        {
            const OFFLINE: &str = "capes/offline";
            if !self.skin_slots.contains(OFFLINE) {
                self.store_skin(OFFLINE.into(), rgba.to_vec());
            }
            capes.insert(local, OFFLINE.into());
        }
        let skin_of = |skins: &HashMap<u128, String>, uuid: u128| match skins.get(&uuid) {
            Some(skin) => (skin.clone(), slim.contains(&uuid)),
            None => {
                let skin = vise_client::tab_list::default_skin(uuid);
                (skin.to_owned(), skin.contains("/slim/"))
            }
        };
        let Some((level, skins, own)) = self.remote.as_mut().map(|r| {
            r.skins = resolved.into_iter().collect();
            (
                r.level_players
                    .iter()
                    .map(|(id, uuid)| (*id as u32, *uuid))
                    .collect::<Vec<_>>(),
                r.skins.clone(),
                r.profile,
            )
        }) else {
            return;
        };
        for (id, uuid) in level {
            let Some(i) = self
                .mobs
                .iter()
                .position(|m| m.id == id && &*m.kind.id == "minecraft:player")
            else {
                continue;
            };
            let (skin, is_slim) = self.drawn_skin(uuid, skin_of(&skins, uuid));
            let cape = capes.get(&uuid).filter(|c| self.again_skin(c)).cloned();
            let mob = &mut self.mobs[i];
            mob.skin = Some(skin);
            mob.slim = is_slim;
            mob.cape = cape;
        }
        if self.player_model.is_some() {
            let (skin, is_slim) = self.drawn_skin(own, skin_of(&skins, own));
            let cape = capes
                .get(&own)
                .filter(|_| self.settings.file.bool("modelPart_cape"))
                .filter(|c| self.again_skin(c))
                .cloned();
            if let Some(mob) = self.player_model.as_mut() {
                mob.skin = Some(skin);
                mob.slim = is_slim;
                mob.cape = cape;
            }
        }
        let mannequins: Vec<(u32, vise_client::skins::Textures)> = self
            .mobs
            .iter()
            .filter(|m| &*m.kind.id == "minecraft:mannequin" && m.profile.is_some())
            .map(|m| {
                let profile = m.profile.as_ref();
                let t = profile.and_then(|p| p.properties.iter().find(|p| p.name == "textures"));
                let textures = if let Some(t) = t {
                    let signature = if t.signature.is_some() {
                        vise_client::skins::Signature::Signed
                    } else {
                        vise_client::skins::Signature::Unsigned
                    };
                    vise_client::skins::unpack(
                        &t.value,
                        signature,
                        &vise_client::skins::TEXTURE_URIS,
                    )
                } else {
                    let key = profile.and_then(|p| match (&p.name, p.uuid) {
                        (Some(name), None) => {
                            Some(vise_client::skins::NameOrId::Name(name.clone()))
                        }
                        (None, Some(id)) => Some(vise_client::skins::NameOrId::Id(id)),
                        _ => None,
                    });
                    key.and_then(|key| match self.skin_profiles.borrow_mut().state(&key) {
                        vise_client::skins::Resolve::Done(Some(profile)) => {
                            profile.textures().map(|(value, signature)| {
                                vise_client::skins::unpack(
                                    &value,
                                    signature,
                                    &vise_client::skins::TEXTURE_URIS,
                                )
                            })
                        }
                        _ => None,
                    })
                    .unwrap_or_default()
                };
                (m.id, textures)
            })
            .collect();
        let mut resolved: Vec<(u32, Option<String>, bool)> = Vec::new();
        for (id, textures) in mannequins {
            let skin = {
                let mut downloads = self.skin_downloads.borrow_mut();
                vise_client::skins::lookup(&textures, false, |kind, url| {
                    downloads.state(kind, url)
                })
            };
            let slim = skin.is_some() && textures.slim;
            resolved.push((id, skin, slim));
        }
        for (id, skin, slim) in resolved {
            let skin = skin.filter(|s| !s.starts_with("skins/") || self.again_skin(s));
            if let Some(mob) = self
                .mobs
                .iter_mut()
                .find(|m| m.id == id && &*m.kind.id == "minecraft:mannequin")
            {
                mob.slim = slim && skin.is_some();
                mob.skin = skin;
            }
        }
    }
    pub(super) fn store_skin(&mut self, location: String, rgba: Vec<u8>) {
        let Some((slot, evicted)) = self.skin_slots.place(location.clone()) else {
            return;
        };
        if let Some(old) = evicted {
            self.gui.atlas.sprites.remove(&old);
        }
        let Some(mut rect) = self
            .gui
            .atlas
            .sprites
            .get(&format!("vise/skin{slot}"))
            .copied()
        else {
            return;
        };
        let (w, h) = (self.gui.atlas.width as f32, self.gui.atlas.height as f32);
        self.map_patches.push(vise_client::font::Patch {
            x: (rect.u0 * w).round() as u32,
            y: (rect.v0 * h).round() as u32,
            width: 64,
            height: 64,
            rgba,
        });
        if location.starts_with("capes/") {
            rect.v1 = (rect.v0 + rect.v1) / 2.0;
        }
        self.gui.atlas.sprites.insert(location, rect);
    }
    fn again_skin(&mut self, location: &str) -> bool {
        let mut downloads = self.skin_downloads.borrow_mut();
        if self.skin_slots.contains(location) {
            downloads.drawn(location);
            true
        } else {
            downloads.again(location);
            false
        }
    }
    pub(super) fn drawn_skin(
        &mut self,
        uuid: u128,
        (skin, slim): (String, bool),
    ) -> (String, bool) {
        if skin.starts_with("skins/") && !self.again_skin(&skin) {
            let skin = vise_client::tab_list::default_skin(uuid);
            return (skin.to_owned(), skin.contains("/slim/"));
        }
        (skin, slim)
    }
    pub(super) fn face_skin(&mut self, uuid: u128, skin: &str) -> String {
        self.drawn_skin(uuid, (skin.to_owned(), false)).0
    }
    fn format_chat(
        &self,
        bound: &play::ChatTypeBound,
        content: serde_json::Value,
    ) -> vise_client::text::Text {
        use vise_client::scoreboard::{comp_text, tag_to_json};
        let Some(r) = self.remote.as_ref() else {
            return comp_text(&content);
        };
        let decoration = r
            .registries
            .data("minecraft:chat_type", bound.chat_type)
            .and_then(|t| t.get("chat"));
        let key = match decoration.and_then(|d| d.get("translation_key")) {
            Some(Tag::String(s)) => s.to_string_lossy(),
            _ => "chat.type.text".into(),
        };
        let parameter = |name: &str| match name {
            "sender" => tag_to_json(&bound.name),
            "target" => bound
                .target
                .as_ref()
                .map_or_else(|| serde_json::Value::from(""), tag_to_json),
            _ => content.clone(),
        };
        let with: Vec<serde_json::Value> = match decoration.and_then(|d| d.get("parameters")) {
            Some(Tag::List(list)) => list
                .iter()
                .map(|p| match p {
                    Tag::String(s) => parameter(&s.to_string_lossy()),
                    _ => serde_json::Value::from(""),
                })
                .collect(),
            _ => vec![parameter("sender"), parameter("content")],
        };
        let mut component = match decoration.and_then(|d| d.get("style")) {
            Some(style @ Tag::Compound(_)) => tag_to_json(style),
            _ => serde_json::json!({}),
        };
        component["translate"] = key.into();
        component["with"] = with.into();
        comp_text(&component)
    }
    fn remote_item_data(&mut self, id: i32, stack: &Option<Stack>) {
        let Some(stack) = self.to_stack(stack) else {
            return;
        };
        let position = self
            .remote
            .as_mut()
            .and_then(|r| r.pending_items.remove(&id));
        if let Some(item) = self.dropped.iter_mut().find(|d| d.id == id as u32) {
            item.stack = stack;
            return;
        }
        let Some((p, velocity)) = position else {
            return;
        };
        let at = Vec3::new(p[0], p[1], p[2]);
        let bob_offset = self.world.random.next_float() * std::f32::consts::PI * 2.0;
        self.dropped.push(ItemEntity {
            health: 5,
            fire_ticks: 0,
            thrower: None,
            target: None,
            id: id as u32,
            stack,
            position: at,
            previous: at,
            velocity,
            on_ground: false,
            age: 0,
            pickup_delay: 0,
            bob_offset,
            ticks: 0,
        });
    }
    fn remote_motion(&mut self, id: i32, velocity: [f64; 3]) {
        self.remote_dragon_motion(id, velocity);
        let v = Vec3::new(velocity[0], velocity[1], velocity[2]);
        if let Some(m) = self.mobs.iter_mut().find(|m| m.id == id as u32) {
            m.velocity = v;
        } else if let Some(d) = self.dropped.iter_mut().find(|d| d.id == id as u32) {
            d.velocity = v;
        } else if let Some(f) = self.falling.iter_mut().find(|f| f.id == id as u32) {
            f.velocity = v;
        } else if let Some(t) = self.tnts.iter_mut().find(|t| t.id == id as u32) {
            t.velocity = v;
        } else if let Some(p) = self
            .creatures
            .projectiles
            .iter_mut()
            .find(|p| p.id == id as u32)
        {
            p.velocity = v;
        } else if let Some(b) = self.dragon_fireballs.iter_mut().find(|b| b.id == id as u32) {
            b.velocity = v;
        } else if let Some(t) = self.tridents.iter_mut().find(|t| t.id == id as u32) {
            t.velocity = v;
        } else if let Some(r) = self.rockets.iter_mut().find(|r| r.id == id as u32) {
            r.velocity = v;
        } else if self.remote_hook_motion(id as u32, v) {
        } else if let Some(p) = self
            .remote
            .as_mut()
            .and_then(|r| r.pending_items.get_mut(&id))
        {
            p.1 = v;
        } else if !self.potions.remote_thrown_motion(id as u32, v) {
            self.end.remote_eye_motion(id as u32, velocity);
        }
    }
    fn apply_remote_health(&mut self) {
        let Some((health, food, saturation)) = self.remote.as_ref().map(|r| r.health) else {
            return;
        };
        if let Some(p) = self.player.as_mut() {
            if (p.health.health() - health).abs() > f32::EPSILON {
                let _ = p.set_health(health.max(0.0), false);
            }
            p.health.food.food = food;
            p.health.food.saturation = saturation;
        }
    }
    fn place_ghast_under(&mut self, id: u32, spot: Vec3) {
        let Some(m) = self
            .mobs
            .iter_mut()
            .find(|m| m.id == id && m.species() == vise_client::mobs::Species::HappyGhast)
        else {
            return;
        };
        let at = Vec3::new(spot.x, spot.y - f64::from(m.height()), spot.z);
        if (at.x - m.position.x).powi(2) + (at.z - m.position.z).powi(2) > 25.0
            || (at.y - m.position.y).abs() > 5.0
        {
            return;
        }
        m.position = at;
        m.previous = at;
        if let Some(handler) = self
            .remote
            .as_mut()
            .and_then(|r| r.stepped.get_mut(&(id as i32)))
        {
            handler.cancel();
        }
    }
    fn remote_look_at(&mut self, p: &play::PlayerLookAt) {
        const EYES: i32 = 1;
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let mut from = [player.position.x, player.position.y, player.position.z];
        if p.from == EYES {
            from[1] += f64::from(player.dimensions.eye_height);
        }
        let to = p
            .entity
            .as_ref()
            .and_then(|e| {
                let m = self.mobs.iter().find(|m| m.id == e.id as u32)?;
                let y = if e.anchor == EYES {
                    m.eye_y()
                } else {
                    m.position.y
                };
                Some([m.position.x, y, m.position.z])
            })
            .unwrap_or(p.target);
        (self.yaw, self.pitch) = look_at_rotation(from, to);
    }
    fn remote_teleport(&mut self, p: &play::PlayerPosition) -> Result<()> {
        use play::relative as rel;
        let current = self
            .player
            .as_ref()
            .map(|pl| pl.position)
            .unwrap_or(Vec3::ZERO);
        let velocity = self
            .player
            .as_ref()
            .map(|pl| pl.velocity)
            .unwrap_or(Vec3::ZERO);
        let pick = |bit: i32, base: f64, v: f64| if p.relatives & bit != 0 { base + v } else { v };
        let position = Vec3::new(
            pick(rel::X, current.x, p.position[0]),
            pick(rel::Y, current.y, p.position[1]),
            pick(rel::Z, current.z, p.position[2]),
        );
        let yaw = if p.relatives & rel::Y_ROT != 0 {
            self.yaw + p.yaw
        } else {
            p.yaw
        };
        let relative_pitch = p.relatives & rel::X_ROT != 0;
        let pitch = position_packet_pitch(self.pitch, p.pitch, relative_pitch);
        let delta = vise_client::player_sync::teleport_velocity(
            velocity,
            (self.yaw, self.pitch),
            (yaw, pitch),
            p,
        );
        match self.player.as_mut() {
            Some(player) => {
                player.position = position;
                player.velocity = delta;
            }
            None => {
                let mut player = PlayerMotion::new(self.motion, position)?;
                player.spectator = self.spectator();
                player.velocity = delta;
                self.player = Some(player);
                self.pad.player_replaced();
                self.sway = Default::default();
                self.eye = 1.62;
                self.previous_eye = 1.62;
                self.clock.reset();
                self.apply_remote_health();
                self.apply_remote_abilities();
            }
        }
        self.previous_position = position;
        self.yaw = yaw;
        self.pitch = pitch;
        if let Some(m) = self.player_model.as_mut() {
            m.previous_head_pitch =
                position_packet_pitch(m.previous_head_pitch, p.pitch, relative_pitch);
            m.head_pitch = pitch;
        }
        if let Some(id) = self.remote.as_mut().and_then(|r| r.ghast_dismount.take()) {
            self.place_ghast_under(id, position);
        }
        if let Some(r) = self.remote.as_mut() {
            r.destroying = None;
        }
        self.mining = None;
        self.send(S::AcceptTeleportation(play::AcceptTeleportation {
            id: p.teleport,
            x: position.x,
            y: position.y,
            z: position.z,
            yaw,
            pitch,
        }));
        Ok(())
    }

    pub(super) fn tick_remote(&mut self) -> Result<()> {
        self.tick_remote_level()?;
        self.native_tick();
        self.tell_native_now();
        if self.remote.as_ref().is_some_and(|r| r.joined) {
            self.send(S::ClientTickEnd(Empty));
        }
        Ok(())
    }

    fn tick_remote_level(&mut self) -> Result<()> {
        self.ticks += 1;
        self.ui.chat.tick(&self.gui.atlas, self.ticks);
        if let Some(edit) = &self.sign_edit
            && !matches!(
                self.world.block_entities.get(&edit.pos),
                Some(BlockEntity::Sign(_))
            )
        {
            self.close_sign_editor();
        }
        self.track_experience_display();
        self.tick_bundle_hover();
        let info = self
            .settings
            .client_information(client_view_distance(self.settings.render_distance));
        if let Some(r) = self.remote.as_mut()
            && r.joined
            && r.sent_info != info
        {
            r.sent_info = info.clone();
            self.send(S::ClientInformation(info));
        }
        let running = self.clock.world_runs();
        self.boss_overlay.tick_darken(running);
        self.tick_item_activation();
        if running {
            self.world.game_time += 1;
            self.remote_clock_tick();
        }
        let w = &mut self.world.weather;
        w.rain_level.0 = w.rain_level.1;
        w.thunder_level.0 = w.thunder_level.1;
        if self.use_delay > 0 {
            self.use_delay -= 1;
        }
        self.remote_handle_input();
        self.cooldowns.tick();
        for pop in &mut self.hotbar_pop {
            *pop = pop.saturating_sub(1);
        }
        self.border.tick();
        self.player_border();
        self.tick_remote_entities(running);
        self.tick_remote_guardian_eyes(running);
        self.tick_block_entity_anims();
        self.tick_remote_spawners();
        self.client_tick_beacons();
        self.tick_remote_potent_sulfur();
        self.with_level(|b, level, ticks| {
            b.client_tick_block_entities(level, ticks);
        });
        let pushes = std::mem::take(&mut self.block_ticks.piston_pushes);
        self.push_piston_entities(pushes);
        self.sync_moving_pistons();
        self.tick_hand();
        self.tick_particles();
        self.tick_client_ambient();
        self.tick_client_effects();
        self.sleep_timer = super::tick_sleep_timer(self.sleep_timer, self.sleeping.is_some());
        self.tick_hud();
        self.tick_subtitles();
        let entity_collisions = self.player_entity_collisions();
        let pushed = self.remote_pushes();
        let sprint_window = self.settings.sprint_window;
        let Some(player) = self.player.as_mut() else {
            return Ok(());
        };
        let (cx, cz) = (
            (player.position.x.floor() as i32) >> 4,
            (player.position.z.floor() as i32) >> 4,
        );
        if !self.streaming.is_lit(cx, cz) {
            if let Some(r) = self.remote.as_ref()
                && r.loaded_sent
            {
                let input = r.last_input;
                self.remote_send_state(input);
            }
            return Ok(());
        }
        if self.remote.as_ref().is_some_and(|r| !r.loaded_sent) {
            self.send(S::PlayerLoaded(Empty));
            if let Some(r) = self.remote.as_mut() {
                r.loaded_sent = true;
            }
            self.capture_loaded_world();
        }
        let stick = self.pad_move();
        if self.player.is_none() {
            return Ok(());
        }
        let playing = self.screen == Screen::Playing;
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
        let input = u8::from(keys.forward)
            | u8::from(keys.backward) << 1
            | u8::from(keys.left) << 2
            | u8::from(keys.right) << 3
            | u8::from(keys.jump) << 4
            | u8::from(keys.shift) << 5
            | u8::from(keys.sprint) << 6;
        let keys = if self.controls_camera() {
            keys
        } else {
            Keys::default()
        };
        let riding = self.screen != Screen::Dead && self.remote_ride(keys);
        self.position_riders();
        let Some(player) = self.player.as_mut() else {
            return Ok(());
        };
        if !riding {
            self.previous_position = player.position;
        }
        let before = (
            player.ground.on_ground,
            [player.velocity.x, player.velocity.y, player.velocity.z],
            [player.position.x, player.position.y, player.position.z],
        );
        let was_flying = player.flying;
        self.sync_player_body();
        if self.screen == Screen::Dead {
            self.ui.death_time = self.ui.death_time.saturating_add(1);
        }
        let Some(player) = self.player.as_mut() else {
            return Ok(());
        };
        let was_gliding = player.fall_flying;
        let dead = self.screen == Screen::Dead;
        if let Some(head) = self.sleeping {
            player.position = Vec3::new(
                f64::from(head[0]) + 0.5,
                f64::from(head[1]) + 0.5625 + 0.125,
                f64::from(head[2]) + 0.5,
            );
            player.velocity = Vec3::ZERO;
        } else if !dead && !riding {
            player.client_side = true;
            player.velocity = player.velocity.add_vector(pushed);
            MotionWorld {
                entities: &entity_collisions,
                player,
                scene: &mut self.world,
                assets: &self.pack.assets,
                collision_binding: self.collision,
                resources: self.motion,
            }
            .controlled_simulation_tick_with_sprint_window(keys, sprint_window)?;
        } else {
            player.health.advance_clock();
            if riding && !dead {
                MotionWorld {
                    entities: &entity_collisions,
                    player: &mut *player,
                    scene: &mut self.world,
                    assets: &self.pack.assets,
                    collision_binding: self.collision,
                    resources: self.motion,
                }
                .passenger_pose_tick()?;
            }
        }
        let flying = player.flying;
        let started_gliding = player.fall_flying && !was_gliding;
        let events = std::mem::take(&mut player.events);
        self.previous_eye = self.eye;
        self.eye += (player.dimensions.eye_height - self.eye) * 0.5;
        let feet = [player.position.x, player.position.y, player.position.z];
        for event in events {
            match event {
                vise_client::motion::MotionEvent::Emission { state, muffled, .. } => {
                    let sounds = self.res.facts().get(state).sounds.clone();
                    let (volume, pitch) = if muffled {
                        (sounds.volume * 0.05, sounds.pitch * 0.8)
                    } else {
                        (sounds.volume * 0.15, sounds.pitch)
                    };
                    self.audio.play(&sounds.step, Some(feet), volume, pitch);
                }
                vise_client::motion::MotionEvent::Swim { volume, pitch } => {
                    self.audio
                        .play("minecraft:entity.player.swim", Some(feet), volume, pitch);
                }
                vise_client::motion::MotionEvent::Splash {
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
                vise_client::motion::MotionEvent::SplashParticle {
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
                vise_client::motion::MotionEvent::Chime { volume, pitch } => {
                    self.audio.play(
                        "minecraft:block.amethyst_block.chime",
                        Some(feet),
                        volume,
                        pitch,
                    );
                }
                vise_client::motion::MotionEvent::InsidePortal { .. } => {
                    self.client_inside_nether_portal();
                }
                vise_client::motion::MotionEvent::HoneySlide { sound: true, .. } => {
                    self.audio
                        .play("minecraft:block.honey_block.slide", Some(feet), 1.0, 1.0);
                }
                vise_client::motion::MotionEvent::Fall(
                    vise_client::landing::FallEvent::Sound {
                        sound,
                        volume,
                        pitch,
                        ..
                    },
                ) => {
                    if let Some(name) = sound.player_fall_sound() {
                        self.audio.play(name, Some(feet), volume, pitch);
                    }
                }
                _ => {}
            }
        }
        self.apply_remote_health();
        self.tick_avatar(before, false);
        self.tick_player_model();
        if !dead {
            self.remote_tick_using();
        }
        if flying != was_flying {
            self.send(S::PlayerAbilities(play::PlayerAbilitiesRequest {
                flags: if flying { 2 } else { 0 },
            }));
        }
        if started_gliding {
            let id = self.remote.as_ref().map_or(0, |r| r.entity_id);
            self.send(S::PlayerCommand(play::PlayerCommand {
                id,
                action: 6,
                data: 0,
            }));
        }
        self.remote_send_state(input);
        Ok(())
    }

    fn apply_remote_abilities(&mut self) {
        let (Some(player), Some(a)) = (
            self.player.as_mut(),
            self.remote.as_ref().and_then(|r| r.abilities.as_ref()),
        ) else {
            return;
        };
        player.invulnerable = a.flags & 1 != 0;
        player.flying = a.flags & 2 != 0;
        player.may_fly = a.flags & 4 != 0;
        player.flying_speed = a.flying_speed;
    }
    fn remote_pushes(&self) -> Vec3 {
        let (Some(p), Some(r)) = (self.player.as_ref(), self.remote.as_ref()) else {
            return Vec3::ZERO;
        };
        let feet = [
            p.position.x.floor() as i32,
            p.position.y.floor() as i32,
            p.position.z.floor() as i32,
        ];
        if self.screen == Screen::Dead
            || p.spectator
            || self.block_in_tag("minecraft:climbable", &self.block_name(feet))
        {
            return Vec3::ZERO;
        }
        let bounds = p.bounds();
        let teams = self.mob_teams();
        let board = &self.score.board;
        let player_team = board.players_team(&r.profile_name);
        let mut push = Vec3::ZERO;
        for m in &self.mobs {
            let listed = r
                .level_players
                .get(&(m.id as i32))
                .and_then(|uuid| r.tab_players.get(uuid));
            let spectator = listed.is_some_and(|t| t.game_mode == SPECTATOR);
            let mob_team = match listed {
                Some(t) => board.players_team(&t.name),
                None => teams.of(Who::Mob(m.id)),
            };
            if !m.alive()
                || spectator
                || m.species() == vise_client::mobs::Species::Bat
                || self
                    .riding
                    .is_some_and(|v| v == m.id || m.extra.mount == Some(v))
                || !m.bounds().intersects(bounds)
                || !vise_client::scoreboard::pushable_by(mob_team, player_team, true)
            {
                continue;
            }
            let (mut xa, mut za) = (m.position.x - p.position.x, m.position.z - p.position.z);
            let mut dd = xa.abs().max(za.abs());
            if dd < f64::from(0.01_f32) {
                continue;
            }
            dd = dd.sqrt();
            xa /= dd;
            za /= dd;
            let pow = (1.0 / dd).min(1.0);
            xa *= pow;
            za *= pow;
            xa *= f64::from(0.05_f32);
            za *= f64::from(0.05_f32);
            push = Vec3::new(push.x - xa, push.y, push.z - za);
        }
        let area = Vec3::new(f64::from(0.2_f32), f64::from(-0.01_f32), f64::from(0.2_f32));
        for b in self.boats.iter().filter(|b| self.riding != Some(b.id)) {
            let bb = b.bounds();
            let times = usize::from(
                bounds.intersects(bb.inflate(area))
                    && vise_client::scoreboard::pushable_by(None, player_team, true),
            ) + usize::from(
                bounds.intersects(bb)
                    && vise_client::scoreboard::pushable_by(player_team, None, true),
            );
            if times > 0
                && super::boat_passengers::boat_pushes(bb, bounds, false)
                && let Some(v) = super::boat_passengers::push_impulse(b.position, p.position)
            {
                push = push.add_vector(v.scale(times as f64));
            }
        }
        push
    }
    fn tick_remote_guardian_eyes(&mut self, running: bool) {
        use vise_client::mobs::{Species, Who, guardian};
        if !running {
            return;
        }
        let camera = self.player.as_ref().map(|p| {
            [
                p.position.x,
                p.position.y + f64::from(p.dimensions.eye_height),
                p.position.z,
            ]
        });
        let looks: Vec<_> = self
            .mobs
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                matches!(m.species(), Species::Guardian | Species::ElderGuardian) && m.alive()
            })
            .filter_map(|(i, m)| {
                let look = match m.ai.target.filter(|_| m.extra.flag) {
                    Some(Who::Mob(id)) => self
                        .mobs
                        .iter()
                        .find(|t| t.id == id)
                        .map(|t| [t.position.x, t.eye_y(), t.position.z]),
                    _ => camera,
                };
                look.map(|look| (i, look))
            })
            .collect();
        for (i, look) in looks {
            guardian::look_at(&mut self.mobs[i], look);
        }
    }
    fn tick_remote_entities(&mut self, running: bool) {
        if running {
            self.tick_text_displays();
            self.tick_remote_ominous_item_spawners();
        }
        let (fluids, rain, thundering): (Vec<_>, Vec<_>, bool) = {
            let w = self.mob_world_base(&[]);
            let players = self.remote.as_ref().map(|r| &r.level_players);
            let fluids = self
                .mobs
                .iter()
                .map(|m| {
                    (running || players.is_some_and(|p| p.contains_key(&(m.id as i32))))
                        .then(|| w.fluids(m.bounds(), m.eye_y()))
                })
                .collect();
            let rain = self
                .mobs
                .iter()
                .map(|m| {
                    m.species() == vise_client::mobs::Species::Wolf
                        && vise_client::mobs::tame::in_rain(m, &w)
                })
                .collect();
            (fluids, rain, w.thundering)
        };
        for (m, fluids) in self.mobs.iter_mut().zip(fluids) {
            if let Some(fluids) = fluids {
                m.fluids = fluids;
            }
        }
        let game_time = self.world.game_time;
        let nether = self.dim == super::Dim::Nether;
        let wither_targets: Vec<(i32, [f64; 3], f64)> = if self
            .mobs
            .iter()
            .any(|m| m.species() == vise_client::mobs::Species::Wither)
        {
            let own = self
                .remote
                .as_ref()
                .zip(self.eye_position())
                .map(|(r, eye)| (r.entity_id, eye, eye[1]));
            self.mobs
                .iter()
                .map(|m| {
                    (
                        m.id as i32,
                        [m.position.x, m.position.y, m.position.z],
                        m.eye_y(),
                    )
                })
                .chain(own)
                .collect()
        } else {
            Vec::new()
        };
        let driven = self.driven_mount().map(|m| m.id);
        let sleepers: Vec<vise_core::geometry::Aabb> = self
            .remote
            .as_ref()
            .map(|r| {
                self.mobs
                    .iter()
                    .filter(|m| {
                        r.level_players.contains_key(&(m.id as i32)) && m.extra.ints[1] != 0
                    })
                    .map(|m| {
                        let p = m.position;
                        vise_core::geometry::Aabb::new(
                            vise_core::geometry::Vec3::new(p.x - 0.1, p.y, p.z - 0.1),
                            vise_core::geometry::Vec3::new(p.x + 0.1, p.y + 0.2, p.z + 0.1),
                        )
                    })
                    .chain(self.sleeping.map(|bed| {
                        let p = vise_core::geometry::Vec3::new(
                            f64::from(bed[0]) + 0.5,
                            f64::from(bed[1]) + 0.6875,
                            f64::from(bed[2]) + 0.5,
                        );
                        vise_core::geometry::Aabb::new(
                            vise_core::geometry::Vec3::new(p.x - 0.1, p.y, p.z - 0.1),
                            vise_core::geometry::Vec3::new(p.x + 0.1, p.y + 0.2, p.z + 0.1),
                        )
                    }))
                    .collect()
            })
            .unwrap_or_default();
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let mut lerps = std::mem::take(&mut r.lerps);
        let stepped = &mut r.stepped;
        let head_lerps = &mut r.head_lerps;
        let players = &r.level_players;
        let mut using = Vec::new();
        let mut mob_fx = Vec::new();
        for (m, rain) in self.mobs.iter_mut().zip(rain) {
            let player = players.contains_key(&(m.id as i32));
            if (!running && !player) || driven == Some(m.id) {
                continue;
            }
            m.previous = m.position;
            m.previous_yaw = m.yaw;
            m.previous_body_yaw = m.body_yaw;
            m.previous_head_yaw = m.head_yaw;
            m.previous_head_pitch = m.head_pitch;
            m.previous_limb_speed = m.limb_speed;
            m.age += 1;
            if m.species() == vise_client::mobs::Species::Rabbit {
                vise_client::mobs::rabbit::base_tick(m);
            }
            if matches!(
                m.species(),
                vise_client::mobs::Species::Horse
                    | vise_client::mobs::Species::Donkey
                    | vise_client::mobs::Species::Mule
                    | vise_client::mobs::Species::SkeletonHorse
                    | vise_client::mobs::Species::ZombieHorse
            ) {
                vise_client::mobs::equine::tail_tick(m);
            }
            if m.species() == vise_client::mobs::Species::Creeper {
                vise_client::mobs::hostile::creeper_client_tick(m);
            }
            if matches!(
                m.species(),
                vise_client::mobs::Species::Guardian
                    | vise_client::mobs::Species::ElderGuardian
            ) {
                vise_client::mobs::guardian::client_tick(m);
            }
            if m.species() == vise_client::mobs::Species::PolarBear {
                vise_client::mobs::polar_bear::client_tick(m);
            }
            {
                use vise_client::mobs::{
                    Species as S, cube, illusioner, nautilus, tame, wither,
                };
                match m.species() {
                    S::Wolf => tame::client_tick(m, rain, &mut mob_fx),
                    S::Slime | S::MagmaCube | S::SulfurCube => cube::client_tick(m, &mut mob_fx),
                    S::Nautilus | S::ZombieNautilus => nautilus::client_tick(m, &mut mob_fx),
                    S::Illusioner => illusioner::client_tick(m, &mut mob_fx),
                    S::Wither => wither::client_smoke(m, &mut mob_fx),
                    _ => {}
                }
            }
            if m.hurt_time > 0 {
                m.hurt_time -= 1;
            }
            if !m.alive() {
                m.death_time += 1;
            }
            m.anim.attack_anim_o = m.anim.attack_anim;
            vise_client::mobs::ai::tick_swing(m);
            if let Some(at) = stepped
                .get_mut(&(m.id as i32))
                .and_then(|h| h.interpolate())
            {
                place_interpolated(m, at);
            }
            if let Some((yaw, steps)) = head_lerps.get_mut(&(m.id as i32))
                && *steps > 0
            {
                m.head_yaw += wrap_degrees(*yaw - m.head_yaw) / *steps as f32;
                *steps -= 1;
            }
            if m.species() == vise_client::mobs::Species::Fox {
                vise_client::mobs::fox::client_tick(m);
            }
            if m.species() == vise_client::mobs::Species::Cat {
                m.anim.vo = m.anim.v;
                vise_client::mobs::tame::cat_amounts(m);
                vise_client::mobs::tame::cat_lying_on_sleeper(m, &sleepers);
            }
            if m.species() == vise_client::mobs::Species::Chicken {
                vise_client::mobs::farm::chicken_flap(m);
            }
            if m.species() == vise_client::mobs::Species::Parrot {
                vise_client::mobs::parrot::flap(m);
            }
            if player {
                let used = if m.extra.using_off_hand {
                    &m.extra.off_hand
                } else {
                    &m.extra.main_hand
                };
                m.extra.blocking = m.extra.using_item > 0
                    && used
                        .as_ref()
                        .and_then(|i| world_items!(self).get(i))
                        .is_some_and(|i| i.blocks_attacks.is_some());
            }
            if m.extra.using_item > 0 {
                if player {
                    let item = if m.extra.using_off_hand {
                        m.extra.off_hand.clone()
                    } else {
                        m.extra.main_hand.clone()
                    };
                    if let Some(item) = item {
                        let eye = m.eye_height();
                        let at = [m.position.x, m.position.y, m.position.z];
                        using.push((
                            item,
                            m.extra.using_item as u32 - 1,
                            at,
                            eye,
                            m.head_pitch,
                            m.yaw,
                        ));
                    }
                }
                m.extra.using_item += 1;
            }
            if player {
                remote_player_head_turn(m);
                m.tick_avatar_pose(m.fluids.in_water);
            } else if m.alive() {
                m.body_rotation_tick();
            }
            vise_client::mobs::aquatic::squid_client_step(m, &mut mob_fx);
            let (dx, dz) = (m.position.x - m.previous.x, m.position.z - m.previous.z);
            let (target, factor) = if vise_client::mobs::camel::is_camel(m.species()) {
                vise_client::mobs::camel::walk_target(m, (dx * dx + dz * dz).sqrt() as f32)
            } else {
                ((((dx * dx + dz * dz).sqrt() * 4.0) as f32).min(1.0), 0.4)
            };
            m.limb_speed += (target - m.limb_speed) * factor;
            m.limb_position += m.limb_speed;
            if m.species() == vise_client::mobs::Species::Rabbit {
                vise_client::mobs::rabbit::post_tick(m);
            }
            if m.species() == vise_client::mobs::Species::Bat {
                vise_client::mobs::bat::setup_animation_states(m);
            }
            vise_client::mobs::sync::client_tick(m, game_time);
            match m.species() {
                vise_client::mobs::Species::Wither => {
                    vise_client::mobs::wither::client_tick(m, |id| {
                        wither_targets
                            .iter()
                            .find(|t| t.0 == id)
                            .map(|t| (t.1, t.2))
                    });
                }
                vise_client::mobs::Species::Hoglin
                | vise_client::mobs::Species::Piglin
                | vise_client::mobs::Species::PiglinBrute => {
                    vise_client::mobs::piglin::client_converting(m, nether);
                }
                vise_client::mobs::Species::Panda => {
                    vise_client::mobs::panda::client_tick(m, thundering);
                }
                _ => {}
            }
        }
        for o in self.orbs.iter_mut().filter(|_| running) {
            o.previous = o.position;
            o.age += 1;
            if let Some(l) = lerps.get_mut(&(o.id as i32))
                && l.steps > 0
            {
                let n = f64::from(l.steps);
                o.position = Vec3::new(
                    o.position.x + (l.target[0] - o.position.x) / n,
                    o.position.y + (l.target[1] - o.position.y) / n,
                    o.position.z + (l.target[2] - o.position.z) / n,
                );
                l.steps -= 1;
            }
        }
        let (world, facts, collision) = (&self.world, self.res.facts(), self.collision);
        for d in self.dropped.iter_mut().filter(|_| running) {
            d.tick(world, facts, collision, false, &self.res.motion().fluids);
        }
        head_lerps.retain(|_, (_, steps)| *steps > 0);
        let _ = self.handle_mob_events(mob_fx);
        for (item, ticks, at, eye, pitch, yaw) in using {
            self.remote_player_eating(&item, ticks, at, eye, pitch, yaw);
        }
        if running {
            self.guardian_bubbles();
            let dragon_lerp = self
                .dragon
                .as_ref()
                .and_then(|d| lerps.get_mut(&(d.id as i32)))
                .filter(|l| l.steps > 0)
                .map(|l| {
                    let steps = l.steps;
                    l.steps -= 1;
                    (l.target, l.rotation.map(|r| r.0), steps)
                });
            self.tick_remote_dragon(dragon_lerp);
            let mut step = |id: u32, at: Vec3| match lerps.get_mut(&(id as i32)) {
                Some(l) if l.steps > 0 => {
                    let n = f64::from(l.steps);
                    l.steps -= 1;
                    Vec3::new(
                        at.x + (l.target[0] - at.x) / n,
                        at.y + (l.target[1] - at.y) / n,
                        at.z + (l.target[2] - at.z) / n,
                    )
                }
                _ => at,
            };
            self.tick_remote_objects(&mut step);
            self.tick_remote_projectiles(&mut step);
            self.flash = self.flash.saturating_sub(1);
            self.tick_bolts();
        }
        lerps.retain(|_, l| l.steps > 0);
        if let Some(r) = self.remote.as_mut() {
            for (id, l) in lerps {
                r.lerps.entry(id).or_insert(l);
            }
        }
    }
    fn remote_player_eating(
        &mut self,
        item: &str,
        ticks: u32,
        at: [f64; 3],
        eye: f64,
        pitch: f32,
        yaw: f32,
    ) {
        let Some(consumable) = self
            .res
            .items()
            .get(item)
            .and_then(|i| i.consumable.as_ref())
            .filter(|c| c.particles)
        else {
            return;
        };
        let duration = consumable.ticks().max(0) as u32;
        let remaining = duration - ticks.min(duration);
        if !consumable.should_emit(remaining as i32) {
            return;
        }
        let options = self.item_particle(&ItemStack::new(item, 1));
        let mut spawned = Vec::new();
        vise_client::particles::item_use_particles(
            &mut self.world.random,
            pitch,
            yaw,
            at,
            at[1] + eye,
            5,
            |p, v| spawned.push((p, v)),
        );
        for (p, v) in spawned {
            self.particles.add(&options, p, v);
        }
    }
    fn remote_send_state(&mut self, input: u8) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let (pos, on_ground, collision, sprinting) = (
            [p.position.x, p.position.y, p.position.z],
            p.ground.on_ground,
            p.collision.horizontal,
            p.sprinting,
        );
        let (yaw, pitch, selected) = (self.yaw, self.pitch, self.selected);
        let riding = self.riding.is_some();
        let controls_camera = self.controls_camera();
        let vehicle = self.remote_vehicle_packets();
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let mut out = Vec::new();
        if r.carried != Some(selected) {
            r.carried = Some(selected);
            out.push(S::SetCarriedItem(play::SetCarriedItem {
                slot: selected as i16,
            }));
        }
        if input != r.last_input {
            r.last_input = input;
            out.push(S::PlayerInput(play::PlayerInput { flags: input }));
        }
        if sprinting != r.was_sprinting {
            r.was_sprinting = sprinting;
            out.push(S::PlayerCommand(play::PlayerCommand {
                id: r.entity_id,
                action: if sprinting { 1 } else { 2 },
                data: 0,
            }));
        }
        if riding {
            out.push(S::MovePlayerRot(play::MovePlayerRot {
                yaw,
                pitch,
                flags: u8::from(on_ground) | u8::from(collision) << 1,
            }));
            out.extend(vehicle);
        } else if controls_camera
            && let Some(packet) =
                r.position_sender
                    .update(pos, (yaw, pitch), on_ground, collision)
        {
            out.push(packet);
        }
        let mut creative_slots = Vec::new();
        if self.creative {
            let in_menu = r.menu.is_some();
            let stacks = creative_sync_stacks(&self.inventory, &self.offhand, &self.armor);
            for (i, stack) in stacks.into_iter().enumerate() {
                if stack != r.synced[i] {
                    r.synced[i] = stack.clone();
                    if !in_menu {
                        creative_slots.push((creative_sync_slot(i), stack));
                    }
                }
            }
        }
        for packet in out {
            self.send(packet);
        }
        for (slot, stack) in creative_slots {
            let item = self.to_network(&stack);
            self.send(S::SetCreativeModeSlot(play::SetCreativeModeSlot {
                slot,
                item,
            }));
        }
    }
    fn creative_stacks(&self) -> Vec<Option<ItemStack>> {
        creative_sync_stacks(&self.inventory, &self.offhand, &self.armor)
    }
    fn remote_synced(&mut self) {
        let stacks = self.creative_stacks();
        if let Some(r) = self.remote.as_mut() {
            r.synced = stacks;
        }
    }
    fn remote_synced_since(&mut self, before: &[Option<ItemStack>]) {
        let now = self.creative_stacks();
        if let Some(r) = self.remote.as_mut() {
            mark_synced(&mut r.synced, before, now);
        }
    }

    pub(super) fn stop_remote_mining(&mut self) {
        if let Some((pos, _, _)) = self.remote.as_mut().and_then(|r| r.destroying.take()) {
            let state = self.world.block(pos[0], pos[1], pos[2]);
            self.tutorial_destroy(state, -1.0);
            self.send(S::PlayerAction(play::PlayerAction {
                action: 2,
                pos,
                direction: 0,
                sequence: 0,
            }));
            self.reset_attack_strength();
        }
    }

    fn remote_mining(&mut self, start: bool) {
        let debug_stick = self
            .held()
            .is_some_and(|stack| &*stack.item == "minecraft:debug_stick");
        if !self
            .res
            .items()
            .can_destroy_block(self.held(), self.creative)
        {
            self.stop_remote_mining();
            self.mining = None;
            return;
        }
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        if !start && r.destroy_delay > 0 {
            r.destroy_delay -= 1;
            return;
        }
        let Some(hit) = self.pick() else {
            self.mining = None;
            return;
        };
        let face = face_direction(hit.face);
        let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        if self.res.facts().is_air(state) {
            return;
        }
        if self.remote_break_restricted() {
            self.mining = None;
            return;
        }
        if self.creative {
            let seq = self.next_sequence();
            if !debug_stick {
                self.predict_break(hit.pos, state, seq);
            }
            self.send(S::PlayerAction(play::PlayerAction {
                action: 0,
                pos: hit.pos,
                direction: face,
                sequence: seq,
            }));
            if let Some(r) = self.remote.as_mut() {
                r.destroy_delay = 5;
            }
            return;
        }
        let facts = self.res.facts().get(state).clone();
        let progress = if facts.hardness == 0.0 {
            1.0
        } else {
            self.destroy_progress(hit.pos)
        };
        let held = self.main_hand_key();
        let same = self.remote.as_ref().is_some_and(|r| {
            r.destroying.is_some_and(|(p, _, _)| p == hit.pos) && r.destroying_item == held
        });
        if start && same {
            return;
        }
        if !same {
            if let Some((old, _, _)) = self.remote.as_mut().and_then(|r| r.destroying.take()) {
                let old_state = self.world.block(old[0], old[1], old[2]);
                self.tutorial_destroy(old_state, -1.0);
                self.send(S::PlayerAction(play::PlayerAction {
                    action: 2,
                    pos: old,
                    direction: face,
                    sequence: 0,
                }));
            }
            let seq = self.next_sequence();
            self.send(S::PlayerAction(play::PlayerAction {
                action: 0,
                pos: hit.pos,
                direction: face,
                sequence: seq,
            }));
            self.tutorial_destroy(state, 0.0);
            self.redstone_ore_particles(hit.pos);
            if progress >= 1.0 {
                if !debug_stick {
                    self.predict_break(hit.pos, state, seq);
                }
                return;
            }
            if let Some(r) = self.remote.as_mut() {
                r.destroying = Some((hit.pos, face, 0.0));
                r.destroying_item = held;
            }
            self.breaking_block_effects(hit.pos, hit.face, true);
            self.mining = Some(Mining {
                pos: hit.pos,
                progress: 0.0,
                ticks: 0,
            });
            return;
        }
        let Some((pos, last_face, done)) = self.remote.as_ref().and_then(|r| r.destroying) else {
            return;
        };
        let done = done + progress;
        self.tutorial_destroy(state, done.clamp(0.0, 1.0));
        let ticks = self.mining.as_ref().map_or(0, |m| m.ticks) + 1;
        if done >= 1.0 {
            let seq = self.next_sequence();
            if !debug_stick {
                self.predict_break(pos, state, seq);
            }
            self.send(S::PlayerAction(play::PlayerAction {
                action: 3,
                pos,
                direction: face,
                sequence: seq,
            }));
            if let Some(r) = self.remote.as_mut() {
                r.destroying = None;
                r.destroy_delay = 5;
            }
            self.mining = None;
        } else {
            if last_face != face {
                self.send(S::PlayerAction(play::PlayerAction {
                    action: 1,
                    pos,
                    direction: face,
                    sequence: 0,
                }));
            }
            if let Some(r) = self.remote.as_mut() {
                r.destroying = Some((pos, face, done));
            }
            self.mining = Some(Mining {
                pos,
                progress: done,
                ticks,
            });
        }
    }
    fn predict_break(&mut self, pos: [i32; 3], state: BlockStateId, sequence: i32) {
        self.tutorial_destroy(state, 1.0);
        if let Some(r) = self.remote.as_mut() {
            r.predictions
                .entry(pos)
                .and_modify(|p| p.0 = sequence)
                .or_insert((sequence, state.0 as i32));
        }
        let facts = self.res.facts().get(state);
        if !facts.has_class("BaseFireBlock") {
            let s = facts.sounds.clone();
            let at = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 0.5,
                f64::from(pos[2]) + 0.5,
            ];
            self.audio.play(
                &s.break_sound,
                Some(at),
                (s.volume + 1.0) / 2.0,
                s.pitch * 0.8,
            );
            self.destroy_particles(pos, state);
        }
        let air = self.world.air;
        if self.world.set(pos[0], pos[1], pos[2], air).is_ok() {
            self.world.block_entities.remove(&(pos[0], pos[1], pos[2]));
        }
        self.mining = None;
    }
    fn remote_break_restricted(&self) -> bool {
        use vise_protocol::item::{WirePatch, component::CAN_BREAK};
        self.spectator()
            || (!self.may_build()
                && !self
                    .held()
                    .and_then(|s| s.wire_patch())
                    .is_some_and(|p| WirePatch::adds(&p, CAN_BREAK)))
    }
    fn remote_use(&mut self) {
        if self.remote.as_ref().is_some_and(|r| r.destroying.is_some()) {
            return;
        }
        self.use_delay = 4;
        let entity = self.entity_targeted();
        vise_client::interaction_input::use_on_target(&mut RemoteUseWorld(self), entity);
    }

    fn remote_use_entity(&mut self, hand: Hand) -> bool {
        let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
        if let Some(hit) = self.remote_object_hit(false)
            && self.mob_hit().is_none_or(|(_, d)| hit.distance < d)
            && self.before_block(hit.distance)
        {
            let passes = self
                .carts
                .iter()
                .find(|c| c.id as i32 == hit.id)
                .is_some_and(|c| {
                    let ridden = self.riding == Some(c.id)
                        || self.mobs.iter().any(|m| m.extra.mount == Some(c.id));
                    c.kind
                        .interact_passes(sneaking, ridden, self.can_use_game_master_blocks())
                });
            if hand == Hand::Off && !passes {
                return false;
            }
            self.send(S::Interact(play::Interact {
                id: hit.id,
                hand: hand_id(hand),
                location: hit.location,
                secondary: sneaking,
            }));
            if passes {
                return false;
            }
            self.swing_arm();
            return true;
        }
        if let Some((i, distance)) = self.mob_hit()
            && self.before_block(distance)
        {
            let id = self.mobs[i].id as i32;
            self.send(S::Interact(play::Interact {
                id,
                hand: hand_id(hand),
                location: [0.0, 1.0, 0.0],
                secondary: sneaking,
            }));
            let player = self
                .remote
                .as_ref()
                .is_some_and(|r| r.level_players.contains_key(&id));
            if !player && self.remote_mob_takes(i, hand) {
                self.remote_feed_particles(i, hand);
                self.with_hand(hand, Game::swing_arm);
                return true;
            }
        }
        false
    }

    fn remote_use_block(&mut self, hand: Hand) -> bool {
        let before = self.with_hand(hand, |g| g.held().map(|s| s.count));
        let swings = self.swings_started;
        let done = self.remote_use_item_on(hand);
        if done
            && self.swings_started != swings
            && let Some(after) = self.with_hand(hand, |g| g.held().map(|s| s.count))
            && (Some(after) != before || self.creative)
        {
            self.heights.item_used(hand);
        }
        done
    }

    fn remote_use_item_on(&mut self, hand: Hand) -> bool {
        let held = self.with_hand(hand, |g| g.held().cloned());
        let held_block = held
            .as_ref()
            .and_then(|h| self.items().get(&h.item))
            .is_some_and(|i| i.block.is_some());
        let debug_stick = held
            .as_ref()
            .is_some_and(|stack| &*stack.item == "minecraft:debug_stick");
        if let Some(hit) = self.pick() {
            let shelf_reacts = hand == Hand::Main
                && held.is_some()
                && !(self.keys.contains(&KeyCode::ShiftLeft)
                    && (self.stack_in(self.selected).is_some() || self.offhand.is_some()))
                && self.shelf_hit(&hit).is_some();
            let spectator = self.spectator();
            let (result, changes) = if hand == Hand::Main && !spectator {
                self.remote_block_reacts(&hit, held.as_ref())
            } else {
                let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
                let hands_full = self.stack_in(self.selected).is_some() || self.offhand.is_some();
                let charges =
                    !(sneaking && hands_full) && self.anchor_takes(hit.pos, held.as_ref());
                let result = if charges {
                    InteractionResult::Success
                } else {
                    InteractionResult::Pass
                };
                (result, DryUse::default())
            };
            let (result, changes) = if debug_stick && !spectator && !result.consumes_action() {
                (InteractionResult::Success, DryUse::default())
            } else {
                (result, changes)
            };
            let reacts = result.consumes_action();
            if hand == Hand::Main && !spectator {
                let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
                let hands_full = self.stack_in(self.selected).is_some() || self.offhand.is_some();
                if !(sneaking && hands_full) {
                    self.redstone_ore_particles(hit.pos);
                    let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
                    let lit_candle = self.block_name(hit.pos).ends_with("candle")
                        && self.pack.assets.registry.state(state).is_some_and(|s| {
                            s.properties.get("lit").map(String::as_str) == Some("true")
                        });
                    if held.is_none() && self.may_build() && lit_candle {
                        self.candle_smoke(hit.pos, state);
                    }
                }
            }
            let stand_fits = held
                .as_ref()
                .is_some_and(|s| &*s.item == "minecraft:armor_stand")
                && held
                    .as_ref()
                    .is_some_and(|s| self.armor_stand_spawn(&hit, s).is_some());
            let seq = self.next_sequence();
            let cursor = [
                (hit.point[0] - f64::from(hit.pos[0])) as f32,
                (hit.point[1] - f64::from(hit.pos[1])) as f32,
                (hit.point[2] - f64::from(hit.pos[2])) as f32,
            ];
            self.send(S::UseItemOn(play::UseItemOn {
                hand: hand_id(hand),
                pos: hit.pos,
                face: i32::from(face_direction(hit.face)),
                cursor,
                inside: hit.face == [0; 3],
                world_border: false,
                sequence: seq,
            }));
            if spectator {
                return true;
            }
            if let Some(r) = self.remote.as_mut() {
                r.use_target = Some(hit.pos);
            }
            if shelf_reacts {
                return true;
            }
            if reacts {
                self.predict_use(&changes, seq);
                if &*self.block_name(hit.pos) == "minecraft:lever" {
                    self.lever_puff(hit.pos);
                }
                self.open_command_block_if_editable((hit.pos[0], hit.pos[1], hit.pos[2]));
                if result == InteractionResult::Success {
                    self.with_hand(hand, Game::swing_arm);
                }
            } else if self.predict_campfire_douse(hit.pos, held.as_ref(), seq) {
                self.with_hand(hand, Game::swing_arm);
                return true;
            } else if held.as_ref().is_some_and(|s| &*s.item == "minecraft:brush") {
                self.with_hand(hand, |g| {
                    if g.using.is_none() {
                        g.using = Some((g.held_slot(), 0));
                    }
                });
                return true;
            } else if held
                .as_ref()
                .is_some_and(|s| &*s.item == "minecraft:bone_meal")
                && self.remote_bone_meal_succeeds(&hit)
            {
                self.with_hand(hand, Game::swing_arm);
                return true;
            } else if stand_fits {
                self.with_hand(hand, Game::swing_arm);
                return true;
            } else if let Some(stack) = held.as_ref()
                && self.predict_item_on_block(stack, &hit)
            {
                self.with_hand(hand, Game::swing_arm);
                return true;
            }
            if held_block {
                let placed = if reacts {
                    None
                } else {
                    held.as_ref()
                        .and_then(|stack| self.remote_can_place(&hit, stack))
                };
                if let Some(placed) = &placed {
                    self.predict_place(placed, seq);
                    self.with_hand(hand, Game::consume_held);
                } else if let Some(r) = self.remote.as_mut() {
                    r.placing = None;
                }
                if placed.is_some() {
                    self.with_hand(hand, Game::swing_arm);
                }
                return true;
            }
            return reacts;
        }
        false
    }

    fn predict_item_on_block(&mut self, stack: &ItemStack, hit: &super::Hit) -> bool {
        let at = [
            f64::from(hit.pos[0]) + 0.5,
            f64::from(hit.pos[1]) + 0.5,
            f64::from(hit.pos[2]) + 0.5,
        ];
        if &*stack.item == "minecraft:honeycomb" {
            let registry = self.pack.assets.registry;
            let clicked = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
            let waxes = registry
                .state(clicked)
                .and_then(|old| self.res.transformers().waxed(registry, old))
                .is_some();
            if waxes {
                self.level_event_at(3003, hit.pos, 0);
                self.audio
                    .play("minecraft:item.honeycomb.wax_on", Some(at), 1.0, 1.0);
            }
            waxes
        } else if let Some((_, sound, _, _, particle)) =
            self.block_transform(stack, hit.pos, hit.face)
        {
            self.transform_particle(&particle, hit.pos);
            self.audio.play(&sound, Some(at), 1.0, 1.0);
            true
        } else if self
            .res
            .items()
            .get(&stack.item)
            .is_some_and(|i| i.cushion_color.is_some())
        {
            self.cushion_spawn(hit).is_some()
        } else {
            false
        }
    }

    fn remote_bone_meal_succeeds(&mut self, hit: &super::Hit) -> bool {
        let pos = vise_core::position::BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
        let face = vise_core::geometry::Direction::ALL[face_direction(hit.face) as usize];
        self.with_level(|b, level, _| {
            b.bone_meal_succeeds_on_client(&mut DryLevel::new(level), pos, face)
        })
    }

    fn predict_use(&mut self, (changes, sounds): &DryUse, seq: i32) {
        let facts = self.res.facts();
        let predicted = |state: BlockStateId| hand_operated(&facts.get(state).name);
        if changes.is_empty()
            || !changes
                .iter()
                .all(|(p, s)| predicted(*s) && predicted(self.world.block(p.x, p.y, p.z)))
        {
            return;
        }
        self.predict_blocks(changes, seq);
        for (pos, sound, volume, pitch) in sounds {
            let at = [
                f64::from(pos.x) + 0.5,
                f64::from(pos.y) + 0.5,
                f64::from(pos.z) + 0.5,
            ];
            let name = if sound.contains(':') {
                sound.to_string()
            } else {
                format!("minecraft:{sound}")
            };
            self.audio.play(&name, Some(at), *volume, *pitch);
        }
    }

    fn remote_block_reacts(
        &mut self,
        hit: &super::Hit,
        held: Option<&ItemStack>,
    ) -> (InteractionResult, DryUse) {
        let hands_full = self.stack_in(self.selected).is_some() || self.offhand.is_some();
        if self.keys.contains(&KeyCode::ShiftLeft) && hands_full {
            return (InteractionResult::Pass, DryUse::default());
        }
        if self.jukebox_takes(hit.pos, held) {
            return (InteractionResult::Success, DryUse::default());
        }
        if !self.anchor_takes(hit.pos, held) && self.anchor_takes(hit.pos, self.offhand.as_ref()) {
            return (InteractionResult::Pass, DryUse::default());
        }
        if held.is_some_and(|s| matches!(&*s.item, "minecraft:shears" | "minecraft:glass_bottle"))
            && self
                .hive_state(hit.pos)
                .is_some_and(|(honey, _)| honey == Some(5))
        {
            return (InteractionResult::Success, DryUse::default());
        }
        if self.composter_takes(hit.pos, held) {
            return (InteractionResult::Success, DryUse::default());
        }
        use vise_core::geometry::Direction;
        let face = match hit.face {
            [0, 1, 0] => Direction::Up,
            [0, -1, 0] => Direction::Down,
            [0, 0, -1] => Direction::North,
            [0, 0, 1] => Direction::South,
            [-1, 0, 0] => Direction::West,
            _ => Direction::East,
        };
        let player = self.use_context();
        self.with_level(|b, level, _| {
            let mut level = DryLevel::new(level);
            let result = b.use_item_on(
                &mut level,
                &mut vise_world::level::Ticks::default(),
                vise_core::position::BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
                face,
                vise_core::geometry::Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
                player,
                held.map(|s| &*s.item),
            );
            (result, (level.changed.into_iter().collect(), level.sounds))
        })
    }

    fn predict_place(&mut self, (pos, changes): &DryPlace, seq: i32) {
        let ticks = self.ticks;
        if let Some(r) = self.remote.as_mut() {
            r.placing = Some(([pos.x, pos.y, pos.z], ticks));
        }
        self.predict_blocks(changes, seq);
    }
    pub(super) fn predict_blocks(&mut self, changes: &DryChanges, seq: i32) {
        for &(p, state) in changes {
            let pos = [p.x, p.y, p.z];
            let before = self.world.block(p.x, p.y, p.z);
            if let Some(r) = self.remote.as_mut() {
                r.predictions
                    .entry(pos)
                    .and_modify(|e| e.0 = seq)
                    .or_insert((seq, before.0 as i32));
            }
            self.set_block(pos, state.0 as i32);
        }
    }

    fn remote_can_place(&mut self, hit: &super::Hit, stack: &ItemStack) -> Option<DryPlace> {
        if !self.may_build() {
            return None;
        }
        let ctx = vise_world::level::PlaceContext {
            clicked_pos: vise_core::position::BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]),
            face: vise_core::geometry::Direction::ALL[face_direction(hit.face) as usize],
            hit: Vec3::new(hit.point[0], hit.point[1], hit.point[2]),
            inside: hit.face == [0; 3],
            yaw: self.yaw,
            pitch: self.pitch,
            sneaking: self.keys.contains(&KeyCode::ShiftLeft),
            item: stack.item.to_string(),
            can_use_game_master_blocks: self.can_use_game_master_blocks(),
        };
        self.with_level(|b, level, _| {
            let mut level = DryLevel::new(level);
            match b.place_block(
                &mut level,
                &mut vise_world::level::Ticks::default(),
                &ctx,
            ) {
                vise_world::level::PlaceOutcome::Placed { pos, .. } => {
                    Some((pos, level.changed.into_iter().collect()))
                }
                vise_world::level::PlaceOutcome::Failed => None,
            }
        })
    }

    fn remote_use_bucket(&mut self, stack: &ItemStack) -> (bool, Option<DryChanges>) {
        if !self.may_build() {
            return (false, None);
        }
        if &*stack.item == "minecraft:bucket" {
            let Some(hit) = self.bucket_hit() else {
                return (false, None);
            };
            let pos = vise_core::position::BlockPos::new(hit.pos[0], hit.pos[1], hit.pos[2]);
            let creative = self.creative;
            return self.with_level(|b, level, ticks| {
                let mut level = DryLevel::new(level);
                let picked = b.bucket_pickup(&mut level, ticks, pos, creative).is_some();
                (picked, Some(level.changed.into_iter().collect()))
            });
        }
        let Some((content, sound, _)) = vise_client::items::bucket_contents(&stack.item) else {
            return (false, None);
        };
        let Some(hit) = self.pick() else {
            return (false, None);
        };
        let target = self.bucket_target(content, &hit);
        let pos = vise_core::position::BlockPos::new(target[0], target[1], target[2]);
        let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
        let creative = self.creative;
        let clicked = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        let client_change =
            !(content.is_water() && self.res.behaviours().is_liquid_container(clicked));
        self.with_level(|b, level, ticks| {
            let mut level = DryLevel::new(level);
            let emptied =
                b.bucket_empty(&mut level, ticks, pos, content, sound, sneaking, creative);
            (
                emptied,
                client_change.then(|| level.changed.into_iter().collect()),
            )
        })
    }

    fn remote_use_item(&mut self, hand: Hand) -> bool {
        let Some(held) = self.with_hand(hand, |g| g.held().cloned()) else {
            return false;
        };
        let seq = self.next_sequence();
        self.send(S::UseItem(play::UseItem {
            hand: hand_id(hand),
            sequence: seq,
            yaw: self.yaw,
            pitch: self.pitch,
        }));
        let Some(info) = self.items().get(&held.item).cloned() else {
            return false;
        };
        let group = vise_client::cooldowns::group(&held, info.use_cooldown.as_ref());
        if self.cooldowns.is_on_cooldown(&group) {
            return false;
        }
        if &*held.item == "minecraft:bucket"
            || vise_client::items::bucket_contents(&held.item).is_some()
        {
            let (succeeded, changes) = self.with_hand(hand, |g| g.remote_use_bucket(&held));
            if let Some(changes) = changes {
                self.predict_blocks(&changes, seq);
            }
            return succeeded;
        }
        let eating = self.with_hand(hand, |g| g.start_eating(&info));
        if info.food.is_some() && !eating {
            return false;
        }
        if self.with_hand(hand, |g| g.use_finds_no_target(&held)) {
            return false;
        }
        if !info.consumable.as_ref().is_some_and(|c| c.ticks() > 0) {
            self.apply_use_cooldown(&held);
        }
        if self.with_hand(hand, |g| g.remote_use_book(&held)) {
            return true;
        }
        self.with_hand(hand, |g| g.remote_start_held_use(&held));
        let gliding = self.player.as_ref().is_some_and(|p| p.fall_flying);
        use_ends_click(&info, gliding)
    }

    fn use_finds_no_target(&self, held: &ItemStack) -> bool {
        let id = &*held.item;
        let facts = |pos: [i32; 3]| {
            self.res
                .facts()
                .get(self.world.block(pos[0], pos[1], pos[2]))
        };
        let source = |want: Option<Fluid>| {
            let fluid_hit = self.pick_fluid();
            if let Some(hit) = self.pick()
                && facts(hit.pos).waterlogged
            {
                let eye = self.aim().map_or(hit.point, |(eye, _)| eye);
                let d = |p: [f64; 3]| (0..3).map(|a| (p[a] - eye[a]).powi(2)).sum::<f64>();
                if fluid_hit
                    .as_ref()
                    .is_none_or(|fluid| d(fluid.point) >= d(hit.point))
                {
                    return want.is_none_or(|want| want == Fluid::Water);
                }
            }
            fluid_hit.is_some_and(|hit| {
                let f = facts(hit.pos);
                let fluid = f.fluid != Fluid::None && f.fluid_level == 8 && !f.fluid_falling;
                match want {
                    Some(want) => fluid && f.fluid == want,
                    None => fluid || self.is_powder_snow(hit.pos),
                }
            })
        };
        if id == "minecraft:powder_snow_bucket" {
            return true;
        }
        if id == "minecraft:bucket" {
            return !self.may_build() || !source(None);
        }
        if vise_client::items::bucket_contents(id).is_some() {
            return !self.may_build() || self.pick().is_none();
        }
        if id == "minecraft:glass_bottle" {
            return !source(Some(Fluid::Water));
        }
        if super::boats::any_boat_wood(id).is_some() {
            return self.boat_to_place(held).is_none();
        }
        false
    }
    fn remote_start_held_use(&mut self, held: &ItemStack) {
        if self.using.is_some() {
            return;
        }
        let bundle = self.items().get(&held.item).is_some_and(|i| i.bundle);
        let start = match &*held.item {
            _ if bundle => true,
            "minecraft:bow" => self.creative || self.projectile_slot("minecraft:bow").is_some(),
            "minecraft:crossbow" => {
                held.data().is_none_or(|d| d.charged.is_empty())
                    && (self.creative || self.projectile_slot("minecraft:crossbow").is_some())
            }
            "minecraft:trident" => {
                self.use_trident(held);
                return;
            }
            "minecraft:spyglass" => true,
            "minecraft:goat_horn" => {
                self.remote_use_instrument(held);
                return;
            }
            "minecraft:ender_eye" => !self.aims_at_end_portal_frame(),
            id if vise_client::spear::Kinetic::of(id).is_some() => true,
            _ => {
                self.raise_shield(self.held_slot());
                return;
            }
        };
        if start {
            self.using = Some((self.held_slot(), 0));
        }
    }
    fn remote_use_instrument(&mut self, held: &ItemStack) {
        let Some(instrument) = self.res.items().instrument(held) else {
            return;
        };
        let group = vise_client::cooldowns::group(held, None);
        if self.cooldowns.is_on_cooldown(&group) {
            return;
        }
        self.using = Some((self.held_slot(), 0));
        let at = self
            .player
            .as_ref()
            .map(|p| [p.position.x, p.position.y, p.position.z]);
        self.audio
            .play(&instrument.sound, at, instrument.range / 16.0, 1.0);
        if instrument.use_duration > 0.0 {
            self.add_cooldown(group, instrument.use_ticks());
        }
    }
    fn held_until_released(&self, stack: &ItemStack) -> bool {
        matches!(
            &*stack.item,
            "minecraft:bow"
                | "minecraft:crossbow"
                | "minecraft:trident"
                | "minecraft:spyglass"
                | "minecraft:ender_eye"
        ) || vise_client::spear::Kinetic::of(&stack.item).is_some()
            || self.blocks_attacks(stack).is_some()
    }

    fn remote_use_hands(&mut self) {
        if self.remote_use_item(Hand::Main) {
            self.heights.item_used(Hand::Main);
            return;
        }
        let off_target = if self.entity_targeted() {
            self.remote_use_entity(Hand::Off)
        } else {
            self.remote_use_block(Hand::Off)
        };
        if off_target {
            return;
        }
        if self.remote_use_item(Hand::Off) {
            self.heights.item_used(Hand::Off);
        }
    }
    fn remote_tick_using(&mut self) {
        let Some((slot, ticks)) = self.using else {
            return;
        };
        let stack = self.stack_in(slot).cloned();
        let item = stack.as_ref().map(|s| Arc::clone(&s.item));
        if let Some(r) = self.remote.as_mut() {
            if ticks == 0 {
                r.use_item = item;
            } else if r.use_item != item {
                self.using = None;
                return;
            }
        }
        let in_hand = slot == self.selected || slot == super::shield::OFFHAND_SLOT;
        if stack
            .as_ref()
            .is_some_and(|s| &*s.item == "minecraft:brush")
        {
            let elapsed = ticks + 1;
            let hit = self.pick().filter(|_| in_hand);
            if let Some(hit) = &hit
                && elapsed % 10 == 5
            {
                self.brush_dust(hit, slot == super::shield::OFFHAND_SLOT);
            }
            if hit.is_none() && in_hand {
                self.send(S::PlayerAction(play::PlayerAction {
                    action: 6,
                    pos: [0; 3],
                    direction: 0,
                    sequence: 0,
                }));
            }
            self.using = hit.and(Some((slot, elapsed))).filter(|_| elapsed < 200);
            return;
        }
        if stack
            .as_ref()
            .is_some_and(|s| self.items().get(&s.item).is_some_and(|i| i.bundle))
        {
            self.using = (in_hand && ticks + 1 < 200).then_some((slot, ticks + 1));
            return;
        }
        if stack.as_ref().is_some_and(|s| self.held_until_released(s)) {
            self.using = in_hand.then_some((slot, ticks + 1));
            return;
        }
        if let Some(duration) = stack
            .as_ref()
            .and_then(|s| self.res.items().instrument(s))
            .map(|i| i.use_ticks().max(0) as u32)
        {
            self.using = (in_hand && ticks + 1 < duration).then_some((slot, ticks + 1));
            return;
        }
        let consumable = stack
            .as_ref()
            .and_then(|s| self.items().get(&s.item))
            .and_then(|i| i.consumable.clone());
        let (Some(stack), Some(consumable)) = (stack, consumable) else {
            self.using = None;
            return;
        };
        if !in_hand {
            self.using = None;
            return;
        }
        if consumable.ticks() <= 0 {
            self.using = None;
            return;
        }
        let remaining = consumable.ticks() - ticks as i32;
        if consumable.should_emit(remaining) {
            self.emit_consume(&consumable, &stack, 5);
        }
        self.using = Some((slot, ticks + 1));
    }
    fn remote_complete_using(&mut self) {
        let Some((slot, _)) = self.using.take() else {
            return;
        };
        let Some(stack) = self.stack_in(slot).cloned() else {
            return;
        };
        let Some(info) = self.items().get(&stack.item).cloned() else {
            return;
        };
        if let Some(consumable) = &info.consumable {
            self.emit_consume(consumable, &stack, 16);
        }
        if let Some((nutrition, saturation)) = info.food
            && let Some(r) = self.remote.as_mut()
        {
            let (_, food, sat) = &mut r.health;
            *food = (*food + nutrition as i32).clamp(0, 20);
            *sat = (*sat + saturation).clamp(0.0, *food as f32);
            self.apply_remote_health();
        }
        self.apply_use_cooldown(&stack);
    }
    pub(super) fn remote_mouse(&mut self, button: MouseButton, pressed: bool) -> Result<()> {
        use vise_client::interaction_input::Button;
        let input = match button {
            MouseButton::Left => {
                self.toggles
                    .button(2, pressed, false, self.settings.toggle_attack);
                self.left = self.toggles.is_down(2);
                Some(Button::Attack)
            }
            MouseButton::Right => {
                self.toggles
                    .button(3, pressed, false, self.settings.toggle_use);
                self.right = self.toggles.is_down(3);
                Some(Button::Use)
            }
            MouseButton::Middle => Some(Button::Pick),
            _ => None,
        };
        if pressed
            && let Some(button) = input
            && let Some(remote) = self.remote.as_mut()
        {
            remote.input.press(button);
        }
        Ok(())
    }

    pub(super) fn request_remote_mining_stop(&mut self) {
        if let Some(r) = self.remote.as_mut() {
            r.stop_mining = true;
        }
    }

    fn remote_handle_input(&mut self) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let mut input = std::mem::take(&mut r.input);
        let stop = std::mem::take(&mut r.stop_mining);
        let ready = r.loaded_sent && self.screen == Screen::Playing && self.sleeping.is_none();
        if stop || !ready {
            self.stop_remote_mining();
        }
        if !ready {
            return;
        }
        let carried_changed = self.remote.as_mut().is_some_and(|r| {
            if r.carried == Some(self.selected) {
                false
            } else {
                r.carried = Some(self.selected);
                true
            }
        });
        if carried_changed {
            self.send(S::SetCarriedItem(play::SetCarriedItem {
                slot: self.selected as i16,
            }));
        }
        let (attack, use_item) = (self.left, self.right);
        input.tick(&mut RemoteInputWorld(self), attack, use_item);
        let pending = self
            .remote
            .as_mut()
            .map(|r| std::mem::take(&mut r.pending_keys))
            .unwrap_or_default();
        for key in pending {
            match key {
                KeyCode::KeyF if !self.spectator() => self.remote_swap_hands(),
                KeyCode::KeyQ if !self.spectator() => {
                    let all = self.has_control_down();
                    self.remote_drop(all);
                }
                KeyCode::KeyE if self.riding_mount_with_menu() => {
                    let id = self.remote.as_ref().map_or(0, |r| r.entity_id);
                    self.send(S::PlayerCommand(play::PlayerCommand {
                        id,
                        action: 5,
                        data: 0,
                    }));
                }
                _ => {}
            }
        }
        if self.clicks.miss_time > 0 {
            self.clicks.miss_time -= 1;
        }
        if let Some(r) = self.remote.as_mut() {
            r.input = input;
        }
    }

    fn remote_start_attack(&mut self) -> bool {
        if self.clicks.miss_time > 0 {
            return false;
        }
        if self.spectator() {
            let target = self.spectator_click_target();
            self.send(S::SpectatorAction(play::SpectatorAction { target }));
            return true;
        }
        if let Some(kinetic) = self
            .held()
            .and_then(|s| vise_client::spear::Kinetic::of(&s.item))
        {
            if self.cannot_attack_with_item(self.hand_strength.ticker, 0) {
                return false;
            }
            self.send(S::PlayerAction(play::PlayerAction {
                action: 8,
                pos: [0; 3],
                direction: 0,
                sequence: 0,
            }));
            self.hand_strength.reset_attack();
            self.swing_hand(Hand::Main, kinetic.attack_animation());
            self.play_at_feet(kinetic.attack_sound);
            return true;
        }
        let mut instant = false;
        if let Some((i, distance)) = self.redirectable_hit()
            && self.mob_hit().is_none_or(|(_, d)| distance <= d)
            && self.before_block(distance)
            && self.creatures.projectiles[i].id != 0
        {
            self.send(S::Attack(play::Attack {
                id: self.creatures.projectiles[i].id as i32,
            }));
            self.reset_attack_strength();
        } else if let Some(hit) = self.remote_object_hit(true)
            && self.mob_hit().is_none_or(|(_, d)| hit.distance < d)
            && self.before_block(hit.distance)
        {
            self.send(S::Attack(play::Attack { id: hit.id }));
            self.reset_attack_strength();
        } else if let Some((i, distance)) = self.mob_hit()
            && self.before_block(distance)
        {
            self.send(S::Attack(play::Attack {
                id: self.mobs[i].id as i32,
            }));
            self.reset_attack_strength();
        } else if !self.entity_targeted()
            && let Some(hit) = self.pick()
        {
            self.remote_mining(true);
            instant = self
                .res
                .facts()
                .is_air(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
        } else {
            if !self.creative {
                self.clicks.miss_time = 10;
            }
            self.reset_attack_strength();
        }
        self.swing_arm();
        self.send(S::Punch(Empty));
        instant
    }

    fn remote_continue_attack(&mut self, down: bool) {
        if !down {
            self.clicks.miss_time = 0;
        }
        if self.clicks.miss_time > 0
            || self.using.is_some()
            || self
                .held()
                .is_some_and(|s| vise_client::spear::Kinetic::of(&s.item).is_some())
        {
            return;
        }
        let entity = self.entity_targeted();
        if down
            && !entity
            && let Some(hit) = self.pick()
        {
            let state = self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
            if self.remote_break_restricted() {
            } else if !self.res.facts().is_air(state) {
                self.remote_mining(false);
                self.swing_arm();
                self.send(S::Punch(Empty));
            } else {
                if let Some(r) = self.remote.as_mut() {
                    r.destroying = None;
                }
                self.mining = None;
            }
        } else {
            self.stop_remote_mining();
            self.mining = None;
        }
    }
    pub(super) fn remote_key(&mut self, key: KeyCode) -> Result<bool> {
        if self.screen != Screen::Playing {
            if (key == KeyCode::KeyE || key == KeyCode::Escape) && self.screen == Screen::Inventory
            {
                self.remote_close_container();
                self.screen = Screen::Playing;
                self.capture(true);
                return Ok(true);
            }
            return Ok(false);
        }
        match key {
            KeyCode::KeyT | KeyCode::Slash | KeyCode::Enter => {
                self.open_chat_method(if key == KeyCode::Slash { "/" } else { "" });
                Ok(true)
            }
            KeyCode::KeyQ => {
                if self.spectator() {
                    return Ok(true);
                }
                if let Some(r) = self.remote.as_mut() {
                    r.pending_keys.push(key);
                }
                Ok(true)
            }
            KeyCode::KeyE if self.riding_mount_with_menu() => {
                if let Some(r) = self.remote.as_mut() {
                    r.pending_keys.push(key);
                }
                Ok(true)
            }
            KeyCode::KeyE => {
                if self.tutorial.open_inventory() {
                    self.persist_tutorial_step();
                }
                self.recipe_ui.reset_screen();
                if self.creative {
                    self.screen = Screen::Creative;
                    self.refresh_creative_tabs();
                    self.creative_screen.reopen();
                    self.refresh_saved_hotbars();
                } else {
                    self.screen = Screen::Inventory;
                }
                self.craft_size = 2;
                self.capture(false);
                Ok(true)
            }
            _ => Ok(false),
        }
    }
    pub(super) fn send_chat(&mut self, message: &str) {
        let message = message.trim();
        if message.is_empty() || !self.chat_abilities().allows(message) {
            return;
        }
        if let Some(command) = message.strip_prefix('/') {
            self.send(S::ChatCommand(play::ChatCommand {
                command: command.to_owned(),
            }));
        } else {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as i64);
            self.send(S::Chat(play::Chat {
                message: message.chars().take(256).collect(),
                timestamp,
                salt: 0,
                signature: None,
                last_seen: play::LastSeenUpdate {
                    offset: 0,
                    acknowledged: [0; 3],
                    checksum: 0,
                },
            }));
        }
    }
    pub(super) fn remote_drop(&mut self, all: bool) {
        let dropped =
            vise_client::items::take_held_drop(&mut self.inventory[self.selected], all, false);
        self.send(S::PlayerAction(play::PlayerAction {
            action: if all { 4 } else { 5 },
            pos: [0; 3],
            direction: 0,
            sequence: 0,
        }));
        if dropped.is_some() {
            self.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
        }
    }
    pub(super) fn remote_swap_hands(&mut self) {
        self.send(S::PlayerAction(play::PlayerAction {
            action: 7,
            pos: [0; 3],
            direction: 0,
            sequence: 0,
        }));
        let (main, off) = (self.stack_in(self.selected), self.offhand.as_ref());
        let moves = !super::hand::matches_for_swap(main, off);
        if let Some(r) = self.remote.as_mut() {
            r.swap_answer = [moves; 2];
        }
    }
    fn remote_hand_slot_set(&mut self, slot: usize, changed: bool) {
        let hand = if slot == super::shield::OFFHAND_SLOT {
            1
        } else if slot == self.selected {
            0
        } else {
            return;
        };
        let answer = self
            .remote
            .as_mut()
            .is_some_and(|r| std::mem::take(&mut r.swap_answer[hand]));
        if answer && changed {
            self.hand_stack_replaced(slot);
        }
    }
    pub(super) fn remote_click(&mut self, right: bool) {
        let Some(size) = self
            .renderer
            .as_ref()
            .map(|r| [r.config.width, r.config.height])
        else {
            return;
        };
        let batch = HudBatch::new(size);
        let Some(internal) = self.slot_at(&batch, self.cursor.0, self.cursor.1) else {
            return;
        };
        let input = if self.has_shift_down() {
            Input::QuickMove
        } else {
            Input::Pickup
        };
        if self.screen == Screen::Creative {
            self.menu_click(Some(internal), i32::from(right), input);
            return;
        }
        self.remote_menu_ops(&[(Some(internal), i32::from(right), input)]);
    }
    fn remote_menu_state(&mut self, container: i32, state: i32) {
        let open = self.remote_menu().0;
        if let Some(r) = self.remote.as_mut().filter(|_| container == open) {
            r.state_id = state;
        }
    }
    pub(super) fn remote_menu(&self) -> (i32, String) {
        match self.remote.as_ref().and_then(|r| r.menu.clone()) {
            Some((id, name)) => (id, name),
            None => (0, "inventory".to_owned()),
        }
    }
    pub(super) fn remote_menu_ops(&mut self, ops: &[(Option<usize>, i32, Input)]) -> bool {
        if let [(Some(slot), button, input)] = ops {
            self.crafter_slot_clicked(*slot, *button, *input);
        }
        let (container, menu) = self.remote_menu();
        let mut clicks = Vec::with_capacity(ops.len());
        for &(slot, button, input) in ops {
            let slot = match slot {
                None => -999,
                Some(internal) => match Self::menu_slot_index(&menu, internal) {
                    Some(s) => s as i16,
                    None => return false,
                },
            };
            clicks.push((slot, button, input));
        }
        let mut sent: Vec<Predicted> = clicks
            .iter()
            .map(|&(slot, button, input)| (slot, button, input, Vec::new(), self.carried.clone()))
            .collect();
        let mut threw = false;
        self.menu_session(|m, env, _| {
            let Some((predicted, dropped)) = predict_clicks(m, env, &clicks) else {
                return Some(false);
            };
            sent = predicted;
            threw = dropped;
            Some(true)
        });
        self.send_clicks(container, sent);
        if threw {
            self.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
        }
        true
    }
    pub(super) fn remote_quick_move_all(&mut self, slot: usize, moved: &ItemStack) {
        let (container, _) = self.remote_menu();
        let mut sent: Vec<Predicted> = Vec::new();
        self.menu_session(|m, env, to_menu| {
            let clicked = to_menu(slot)?;
            let mut before = menu_items(m);
            let mut each = |m: &vise_client::menu::Menu, t: usize| {
                let changed = changed_slots(&before, m);
                sent.push((t as i16, 0, Input::QuickMove, changed, m.carried.clone()));
                before = menu_items(m);
            };
            Some(
                m.quick_move_all_each(env, clicked, moved, &mut each)
                    .is_ok(),
            )
        });
        self.send_clicks(container, sent);
    }
    fn send_clicks(&mut self, container: i32, clicks: Vec<Predicted>) {
        let state = self.remote.as_ref().map_or(0, |r| r.state_id);
        let hashed = |g: &Self, s: &Option<ItemStack>| g.to_network(s).map(|s| Hashed::of(&s));
        for (slot, button, input, changed, carried) in clicks {
            let changed = changed.iter().map(|(i, s)| (*i, hashed(self, s))).collect();
            let carried = hashed(self, &carried);
            self.send(S::ContainerClick(play::ContainerClick {
                container,
                state,
                slot,
                button: button as i8,
                input: input_ordinal(input),
                changed,
                carried,
            }));
        }
    }
    pub(super) fn remote_storage_open(&self) -> bool {
        match self.remote.as_ref().and_then(|r| r.menu.as_ref()) {
            None => true,
            Some((_, name)) => {
                name == "inventory"
                    || name.starts_with("minecraft:generic_9x")
                    || name == "minecraft:shulker_box"
            }
        }
    }
    pub(super) fn remote_throw(&mut self, right: bool, input: vise_client::menu::Input) {
        use vise_client::menu::Input;
        let container = self
            .remote
            .as_ref()
            .and_then(|r| r.menu.as_ref())
            .map_or(0, |m| m.0);
        if input == Input::Pickup
            && let Some(c) = self.carried.as_mut()
        {
            if right && c.count > 1 {
                c.count -= 1;
            } else {
                self.carried = None;
            }
            self.swing_arm();
        }
        let carried = self.to_network(&self.carried).map(|s| Hashed::of(&s));
        let state = self.remote.as_ref().map_or(0, |r| r.state_id);
        self.send(S::ContainerClick(play::ContainerClick {
            container,
            state,
            slot: -999,
            button: i8::from(right),
            input: if input == Input::Throw { 4 } else { 0 },
            changed: Vec::new(),
            carried,
        }));
    }
    pub(super) fn remote_creative_drop(&mut self, right: bool) {
        let one = right && !self.creative_inventory_open();
        let Some(carried) = self.carried.as_mut() else {
            return;
        };
        let thrown = if one && carried.count > 1 {
            carried.count -= 1;
            ItemStack {
                count: 1,
                ..carried.clone()
            }
        } else {
            self.carried.take().expect("carried")
        };
        self.remote_creative_throw(thrown);
    }
    pub(super) fn remote_creative_throw(&mut self, thrown: ItemStack) {
        if !self.creative {
            return;
        }
        self.swing_arm();
        let item = self.to_network(&Some(thrown));
        self.send(S::SetCreativeModeSlot(play::SetCreativeModeSlot {
            slot: -1,
            item,
        }));
    }
    pub(super) fn remote_close_container(&mut self) {
        let id = self
            .remote
            .as_ref()
            .and_then(|r| r.menu.as_ref().map(|m| m.0))
            .unwrap_or(0);
        self.send(S::ContainerClose(play::ContainerClose { container: id }));
        self.remote_close_local();
    }
    fn remote_close_local(&mut self) {
        self.recipe_ui.reset_screen();
        if let Some(r) = self.remote.as_mut() {
            r.menu = None;
            r.menu_title = None;
        }
        self.mount_menu_reset();
        self.station = None;
        self.craft_grid.fill(None);
        self.carried = None;
        self.craft_size = 2;
        if let Some(key) = self.container.take() {
            if let Some(entity) = self.world.block_entities.get_mut(&key) {
                for s in entity.slots_mut() {
                    *s = None;
                }
            }
        }
    }
    fn remote_open_merchant(&mut self, container: i32, title: String) {
        if let Some(r) = self.remote.as_mut() {
            r.menu = Some((container, "minecraft:merchant".to_owned()));
        }
        let session = vise_client::merchant::Session::new(Vec::new(), true);
        self.open_remote_merchant(session, (!title.is_empty()).then_some(title));
    }
    fn remote_merchant_offers(&mut self, p: play::MerchantOffers) {
        if self.open_menu_name(p.container) != "minecraft:merchant" {
            return;
        }
        let cost = |g: &Self, c: &play::NetCost| {
            g.to_stack(&Some(c.clone().into()))
                .map(|stack| vise_client::merchant::ItemCost { stack })
        };
        let offers: Vec<_> = p
            .offers
            .iter()
            .filter_map(|o| {
                let mut offer = vise_client::merchant::MerchantOffer {
                    base_cost_a: cost(self, &o.cost_a)?,
                    cost_b: match &o.cost_b {
                        Some(b) => Some(cost(self, b)?),
                        None => None,
                    },
                    result: self.to_stack(&Some(o.result.clone()))?,
                    uses: o.uses,
                    max_uses: o.max_uses,
                    reward_exp: true,
                    special_price_diff: o.special_price,
                    demand: o.demand,
                    price_multiplier: o.price_multiplier,
                    xp: o.xp,
                };
                if o.out_of_stock {
                    offer.uses = offer.max_uses;
                }
                Some(offer)
            })
            .collect();
        if let Some(m) = self.station.as_mut().and_then(|s| s.merchant.as_mut()) {
            m.session.offers = offers;
            m.session.level = p.level;
            m.session.xp = p.xp;
            m.session.villager = p.show_progress;
        }
    }
    fn remote_open_screen(&mut self, p: play::OpenScreen) {
        self.recipe_ui.reset_screen();
        let menu = self
            .remote
            .as_ref()
            .and_then(|r| {
                r.builtin
                    .get("minecraft:menu")?
                    .name(p.menu)
                    .map(str::to_owned)
            })
            .unwrap_or_default();
        let target = self
            .remote
            .as_ref()
            .and_then(|r| r.use_target)
            .map(|t| (t[0], t[1], t[2]));
        if menu == "minecraft:lectern" {
            if let Some(r) = self.remote.as_mut() {
                r.menu = Some((p.container, menu));
            }
            self.remote_open_lectern(target.unwrap_or_default());
            return;
        }
        if let Some(kind) = Self::remote_station_kind(&menu) {
            let title = menu_title(&p.title);
            let pos = target.map_or([0, 0, 0], |t| [t.0, t.1, t.2]);
            self.remote_open_station(p.container, menu, kind, title, pos);
            return;
        }
        if menu == "minecraft:merchant" {
            let title = plain(
                &p.title,
                &self
                    .remote
                    .as_ref()
                    .map(|r| r.lang.clone())
                    .unwrap_or_default(),
            );
            self.remote_open_merchant(p.container, title);
            return;
        }
        let entity = match menu.as_str() {
            "minecraft:crafting" => None,
            "minecraft:furnace" | "minecraft:blast_furnace" | "minecraft:smoker" => {
                let kind = match menu.as_str() {
                    "minecraft:blast_furnace" => FurnaceKind::BlastFurnace,
                    "minecraft:smoker" => FurnaceKind::Smoker,
                    _ => FurnaceKind::Furnace,
                };
                Some(BlockEntity::Furnace(Furnace {
                    kind,
                    items: [None, None, None],
                    lit_remaining: 0,
                    lit_total: 0,
                    cooking_timer: 0,
                    cooking_total: 0,
                    speed: 1.0,
                    recipes_used: std::collections::HashMap::new(),
                }))
            }
            "minecraft:hopper" => Some(BlockEntity::Chest(vec![None; 5])),
            "minecraft:brewing_stand" => Some(BlockEntity::Brewing(Box::new(
                vise_client::brewing::BrewingStand::new(),
            ))),
            "minecraft:generic_3x3" => Some(BlockEntity::Chest(vec![None; 9])),
            "minecraft:crafter_3x3" => Some(BlockEntity::Crafter(Default::default())),
            m if m.starts_with("minecraft:generic_9x") || m == "minecraft:shulker_box" => {
                let rows = m
                    .strip_prefix("minecraft:generic_9x")
                    .and_then(|r| r.parse::<usize>().ok())
                    .unwrap_or(3);
                Some(BlockEntity::Chest(vec![None; rows * 9]))
            }
            _ => {
                self.send(S::ContainerClose(play::ContainerClose {
                    container: p.container,
                }));
                self.chat_line(
                    vise_client::text::Text::literal(format!(
                        "(Vise cannot show {menu} screens yet)"
                    )),
                    vise_client::chat::ChatSource::Client,
                );
                return;
            }
        };
        let title = menu_title(&p.title);
        if let Some(r) = self.remote.as_mut() {
            r.menu = Some((p.container, menu.clone()));
            r.menu_title = Some(title);
        }
        self.craft_size = if menu == "minecraft:crafting" { 3 } else { 2 };
        self.container = None;
        if let Some(entity) = entity {
            let holds = |k: &(i32, i32, i32)| {
                let name = &self.res.facts().get(self.world.block(k.0, k.1, k.2)).name;
                BlockEntity::for_block(name).is_some_and(|e| {
                    e.id() == entity.id() && e.slots().len() == entity.slots().len()
                })
            };
            let key = target.filter(holds).unwrap_or((i32::MIN, 0, 0));
            self.world.block_entities.insert(key, entity);
            self.container = Some(key);
        }
        self.screen = Screen::Inventory;
        self.capture(false);
    }
    fn remote_open_mount(&mut self, p: play::MountScreenOpen) {
        let columns = usize::try_from(p.columns).unwrap_or(0).min(5);
        if !self.open_remote_mount_menu(p.entity as u32, columns) {
            self.send(S::ContainerClose(play::ContainerClose {
                container: p.container,
            }));
            return;
        }
        if let Some(r) = self.remote.as_mut() {
            r.menu = Some((p.container, format!("{MOUNT_MENU}{columns}")));
            r.menu_title = None;
        }
    }
    pub(super) fn remote_menu_type(&self) -> Option<&str> {
        Some(self.remote.as_ref()?.menu.as_ref()?.1.as_str())
    }
    pub(super) fn remote_menu_title(&self) -> Option<vise_client::text::Text> {
        self.remote.as_ref()?.menu_title.clone()
    }
    pub(super) fn refresh_remote_language(&mut self) {
        if let Some(lang) = vise_client::lang::current()
            && let Some(r) = self.remote.as_mut()
        {
            r.lang = Arc::new(lang.map().clone());
        }
    }
    pub(super) fn remote_chest_menu(&self) -> Option<(usize, vise_client::text::Text)> {
        let r = self.remote.as_ref()?;
        let rows = r
            .menu
            .as_ref()?
            .1
            .strip_prefix("minecraft:generic_9x")?
            .parse()
            .ok()?;
        Some((
            rows,
            r.menu_title
                .clone()
                .unwrap_or_else(vise_client::text::Text::empty),
        ))
    }
    pub(super) fn remote_stop_sleeping(&mut self) {
        let id = self.remote.as_ref().map_or(0, |r| r.entity_id);
        self.send(S::PlayerCommand(play::PlayerCommand {
            id,
            action: 0,
            data: 0,
        }));
    }
    pub(super) fn remote_respawn(&mut self) {
        self.send(S::ClientCommand(play::ClientCommand { action: 0 }));
    }

    pub(super) fn remote_overlay(&self, batch: &mut HudBatch) -> bool {
        let Some(r) = &self.remote else { return false };
        if self.player.is_some() {
            return false;
        }
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        if r.joined {
            vise_client::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, false);
            vise_client::level_loading::draw(batch, atlas, None, None);
            return true;
        }
        let cancel = self.connect_cancel((w, h)).is_some();
        let status = vise_client::text::Text::literal(r.status.as_str());
        vise_client::level_loading::draw_connecting(batch, atlas, &status, self.cursor, cancel);
        true
    }
    pub(super) fn remote_loading(&self) -> bool {
        self.remote.is_some() && self.player.is_none()
    }
    fn connect_cancel(&self, (w, h): (f32, f32)) -> Option<(f32, f32)> {
        let r = self.remote.as_ref()?;
        (self.player.is_none() && !r.joined && self.integrated.is_none())
            .then(|| vise_client::level_loading::connect_cancel(w, h))
    }
    pub(super) fn connect_screen_click(&mut self) -> Result<()> {
        let Some((x, y)) = self.connect_cancel(self.gui_size()) else {
            return Ok(());
        };
        if vise_client::ui::hovered(self.cursor, x, y, 200.0, 20.0) {
            self.audio.play_button_click();
            self.leave_server(None)?;
        }
        Ok(())
    }
    pub(super) fn remote_hud(&self, batch: &mut HudBatch) {
        let Some(r) = &self.remote else { return };
        let atlas = &self.gui.atlas;
        let w = batch.width;
        if self.screen == Screen::Dead
            && let Some(message) = &r.death_message
        {
            let tw = atlas.font.width_text(message);
            batch.text_styled(
                atlas,
                message,
                ((w - tw) / 2.0).floor(),
                85.0,
                u32::MAX,
                true,
            );
        }
    }
    pub(super) fn remote_glowing(&self, id: u32) -> Option<bool> {
        let r = self.remote.as_ref()?;
        Some(r.glowing.contains(&(id as i32)))
    }
    pub(super) fn remote_player_name(&self, id: u32) -> Option<&str> {
        let r = self.remote.as_ref()?;
        let uuid = r.level_players.get(&(id as i32))?;
        if *uuid == r.profile {
            return None;
        }
        r.level_player_name(id as i32, uuid)
    }
    pub(super) fn remote_player_names(&self) -> Vec<(u32, String)> {
        let Some(r) = self.remote.as_ref() else {
            return Vec::new();
        };
        r.level_players
            .iter()
            .filter(|(_, uuid)| **uuid != r.profile)
            .filter_map(|(id, uuid)| Some((*id as u32, r.level_player_name(*id, uuid)?.to_owned())))
            .collect()
    }
    fn remote_clock_tick(&mut self) {
        let Some(r) = self.remote.as_mut() else {
            return;
        };
        let delta = self.world.game_time - r.clock_game_time;
        r.clock_game_time = self.world.game_time;
        let partial = f64::from(r.clock_partial) + delta as f64 * f64::from(r.clock_rate);
        let full = partial.floor();
        r.clock_partial = (partial - full) as f32;
        self.world.day_time += full as i64;
    }
    pub(super) fn remote_status(&self) -> Option<serde_json::Value> {
        let r = self.remote.as_ref()?;
        Some(serde_json::json!({
            "address": vise_client::net::shown_address(&r.session.address),
            "name": r.session.name,
            "joined": r.joined,
            "loaded": r.loaded_sent,
            "status": r.status,
            "entity_id": r.entity_id,
            "dimension": r.dimension.name,
            "game_mode": r.game_mode,
            "chunks": self.world.chunks.len(),
            "entities": r.entities.values().fold(std::collections::BTreeMap::<&str, u32>::new(), |mut m, t| {
                *m.entry(t.kind.as_str()).or_default() += 1;
                m
            }),
            "players": r.players.values().collect::<Vec<_>>(),
            "player_equipment": self
                .mobs
                .iter()
                .filter(|m| r.level_players.contains_key(&(m.id as i32)))
                .map(|m| {
                    let e = &m.extra;
                    let armor = [&e.head, &e.chest, &e.legs, &e.feet].map(|s| s.as_deref());
                    serde_json::json!({
                        "id": m.id,
                        "swinging": m.ai.swinging,
                        "using": m.extra.using_item,
                        "armor": armor,
                        "main_hand": e.main_hand.as_deref(),
                        "off_hand": e.off_hand.as_deref(),
                    })
                })
                .collect::<Vec<_>>(),
            "player_positions": r
                .entities
                .values()
                .filter(|t| t.kind == "minecraft:player")
                .map(|t| t.position)
                .collect::<Vec<_>>(),
            "dead_players": self
                .mobs
                .iter()
                .filter(|m| r.level_players.contains_key(&(m.id as i32)) && !m.alive())
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            "lying_players": self
                .mobs
                .iter()
                .filter(|m| r.level_players.contains_key(&(m.id as i32)) && m.extra.ints[1] != 0)
                .map(|m| m.id)
                .collect::<Vec<_>>(),
            "player_model": self.player_model.is_some(),
            "own_body": self.player_model_shown() && self.own_body_visible(),
            "text_display_vertices": self.text_display_vertices,
            "text_displays": self.text_displays.len(),
            "health": r.health.0,
            "food": r.health.1,
            "decode_errors": r.decode_errors,
            "chat": r.chat.iter().rev().take(10).rev().map(|(t, _)| t.clone()).collect::<Vec<_>>(),
        }))
    }
}

fn look_at_rotation(from: [f64; 3], to: [f64; 3]) -> (f32, f32) {
    use vise_core::geometry::trig::atan2;
    let (dx, dy, dz) = (to[0] - from[0], to[1] - from[1], to[2] - from[2]);
    let horizontal = (dx * dx + dz * dz).sqrt();
    let degrees = |r: f64| r * 180.0 / f64::from(std::f32::consts::PI);
    let pitch = wrap_degrees((-degrees(atan2(dy, horizontal))) as f32);
    let yaw = wrap_degrees(degrees(atan2(dz, dx)) as f32 - 90.0);
    (yaw, pitch)
}

fn position_packet_pitch(source: f32, change: f32, relative: bool) -> f32 {
    let offset = if relative { source } else { 0.0 };
    (offset + change).clamp(-90.0, 90.0)
}

fn wrap_degrees(a: f32) -> f32 {
    let a = a % 360.0;
    if a >= 180.0 {
        a - 360.0
    } else if a < -180.0 {
        a + 360.0
    } else {
        a
    }
}

fn remote_player_head_turn(m: &mut Mob) {
    let (dx, dz) = (m.position.x - m.previous.x, m.position.z - m.previous.z);
    let mut target = m.body_yaw;
    if (dx * dx + dz * dz) as f32 > 0.002_500_000_2 {
        let walk = (dz.atan2(dx) as f32).to_degrees() - 90.0;
        let facing = (wrap_degrees(m.yaw) - walk).abs();
        target = if 95.0 < facing && facing < 265.0 {
            walk - 180.0
        } else {
            walk
        };
    }
    if m.anim.attack_anim > 0.0 {
        target = m.yaw;
    }
    m.body_yaw += wrap_degrees(target - m.body_yaw) * 0.3;
    let head = wrap_degrees(m.yaw - m.body_yaw);
    if head.abs() > 50.0 {
        m.body_yaw += head - head.signum() * 50.0;
    }
}

type Predicted = (
    i16,
    i32,
    Input,
    Vec<(i16, Option<ItemStack>)>,
    Option<ItemStack>,
);

const CREATIVE_SYNCED: usize = 41;

fn creative_sync_stacks(
    inventory: &[Option<ItemStack>],
    offhand: &Option<ItemStack>,
    armor: &[Option<ItemStack>; 4],
) -> Vec<Option<ItemStack>> {
    let mut stacks: Vec<Option<ItemStack>> = inventory.iter().take(36).cloned().collect();
    stacks.resize(36, None);
    stacks.push(offhand.clone());
    stacks.extend(armor.iter().cloned());
    stacks
}

fn mark_synced(
    synced: &mut [Option<ItemStack>],
    before: &[Option<ItemStack>],
    now: Vec<Option<ItemStack>>,
) {
    for (i, stack) in now.into_iter().enumerate() {
        if before.get(i) != Some(&stack)
            && let Some(s) = synced.get_mut(i)
        {
            *s = stack;
        }
    }
}

fn creative_sync_slot(i: usize) -> i16 {
    (match i {
        0..9 => 36 + i,
        36 => 45,
        37.. => 5 + (i - 37),
        _ => i,
    }) as i16
}

fn predict_clicks(
    m: &mut vise_client::menu::Menu,
    env: &vise_client::menu::Env,
    clicks: &[(i16, i32, Input)],
) -> Option<(Vec<Predicted>, bool)> {
    let mut predicted = Vec::with_capacity(clicks.len());
    let mut dropped = false;
    for &(slot, button, input) in clicks {
        let before = menu_items(m);
        let thrown = m.dropped.len();
        m.click(env, i32::from(slot), button, input).ok()?;
        dropped |= m.dropped.len() > thrown;
        let changed = changed_slots(&before, m);
        predicted.push((slot, button, input, changed, m.carried.clone()));
    }
    Some((predicted, dropped))
}

fn menu_items(m: &vise_client::menu::Menu) -> Vec<Option<ItemStack>> {
    (0..m.slot_count()).map(|i| m.slot(i).cloned()).collect()
}

fn changed_slots(
    before: &[Option<ItemStack>],
    m: &vise_client::menu::Menu,
) -> Vec<(i16, Option<ItemStack>)> {
    (0..m.slot_count())
        .filter(|&i| m.slot(i) != before.get(i).and_then(Option::as_ref))
        .map(|i| (i as i16, m.slot(i).cloned()))
        .collect()
}

pub(super) fn input_ordinal(input: Input) -> i32 {
    match input {
        Input::Pickup => 0,
        Input::QuickMove => 1,
        Input::Swap => 2,
        Input::Clone => 3,
        Input::Throw => 4,
        Input::QuickCraft => 5,
        Input::PickupAll => 6,
    }
}

struct DryLevel<'w> {
    base: &'w dyn vise_world::level::LevelAccess,
    changed: HashMap<vise_core::position::BlockPos, BlockStateId>,
    random: vise_core::random::LegacyRandom,
    sounds: Vec<DrySound>,
}

type DrySound = (
    vise_core::position::BlockPos,
    std::sync::Arc<str>,
    f32,
    f32,
);
type DryChanges = Vec<(vise_core::position::BlockPos, BlockStateId)>;
type DryUse = (DryChanges, Vec<DrySound>);
type DryPlace = (vise_core::position::BlockPos, DryChanges);

pub(super) fn hand_operated(name: &str) -> bool {
    let name = name.trim_start_matches("minecraft:");
    ((name.ends_with("_door") || name.ends_with("_trapdoor")) && !name.starts_with("iron_"))
        || name.ends_with("_fence_gate")
        || name.ends_with("_button")
        || name == "lever"
}

impl<'w> DryLevel<'w> {
    fn new(base: &'w dyn vise_world::level::LevelAccess) -> Self {
        Self {
            base,
            changed: HashMap::new(),
            random: vise_core::random::LegacyRandom::new(0),
            sounds: Vec::new(),
        }
    }
}

impl vise_world::level::LevelAccess for DryLevel<'_> {
    fn emit(&mut self, event: vise_world::level::LevelEvent) {
        if let vise_world::level::LevelEvent::Sound {
            pos,
            sound,
            volume,
            pitch,
        } = event
        {
            self.sounds.push((pos, sound, volume, pitch));
        }
    }
    fn block_state(&self, pos: vise_core::position::BlockPos) -> BlockStateId {
        self.changed
            .get(&pos)
            .copied()
            .unwrap_or_else(|| self.base.block_state(pos))
    }
    fn set_raw(
        &mut self,
        pos: vise_core::position::BlockPos,
        state: BlockStateId,
    ) -> Option<BlockStateId> {
        let previous = self.block_state(pos);
        self.changed.insert(pos, state);
        Some(previous)
    }
    fn is_loaded(&self, pos: vise_core::position::BlockPos) -> bool {
        self.base.is_loaded(pos)
    }
    fn min_y(&self) -> i32 {
        self.base.min_y()
    }
    fn max_y(&self) -> i32 {
        self.base.max_y()
    }
    fn block_light(&self, pos: vise_core::position::BlockPos) -> i32 {
        self.base.block_light(pos)
    }
    fn sky_light(&self, pos: vise_core::position::BlockPos) -> i32 {
        self.base.sky_light(pos)
    }
    fn sky_darken(&self) -> i32 {
        self.base.sky_darken()
    }
    fn is_unobstructed(&self, pos: vise_core::position::BlockPos, state: BlockStateId) -> bool {
        self.base.is_unobstructed(pos, state)
    }
    fn random(&mut self) -> &mut vise_core::random::LegacyRandom {
        &mut self.random
    }
    fn game_time(&self) -> i64 {
        self.base.game_time()
    }
}

fn hand_id(hand: Hand) -> i32 {
    match hand {
        Hand::Main => 0,
        Hand::Off => 1,
    }
}

fn use_ends_click(info: &vise_client::items::ItemInfo, fall_flying: bool) -> bool {
    item_has_use(info) || (fall_flying && &*info.id == "minecraft:firework_rocket")
}

fn item_has_use(info: &vise_client::items::ItemInfo) -> bool {
    let id = info.id.trim_start_matches("minecraft:");
    id.ends_with("_spear")
        || info.consumable.is_some()
        || info.equip_slot.is_some()
        || info.blocks_attacks.is_some()
        || vise_client::items::bucket_contents(&info.id).is_some()
        || id.ends_with("_boat")
        || id.ends_with("_raft")
        || id.ends_with("_bundle")
        || matches!(
            id,
            "bow"
                | "crossbow"
                | "trident"
                | "spyglass"
                | "fishing_rod"
                | "snowball"
                | "egg"
                | "blue_egg"
                | "brown_egg"
                | "ender_pearl"
                | "ender_eye"
                | "wind_charge"
                | "splash_potion"
                | "lingering_potion"
                | "experience_bottle"
                | "bucket"
                | "glass_bottle"
                | "map"
                | "writable_book"
                | "written_book"
                | "goat_horn"
                | "bundle"
                | "brush"
                | "carrot_on_a_stick"
                | "warped_fungus_on_a_stick"
        )
}

fn client_view_distance(render_distance: i32) -> i8 {
    render_distance.clamp(2, i32::from(i8::MAX)) as i8
}

struct RemoteInputWorld<'g, 'a>(&'g mut Game<'a>);

struct RemoteUseWorld<'g, 'a>(&'g mut Game<'a>);

impl vise_client::interaction_input::TargetUseWorld for RemoteUseWorld<'_, '_> {
    fn use_entity(&mut self) -> bool {
        self.0.remote_use_entity(Hand::Main)
    }
    fn use_block(&mut self) -> bool {
        self.0.remote_use_block(Hand::Main)
    }
    fn use_item(&mut self) {
        self.0.remote_use_hands();
    }
}

impl vise_client::interaction_input::InteractionWorld for RemoteInputWorld<'_, '_> {
    fn pick_hit_result(&mut self) {
        self.0.tick_hit = Some(self.0.pick());
    }
    fn release_hit_result(&mut self) {
        self.0.tick_hit = None;
    }
    fn using_item(&self) -> bool {
        self.0.using.is_some()
    }
    fn release_using(&mut self) {
        self.0.send(S::PlayerAction(play::PlayerAction {
            action: 6,
            pos: [0; 3],
            direction: 0,
            sequence: 0,
        }));
        let hand = self.0.used_hand();
        if let Some((slot, ticks)) = self.0.using.take()
            && let Some(hand) = hand
            && self
                .0
                .stack_in(slot)
                .is_some_and(|s| &*s.item == "minecraft:trident")
        {
            self.0.with_hand(hand, |g| g.release_trident(ticks));
        }
    }
    fn start_attack(&mut self) -> bool {
        self.0.remote_start_attack()
    }
    fn start_use(&mut self) {
        self.0.remote_use();
    }
    fn pick(&mut self) {
        if let Some(entity) = self.0.pick_entity_hit() {
            if let Some(id) = entity {
                self.0.send(S::PickItemFromEntity(play::PickItemFromEntity {
                    id,
                    include_data: self.0.has_control_down(),
                }));
            }
        } else if let Some(hit) = self.0.pick() {
            self.0.send(S::PickItemFromBlock(play::PickItemFromBlock {
                pos: hit.pos,
                include_data: self.0.has_control_down(),
            }));
        }
    }
    fn use_delay(&self) -> u32 {
        self.0.use_delay
    }
    fn continue_attack(&mut self, down: bool) {
        self.0.remote_continue_attack(down);
    }
}

impl Remote {
    fn level_player_name(&self, id: i32, uuid: &u128) -> Option<&str> {
        self.level_player_names
            .get(&id)
            .or_else(|| self.players.get(uuid))
            .map(String::as_str)
    }
    pub(super) fn player_count(&self) -> usize {
        self.players.len()
    }
    pub(super) fn profile(&self) -> u128 {
        self.profile
    }
    pub(super) fn player_entity(&self, uuid: u128) -> Option<u32> {
        self.level_players
            .iter()
            .find(|(_, u)| **u == uuid)
            .map(|(id, _)| *id as u32)
    }
}

fn skin_entries(
    level_players: &HashMap<i32, u128>,
    listed: &HashMap<u128, vise_client::skins::Textures>,
    kept: &mut HashMap<i32, vise_client::skins::Textures>,
) -> Vec<(u128, vise_client::skins::Textures)> {
    for (id, uuid) in level_players {
        if !kept.contains_key(id)
            && let Some(textures) = listed.get(uuid)
        {
            kept.insert(*id, textures.clone());
        }
    }
    let mut entries: Vec<_> = listed.iter().map(|(u, t)| (*u, t.clone())).collect();
    for (id, textures) in kept.iter() {
        if let Some(uuid) = level_players.get(id)
            && !listed.contains_key(uuid)
            && !entries.iter().any(|(u, _)| u == uuid)
        {
            entries.push((*uuid, textures.clone()));
        }
    }
    entries
}

fn snapshot_value(s: &play::AttributeSnapshot) -> f64 {
    let amounts = |op: i32| {
        s.modifiers
            .iter()
            .filter(move |m| m.operation == op)
            .map(|m| m.amount)
    };
    let base = s.base + amounts(0).sum::<f64>();
    let mut value = base + amounts(1).map(|a| base * a).sum::<f64>();
    for amount in amounts(2) {
        value *= 1.0 + amount;
    }
    value
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_player_entity_keeps_the_skin_its_entry_gave() {
        use vise_client::skins::Textures;
        use std::collections::HashMap;
        let npc = Textures {
            skin: Some("http://textures.minecraft.net/texture/npc".into()),
            signed: true,
            ..Textures::default()
        };
        let (mut listed, mut kept) = (HashMap::from([(7u128, npc.clone())]), HashMap::new());
        let mut players = HashMap::from([(2i32, 7u128), (3, 8)]);
        assert_eq!(
            super::skin_entries(&players, &listed, &mut kept),
            [(7, npc.clone())]
        );
        listed.remove(&7);
        assert_eq!(
            super::skin_entries(&players, &listed, &mut kept),
            [(7, npc.clone())]
        );
        let late = Textures {
            skin: Some("http://textures.minecraft.net/texture/late".into()),
            ..npc.clone()
        };
        listed.insert(8, late.clone());
        let mut entries = super::skin_entries(&players, &listed, &mut kept);
        entries.sort_by_key(|(u, _)| *u);
        assert_eq!(entries, [(7, npc), (8, late)]);
        players.remove(&2);
        kept.remove(&2);
        listed.clear();
        assert_eq!(super::skin_entries(&players, &listed, &mut kept).len(), 1);
    }

    #[test]
    fn attribute_values_add_then_multiply_as_vanilla() {
        use vise_protocol::packets::play::{AttributeModifier, AttributeSnapshot};
        let modifier = |amount, operation| AttributeModifier {
            id: "minecraft:proof".into(),
            amount,
            operation,
        };
        let snapshot = |base, modifiers| AttributeSnapshot {
            attribute: 0,
            base,
            modifiers,
        };
        assert_eq!(super::snapshot_value(&snapshot(2.0, vec![])), 2.0);
        assert_eq!(
            super::snapshot_value(&snapshot(1.0, vec![modifier(1.0, 0)])),
            2.0
        );
        let all = vec![modifier(1.0, 0), modifier(0.5, 1), modifier(1.0, 2)];
        assert_eq!(super::snapshot_value(&snapshot(1.0, all)), 6.0);
    }

    use super::Game;

    #[test]
    fn look_at_turns_towards_the_target() {
        let close = |(yaw, pitch): (f32, f32), want: (f32, f32)| {
            assert!(
                (yaw - want.0).abs() < 1e-3 && (pitch - want.1).abs() < 1e-3,
                "{yaw} {pitch}, want {want:?}"
            );
        };
        let at = [10.0, 64.0, -5.0];
        let off = |d: [f64; 3]| [at[0] + d[0], at[1] + d[1], at[2] + d[2]];
        close(
            super::look_at_rotation(at, off([0.0, 0.0, 3.0])),
            (0.0, 0.0),
        );
        close(
            super::look_at_rotation(at, off([3.0, 0.0, 0.0])),
            (-90.0, 0.0),
        );
        close(
            super::look_at_rotation(at, off([-3.0, 0.0, 0.0])),
            (90.0, 0.0),
        );
        close(
            super::look_at_rotation(at, off([0.0, 0.0, -3.0])),
            (-180.0, 0.0),
        );
        close(
            super::look_at_rotation(at, off([0.0, 2.0, 2.0])),
            (0.0, -45.0),
        );
        close(
            super::look_at_rotation(at, off([0.0, -4.0, 4.0])),
            (0.0, 45.0),
        );
    }

    #[test]
    fn a_position_packet_moves_both_pitch_ends() {
        use super::position_packet_pitch as pitch;
        assert_eq!(
            (pitch(30.0, 10.0, true), pitch(0.0, 10.0, true)),
            (40.0, 10.0)
        );
        assert_eq!(
            (pitch(80.0, 20.0, true), pitch(60.0, 20.0, true)),
            (90.0, 80.0)
        );
        assert_eq!(
            (pitch(30.0, -45.0, false), pitch(0.0, -45.0, false)),
            (-45.0, -45.0)
        );
        assert_eq!(pitch(0.0, 120.0, false), 90.0);
    }

    #[test]
    fn a_predicted_throw_from_a_slot_swings_the_hand() {
        use vise_client::{
            banner::Banners,
            brewing::Brewing,
            crafting::Recipes,
            creative::Names,
            enchant::Enchantments,
            items::{ItemStack, Items},
            menu::{Env, Input, Kind, Menu},
            stations::StationRecipes,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !root.join("items.json").exists() {
            return;
        }
        let none = &vise_world::loot::PackOverlay::NONE;
        let items = Items::catalog(&root, none).unwrap();
        let recipes = Recipes::load(&root, &items, none).unwrap();
        let brewing = Brewing::load(&root, &items, none);
        let banners = Banners::load(&root);
        let station_recipes = StationRecipes::load(&root, &items, none);
        let enchantments = Enchantments::default();
        let names = Names::default();
        let env = Env {
            items: &items,
            recipes: &recipes,
            brewing: &brewing,
            banners: &banners,
            station_recipes: &station_recipes,
            enchantments: &enchantments,
            names: &names,
            trade: None,
            maps: None,
        };
        let mut menu = Menu::new(Kind::Inventory);
        let cobblestone = ItemStack {
            item: "minecraft:cobblestone".into(),
            count: 16,
            damage: 0,
            extra: None,
        };
        menu.set_slot(&env, 36, Some(cobblestone));
        let (sent, threw) =
            super::predict_clicks(&mut menu, &env, &[(36, 0, Input::Throw)]).unwrap();
        assert!(threw, "Q over a slot drops one and swings");
        assert_eq!(menu.slot(36).map(|s| s.count), Some(15));
        assert_eq!(sent.len(), 1);
        let (_, threw) = super::predict_clicks(&mut menu, &env, &[(36, 0, Input::Pickup)]).unwrap();
        assert!(!threw, "picking the stack up drops nothing");
        let (_, threw) =
            super::predict_clicks(&mut menu, &env, &[(-999, 1, Input::Pickup)]).unwrap();
        assert!(threw, "one of the carried stack outside the window");
    }

    #[test]
    fn placement_preview_checks_obstruction_without_changing_the_level() {
        use super::DryLevel;
        use vise_core::{position::BlockPos, random::LegacyRandom};
        use vise_world::{
            level::{
                Behaviours, LevelAccess, PlaceContext, PlaceOutcome, Ticks, data::load_registry,
            },
            registry::BlockStateId,
        };

        struct Level {
            air: BlockStateId,
            stone: BlockStateId,
            obstructed: bool,
            loaded: bool,
            ceiling: i32,
        }
        impl LevelAccess for Level {
            fn block_state(&self, pos: BlockPos) -> BlockStateId {
                if pos == BlockPos::new(0, 0, 0) {
                    self.stone
                } else {
                    self.air
                }
            }
            fn set_raw(&mut self, _: BlockPos, _: BlockStateId) -> Option<BlockStateId> {
                panic!("preview must not write to the live level");
            }
            fn is_loaded(&self, _: BlockPos) -> bool {
                self.loaded
            }
            fn max_y(&self) -> i32 {
                self.ceiling
            }
            fn is_unobstructed(&self, pos: BlockPos, _: BlockStateId) -> bool {
                assert_eq!(pos, BlockPos::new(0, 1, 0));
                !self.obstructed
            }
            fn random(&mut self) -> &mut LegacyRandom {
                panic!("preview must not advance the live level's random source");
            }
            fn game_time(&self) -> i64 {
                0
            }
        }

        let resources = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !resources.join("block-behaviours.json").exists() {
            return;
        }
        let registry = load_registry(&resources).unwrap();
        let behaviours = Behaviours::load(&registry, &resources).unwrap();
        let mut base = Level {
            air: registry.default_state("minecraft:air").unwrap(),
            stone: registry.default_state("minecraft:stone").unwrap(),
            obstructed: false,
            loaded: true,
            ceiling: 320,
        };
        for item in [
            "minecraft:stone",
            "minecraft:oak_slab",
            "minecraft:oak_door",
            "minecraft:twisting_vines",
        ] {
            let ctx = PlaceContext {
                clicked_pos: BlockPos::new(0, 0, 0),
                face: vise_core::geometry::Direction::Up,
                hit: vise_core::geometry::Vec3::new(0.5, 1.0, 0.5),
                inside: false,
                yaw: 0.0,
                pitch: 45.0,
                sneaking: false,
                item: item.into(),
                can_use_game_master_blocks: false,
            };
            for (obstructed, loaded, ceiling) in [
                (false, true, 320),
                (true, true, 320),
                (false, false, 320),
                (false, true, 1),
            ] {
                base.obstructed = obstructed;
                base.loaded = loaded;
                base.ceiling = ceiling;
                let valid = !obstructed && loaded && ceiling > 1;
                let mut preview = DryLevel::new(&base);
                let outcome = behaviours.place_block(&mut preview, &mut Ticks::default(), &ctx);
                assert_eq!(
                    matches!(outcome, PlaceOutcome::Placed { .. }),
                    valid,
                    "{item}, obstructed={obstructed}, loaded={loaded}, ceiling={ceiling}"
                );
                assert_eq!(preview.changed.is_empty(), !valid);
                assert_eq!(base.block_state(BlockPos::new(0, 1, 0)), base.air);
            }
        }
    }

    #[test]
    fn the_creative_sync_covers_inventory_menu_slots_5_to_45_once_each() {
        let stack = |item: &str| Some(vise_client::items::ItemStack::new(item, 1));
        let inventory: Vec<_> = (0..36)
            .map(|i| stack(&format!("minecraft:item_{i}")))
            .collect();
        let offhand = stack("minecraft:shield");
        let armor = [
            stack("minecraft:diamond_helmet"),
            stack("minecraft:diamond_chestplate"),
            stack("minecraft:diamond_leggings"),
            stack("minecraft:diamond_boots"),
        ];
        let stacks = super::creative_sync_stacks(&inventory, &offhand, &armor);
        assert_eq!(stacks.len(), super::CREATIVE_SYNCED);
        let mut slots: Vec<i16> = (0..stacks.len()).map(super::creative_sync_slot).collect();
        for (i, &slot) in slots.iter().enumerate() {
            let game_slot = Game::menu_slot("inventory", i32::from(slot)).unwrap();
            let expected = match game_slot {
                0..36 => &inventory[game_slot],
                504 => &offhand,
                500..504 => &armor[game_slot - 500],
                _ => panic!("entry {i} went to game slot {game_slot}"),
            };
            assert_eq!(&stacks[i], expected, "entry {i}, menu slot {slot}");
        }
        slots.sort();
        assert_eq!(slots, (5..=45).collect::<Vec<i16>>());
    }

    #[test]
    fn a_server_slot_update_leaves_an_unsent_creative_click_to_be_sent() {
        let stack = |item: &str| Some(vise_client::items::ItemStack::new(item, 1));
        let none = || vec![None; super::CREATIVE_SYNCED];
        let mut synced = none();
        let mut before = none();
        before[37] = stack("minecraft:diamond_helmet");
        let mut now = before.clone();
        now[0] = stack("minecraft:stick");
        super::mark_synced(&mut synced, &before, now.clone());
        assert_eq!(synced[0], now[0]);
        assert_eq!(
            synced[37], None,
            "the helmet still differs, so the next tick sends it"
        );
    }

    #[test]
    fn every_inventory_menu_slot_is_a_game_slot_and_45_is_the_off_hand() {
        assert_eq!(Game::menu_slot("inventory", 45), Some(504));
        let mut seen = std::collections::HashSet::new();
        for slot in 0..46 {
            let internal = Game::menu_slot("inventory", slot);
            assert!(internal.is_some_and(|i| seen.insert(i)), "slot {slot}");
        }
        assert_eq!(Game::menu_slot("inventory", 46), None);
    }

    #[test]
    fn remote_clicks_find_slots_past_the_old_large_chest_cutoff() {
        for (menu, internal, index) in [
            ("minecraft:generic_9x6", 18, 63),
            ("minecraft:generic_9x6", 19, 64),
            ("minecraft:generic_9x6", 0, 81),
            ("minecraft:generic_9x6", 8, 89),
            ("minecraft:generic_9x6", 353, 53),
            ("minecraft:generic_9x5", 8, 80),
            ("minecraft:generic_9x4", 8, 71),
            ("minecraft:generic_9x3", 8, 62),
            ("inventory", 504, 45),
        ] {
            assert_eq!(
                Game::menu_slot_index(menu, internal),
                Some(index),
                "{menu}, game slot {internal}"
            );
        }
        assert_eq!(Game::menu_slot_index("minecraft:generic_9x6", 354), None);
        assert_eq!(Game::menu_slot_index("minecraft:generic_9x6", 504), None);
        assert_eq!(Game::menu_slot_index("unknown", 0), None);
    }

    #[test]
    fn a_rocket_used_while_gliding_ends_the_click() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !root.join("items.json").exists() {
            eprintln!("skipping: no prepared resources in {}", root.display());
            return;
        }
        let registry = vise_world::level::data::load_registry(&root).unwrap();
        let facts = vise_client::blocks::BlockFacts::load(&root, &registry).unwrap();
        let items = vise_client::items::Items::load(
            &root,
            &registry,
            &facts,
            &vise_world::loot::PackOverlay::NONE,
        )
        .unwrap();
        let rocket = items.get("minecraft:firework_rocket").unwrap();
        assert!(super::use_ends_click(rocket, true), "gliding");
        assert!(!super::use_ends_click(rocket, false), "on foot");
        let shield = items.get("minecraft:shield").unwrap();
        let stick = items.get("minecraft:stick").unwrap();
        for gliding in [false, true] {
            assert!(super::use_ends_click(shield, gliding));
            assert!(!super::use_ends_click(stick, gliding));
        }
    }
}
