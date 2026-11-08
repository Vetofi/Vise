use super::Game;
pub(super) use vise_client::hud_overlays::Titles;
use vise_client::{
    hud::HudBatch,
    hud_layout,
    hud_overlays::HeldName,
    item_tooltip,
    text::{self, Text},
};

impl Game<'_> {
    pub(super) fn draw_titles(&self, batch: &mut HudBatch, bar_height: i32) {
        let (w, h) = (batch.width as i32, batch.height as i32);
        let partial = self.clock.fraction() as f32;
        let background = self.settings.text_background(0.0);
        let font = &self.gui.atlas.font;
        let mut draws = self
            .titles
            .overlay_message(w, h, bar_height, partial, font, background);
        draws.extend(self.titles.title(w, h, partial, font, background));
        hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
    pub(super) fn draw_saving_indicator(&self, batch: &mut HudBatch) {
        if !self.settings.autosave_indicator {
            return;
        }
        let draws = self.saving_indicator.draw(
            batch.width as i32,
            batch.height as i32,
            self.clock.fraction() as f32,
            &self.gui.atlas.font,
            Text::literal(self.translate("menu.savingLevel", "Saving world")),
            self.settings.text_background(0.0),
        );
        hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
    fn held_name(&self) -> Option<HeldName> {
        let stack = self.inventory.get(self.selected)?.as_ref()?;
        let custom = stack.data().is_some_and(|data| data.custom_name.is_some());
        let default_rarity = self.item_rarity(&stack.item);
        let rarity = item_tooltip::rarity(stack, &default_rarity);
        Some(HeldName {
            item: stack.item.to_string(),
            hover_name: item_tooltip::styled_name(stack, || {
                item_tooltip::name_line(stack, &|key| self.pack.names.display_get(key))
            }),
            rarity_color: match rarity {
                "uncommon" => text::YELLOW,
                "rare" => text::AQUA,
                "epic" => text::LIGHT_PURPLE,
                _ => text::WHITE,
            },
            custom_name: custom,
        })
    }
    pub(super) fn tick_hud(&mut self) {
        self.titles.tick();
        self.tick_in_bed_chat();
        let saving = self
            .integrated
            .as_ref()
            .map_or(self.autosaving, |server| server.is_saving());
        self.saving_indicator.tick(saving);
        let held = self.held_name();
        self.tool_highlight
            .tick(held, f64::from(self.settings.notification_display_time));
    }
    pub(super) fn draw_selected_name(&self, batch: &mut HudBatch, bar_height: i32, can_hurt: bool) {
        let draws = self.tool_highlight.draw(
            batch.width as i32,
            batch.height as i32,
            bar_height,
            can_hurt,
            &self.gui.atlas.font,
            self.settings.text_background(0.0),
        );
        hud_layout::paint(batch, &self.gui.atlas, &draws);
    }
}
