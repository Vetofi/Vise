use super::{Game, Screen};
use vise_client::{
    chat::{
        Chat, ChatAbilities, ChatAction, ChatScreen, ChatSource, ChatVisibility, Commands,
        SYSTEM_INDICATOR,
    },
    command::Registries,
    gamerules::GameRules,
    hud::HudBatch,
    screen_draw::Measure,
    text::{ClickEvent, Style, Text},
};
use vise_world::registry::{BlockRegistry, BlockStateId};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub(super) fn chat_tree(
    remote: &Option<super::remote::Remote>,
) -> &vise_client::command::Dispatcher {
    match remote.as_ref().and_then(|r| r.commands.as_ref()) {
        Some(tree) => tree,
        None => vise_client::command::vanilla::game_dispatcher(),
    }
}
use winit::{
    event::KeyEvent,
    keyboard::{KeyCode, PhysicalKey},
};

pub const PLAYER_NAME: &str = "Player";

#[derive(Clone, Copy, Debug)]
pub struct WorldClock {
    pub partial: f32,
    pub rate: f32,
    pub paused: bool,
}
impl Default for WorldClock {
    fn default() -> Self {
        Self {
            partial: 0.0,
            rate: 1.0,
            paused: false,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct TimeMarker {
    pub ticks: i64,
    pub period: Option<i64>,
    pub show: bool,
}
impl TimeMarker {
    pub fn occurs_at(&self, total: i64) -> bool {
        match self.period {
            Some(p) => self.ticks == total.rem_euclid(p),
            None => self.ticks == total,
        }
    }
    pub fn resolve(&self, total: i64) -> i64 {
        match self.period {
            Some(p) => {
                let d = self.ticks - total.rem_euclid(p);
                total + if d > 0 { d } else { p + d }
            }
            None => self.ticks,
        }
    }
}
pub struct ChatDraft {
    pub text: String,
    pub command: bool,
}
#[derive(Default)]
pub struct UiState {
    pub chat: Chat,
    pub chat_screen: Option<ChatScreen>,
    pub chat_draft: Option<ChatDraft>,
    pub chat_draft_shown: bool,
    pub chat_opened: Option<std::time::Instant>,
    pub in_bed_chat: bool,
    pub registries: Registries,
    pub rules: GameRules,
    pub clock: WorldClock,
    pub markers: BTreeMap<String, TimeMarker>,
    pub timelines: BTreeMap<String, Option<i64>>,
    pub difficulty: u8,
    pub hide_gui: bool,
    pub death_cause: Option<String>,
    pub screenshot_message: Option<std::path::PathBuf>,
    pub options: vise_client::options_ui::OptionsUi,
    pub vanilla_root: std::path::PathBuf,
    pub death_text: Option<Text>,
    pub died_at: Option<std::time::Instant>,
    pub death_confirm: Option<std::time::Instant>,
    pub death_time: u32,
}

impl UiState {
    pub fn new() -> Self {
        let mut ui = Self::default();
        if let Ok(dir) = std::env::current_dir() {
            ui.chat.load_history(&dir);
        }
        ui
    }
}

struct ResourceIds {
    recipes: Vec<String>,
    damage_types: Vec<String>,
    loot_tables: Vec<String>,
    templates: Vec<String>,
}
static RESOURCE_IDS: Mutex<Vec<(PathBuf, Arc<ResourceIds>)>> = Mutex::new(Vec::new());

fn resource_ids(root: &Path) -> Arc<ResourceIds> {
    let mut cache = RESOURCE_IDS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, ids)) = cache.iter().find(|(cached, _)| cached == root) {
        return ids.clone();
    }
    let data = root.join("data/minecraft");
    let ids = Arc::new(ResourceIds {
        recipes: ids_under(&data.join("recipe"), "json"),
        damage_types: ids_under(&data.join("damage_type"), "json"),
        loot_tables: ids_under(&data.join("loot_table"), "json"),
        templates: templates_under(&root.join("worldgen/structure")),
    });
    cache.push((root.to_owned(), ids.clone()));
    ids
}

type BlockProperties = BTreeMap<String, Vec<(String, Vec<String>)>>;
static BLOCK_PROPERTIES: Mutex<Vec<(usize, Arc<BlockProperties>)>> = Mutex::new(Vec::new());

fn block_properties(registry: &'static BlockRegistry) -> Arc<BlockProperties> {
    let key = std::ptr::from_ref(registry) as usize;
    let mut cache = BLOCK_PROPERTIES.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, properties)) = cache.iter().find(|(cached, _)| *cached == key) {
        return properties.clone();
    }
    let mut blocks: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
    for id in 0..registry.len() {
        let Some(state) = registry.state(BlockStateId(id as u32)) else {
            continue;
        };
        let props = blocks.entry(state.name.clone()).or_default();
        for (k, v) in &state.properties {
            let values = props.entry(k.clone()).or_default();
            if !values.contains(v) {
                values.push(v.clone());
            }
        }
    }
    let properties: Arc<BlockProperties> = Arc::new(
        blocks
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().collect()))
            .collect(),
    );
    cache.push((key, properties.clone()));
    properties
}

static COMMAND_EXPORTS: Mutex<Vec<(PathBuf, Option<Arc<serde_json::Value>>)>> =
    Mutex::new(Vec::new());

fn command_export(dir: &Path) -> Option<Arc<serde_json::Value>> {
    let mut cache = COMMAND_EXPORTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, export)) = cache.iter().find(|(cached, _)| cached == dir) {
        return export.clone();
    }
    let export = std::fs::read(dir.join("command-registries.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .map(Arc::new);
    cache.push((dir.to_owned(), export.clone()));
    export
}

fn ids_under(root: &Path, extension: &str) -> Vec<String> {
    ids_in(root, extension, "minecraft")
}

fn templates_under(root: &Path) -> Vec<String> {
    let Ok(namespaces) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = namespaces
        .flatten()
        .filter(|e| e.path().is_dir())
        .flat_map(|e| ids_in(&e.path(), "nbt", &e.file_name().to_string_lossy()))
        .collect();
    out.sort();
    out
}

fn ids_in(root: &Path, extension: &str, namespace: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            let is_dir = e
                .file_type()
                .is_ok_and(|t| t.is_dir() || (t.is_symlink() && path.is_dir()));
            if is_dir {
                stack.push(path);
            } else if path.extension().and_then(|x| x.to_str()) == Some(extension)
                && let Ok(rel) = path.strip_prefix(root)
            {
                out.push(format!(
                    "{namespace}:{}",
                    rel.with_extension("").to_string_lossy().replace('\\', "/")
                ));
            }
        }
    }
    out.sort();
    out
}

impl Game<'_> {
    pub(super) fn allow_commands(&self) -> bool {
        self.level.allow_commands == Some(true)
    }
    pub(super) fn set_allow_commands(&mut self, allow: bool) {
        self.level.allow_commands = Some(allow);
        self.ui.registries.permission = owner_permission(allow);
    }
    pub(super) fn as_console<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let level = std::mem::replace(&mut self.ui.registries.permission, 4);
        let out = f(self);
        self.ui.registries.permission = level;
        out
    }
    pub(super) fn build_registries(&mut self) {
        let permission = owner_permission(self.allow_commands());
        let reg = &mut self.ui.registries;
        reg.permission = permission;
        let ids = resource_ids(&self.options.resources);
        self.ui.registries.recipes = ids.recipes.clone();
        self.ui.registries.damage_types = ids.damage_types.clone();
        self.ui.registries.loot_tables = ids.loot_tables.clone();
        self.ui.registries.templates = ids.templates.clone();
        self.ui.registries.sounds = self.audio.event_names();
        self.ui.registries.attributes = [
            "armor",
            "armor_toughness",
            "attack_damage",
            "attack_knockback",
            "attack_speed",
            "block_break_speed",
            "block_interaction_range",
            "burning_time",
            "camera_distance",
            "explosion_knockback_resistance",
            "entity_interaction_range",
            "fall_damage_multiplier",
            "flying_speed",
            "follow_range",
            "gravity",
            "jump_strength",
            "knockback_resistance",
            "luck",
            "max_absorption",
            "max_health",
            "mining_efficiency",
            "movement_efficiency",
            "movement_speed",
            "oxygen_bonus",
            "safe_fall_distance",
            "scale",
            "sneaking_speed",
            "spawn_reinforcements",
            "step_height",
            "submerged_mining_speed",
            "sweeping_damage_ratio",
            "tempt_range",
            "water_movement_efficiency",
        ]
        .iter()
        .map(|a| format!("minecraft:{a}"))
        .collect();
        let reg = &mut self.ui.registries;
        let mut items: Vec<String> = self
            .res
            .items()
            .order
            .iter()
            .map(|s| s.to_string())
            .collect();
        items.sort();
        reg.items = items;
        reg.item_tags = self
            .res
            .items()
            .item_tags
            .keys()
            .map(|k| vise_client::command::args::normalize(k))
            .collect();
        reg.item_tags.sort();
        reg.block_tags = self
            .res
            .items()
            .block_tags
            .keys()
            .map(|k| vise_client::command::args::normalize(k))
            .collect();
        reg.block_tags.sort();
        reg.blocks = BlockProperties::clone(&block_properties(self.pack.assets.registry));
        let mut summonable: Vec<String> = self
            .res
            .mob_types()
            .keys()
            .map(|k| vise_client::command::args::normalize(k))
            .collect();
        summonable.extend(
            [
                "minecraft:item",
                "minecraft:experience_orb",
                "minecraft:falling_block",
                "minecraft:arrow",
                "minecraft:lightning_bolt",
                "minecraft:minecart",
                "minecraft:chest_minecart",
                "minecraft:hopper_minecart",
                "minecraft:furnace_minecart",
                "minecraft:tnt_minecart",
            ]
            .map(String::from),
        );
        summonable.sort();
        summonable.dedup();
        reg.summonable = summonable;
        let root = self.options.resources.clone();
        let mut types: Vec<String> = serde_json::from_slice::<serde_json::Value>(
            &std::fs::read(root.join("entity-types.json")).unwrap_or_default(),
        )
        .ok()
        .and_then(|v| {
            v.as_object().map(|o| {
                o.keys()
                    .map(|k| vise_client::command::args::normalize(k))
                    .collect()
            })
        })
        .unwrap_or_default();
        if types.is_empty() {
            types = reg.summonable.clone();
            types.push("minecraft:player".into());
        }
        types.sort();
        reg.entity_types = types;
        let list = |dir: &str| -> Vec<String> {
            let mut out: Vec<String> = std::fs::read_dir(root.join(dir))
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|e| {
                    e.file_name()
                        .to_str()?
                        .strip_suffix(".json")
                        .map(|n| format!("minecraft:{n}"))
                })
                .collect();
            out.sort();
            out
        };
        reg.effects = vise_client::brewing::EFFECTS
            .iter()
            .map(|e| format!("minecraft:{}", e.id))
            .collect();
        reg.effects.sort();
        reg.enchantments = list("data/minecraft/enchantment");
        reg.structures = list("data/minecraft/worldgen/structure");
        reg.biomes = list("data/minecraft/worldgen/biome");
        reg.clocks = list("data/minecraft/world_clock");
        reg.dimensions = vec![
            "minecraft:overworld".into(),
            "minecraft:the_nether".into(),
            "minecraft:the_end".into(),
        ];
        reg.players = vec![PLAYER_NAME.into()];
        self.ui.markers.clear();
        self.ui.timelines.clear();
        for name in list("data/minecraft/timeline") {
            let path = root.join(format!(
                "data/minecraft/timeline/{}.json",
                name.trim_start_matches("minecraft:")
            ));
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(
                &std::fs::read(path).unwrap_or_default(),
            ) else {
                continue;
            };
            if v["clock"].as_str() != Some("minecraft:overworld") {
                continue;
            }
            let period = v["period_ticks"].as_i64();
            self.ui.timelines.insert(name.clone(), period);
            for (marker, def) in v["time_markers"].as_object().into_iter().flatten() {
                let (ticks, show) = match def {
                    serde_json::Value::Number(n) => (n.as_i64().unwrap_or(0), false),
                    other => (
                        other["ticks"].as_i64().unwrap_or(0),
                        other["show_in_commands"].as_bool().unwrap_or(false),
                    ),
                };
                self.ui.markers.insert(
                    marker.clone(),
                    TimeMarker {
                        ticks,
                        period,
                        show,
                    },
                );
            }
        }
        for dir in [&self.options.resources, &self.ui.vanilla_root] {
            if let Some(export) = command_export(dir) {
                self.ui.registries.fill_from_export(&export);
                break;
            }
        }
        let reg = &mut self.ui.registries;
        reg.timelines = self.ui.timelines.keys().cloned().collect();
        reg.time_markers = self
            .ui
            .markers
            .iter()
            .filter(|(_, m)| m.show)
            .map(|(k, _)| k.clone())
            .collect();
    }
    fn refresh_dynamic_registries(&mut self) {
        let hit = if self.pick_entity_hit().is_some() {
            None
        } else {
            self.pick()
        };
        self.ui.registries.target_block = hit.as_ref().map(|h| h.pos);
        self.ui.registries.target_point =
            hit.map(|h| h.point.map(vise_client::java_text::format_fixed2));
        self.ui.registries.selected_entities = self.crosshair_entity_uuid().into_iter().collect();
        if let Some(names) = self.remote_online_names() {
            self.ui.registries.players = names;
        }
        self.ui.registries.custom_completions = self.remote_custom_completions();
    }
    pub(super) fn open_chat_method(&mut self, prefix: &str) {
        let command = prefix == "/";
        let draft = self
            .ui
            .chat_draft
            .as_ref()
            .filter(|d| !command || d.command)
            .map(|d| d.text.clone());
        self.open_chat(draft.as_deref().unwrap_or(prefix));
        self.ui.chat_draft_shown = draft.is_some() && self.ui.chat_screen.is_some();
    }
    pub(super) fn open_chat(&mut self, initial: &str) {
        if self.player.is_none() {
            return;
        }
        self.ui.chat_draft_shown = false;
        self.refresh_dynamic_registries();
        let size = self
            .renderer
            .as_ref()
            .map_or([1280, 720], |r| [r.config.width, r.config.height]);
        let b = HudBatch::new(size);
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        let mut screen = ChatScreen::new(
            &m,
            &c,
            initial,
            self.ui.chat.recent.len(),
            b.width as i32,
            b.height as i32,
            self.chat_abilities(),
        );
        screen.auto_suggestions = self.settings.auto_suggestions;
        self.ui.chat_screen = Some(screen);
        self.ui.chat_opened = Some(std::time::Instant::now());
        self.screen = Screen::Chat;
        self.keys.clear();
        self.left = false;
        self.right = false;
        self.capture(false);
    }
    pub(super) fn close_chat(&mut self) {
        if self.ui.in_bed_chat && self.sleeping.is_some() {
            self.wake_up();
            return;
        }
        let text = self
            .ui
            .chat_screen
            .as_ref()
            .map_or_else(String::new, |s| s.value().to_owned());
        self.keep_chat_draft(&text);
        self.ui.chat_draft_shown = false;
        self.ui.chat_screen = None;
        self.ui.chat.reset_scroll();
        if self.screen == Screen::Chat {
            self.screen = Screen::Playing;
            self.capture(true);
        }
    }
    fn keep_chat_draft(&mut self, text: &str) {
        let unchanged =
            self.ui.chat_draft_shown && self.ui.chat_draft.as_ref().is_some_and(|d| d.text == text);
        if !self.settings.file.bool("saveChatDrafts") || text.trim().is_empty() {
            self.ui.chat_draft = None;
        } else if !unchanged {
            self.ui.chat_draft = Some(ChatDraft {
                text: text.to_owned(),
                command: text.starts_with('/'),
            });
        }
    }
    pub(super) fn tick_in_bed_chat(&mut self) {
        if self.screen != Screen::Chat {
            self.ui.in_bed_chat = false;
        }
        if self.ui.in_bed_chat {
            if self.sleeping.is_none() {
                self.ui.in_bed_chat = false;
                let text = self
                    .ui
                    .chat_screen
                    .as_ref()
                    .map_or_else(String::new, |s| s.value().to_owned());
                self.close_chat();
                if !text.is_empty() {
                    self.open_chat(&text);
                }
            }
        } else if self.sleeping.is_some() && self.screen == Screen::Playing {
            self.open_chat("");
            self.ui.in_bed_chat = self.screen == Screen::Chat;
        }
    }
    fn leave_bed_button(width: f32, height: f32) -> (i32, i32, i32, i32) {
        (width as i32 / 2 - 100, height as i32 - 40, 200, 20)
    }
    pub(super) fn chat_abilities(&self) -> ChatAbilities {
        ChatAbilities::new(ChatVisibility::from_option(self.settings.chat_visibility))
    }
    pub(super) fn system_message(&mut self, text: Text) {
        if let Some(capture) = self.chat_capture.as_mut() {
            capture.push(text);
            return;
        }
        if self.remote.is_some() {
            self.chat_line(text, ChatSource::Server);
            return;
        }
        self.local_chat_line(text, Some(SYSTEM_INDICATOR), ChatSource::Server);
    }
    pub(super) fn player_message(&mut self, text: Text) {
        if let Some(capture) = self.chat_capture.as_mut() {
            capture.push(text);
            return;
        }
        if self.remote.is_none() {
            self.local_chat_line(text, None, ChatSource::Player);
        }
    }
    fn local_chat_line(&mut self, text: Text, indicator: Option<u32>, source: ChatSource) {
        if !self.ui.chat.shows(source) {
            return;
        }
        let focused = self.screen == Screen::Chat;
        self.ui.chat.add(
            &self.gui.atlas,
            text,
            indicator,
            source,
            self.ticks,
            focused,
        );
    }
    pub(super) fn chat_key(&mut self, event: &KeyEvent) {
        if let Some(shortcut) = self.edit_shortcut(event) {
            let m = Measure(&self.gui.atlas.font);
            let c = Commands {
                dispatcher: chat_tree(&self.remote),
                registries: &self.ui.registries,
            };
            if let Some(screen) = self.ui.chat_screen.as_mut() {
                screen.shortcut(&m, &c, shortcut, &mut self.clipboard);
            }
            return;
        }
        let PhysicalKey::Code(key) = event.physical_key else {
            return;
        };
        self.chat_key_code(key, event.text.as_deref());
    }
    pub(super) fn chat_key_code(&mut self, key: KeyCode, typed_text: Option<&str>) {
        let (left, right) = if cfg!(target_os = "macos") {
            (KeyCode::SuperLeft, KeyCode::SuperRight)
        } else {
            (KeyCode::ControlLeft, KeyCode::ControlRight)
        };
        let ctrl = self.keys.contains(&left) || self.keys.contains(&right);
        let shift =
            self.keys.contains(&KeyCode::ShiftLeft) || self.keys.contains(&KeyCode::ShiftRight);
        match key {
            KeyCode::PageUp | KeyCode::PageDown
                if !self
                    .ui
                    .chat_screen
                    .as_ref()
                    .is_some_and(|s| s.suggestions_visible()) =>
            {
                let n = self.ui.chat.lines_per_page(true) as i32 - 1;
                self.ui
                    .chat
                    .scroll_by(if key == KeyCode::PageUp { n } else { -n }, true);
                return;
            }
            _ => {}
        }
        let Some(mut screen) = self.ui.chat_screen.take() else {
            return;
        };
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        match screen.key(&m, &c, &self.ui.chat.recent, key, shift, ctrl) {
            ChatAction::Close => {
                self.ui.chat_screen = Some(screen);
                self.close_chat();
            }
            ChatAction::Submit(message) => {
                if self.ui.in_bed_chat && self.sleeping.is_some() {
                    screen.suggest_command(&m, &c, "");
                    self.ui.chat.reset_scroll();
                    self.ui.chat_screen = Some(screen);
                } else {
                    self.close_chat();
                }
                if !message.is_empty() {
                    self.handle_chat_input(&message);
                }
            }
            ChatAction::None => {
                if let Some(text) = typed_text
                    && self.types_text()
                {
                    screen.type_text(&m, &c, text);
                }
                self.ui.chat_screen = Some(screen);
            }
        }
    }
    pub(super) fn chat_server_suggestions(
        &mut self,
        id: i32,
        range: (usize, usize),
        list: Vec<(String, Option<String>)>,
    ) {
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        if let Some(screen) = self.ui.chat_screen.as_mut() {
            screen.server_suggestions(&m, &c, id, range, list);
        }
    }
    pub(super) fn chat_type(&mut self, text: &str) {
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        if let Some(screen) = self.ui.chat_screen.as_mut() {
            screen.type_text(&m, &c, text);
        }
    }
    pub(super) fn handle_chat_input(&mut self, message: &str) {
        if !self.chat_abilities().allows(message) {
            return;
        }
        self.ui.chat.add_recent(message);
        if self.remote.is_some() {
            if let Some(command) = message.strip_prefix('/')
                && self.client_mod_command(command)
            {
                return;
            }
            self.send_chat(message);
            return;
        }
        if let Some(command) = message.strip_prefix('/') {
            if !self.client_mod_command(command) {
                self.run_command(command);
            }
        } else {
            let line = Text::translate(
                "chat.type.text",
                &[self.player_display_name(), Text::literal(message)],
            );
            let focused = self.screen == Screen::Chat;
            self.ui.chat.add(
                &self.gui.atlas,
                line,
                None,
                ChatSource::Player,
                self.ticks,
                focused,
            );
            self.lan_host_chat(message);
        }
    }
    pub(super) fn chat_scroll(&mut self, amount: f32) {
        let cursor = self.cursor;
        let height = self.renderer.as_ref().map_or(720.0, |r| {
            HudBatch::new([r.config.width, r.config.height]).height
        });
        let _ = height;
        let mouse = (cursor.0.floor() as i32, cursor.1.floor() as i32);
        if let Some(screen) = self.ui.chat_screen.as_mut()
            && screen.scroll(mouse, amount as f64)
        {
            return;
        }
        let shift = self.has_shift_down();
        let amount = amount.clamp(-1.0, 1.0) * if shift { 1.0 } else { 7.0 };
        self.ui.chat.scroll_by(amount as i32, true);
    }
    pub(super) fn chat_click(&mut self) {
        let cursor = self.cursor;
        let height = self.renderer.as_ref().map_or(720.0, |r| {
            HudBatch::new([r.config.width, r.config.height]).height
        });
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        if self.ui.in_bed_chat && self.sleeping.is_some() {
            let width = self.renderer.as_ref().map_or(1280.0, |r| {
                HudBatch::new([r.config.width, r.config.height]).width
            });
            let (x, y, w, h) = Self::leave_bed_button(width, height);
            if vise_client::screen_draw::inside((cursor.0 as i32, cursor.1 as i32), x, y, w, h) {
                self.wake_up();
                return;
            }
        }
        if let Some(screen) = self.ui.chat_screen.as_mut() {
            if screen.click(&m, &c, cursor.0 as f64, cursor.1 as f64) {
                return;
            }
        } else {
            return;
        }
        let Some(style) = self.ui.chat.style_at(
            &self.gui.atlas.font,
            height as i32,
            cursor.0 as f64,
            cursor.1 as f64,
        ) else {
            return;
        };
        if self.has_shift_down() {
            if let Some(insertion) = style.insertion
                && let Some(screen) = self.ui.chat_screen.as_mut()
            {
                screen.type_text(&m, &c, &insertion);
            }
            return;
        }
        match style.click {
            Some(ClickEvent::CopyToClipboard(value)) => {
                use vise_client::clipboard::Clipboard;
                self.clipboard.set(&value);
            }
            Some(ClickEvent::OpenFile(path)) => {
                super::packs::open_folder(std::path::Path::new(&path));
            }
            Some(ClickEvent::OpenUrl(url)) => self.click_url(url),
            Some(ClickEvent::SuggestCommand(command)) => {
                if let Some(screen) = self.ui.chat_screen.as_mut() {
                    screen.suggest_command(&m, &c, &command);
                }
            }
            Some(ClickEvent::RunCommand(command)) => self.click_command(&command),
            Some(ClickEvent::ShowDialog(dialog)) => self.show_dialog_ref(&dialog),
            Some(ClickEvent::ChangePage(_)) | None => {}
        }
    }
    pub(super) fn click_url(&mut self, url: String) {
        if !self.settings.chat_links {
            return;
        }
        if self.settings.file.bool("chatLinksPrompt") {
            self.link_prompt = Some(url);
        } else {
            super::packs::open_uri(&url);
        }
    }
    fn link_prompt_layout(
        &self,
        url: &str,
        size: (f32, f32),
    ) -> vise_client::confirm_screen::Choice {
        let warning = Text::translate("chat.link.warning", &[]).with_color(0xFF_CCCC);
        let message = Text::literal(url)
            .append(Text::literal("\n"))
            .append(warning);
        vise_client::confirm_screen::Choice::new(
            &self.gui.atlas.font,
            size,
            &Text::translate("chat.link.confirm", &[]),
            &message,
            3,
            100.0,
        )
    }
    pub(super) fn draw_link_prompt(&self, batch: &mut HudBatch) {
        let Some(url) = &self.link_prompt else {
            return;
        };
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        vise_client::ui::menu_background(batch, atlas, 0.0, 0.0, w, h, true);
        let labels = [
            Text::translate("gui.yes", &[]),
            Text::translate("chat.copy", &[]),
            Text::translate("gui.no", &[]),
        ];
        self.link_prompt_layout(url, (w, h))
            .draw(batch, atlas, self.cursor, &labels);
    }
    pub(super) fn answer_link_prompt(&mut self, yes: bool) {
        if let Some(url) = self.link_prompt.take()
            && yes
        {
            super::packs::open_uri(&url);
        }
    }
    pub(super) fn link_prompt_click(&mut self) {
        let Some(url) = self.link_prompt.clone() else {
            return;
        };
        let pressed = self
            .link_prompt_layout(&url, self.gui_size())
            .pressed_at(self.cursor);
        if let Some(button) = pressed {
            self.audio.play_button_click();
            if button == LINK_COPY {
                use vise_client::clipboard::Clipboard;
                self.clipboard.set(&url);
            }
            self.answer_link_prompt(button == LINK_YES);
        }
    }
    pub(super) fn click_command(&mut self, command: &str) {
        let command = command.strip_prefix('/').unwrap_or(command);
        if self.client_mod_command(command) {
            return;
        }
        if self.remote.is_some() {
            self.send_chat(&format!("/{command}"));
        } else {
            self.run_command(command);
        }
    }
    pub(super) fn draw_chat(&mut self, batch: &mut HudBatch) {
        let focused = self.screen == Screen::Chat;
        if self.ui.hide_gui && !focused {
            return;
        }
        self.ui
            .chat
            .draw(batch, &self.gui.atlas, self.ticks, focused);
    }
    fn entity_tooltip_lines(entity: &serde_json::Value) -> Vec<(Text, u32)> {
        let mut lines = Vec::new();
        if let Some(name) = entity.get("name") {
            let name = vise_client::component::to_text(name, &mut |s| Text::literal(s));
            lines.push((name, u32::MAX));
        }
        let id = entity
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let kind = Text::translate(&format!("entity.{}", id.replace(':', ".")), &[]);
        lines.push((
            Text::translate("gui.entity_tooltip.type", &[kind]),
            u32::MAX,
        ));
        if let Some(uuid) = entity.get("uuid").and_then(Self::entity_uuid) {
            lines.push((Text::literal(uuid), u32::MAX));
        }
        lines
    }
    fn entity_uuid(v: &serde_json::Value) -> Option<String> {
        if let Some(s) = v.as_str() {
            return Some(s.to_owned());
        }
        let words = v.as_array().filter(|w| w.len() == 4)?;
        let mut bits = 0u128;
        for w in words {
            bits = (bits << 32) | u128::from(w.as_i64()? as u32);
        }
        Some(vise_protocol::crypto::uuid_string(bits))
    }
    pub(super) fn draw_chat_screen(&mut self, batch: &mut HudBatch) {
        let Some(mut screen) = self.ui.chat_screen.take() else {
            return;
        };
        let m = Measure(&self.gui.atlas.font);
        let c = Commands {
            dispatcher: chat_tree(&self.remote),
            registries: &self.ui.registries,
        };
        screen.resize(&m, &c, batch.width as i32, batch.height as i32);
        let background = if self.settings.file.bool("backgroundForChatOnly") {
            i32::MIN
        } else {
            self.settings.text_background(0.0) as i32
        };
        let mouse = (self.cursor.0.floor() as i32, self.cursor.1.floor() as i32);
        let blink = self
            .ui
            .chat_opened
            .map_or(0, |t| t.elapsed().as_millis() as u64);
        let draws = screen.draw(&m, mouse, blink, background);
        vise_client::screen_draw::paint(batch, &self.gui.atlas, &draws);
        self.ui.chat_screen = Some(screen);
        if self.ui.in_bed_chat && self.sleeping.is_some() {
            let (x, y, w, h) = Self::leave_bed_button(batch.width, batch.height);
            let label = Text::literal(vise_client::lang::get_or(
                "multiplayer.stopSleeping",
                "Leave Bed",
            ));
            let mut button = Vec::new();
            vise_client::screen_draw::button(
                &mut button,
                &m,
                &label,
                x,
                y,
                w,
                h,
                true,
                vise_client::screen_draw::inside(mouse, x, y, w, h),
            );
            vise_client::screen_draw::paint(batch, &self.gui.atlas, &button);
        }
        let height = batch.height as i32;
        let (cx, cy) = (f64::from(self.cursor.0), f64::from(self.cursor.1));
        let wrap = (batch.width as i32 / 2).max(200);
        if let Some(style) = self.ui.chat.style_at(&self.gui.atlas.font, height, cx, cy) {
            if style.insertion.is_some() && self.has_shift_down() {
                vise_client::widgets::request_cursor(
                    vise_client::widgets::Cursor::PointingHand,
                );
            }
            if let Some(hover) = style.hover {
                let lines = self.wrapped_rows(&hover, wrap);
                self.draw_tooltip_lines(batch, &lines, self.cursor.0, self.cursor.1);
            } else if let Some(stack) = style
                .hover_item
                .as_deref()
                .and_then(vise_client::item_data::stack_from_hover)
                .filter(|s| self.items().get(&s.item).is_some())
            {
                self.draw_stack_tooltip(batch, &stack, self.cursor.0, self.cursor.1);
            } else if let Some(entity) = style
                .hover_entity
                .as_deref()
                .filter(|_| self.settings.file.bool("advancedItemTooltips"))
            {
                let lines = Self::entity_tooltip_lines(entity);
                self.draw_tooltip_lines(batch, &lines, self.cursor.0, self.cursor.1);
            }
        } else if self.ui.chat.indicator_at(height, cx, cy) {
            let tag = Text::translate("chat.tag.system_single_player", &[]);
            let lines = self.wrapped_rows(&tag, 210);
            self.draw_tooltip_lines(batch, &lines, self.cursor.0, self.cursor.1);
        }
    }
    pub(super) fn wrapped_rows(&self, text: &Text, width: i32) -> Vec<(Text, u32)> {
        let split = self
            .gui
            .atlas
            .font
            .split(&text.resolve_legacy(), width as f32);
        split.into_iter().map(|row| (row, u32::MAX)).collect()
    }
    pub(super) fn advance_clock(&mut self) {
        if !self.ui.rules.bool("advance_time") || self.ui.clock.paused {
            return;
        }
        let c = &mut self.ui.clock;
        c.partial += c.rate;
        let whole = c.partial.floor();
        c.partial -= whole;
        self.world.day_time += whole as i64;
    }
    pub(super) fn network_clock_rate(&self) -> f32 {
        if self.ui.rules.bool("advance_time") && !self.ui.clock.paused {
            self.ui.clock.rate
        } else {
            0.0
        }
    }
    pub(super) fn level_rules(&self) -> vise_world::level::GameRules {
        let r = &self.ui.rules;
        vise_world::level::GameRules {
            water_source_conversion: r.bool("water_source_conversion"),
            lava_source_conversion: r.bool("lava_source_conversion"),
            random_tick_speed: r.int("random_tick_speed"),
            block_drops: r.bool("block_drops"),
            mob_griefing: r.bool("mob_griefing"),
            max_snow_accumulation_height: r.int("max_snow_accumulation_height"),
            spawn_mobs: r.bool("spawn_mobs"),
            spawn_monsters: r.bool("spawn_monsters"),
            fire_spread_radius_around_player: r.int("fire_spread_radius_around_player"),
            spread_vines: r.bool("spread_vines"),
        }
    }
    pub(super) fn sync_rules(&mut self) {
        self.world.health_rules.natural_regeneration =
            self.ui.rules.bool("natural_health_regeneration");
        self.world.health_rules.drowning_damage = self.ui.rules.bool("drowning_damage");
        self.world.health_rules.fall_damage = self.ui.rules.bool("fall_damage");
        self.world.health_rules.fire_damage = self.ui.rules.bool("fire_damage");
        self.world.health_rules.freeze_damage = self.ui.rules.bool("freeze_damage");
        use vise_world::food::Difficulty;
        self.world.health_rules.difficulty = match self.ui.difficulty {
            0 => Difficulty::Peaceful,
            1 => Difficulty::Easy,
            3 => Difficulty::Hard,
            _ => Difficulty::Normal,
        };
    }
    pub(super) fn load_world_state(&mut self) {
        self.ui.rules = GameRules::load(&self.world_root);
        self.ui.clock = WorldClock::default();
        self.ui.difficulty = self.level.difficulty_id();
        if let Ok(bytes) = std::fs::read(self.world_root.join("data/minecraft/world_clocks.dat"))
            && let Ok(raw) = vise_world::compression::Compression::Gzip.decode(&bytes, 1 << 20)
            && let Ok((root, _)) =
                vise_world::nbt::read_named(&raw, vise_world::nbt::Limits::default())
            && let Some(c) = root
                .tag
                .get("data")
                .and_then(|d| d.get("minecraft:overworld"))
        {
            use vise_world::nbt::Tag;
            if let Some(Tag::Float(r)) = c.get("rate") {
                self.ui.clock.rate = f32::from_bits(*r);
            }
            if let Some(Tag::Float(p)) = c.get("partial_tick") {
                self.ui.clock.partial = f32::from_bits(*p);
            }
            if let Some(Tag::Byte(p)) = c.get("paused") {
                self.ui.clock.paused = *p != 0;
            }
        }
        self.ui.chat.clear();
        self.build_registries();
        self.sync_rules();
    }
    pub(super) fn save_world_state(&mut self) {
        if self.world_root.as_os_str().is_empty() {
            return;
        }
        use vise_world::nbt::Tag;
        let c = self.ui.clock;
        let path = self.world_root.join("data/minecraft/world_clocks.dat");
        let existing = vise_client::save::read_dat(&path);
        let mut data = match existing.as_ref().and_then(|r| r.get("data")) {
            Some(d @ Tag::Compound(_)) => d.clone(),
            _ => Tag::Compound(Default::default()),
        };
        let mut clock = match data.get("minecraft:overworld") {
            Some(c @ Tag::Compound(_)) => c.clone(),
            _ => Tag::Compound(Default::default()),
        };
        if let Tag::Compound(map) = &mut clock {
            map.insert("total_ticks".into(), Tag::Long(self.world.day_time));
            for (key, value, default) in [
                (
                    "partial_tick",
                    Tag::Float(c.partial.to_bits()),
                    c.partial == 0.0,
                ),
                ("rate", Tag::Float(c.rate.to_bits()), c.rate == 1.0),
                ("paused", Tag::Byte(i8::from(c.paused)), !c.paused),
            ] {
                if default {
                    map.remove(&key.into());
                } else {
                    map.insert(key.into(), value);
                }
            }
        }
        if let Tag::Compound(map) = &mut data {
            map.insert("minecraft:overworld".into(), clock);
        }
        let root = Tag::compound([
            ("data", data),
            ("DataVersion", Tag::Int(vise_world::DATA_VERSION)),
        ]);
        if let Err(e) = vise_client::save::write_dat(&path, root) {
            log::warn!("the world clocks were not written: {e}");
        }
    }
    pub(super) fn death_message(&mut self) {
        let cause = self.ui.death_cause.take();
        self.ui.died_at = Some(std::time::Instant::now());
        self.ui.death_text = None;
        self.ui.death_confirm = None;
        if !self.ui.rules.bool("show_death_messages") {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let below_world = p.position.y < f64::from(self.world.bottom_y() - 64);
        let name = self.player_display_text();
        let message = match cause {
            Some(key) => Text::translate(&key, &[name]),
            None if below_world => Text::translate("death.attack.outOfWorld", &[name]),
            None => self.combat_death_message(),
        };
        self.ui.death_text = Some(message.clone());
        let board = &self.score.board;
        if let Some(host) = &self.lan {
            host.hub.system_message_to(
                vise_client::lan::data::component_nbt(&message.component()),
                |other| {
                    vise_client::scoreboard::death_message_reaches(board, PLAYER_NAME, other)
                },
            );
        }
        if vise_client::scoreboard::death_message_reaches(board, PLAYER_NAME, PLAYER_NAME) {
            self.system_message(message);
        }
    }
    pub(super) fn toggle_hide_gui(&mut self) {
        self.ui.hide_gui = !self.ui.hide_gui;
    }
    pub(super) fn take_screenshot(&mut self) {
        let dir = std::env::current_dir()
            .unwrap_or_default()
            .join("screenshots");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        let name = screenshot_name(&dir);
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        let path = dir.join(name);
        renderer.screenshot = Some(vise_client::gfx::ScreenshotRequest {
            path: path.clone(),
            size: None,
        });
        self.ui.screenshot_message = Some(path);
    }
    pub(super) fn screenshot_shortcut(&mut self, key: KeyCode, repeat: bool) -> bool {
        if repeat || self.debug_keys.modifier_down {
            return false;
        }
        if vise_client::keybinds::key_name(key) != Some(self.settings.keys.get("key.screenshot"))
        {
            return false;
        }
        self.take_screenshot();
        true
    }
    pub(super) fn finish_screenshot(&mut self, result: &std::result::Result<[u32; 2], String>) {
        let Some(path) = self.ui.screenshot_message.take() else {
            return;
        };
        let message = match result {
            Ok(_) => {
                log::info!("Saved screenshot to {}", path.display());
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                let link = Text::styled(
                    name,
                    Style {
                        underlined: Some(true),
                        click: Some(ClickEvent::OpenFile(path.to_string_lossy().into_owned())),
                        ..Style::default()
                    },
                );
                Text::translate("screenshot.success", &[link])
            }
            Err(e) => Text::translate("screenshot.failure", &[Text::literal(e.clone())]),
        };
        let focused = self.screen == Screen::Chat;
        self.ui.chat.add(
            &self.gui.atlas,
            message,
            None,
            ChatSource::Client,
            self.ticks,
            focused,
        );
    }
}
pub fn screenshot_name(dir: &std::path::Path) -> String {
    let base = chrono::Local::now().format("%Y-%m-%d_%H.%M.%S").to_string();
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{base}.png")
        } else {
            format!("{base}_{n}.png")
        };
        if !dir.join(&name).exists() {
            return name;
        }
        n += 1;
    }
}

fn owner_permission(allow_commands: bool) -> u8 {
    if allow_commands { 4 } else { 0 }
}

const LINK_YES: usize = 0;
const LINK_COPY: usize = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_ids_are_listed_once_per_root() {
        let root = std::env::temp_dir().join(format!("vise-ids-{}", std::process::id()));
        let recipe = root.join("data/minecraft/recipe");
        std::fs::create_dir_all(recipe.join("sub")).unwrap();
        std::fs::write(recipe.join("b.json"), "{}").unwrap();
        std::fs::write(recipe.join("sub/a.json"), "{}").unwrap();
        std::fs::write(recipe.join("notes.txt"), "").unwrap();
        let first = resource_ids(&root);
        assert_eq!(first.recipes, ["minecraft:b", "minecraft:sub/a"]);
        assert!(first.loot_tables.is_empty());
        std::fs::write(recipe.join("c.json"), "{}").unwrap();
        assert!(Arc::ptr_eq(&first, &resource_ids(&root)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn command_exports_are_read_once_per_folder() {
        let dir = std::env::temp_dir().join(format!("vise-export-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("command-registries.json"), r#"{"effects":["a"]}"#).unwrap();
        let first = command_export(&dir).unwrap();
        assert_eq!(first["effects"][0], "a");
        std::fs::write(dir.join("command-registries.json"), "{}").unwrap();
        assert!(Arc::ptr_eq(&first, &command_export(&dir).unwrap()));
        assert!(command_export(&dir.join("missing")).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
