use crate::hud::{HudAtlas, PACK_ICON_SIZE, decode_bytes};
use crate::ui_theme::{Face, Ui, color};

const ICON: &[u8] = include_bytes!("../assets/splash-logo.png");
const SPRITE: &str = "vise/pack-icon0";
const ICON_SIZE: f32 = 112.0;
const BAR_GAP: f32 = 44.0;
const BAR_W: f32 = 180.0;
const BAR_H: f32 = 3.0;
const LABEL_GAP: f32 = 16.0;
const LABEL_SIZE: f32 = 13.0;

pub fn atlas() -> Option<HudAtlas> {
    let mut atlas = HudAtlas::build(std::path::Path::new("vise-splash")).ok()?;
    let rect = *atlas.sprites.get(SPRITE)?;
    let (w, h, rgba) = decode_bytes(ICON).ok()?;
    if w != PACK_ICON_SIZE || h != PACK_ICON_SIZE {
        return None;
    }
    let x0 = (rect.u0 * atlas.width as f32).round() as usize;
    let y0 = (rect.v0 * atlas.height as f32).round() as usize;
    let stride = atlas.width as usize * 4;
    let row = w as usize * 4;
    for y in 0..h as usize {
        let dst = (y0 + y) * stride + x0 * 4;
        atlas
            .pixels
            .get_mut(dst..dst + row)?
            .copy_from_slice(&rgba[y * row..(y + 1) * row]);
    }
    Some(atlas)
}

pub struct LoadingSplash {
    shown: f32,
    last: f32,
}

impl Default for LoadingSplash {
    fn default() -> Self {
        Self::new()
    }
}

impl LoadingSplash {
    pub fn new() -> Self {
        Self {
            shown: 0.0,
            last: 0.0,
        }
    }

    pub fn draw(&mut self, ui: &mut Ui, progress: f32, label: &str, t: f32) {
        let dt = (t - self.last).clamp(0.0, 0.25);
        self.last = t;
        let target = progress.clamp(0.0, 1.0).max(self.shown);
        self.shown += (target - self.shown) * (1.0 - (-dt * 8.0).exp());

        ui.rounded(0.0, 0.0, ui.w, ui.h, 0.0, color::BG);

        let height = ICON_SIZE + BAR_GAP + BAR_H + LABEL_GAP + LABEL_SIZE * 1.45;
        let top = ((ui.h - height) / 2.0).round();
        let cx = ui.w / 2.0;

        let k = ui.density / ui.batch.scale;
        let size = ICON_SIZE;
        let (x, y) = (cx - size / 2.0, top);
        ui.batch.sprite_scaled(
            ui.atlas,
            SPRITE,
            (x * k).round(),
            (y * k).round(),
            (size * k).round(),
            (size * k).round(),
            u32::MAX,
        );

        let width = (ui.w * 0.34).clamp(BAR_W, 320.0).min(ui.w - 64.0).max(80.0);
        let bx = (cx - width / 2.0).round();
        let by = top + ICON_SIZE + BAR_GAP;
        ui.rounded(
            bx,
            by,
            width,
            BAR_H,
            BAR_H / 2.0,
            color::BORDER,
        );
        ui.rounded(
            bx,
            by,
            (width * self.shown).max(if self.shown > 0.0 { BAR_H } else { 0.0 }),
            BAR_H,
            BAR_H / 2.0,
            color::ACCENT,
        );
        ui.text_centered(
            Face::Regular,
            LABEL_SIZE,
            cx,
            by + BAR_H + LABEL_GAP,
            color::MUTED,
            label,
        );
    }
}
