use super::native_lifecycle::{NativeLifecycleContext, ServerTickPhase, WorldPhase};
use super::{
    Game, GameLevel, LevelDimension, Screen,
    execution_role::ExecutionRole,
    native_panel_recipes::RecipePage,
    native_server::{NativeCommand, NativeServerPlayer},
};
use vise_client::{
    block_entities::Pos,
    chat::ChatSource,
    containers::{BlockEntity, MOD_MENUS, ModBlockEntity, ModContainer},
    extras::Corner,
    extras_hud::Corners,
    fast_hash::FxHashMap,
    hud::{HudBatch, rgba},
    items::ItemStack,
    keybinds,
    mod_list::{self, Installed, NativePage},
    particles::{ParticleOptions, ParticleType},
    text::Text,
};
use vise_core::position::BlockPos;
use vise_world::{
    level::{BlockId, flags},
    nbt::Compound,
    registry::BlockStateId,
};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use winit::{event::MouseButton, keyboard::KeyCode};

#[allow(dead_code)]
pub(crate) const NATIVE_HOOKS_API: u32 = 20;

impl GameLevel<'_> {
    #[inline(always)]
    pub(super) fn native_block_changed(
        &self,
        dimension: LevelDimension,
        pos: BlockPos,
        old: BlockStateId,
        new: BlockStateId,
    ) {
        super::mod_api::block_changed(dimension, pos, old, new);
    }

    #[inline(always)]
    pub(super) fn native_neighbor_changed(
        &self,
        dimension: LevelDimension,
        pos: BlockPos,
        state: BlockStateId,
        changed: BlockId,
        moved: bool,
    ) {
        super::mod_api::neighbor_changed(dimension, pos, state, changed, moved);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeZoom {
    pub(crate) factor: f32,
    pub(crate) smooth: bool,
    pub(crate) turn: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct NativeMinimap {
    pub(crate) corner: Corner,
    pub(crate) size: f32,
    pub(crate) blocks_per_pixel: f32,
    pub(crate) rotate: bool,
    pub(crate) round: bool,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativePlayer {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) z: f64,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
    pub(crate) dimension: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeMod {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) keys: Vec<[String; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeStack {
    pub(crate) item: String,
    pub(crate) count: u32,
    pub(crate) components: String,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeRecipeLayout {
    Grid,
    Cooking,
    Single,
    Smithing,
    Brewing,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeRecipeView {
    pub(crate) layout: NativeRecipeLayout,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) slots: Vec<Vec<NativeStack>>,
    pub(crate) results: Vec<NativeStack>,
    pub(crate) note: String,
    pub(crate) action: Option<bool>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NativePanelEvent {
    Clicked { index: u32, right: bool },
    Search(String),
    Key { key: String, stack: NativeStack },
    SlotClicked {
        recipe: u32,
        slot: u32,
        stack: NativeStack,
        right: bool,
    },
    Action(u32),
    Page(u32),
    Back,
    Tab(u32),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeRecipeStack {
    pub(crate) item: u32,
    pub(crate) count: u32,
    pub(crate) components: String,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NativeRecipeKind {
    CraftingShaped,
    CraftingShapeless,
    Smelting,
    Blasting,
    Smoking,
    Campfire,
    Stonecutting,
    Smithing,
    Brewing,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeRecipe {
    pub(crate) id: String,
    pub(crate) kind: NativeRecipeKind,
    pub(crate) category: String,
    pub(crate) group: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) slots: Vec<Vec<NativeRecipeStack>>,
    pub(crate) results: Vec<NativeRecipeStack>,
    pub(crate) cooking_time: u32,
    pub(crate) experience: f32,
    pub(crate) book_id: i32,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeItem {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) tags: Vec<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NativeRecipes {
    pub(crate) items: Vec<NativeItem>,
    pub(crate) recipes: Vec<NativeRecipe>,
}

#[allow(dead_code)]
const MAPPINGS: [(&str, Option<KeyCode>); 9] = [
    ("key.forward", Some(KeyCode::KeyW)),
    ("key.left", Some(KeyCode::KeyA)),
    ("key.back", Some(KeyCode::KeyS)),
    ("key.right", Some(KeyCode::KeyD)),
    ("key.jump", Some(KeyCode::Space)),
    ("key.sneak", Some(KeyCode::ShiftLeft)),
    ("key.sprint", Some(KeyCode::ControlLeft)),
    ("key.attack", None),
    ("key.use", None),
];

impl Game<'_> {
    pub(super) fn native_frame(&mut self, elapsed: Duration) {
        let _ = elapsed;
    }
    pub(super) fn native_tick(&mut self) {
        self.mod_api_tick();
    }
    pub(super) fn native_register_mods(&mut self) {}
    pub(super) fn native_player_joined(&mut self, player: &NativeServerPlayer) {
        super::mod_api::player_joined(player);
    }
    pub(super) fn native_player_chat(&mut self, player: &NativeServerPlayer, text: &str) {
        super::mod_api::player_chat(player, text);
    }
    pub(super) fn native_player_left(&mut self, player: &NativeServerPlayer, reason: &str) {
        super::mod_api::player_left(player, reason);
    }
    pub(super) fn native_world_lifecycle(
        &mut self,
        context: &NativeLifecycleContext,
        phase: WorldPhase,
        error_text: &str,
    ) {
        super::mod_api::world_lifecycle(context, phase, error_text);
    }
    pub(super) fn native_server_tick(
        &mut self,
        context: &NativeLifecycleContext,
        phase: ServerTickPhase,
    ) {
        let _ = context;
        super::mod_api::server_tick(phase);
    }
    pub(super) fn native_hud(&mut self, batch: &mut HudBatch, corners: &mut Corners) {
        let _ = (batch, corners);
    }
    pub(super) fn native_key(&mut self, key: &str, pressed: bool) {
        vise_api::bridge::key(key, pressed);
    }
    pub(super) fn native_wheel(&mut self, steps: f32, claimed: &mut bool) {
        if vise_api::bridge::mouse_scroll(steps) {
            *claimed = true;
        }
    }
    pub(super) fn native_command(&mut self, command: &str, ran: &mut bool) {
        let _ = (command, ran);
    }
    pub(super) fn native_zoom(&self, zoom: &mut Option<NativeZoom>) {
        let _ = zoom;
    }
    pub(super) fn native_minimap(&self, minimap: &mut Option<NativeMinimap>) {
        let _ = minimap;
    }
    pub(super) fn native_mods(&mut self, mods: &mut Vec<NativeMod>) {
        let _ = mods;
    }
    pub(super) fn native_settings(&mut self, mod_id: &str, buttons: &mut Vec<String>) {
        let _ = (mod_id, buttons);
    }
    pub(super) fn native_settings_click(&mut self, mod_id: &str, button: usize) {
        let _ = (mod_id, button);
    }
    pub(super) fn native_panel_event(&mut self, mod_id: &str, event: &NativePanelEvent) {
        let _ = (mod_id, event);
    }
    pub(super) fn native_block_entity(
        &mut self,
        block: &str,
        entity: &mut Option<(String, Compound)>,
    ) {
        let _ = (block, entity);
    }
    pub(super) fn native_block_entity_ticks(&self, block: &str, ticks: &mut bool) {
        let _ = (block, ticks);
    }
    pub(super) fn native_block_entity_container(
        &self,
        block: &str,
        container: &mut Option<ModContainer>,
    ) {
        let _ = (block, container);
    }
    pub(super) fn native_block_entity_tick(&mut self, pos: [i32; 3], entity: &mut ModBlockEntity) {
        let _ = (pos, entity);
    }
    pub(super) fn native_register_commands(&mut self, commands: &mut Vec<NativeCommand>) {
        super::mod_api::register_commands(commands);
    }
    pub(super) fn native_perform_command(
        &mut self,
        player: &NativeServerPlayer,
        command: &str,
        ran: &mut bool,
    ) {
        if self.mod_api_command(player, command) {
            *ran = true;
        }
    }
    pub(super) fn native_menu_clicked(
        &mut self,
        uuid: u128,
        window: i32,
        slot: i32,
        button: i32,
        mode: i32,
    ) {
        vise_api::bridge::menu_click(uuid, window, slot, button, mode);
    }
    pub(super) fn native_menu_closed(&mut self, uuid: u128, window: i32) {
        vise_api::bridge::menu_close(uuid, window);
    }
    pub(super) fn native_tooltip(&self, stack: &ItemStack, lines: &mut Vec<(Text, u32)>) {
        super::mod_api::tooltip(stack, lines);
    }
    pub(super) fn native_use_item_on(
        &mut self,
        stack: &ItemStack,
        pos: [i32; 3],
        face: [i32; 3],
        claimed: &mut bool,
    ) {
        if vise_api::bridge::use_item_on(super::mod_api::item_view(stack), pos, face) {
            *claimed = true;
        }
    }
    pub(super) fn native_use_item(&mut self, stack: &ItemStack, claimed: &mut bool) {
        if vise_api::bridge::use_item(super::mod_api::item_view(stack)) {
            *claimed = true;
        }
    }
    pub(super) fn native_destroy_block(
        &mut self,
        pos: [i32; 3],
        state: BlockStateId,
        canceled: &mut bool,
    ) {
        if vise_api::bridge::destroy_block(pos, state.0) {
            *canceled = true;
        }
    }
    pub(super) fn native_place_block(
        &mut self,
        stack: &ItemStack,
        pos: [i32; 3],
        state: BlockStateId,
    ) {
        vise_api::bridge::place_block(super::mod_api::item_view(stack), pos, state.0);
    }
}

#[allow(dead_code)]
impl GameLevel<'_> {
    pub(super) fn native_send_particles(
        &mut self,
        options: &ParticleOptions,
        state: Option<BlockStateId>,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: i32,
    ) -> bool {
        use ParticleType as T;
        let supported = match options.ty {
            T::Block | T::BlockMarker | T::FallingDust | T::DustPillar | T::BlockCrumble => {
                state.is_some_and(|id| self.registry.state(id).is_some())
            }
            T::Item | T::Trail | T::Vibration => false,
            T::Geyser | T::GeyserPlume | T::GeyserBase | T::GeyserPoof => options
                .geyser
                .is_some_and(|(_, impulse)| impulse.is_finite()),
            _ => true,
        };
        if !self.native_authoritative
            || !supported
            || !at.into_iter().all(f64::is_finite)
            || !spread
                .into_iter()
                .all(|v| v.is_finite() && v.abs() <= f32::MAX as f64)
            || !speed.is_finite()
            || speed.abs() > f32::MAX as f64
            || !options.color.into_iter().all(f32::is_finite)
            || !options.to_color.into_iter().all(f32::is_finite)
            || !options.scale.is_finite()
        {
            return false;
        }
        let dimension = match self.dimension {
            LevelDimension::Overworld => "minecraft:overworld",
            LevelDimension::Nether => "minecraft:the_nether",
            LevelDimension::End => "minecraft:the_end",
        };
        self.native_particles
            .push((dimension, options.clone(), state, at, spread, speed, count));
        true
    }
}

#[allow(dead_code)]
impl Game<'_> {
    pub(super) fn native_on(&self, mod_id: &str) -> bool {
        !self.settings.mods.disabled.contains(mod_id)
    }
    pub(super) fn native_set_block_property(
        &mut self,
        pos: [i32; 3],
        key: &str,
        value: &str,
    ) -> bool {
        let registry = self.pack.assets.registry;
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let Some(mut s) = registry.state(state).cloned() else {
            return false;
        };
        match s.properties.get(key) {
            Some(v) if v == value => return true,
            Some(_) => {}
            None => return false,
        }
        s.properties.insert(key.into(), value.into());
        let Ok(id) = registry.resolve(&s) else {
            return false;
        };
        let at = BlockPos::new(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| b.set_block(level, ticks, at, id, flags::UPDATE_ALL))
    }
    pub(super) fn native_with_mod_block_entity<R>(
        &mut self,
        pos: [i32; 3],
        change: impl FnOnce(&mut ModBlockEntity) -> R,
    ) -> Option<R> {
        let key = (pos[0], pos[1], pos[2]);
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&key) else {
            return None;
        };
        if m.id.is_empty() {
            return None;
        }
        let items = m.items.clone();
        let out = change(m);
        let moved = m.items != items;
        self.world.modified.insert((key.0 >> 4, key.2 >> 4));
        if moved {
            self.container_changed(key);
        }
        Some(out)
    }
    pub(super) fn native_playing(&self) -> bool {
        self.screen == Screen::Playing
            && self.title.is_none()
            && self.sign_edit.is_none()
            && self.book.is_none()
    }
    pub(super) fn native_window(&self) -> bool {
        self.options.execution_role == ExecutionRole::Client
    }
    pub(super) fn native_corner_shown(&self, corner: Corner) -> bool {
        let charts = self.debug_charts.corners(!self.has_singleplayer_server());
        !debug_takes(corner, self.debug, charts)
    }
    pub(super) fn native_text_width(&self, text: &str) -> f32 {
        HudBatch::text_width(&self.gui.atlas, text)
    }
    pub(super) fn native_fill(&self, batch: &mut HudBatch, rect: [f32; 4], argb: u32) {
        let [x, y, w, h] = rect;
        let [a, r, g, b] = argb.to_be_bytes();
        batch.fill(&self.gui.atlas, x, y, w, h, rgba(r, g, b, a));
    }
    pub(super) fn native_text(
        &self,
        batch: &mut HudBatch,
        text: &str,
        at: [f32; 2],
        rgb: u32,
        shadow: bool,
    ) {
        let [x, y] = at;
        let [_, r, g, b] = rgb.to_be_bytes();
        let color = rgba(r, g, b, 0xFF);
        batch.text_styled(&self.gui.atlas, &Text::literal(text), x, y, color, shadow);
    }
    pub(super) fn native_player(&self) -> Option<NativePlayer> {
        let p = self.player.as_ref()?;
        Some(NativePlayer {
            x: p.position.x,
            y: p.position.y,
            z: p.position.z,
            yaw: self.yaw,
            pitch: self.pitch,
            dimension: self.dim.id(),
        })
    }
    pub(super) fn native_frame_rate(&self) -> (u32, u32, u32) {
        self.frame_reading()
    }
    pub(super) fn native_clicks(&mut self) -> (u32, u32) {
        self.mouse_reading().0
    }
    pub(super) fn native_mapping(&mut self, name: &str) -> Option<(bool, String)> {
        let &(_, key) = MAPPINGS.iter().find(|m| m.0 == name)?;
        let label = keybinds::display_name(self.settings.keys.get(name)).plain();
        let down = match key {
            Some(k) => self.keys.contains(&k),
            None => self.mouse_reading().1[usize::from(name == "key.use")],
        };
        Some((down, label))
    }
    pub(super) fn native_mod_key(&self, mod_id: &str, name: &str, default: &str) -> String {
        let chosen = self
            .settings
            .mods
            .keys
            .get(mod_id)
            .and_then(|k| k.get(name));
        chosen.map_or(default, String::as_str).to_owned()
    }
    pub(super) fn native_chat(&mut self, text: &str) {
        let focused = self.screen == Screen::Chat;
        let line = Text::literal(text);
        let atlas = &self.gui.atlas;
        self.ui
            .chat
            .add(atlas, line, None, ChatSource::Client, self.ticks, focused);
    }
    pub(super) fn native_data_dir(&self, mod_id: &str) -> PathBuf {
        let mods = Self::options_path().with_file_name(mod_list::DIR);
        mods.join(".vise-mod-data").join(mod_id)
    }
    pub(super) fn native_setting(&self, mod_id: &str, key: &str) -> Option<String> {
        let path = self.native_data_dir(mod_id).join(key);
        fs::read_to_string(path).ok().map(|v| v.trim().to_owned())
    }
    pub(super) fn native_set_setting(&self, mod_id: &str, key: &str, value: &str) {
        let dir = self.native_data_dir(mod_id);
        let _ = fs::create_dir_all(&dir).and_then(|()| fs::write(dir.join(key), value));
    }
    pub(super) fn native_panel_items(&mut self, mod_id: &str, items: Vec<NativeStack>) {
        self.panel.show_items(mod_id, items);
    }
    pub(super) fn native_panel_recipes(
        &mut self,
        mod_id: &str,
        title: &str,
        tabs: Vec<NativeStack>,
        tab: u32,
        views: Vec<NativeRecipeView>,
    ) {
        let page = RecipePage::new(title, tabs, tab, views);
        self.panel.show_recipes(mod_id, page);
    }
    pub(super) fn native_recipes(&mut self) -> Option<Arc<NativeRecipes>> {
        self.player.as_ref()?;
        self.made_recipes()
    }
    pub(super) fn native_known_recipes(&mut self) -> Arc<[i32]> {
        if self.player.is_none() {
            return Arc::from([]);
        }
        self.known_displays()
    }
    pub(super) fn native_place_recipe(&mut self, book_id: i32, all: bool) -> Result<(), String> {
        self.recipe_place(book_id, all)
    }
    pub(super) fn native_give(&mut self, item: &str, count: u32) -> Result<(), String> {
        self.recipe_give(item, count)
    }
}

impl Game<'_> {
    pub(super) fn new_block_entity(&mut self, name: &str) -> Option<BlockEntity> {
        if let Some(entity) = BlockEntity::for_block(name) {
            return Some(entity);
        }
        let mut entity = None;
        self.native_block_entity(name, &mut entity);
        let (id, nbt) = entity?;
        Some(BlockEntity::Mod(Box::new(ModBlockEntity {
            id,
            nbt,
            ..ModBlockEntity::default()
        })))
    }
    pub(super) fn native_containers(&self) -> FxHashMap<u32, Arc<ModContainer>> {
        let registry = self.pack.assets.registry;
        let mut by_block: FxHashMap<String, Option<Arc<ModContainer>>> = FxHashMap::default();
        let mut out = FxHashMap::default();
        for i in 0..registry.len() as u32 {
            let Some(state) = registry.state(vise_world::registry::BlockStateId(i)) else {
                continue;
            };
            if state.name.starts_with("minecraft:") {
                continue;
            }
            let container = by_block.entry(state.name.clone()).or_insert_with(|| {
                let mut container = None;
                self.native_block_entity_container(&state.name, &mut container);
                container.map(|mut c| {
                    c.slots = c.slots.min(256);
                    let n = c.slots;
                    for list in c.faces.iter_mut().chain([&mut c.inputs, &mut c.outputs]) {
                        list.retain(|&s| s < n);
                    }
                    c.menu = c
                        .menu
                        .filter(|m| mod_menu(m).is_some_and(|(_, size)| size == n));
                    Arc::new(c)
                })
            });
            if let Some(c) = container {
                out.insert(i, Arc::clone(c));
            }
        }
        out
    }
    pub(super) fn native_ticks(&self, name: &str) -> bool {
        let mut ticks = false;
        self.native_block_entity_ticks(name, &mut ticks);
        ticks
    }
    pub(super) fn open_mod_menu(&mut self, key: Pos) -> bool {
        let state = self.world.block(key.0, key.1, key.2).0;
        let Some(c) = self.world.block_entities.mod_container(state) else {
            return false;
        };
        let slots = c.slots;
        if c.menu.is_none() {
            return false;
        }
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&key) else {
            return false;
        };
        if m.items.len() < slots {
            m.items.resize(slots, None);
        }
        true
    }
    pub(super) fn mod_menu_at(&self, key: Pos) -> Option<&'static str> {
        let state = self.world.block(key.0, key.1, key.2).0;
        let c = self.world.block_entities.mod_container(state)?;
        mod_menu(c.menu.as_deref()?).map(|(menu, _)| menu)
    }
    pub(super) fn tick_mod_block_entity(&mut self, pos: Pos) {
        let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&pos) else {
            return;
        };
        let mut taken = std::mem::take(&mut **m);
        let items = taken.items.clone();
        let state = self.world.block(pos.0, pos.1, pos.2).0;
        let slots = self
            .world
            .block_entities
            .mod_container(state)
            .map_or(0, |c| c.slots);
        if taken.items.len() < slots {
            taken.items.resize(slots, None);
        }
        if slots == 0 && !taken.items.is_empty() {
            let items = vise_client::save::encode_items(&std::mem::take(&mut taken.items));
            taken.nbt.insert("Items".into(), items);
        }
        self.native_block_entity_tick([pos.0, pos.1, pos.2], &mut taken);
        if let Some(BlockEntity::Mod(m)) = self.world.block_entities.get_mut(&pos)
            && m.id.is_empty()
        {
            let moved = taken.items != items;
            **m = taken;
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
            if moved {
                self.container_changed(pos);
            }
        } else if taken.items.iter().any(Option::is_some) {
            let lost = std::mem::take(&mut taken.items);
            let at = BlockPos {
                x: pos.0,
                y: pos.1,
                z: pos.2,
            };
            self.with_level(|_, level, _| level.drop_lost_contents(at, lost));
            self.world.modified.insert((pos.0 >> 4, pos.2 >> 4));
        }
    }
    pub(super) fn list_native_mods(&mut self) {
        let mut mods = Vec::new();
        self.native_mods(&mut mods);
        self.settings
            .mods
            .installed
            .retain(|row| row.native.is_none() && !mods.iter().any(|m| m.id == row.id));
        for m in mods {
            let mut buttons = Vec::new();
            self.native_settings(&m.id, &mut buttons);
            let page = NativePage {
                keys: m.keys,
                buttons,
            };
            self.settings.mods.installed.push(Installed {
                id: m.id,
                title: m.title,
                description: m.description,
                problem: None,
                dev: false,
                native: Some(page),
            });
        }
    }
    pub(super) fn native_settings_event(&mut self, i: usize, button: usize) {
        let Some(id) = self.settings.mods.installed.get(i).map(|m| m.id.clone()) else {
            return;
        };
        self.native_settings_click(&id, button);
        self.native_settings_refresh(i);
    }
    pub(super) fn native_settings_refresh(&mut self, i: usize) {
        let Some(id) = self.settings.mods.installed.get(i).map(|m| m.id.clone()) else {
            return;
        };
        let mut buttons = Vec::new();
        self.native_settings(&id, &mut buttons);
        let row = self.settings.mods.installed.get_mut(i);
        if let Some(page) = row.and_then(|m| m.native.as_mut()) {
            page.buttons = buttons;
        }
    }
    pub(super) fn native_key_event(&mut self, key: KeyCode, pressed: bool, repeat: bool) {
        if repeat {
            return;
        }
        if let Some(name) = keybinds::key_name(key) {
            if pressed {
                self.native_panel_key(name);
            }
            self.native_key(name, pressed);
        }
    }
    pub(super) fn native_mouse_event(&mut self, button: MouseButton, pressed: bool) {
        let name = keybinds::mouse_name(button);
        self.native_key(&name, pressed);
    }
    pub(super) fn native_wheel_event(&mut self, steps: f32) -> bool {
        let mut claimed = false;
        self.native_wheel(steps, &mut claimed);
        claimed
    }
    pub(super) fn client_mod_command(&mut self, command: &str) -> bool {
        let mut ran = false;
        self.native_command(command, &mut ran);
        ran
    }
    pub(super) fn native_zoom_now(&self) -> Option<NativeZoom> {
        if !self.native_playing() || self.scoping() {
            return None;
        }
        let mut zoom = None;
        self.native_zoom(&mut zoom);
        let z = zoom?;
        let factor = finite(z.factor, 1.0, 10.0);
        (factor > 1.0).then_some(NativeZoom {
            factor,
            smooth: z.smooth,
            turn: if z.turn.is_finite() {
                z.turn.clamp(0.05, 1.0)
            } else {
                1.0
            },
        })
    }
    pub(super) fn native_minimap_now(&self) -> Option<NativeMinimap> {
        let mut minimap = None;
        self.native_minimap(&mut minimap);
        minimap.map(|m| NativeMinimap {
            size: finite(m.size, 16.0, 256.0),
            blocks_per_pixel: finite(m.blocks_per_pixel, 0.5, 4.0),
            ..m
        })
    }
}

fn debug_takes(corner: Corner, debug: bool, charts: [bool; 2]) -> bool {
    debug
        && match corner {
            Corner::TopLeft | Corner::TopRight => true,
            Corner::BottomLeft => charts[0],
            Corner::BottomRight => charts[1],
        }
}

fn mod_menu(name: &str) -> Option<(&'static str, usize)> {
    MOD_MENUS.into_iter().find(|(menu, _)| *menu == name)
}

fn finite(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hook_points_stay_hookable() {
        let source = include_str!("native_hooks.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(code.contains("\npub(crate) const NATIVE_HOOKS_API: u32 = "));
        let lines = code.lines().filter(|l| !l.trim_start().starts_with("//"));
        for line in lines {
            assert!(!line.contains("cfg"), "a cfg in the hook points: {line}");
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(!line.contains(mac), "a macro in the hook points: {line}");
            }
        }
    }

    #[test]
    fn lifecycle_source_stays_hookable() {
        let source = include_str!("native_lifecycle.rs");
        let code = source.split("#[cfg(test)]").next().unwrap_or_default();
        for line in code.lines().filter(|l| !l.trim_start().starts_with("//")) {
            assert!(!line.contains("cfg"), "a cfg in lifecycle source: {line}");
            for mac in ["!(", "![", "! {", "!{"] {
                assert!(!line.contains(mac), "a macro in lifecycle source: {line}");
            }
        }
    }

    #[test]
    fn catalog_matches_hook_points() {
        use std::collections::BTreeSet;

        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../native-hooks.json")).unwrap();
        let hooks = catalog["hooks"].as_array().unwrap();
        let source = include_str!("native_hooks.rs");
        let section = source
            .split("impl Game<'_> {")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        let level_section = source
            .split("impl GameLevel<'_> {")
            .nth(1)
            .unwrap()
            .split("\n}")
            .next()
            .unwrap();
        let level_hooks: BTreeSet<_> = level_section
            .lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("pub(super) fn native_")?
                    .split('(')
                    .next()
            })
            .collect();
        let declared: BTreeSet<_> = section
            .lines()
            .chain(level_section.lines())
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("pub(super) fn native_")?
                    .split('(')
                    .next()
            })
            .collect();
        let mut names = BTreeSet::new();
        let mut contracts = BTreeSet::new();
        for hook in hooks {
            let name = hook["name"].as_str().unwrap();
            let version = hook["version"].as_u64().unwrap();
            assert!(
                version > 0 && contracts.insert((name, version)),
                "invalid or duplicate hook: {name} v{version}"
            );
            names.insert(name);
            let owner = if level_hooks.contains(name) {
                "GameLevel"
            } else {
                "Game"
            };
            assert_eq!(
                hook["target"].as_str().unwrap(),
                format!("vise_client::game::native_hooks::{owner}<'_>::native_{name}")
            );
        }
        assert_eq!(names, declared);
    }

    #[test]
    fn mod_menus_are_slot_grids_and_furnaces() {
        use super::mod_menu;
        assert_eq!(mod_menu("minecraft:hopper"), Some(("minecraft:hopper", 5)));
        assert_eq!(mod_menu("minecraft:generic_9x3").map(|m| m.1), Some(27));
        assert_eq!(mod_menu("minecraft:furnace").map(|m| m.1), Some(3));
        assert_eq!(mod_menu("minecraft:brewing_stand"), None);
        assert_eq!(mod_menu("hopper"), None);
    }

    #[test]
    fn the_debug_screen_takes_both_top_corners_and_its_charts_corners() {
        use super::{Corner, debug_takes};
        for corner in Corner::ALL {
            assert!(!debug_takes(corner, false, [true, true]), "{corner:?}");
        }
        assert!(debug_takes(Corner::TopLeft, true, [false, false]));
        assert!(debug_takes(Corner::TopRight, true, [false, false]));
        assert!(!debug_takes(Corner::BottomLeft, true, [false, false]));
        assert!(!debug_takes(Corner::BottomRight, true, [false, false]));
        assert!(debug_takes(Corner::BottomLeft, true, [true, false]));
        assert!(!debug_takes(Corner::BottomRight, true, [true, false]));
        assert!(debug_takes(Corner::BottomRight, true, [false, true]));
    }
}
