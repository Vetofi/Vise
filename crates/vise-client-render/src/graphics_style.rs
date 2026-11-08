use crate::{
    hud::{HudAtlas, HudBatch},
    text::Text,
    ui,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cost {
    None,
    Low,
    Medium,
    High,
    Saves,
}
impl Cost {
    pub fn label(self) -> &'static str {
        match self {
            Cost::None => "No cost",
            Cost::Low => "Low cost",
            Cost::Medium => "Medium cost",
            Cost::High => "High cost",
            Cost::Saves => "Saves time",
        }
    }
}

pub struct Style {
    pub control_width: f32,
    pub gap: f32,
    pub max_text_width: f32,
    pub row_height: f32,
    pub header_height: f32,
    pub band: f32,
    pub title_color: u32,
    pub header_color: u32,
    pub description_color: u32,
    pub cost_colors: [u32; 5],
}

const fn opaque(r: u8, g: u8, b: u8) -> u32 {
    0xFF00_0000 | (b as u32) << 16 | (g as u32) << 8 | r as u32
}

pub const STYLE: Style = Style {
    control_width: 150.0,
    gap: 8.0,
    max_text_width: 250.0,
    row_height: 25.0,
    header_height: 16.0,
    band: 33.0,
    title_color: u32::MAX,
    header_color: opaque(255, 255, 85),
    description_color: opaque(170, 170, 170),
    cost_colors: [
        opaque(120, 200, 120),
        opaque(170, 220, 120),
        opaque(240, 200, 90),
        opaque(240, 110, 90),
        opaque(85, 255, 85),
    ],
};

impl Style {
    pub fn text_width(&self, screen_width: f32) -> f32 {
        (screen_width - self.control_width - self.gap - 24.0).clamp(120.0, self.max_text_width)
    }
    pub fn row_x(&self, screen_width: f32) -> f32 {
        let total = self.control_width + self.gap + self.text_width(screen_width);
        ((screen_width - total) / 2.0).floor()
    }
    pub fn background(
        &self,
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        top: f32,
        bottom: f32,
        in_world: bool,
    ) {
        let (w, h) = (batch.width, batch.height);
        ui::menu_background(batch, atlas, 0.0, 0.0, w, h, in_world);
        ui::list_background(batch, atlas, top, bottom, in_world);
    }
    pub fn frame(
        &self,
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        top: f32,
        bottom: f32,
        in_world: bool,
    ) {
        ui::separators(batch, atlas, top, bottom, in_world);
    }
    pub fn title(&self, batch: &mut HudBatch, atlas: &HudAtlas, text: &Text) {
        let w = batch.width;
        ui::centered_text(batch, atlas, text, 0.0, 12.0, w, self.title_color);
    }
    pub fn header(&self, batch: &mut HudBatch, atlas: &HudAtlas, text: &Text, x: f32, y: f32) {
        batch.text_styled(atlas, text, x, y + 4.0, self.header_color, true);
    }
    pub fn button(
        &self,
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        (x, y, w): (f32, f32, f32),
        label: &Text,
        hover: bool,
        active: bool,
    ) {
        ui::button(batch, atlas, x, y, w, 20.0, label, active, hover);
    }
    pub fn slider(
        &self,
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        (x, y, w): (f32, f32, f32),
        label: &Text,
        value: f32,
        hover: bool,
        active: bool,
        dragging: bool,
    ) {
        ui::slider(
            batch,
            atlas,
            x,
            y,
            w,
            label,
            value,
            hover && active,
            active,
            dragging && active,
        );
    }
    pub fn description(
        &self,
        batch: &mut HudBatch,
        atlas: &HudAtlas,
        x: f32,
        y: f32,
        cost: Cost,
        text: &str,
    ) {
        let tag = Text::literal(cost.label());
        batch.text_styled(atlas, &tag, x, y, self.cost_colors[cost as usize], true);
        let indent = atlas.font.width_text(&tag) + 4.0;
        let lines = wrap(atlas, text, self.text_width(batch.width), indent);
        for (i, line) in lines.iter().take(2).enumerate() {
            let lx = if i == 0 { x + indent } else { x };
            batch.text_styled(
                atlas,
                &Text::literal(line.clone()),
                lx,
                y + i as f32 * 10.0,
                self.description_color,
                true,
            );
        }
    }
}

fn wrap(atlas: &HudAtlas, text: &str, width: f32, indent: f32) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let room = if lines.len() == 1 {
            width - indent
        } else {
            width
        };
        let line = lines.last().expect("one line");
        let candidate = if line.is_empty() {
            word.to_owned()
        } else {
            format!("{line} {word}")
        };
        if atlas.font.width(&candidate) <= room || line.is_empty() {
            *lines.last_mut().expect("one line") = candidate;
        } else {
            lines.push(word.to_owned());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cost_has_a_label_and_a_colour() {
        let all = [Cost::None, Cost::Low, Cost::Medium, Cost::High, Cost::Saves];
        assert_eq!(all.len(), STYLE.cost_colors.len());
        for (i, cost) in all.into_iter().enumerate() {
            assert_eq!(cost as usize, i);
            assert!(!cost.label().is_empty());
        }
    }
}
