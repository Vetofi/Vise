use super::{Button, Title};
use crate::{
    hud::{HudAtlas, HudBatch},
    text::Text,
    ui,
};

const TAB_BAR: f32 = 24.0;
const FOOTER: f32 = 33.0;
const TABS: [&str; 3] = [
    "createWorld.tab.game.title",
    "createWorld.tab.world.title",
    "createWorld.tab.more.title",
];
pub const DIFFICULTIES: [&str; 4] = ["peaceful", "easy", "normal", "hard"];

#[derive(Clone, Debug, PartialEq)]
pub struct WorldOptions {
    pub tab: usize,
    pub hardcore: bool,
    pub difficulty: usize,
    pub commands: Option<bool>,
    pub world_type: crate::save::WorldPreset,
    pub flat_settings: Option<super::flat::FlatSettings>,
    pub structures: bool,
    pub bonus_chest: bool,
    pub rules: crate::gamerules::GameRules,
    pub data_packs: Vec<String>,
    pub pack_dir: Option<std::path::PathBuf>,
}
impl Default for WorldOptions {
    fn default() -> Self {
        Self {
            tab: 0,
            hardcore: false,
            difficulty: 2,
            commands: None,
            world_type: crate::save::WorldPreset::Normal,
            flat_settings: None,
            structures: true,
            bonus_chest: false,
            rules: crate::gamerules::GameRules::default(),
            data_packs: vec!["vanilla".into()],
            pack_dir: None,
        }
    }
}
impl WorldOptions {
    pub(super) fn of_world(
        level: &crate::save::Level,
        commands: bool,
        dir: &std::path::Path,
    ) -> Self {
        let generation = crate::save::WorldGen::read(dir).unwrap_or_default();
        Self {
            hardcore: level.hardcore,
            difficulty: DIFFICULTIES
                .iter()
                .position(|d| *d == level.difficulty)
                .unwrap_or(2),
            commands: Some(commands),
            world_type: generation.preset,
            flat_settings: (generation.preset == crate::save::WorldPreset::Flat)
                .then(|| super::flat::FlatSettings::from_world(dir))
                .flatten(),
            structures: generation.structures,
            bonus_chest: generation.bonus_chest,
            rules: crate::gamerules::GameRules::load(dir),
            ..Self::default()
        }
    }
    pub fn difficulty(&self) -> usize {
        if self.hardcore { 3 } else { self.difficulty }
    }
    pub fn commands(&self, creative: bool) -> bool {
        !self.hardcore && self.commands.unwrap_or(creative)
    }
    pub fn bonus_chest(&self) -> bool {
        !self.hardcore && self.bonus_chest
    }
    pub fn world_gen(&self) -> crate::save::WorldGen {
        crate::save::WorldGen {
            preset: self.world_type,
            structures: self.structures,
            bonus_chest: self.bonus_chest(),
        }
    }
    pub fn flat_settings(&self) -> Option<vise_world::nbt::Tag> {
        self.flat_settings
            .as_ref()
            .map(super::flat::FlatSettings::to_tag)
    }
    pub(super) fn press(&mut self, id: u8, creative: &mut bool) {
        match id {
            20 => {
                (*creative, self.hardcore) = match (*creative, self.hardcore) {
                    (false, false) => (false, true),
                    (false, true) => (true, false),
                    _ => (false, false),
                }
            }
            23 => self.difficulty = (self.difficulty + 1) % DIFFICULTIES.len(),
            24 => self.commands = Some(!self.commands(*creative)),
            25 => {
                self.world_type = self.world_type.next();
                if self.world_type == crate::save::WorldPreset::Flat {
                    self.flat_settings = Some(super::flat::FlatSettings::default());
                }
            }
            27 => self.structures = !self.structures,
            28 => self.bonus_chest = !self.bonus_chest,
            _ => {}
        }
    }
}

fn round_toward(value: f32, step: f32) -> f32 {
    (value / step).ceil() * step
}
fn tabs(w: f32) -> (f32, f32) {
    let width = w.min(400.0) - 28.0;
    (
        round_toward(((w - width) / 2.0).trunc(), 2.0),
        round_toward((width / TABS.len() as f32).trunc(), 2.0),
    )
}
pub(super) fn tab_at(w: f32, (x, y): (f32, f32)) -> Option<usize> {
    let (x0, width) = tabs(w);
    ((0.0..TAB_BAR).contains(&y) && x >= x0)
        .then(|| ((x - x0) / width) as usize)
        .filter(|i| *i < TABS.len())
}
fn grid(w: f32, h: f32, tab: usize) -> (f32, f32) {
    let (gw, gh) = match tab {
        0 => (210.0, 117.0),
        1 => (310.0, 117.0),
        _ => (210.0, 76.0),
    };
    let area = h - TAB_BAR - FOOTER;
    (
        ((w - gw) * 0.5).trunc(),
        TAB_BAR + ((area - gh) * 0.166_666_67).trunc(),
    )
}
pub(super) fn field_rect(w: f32, h: f32, tab: usize) -> Option<(u8, [f32; 4])> {
    let (gx, gy) = grid(w, h, tab);
    match tab {
        0 => Some((0, [gx + 1.0, gy + 13.0, 208.0, 20.0])),
        1 => Some((1, [gx, gy + 41.0, 308.0, 20.0])),
        _ => None,
    }
}
pub(super) fn field_at(w: f32, h: f32, tab: usize, (x, y): (f32, f32)) -> Option<u8> {
    let (id, [fx, fy, fw, fh]) = field_rect(w, h, tab)?;
    ui::hovered((x, y), fx, fy, fw, fh).then_some(id)
}

fn tr(key: &str) -> String {
    Text::translate(key, &[]).plain()
}
fn labelled(key: &str, value: String) -> String {
    Text::translate(
        "options.generic_value",
        &[Text::translate(key, &[]), Text::literal(value)],
    )
    .plain()
}
fn on_off(on: bool) -> String {
    tr(if on { "options.on" } else { "options.off" })
}
fn game_mode(creative: bool, hardcore: bool) -> &'static str {
    match (creative, hardcore) {
        (true, _) => "selectWorld.gameMode.creative",
        (false, true) => "selectWorld.gameMode.hardcore",
        (false, false) => "selectWorld.gameMode.survival",
    }
}

pub(super) fn buttons(w: f32, h: f32, creative: bool, o: &WorldOptions) -> Vec<Button> {
    let b = |x: f32, y: f32, w: f32, label: String, enabled: bool, id: u8| Button {
        x,
        y,
        w,
        label,
        enabled,
        id,
        icon: None,
    };
    let (gx, gy) = grid(w, h, o.tab);
    let mut buttons = match o.tab {
        0 => vec![
            b(
                gx,
                gy + 41.0,
                210.0,
                labelled("selectWorld.gameMode", tr(game_mode(creative, o.hardcore))),
                true,
                20,
            ),
            b(
                gx,
                gy + 69.0,
                210.0,
                labelled(
                    "options.difficulty",
                    tr(&format!(
                        "options.difficulty.{}",
                        DIFFICULTIES[o.difficulty()]
                    )),
                ),
                !o.hardcore,
                23,
            ),
            b(
                gx,
                gy + 97.0,
                210.0,
                labelled("selectWorld.allowCommands", on_off(o.commands(creative))),
                !o.hardcore,
                24,
            ),
        ],
        1 => vec![
            b(
                gx,
                gy,
                150.0,
                labelled("selectWorld.mapType", tr(o.world_type.label())),
                true,
                25,
            ),
            b(
                gx + 160.0,
                gy,
                150.0,
                tr("selectWorld.customizeType"),
                o.world_type == crate::save::WorldPreset::Flat,
                26,
            ),
            b(gx + 266.0, gy + 73.0, 44.0, on_off(o.structures), true, 27),
            b(
                gx + 266.0,
                gy + 97.0,
                44.0,
                on_off(o.bonus_chest()),
                !o.hardcore,
                28,
            ),
        ],
        _ => [
            "selectWorld.gameRules",
            "selectWorld.experiments",
            "selectWorld.dataPacks",
        ]
        .iter()
        .enumerate()
        .map(|(i, key)| {
            b(
                gx,
                gy + 28.0 * i as f32,
                210.0,
                tr(key),
                i != 1,
                29 + i as u8,
            )
        })
        .collect(),
    };
    let x = ((w - 308.0) * 0.5).trunc();
    buttons.push(b(
        x,
        h - 27.0,
        150.0,
        crate::lang::get_or("selectWorld.create", "Create New World"),
        true,
        21,
    ));
    buttons.push(b(
        x + 158.0,
        h - 27.0,
        150.0,
        crate::lang::get_or("gui.cancel", "Cancel"),
        true,
        22,
    ));
    buttons
}

impl Title {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_create(
        &self,
        atlas: &HudAtlas,
        batch: &mut HudBatch,
        cursor: (f32, f32),
        name: &str,
        seed: &str,
        focus: u8,
        o: &WorldOptions,
    ) {
        let (w, h) = (batch.width.ceil(), batch.height.ceil());
        ui::tiled(
            batch,
            atlas,
            "gui/tab_header_background",
            0.0,
            0.0,
            w,
            TAB_BAR,
            16.0,
            u32::MAX,
        );
        ui::menu_background(batch, atlas, 0.0, TAB_BAR, w, h - TAB_BAR, false);
        let (x0, tw) = tabs(w);
        let right = x0 + tw * TABS.len() as f32;
        for (x, width) in [(0.0, x0), (right, w - right)] {
            ui::tiled_sized(
                batch,
                atlas,
                "gui/header_separator",
                x,
                TAB_BAR - 2.0,
                width,
                2.0,
                [32.0, 2.0],
                u32::MAX,
            );
        }
        for (i, key) in TABS.iter().enumerate() {
            let x = x0 + tw * i as f32;
            let selected = o.tab == i;
            let label = Text::translate(key, &[]);
            crate::widgets::record(&label.plain(), [x, 0.0, tw, TAB_BAR], true);
            let sprite = match (selected, ui::hovered(cursor, x, 0.0, tw, TAB_BAR)) {
                (true, true) => "gui/sprites/widget/tab_selected_highlighted",
                (true, false) => "gui/sprites/widget/tab_selected",
                (false, true) => "gui/sprites/widget/tab_highlighted",
                (false, false) => "gui/sprites/widget/tab",
            };
            ui::nine_slice(batch, atlas, sprite, x, 0.0, tw, TAB_BAR, u32::MAX);
            let lw = atlas.font.width_text(&label).floor();
            if selected {
                ui::menu_background(batch, atlas, x + 2.0, 2.0, tw - 4.0, TAB_BAR - 2.0, false);
                let uw = lw.min(tw - 4.0);
                batch.fill(
                    atlas,
                    x + ((tw - uw) / 2.0).floor(),
                    TAB_BAR - 2.0,
                    uw,
                    1.0,
                    u32::MAX,
                );
            }
            let top = if selected { 0.0 } else { 3.0 };
            batch.text_styled(
                atlas,
                &label,
                x + (tw / 2.0).floor() - (lw / 2.0).floor(),
                ((top + TAB_BAR - 9.0) / 2.0).floor() + 1.0,
                u32::MAX,
                true,
            );
        }
        let (gx, gy) = grid(w, h, o.tab);
        let label = |batch: &mut HudBatch, key: &str, x: f32, y: f32| {
            batch.text_styled(atlas, &Text::translate(key, &[]), x, y, u32::MAX, true);
        };
        match o.tab {
            0 => {
                label(batch, "selectWorld.enterName", gx + 1.0, gy);
                self.create_field(
                    atlas,
                    batch,
                    (super::WORLD_NAME, name, 32),
                    [gx + 1.0, gy + 13.0, 208.0],
                    focus == 0,
                    None,
                );
            }
            1 => {
                label(batch, "selectWorld.enterSeed", gx, gy + 28.0);
                self.create_field(
                    atlas,
                    batch,
                    (super::WORLD_SEED, seed, 32),
                    [gx, gy + 41.0, 308.0],
                    focus == 1,
                    Some("selectWorld.seedInfo"),
                );
                label(batch, "selectWorld.mapFeatures", gx, gy + 78.0);
                label(batch, "selectWorld.bonusItems", gx, gy + 102.0);
            }
            _ => {}
        }
        ui::tiled_sized(
            batch,
            atlas,
            "gui/footer_separator",
            0.0,
            h - FOOTER - 2.0,
            w,
            2.0,
            [32.0, 2.0],
            u32::MAX,
        );
    }
    fn create_field(
        &self,
        atlas: &HudAtlas,
        batch: &mut HudBatch,
        (id, value, max): (crate::clipboard::FieldId, &str, usize),
        [x, y, width]: [f32; 3],
        focused: bool,
        hint: Option<&str>,
    ) {
        let rect = [x, y, width, 20.0];
        self.caret
            .borrow_mut()
            .draw(batch, atlas, (id, value, max), rect, focused, self.started);
        let field = ui::EditBox::new(x, y, width, 20.0);
        if let Some(hint) = hint
            && value.is_empty()
            && !focused
        {
            batch.text_styled(
                atlas,
                &Text::translate(hint, &[]).with_color(crate::text::DARK_GRAY),
                field.text_x(),
                field.text_y(),
                u32::MAX,
                true,
            );
        }
    }
    pub(super) fn create_tooltip(
        &self,
        atlas: &HudAtlas,
        batch: &mut HudBatch,
        cursor: (f32, f32),
        name: &str,
        creative: bool,
        o: &WorldOptions,
    ) {
        let (w, h) = (batch.width.ceil(), batch.height.ceil());
        let (gx, gy) = grid(w, h, o.tab);
        let over = |x: f32, y: f32, width: f32| ui::hovered(cursor, x, y, width, 20.0);
        let text = match o.tab {
            0 if over(gx + 1.0, gy + 13.0, 208.0) => Some(Text::translate(
                "selectWorld.targetFolder",
                &[Text::literal(self.target_folder(name)).italic()],
            )),
            0 if over(gx, gy + 41.0, 210.0) => Some(Text::translate(
                &format!("{}.info", game_mode(creative, o.hardcore)),
                &[],
            )),
            0 if over(gx, gy + 69.0, 210.0) => Some(Text::translate(
                &format!("options.difficulty.{}.info", DIFFICULTIES[o.difficulty()]),
                &[],
            )),
            0 if over(gx, gy + 97.0, 210.0) => {
                Some(Text::translate("selectWorld.allowCommands.info", &[]))
            }
            1 if over(gx + 266.0, gy + 73.0, 44.0) => {
                Some(Text::translate("selectWorld.mapFeatures.info", &[]))
            }
            _ => None,
        };
        if let Some(text) = text {
            ui::tooltip(batch, atlas, &atlas.font.split(&text, 170.0), cursor);
        }
    }
}
