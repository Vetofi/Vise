use crate::hud::{HudAtlas, HudBatch};
use std::time::Instant;

#[derive(Default)]
pub struct Slide {
    pub origin: Option<Instant>,
    begun: bool,
    start: i64,
    fully_at: i64,
    show: bool,
    pub portion: f32,
    pub fully_for: i64,
}
impl Slide {
    pub const MS: i64 = 600;
    pub fn update(
        &mut self,
        now: i64,
        wants_show: impl FnOnce(i64) -> bool,
    ) -> (Option<bool>, bool) {
        let was = self.show;
        if !self.begun {
            self.begun = true;
            self.start = now;
            self.show = true;
        }
        if self.show && now - self.start <= Self::MS {
            self.fully_at = now;
        }
        self.fully_for = now - self.fully_at;
        let progress = ((now - self.start) as f32 / Self::MS as f32).clamp(0.0, 1.0);
        let progress = progress * progress;
        self.portion = if self.show { progress } else { 1.0 - progress };
        let wanted = wants_show(self.fully_for);
        if wanted != self.show {
            self.start = now - ((1.0 - self.portion) * Self::MS as f32) as i64;
            self.show = wanted;
        }
        let finished = !self.show && now - self.start > Self::MS;
        ((self.show != was).then_some(self.show), finished)
    }
    pub fn update_now(&mut self, wants_show: impl FnOnce(i64) -> bool) -> (Option<bool>, bool) {
        let origin = *self.origin.get_or_insert_with(Instant::now);
        self.update(origin.elapsed().as_millis() as i64, wants_show)
    }
}

pub struct SystemToast {
    pub slide: Slide,
    pub slot: Option<usize>,
    title: String,
    message: Option<String>,
    changed: bool,
    last_changed: i64,
    force_hide: bool,
}
impl SystemToast {
    const DISPLAY_TIME: f64 = 5000.0;
    pub fn new(title: String, message: Option<String>) -> Self {
        Self {
            slide: Slide::default(),
            slot: None,
            title,
            message,
            changed: false,
            last_changed: 0,
            force_hide: false,
        }
    }
    pub fn reset(&mut self, title: String, message: Option<String>) {
        self.title = title;
        self.message = message;
        self.changed = true;
    }
    pub fn force_hide(&mut self) {
        self.force_hide = true;
    }
    pub fn update(&mut self, multiplier: f64) -> (Option<bool>, bool) {
        self.slide.update_now(|fully| {
            if std::mem::take(&mut self.changed) {
                self.last_changed = fully;
            }
            !self.force_hide
                && ((fully - self.last_changed) as f64) < Self::DISPLAY_TIME * multiplier
        })
    }
    pub fn draw(&self, atlas: &HudAtlas, batch: &mut HudBatch, y: f32) {
        let width = std::iter::once(&self.title)
            .chain(&self.message)
            .map(|line| HudBatch::text_width(atlas, line))
            .fold(0.0, f32::max)
            .ceil()
            .max(160.0)
            + 30.0;
        let x = batch.width - width * self.slide.portion;
        background(atlas, batch, x, y, width);
        let yellow = crate::hud::rgba(255, 255, 0, 255);
        match &self.message {
            Some(message) => {
                batch.text(atlas, &self.title, x + 18.0, y + 7.0, yellow, false);
                batch.text(atlas, message, x + 18.0, y + 19.0, u32::MAX, false);
            }
            None => {
                batch.text(atlas, &self.title, x + 18.0, y + 12.0, yellow, false);
            }
        }
    }
}
pub fn background(atlas: &HudAtlas, batch: &mut HudBatch, x: f32, y: f32, width: f32) {
    let Some(rect) = atlas.sprites.get("gui/sprites/toast/system").copied() else {
        return;
    };
    let source_x = [0.0, 17.0, rect.w as f32 - 4.0, rect.w as f32];
    let target_x = [x, x + 17.0, x + width - 4.0, x + width];
    for col in 0..3 {
        for (sy, sh, y, h) in [
            (0.0, 28.0, y, 28.0),
            (rect.h as f32 - 4.0, 4.0, y + 28.0, 4.0),
        ] {
            let mut piece = rect;
            let du = (rect.u1 - rect.u0) / rect.w as f32;
            let dv = (rect.v1 - rect.v0) / rect.h as f32;
            piece.u0 = rect.u0 + source_x[col] * du;
            piece.u1 = rect.u0 + source_x[col + 1] * du;
            piece.v0 = rect.v0 + sy * dv;
            piece.v1 = rect.v0 + (sy + sh) * dv;
            batch.rect_uv(
                target_x[col],
                y,
                target_x[col + 1] - target_x[col],
                h,
                piece,
                u32::MAX,
            );
        }
    }
}
