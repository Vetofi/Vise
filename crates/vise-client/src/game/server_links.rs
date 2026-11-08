use super::{Game, Screen, packs::open_uri};
use vise_client::{Result, clipboard::Clipboard, hud::HudBatch, text::Text, ui};
use vise_protocol::packets::configuration::{ServerLink, ServerLinkKind};

const HEADER: f32 = 33.0;
const FOOTER: f32 = 33.0;
const CONTENT_TOP: f32 = HEADER + 30.0;
const LINK_WIDTH: f32 = 310.0;
const ROW: f32 = 22.0;
const SCROLL_RESERVE: f32 = 10.0;
const WARNING_BUTTON: f32 = 20.0;

#[derive(Default)]
pub(super) struct LinksUi {
    pub scroll: f32,
    pub confirm: Option<String>,
}

pub(super) fn valid_untrusted_uri(uri: &str) -> bool {
    let Some(colon) = uri.find(':') else {
        return false;
    };
    let scheme = &uri[..colon];
    let mut chars = scheme.chars();
    if !chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return false;
    }
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
        return false;
    }
    !uri.chars().any(|c| c.is_control() || c == ' ')
}
pub(super) fn trusted(links: Vec<ServerLink>) -> Vec<ServerLink> {
    links
        .into_iter()
        .filter(|l| {
            let ok = valid_untrusted_uri(&l.link);
            if !ok {
                log::warn!("Vise: server sent an invalid link: {}", l.link);
            }
            ok
        })
        .collect()
}
fn link_label(link: &ServerLink) -> Text {
    const KNOWN: [&str; 10] = [
        "report_bug",
        "community_guidelines",
        "support",
        "status",
        "feedback",
        "community",
        "website",
        "forums",
        "news",
        "announcements",
    ];
    match &link.kind {
        ServerLinkKind::Known(id) => Text::translate(
            &format!(
                "known_server_link.{}",
                KNOWN.get(*id as usize).copied().unwrap_or(KNOWN[0])
            ),
            &[],
        ),
        ServerLinkKind::Custom(tag) => {
            vise_client::scoreboard::comp_text(&vise_client::scoreboard::tag_to_json(tag))
        }
    }
}

fn list_height(n: usize) -> f32 {
    n as f32 * ROW - 2.0
}
fn list_top(h: f32, n: usize) -> f32 {
    CONTENT_TOP.min(h - FOOTER - list_height(n).min(h - HEADER - FOOTER))
}
fn scroll_limit(h: f32, n: usize) -> f32 {
    (list_height(n) - (h - HEADER - FOOTER)).max(0.0)
}

fn link_buttons(w: f32, top: f32, n: usize) -> Vec<(f32, f32)> {
    let x = ((w - LINK_WIDTH - 2.0 * SCROLL_RESERVE) / 2.0 + SCROLL_RESERVE).floor();
    (0..n).map(|i| (x, top + i as f32 * ROW)).collect()
}
fn warning_button(w: f32, h: f32, title_width: f32) -> (f32, f32) {
    let x = ((w - title_width - 30.0) / 2.0).floor() + title_width + 10.0;
    let y = ((HEADER - WARNING_BUTTON) / 2.0).floor();
    if x < 0.0 || y < 0.0 || x > w - WARNING_BUTTON || y > h - WARNING_BUTTON {
        (0.0f32.max(w - 40.0), 5.0f32.min(h))
    } else {
        (x, y)
    }
}
fn back_button(w: f32, h: f32) -> (f32, f32) {
    (
        ((w - 200.0) / 2.0).floor(),
        h - FOOTER + ((FOOTER - 20.0) / 2.0).floor(),
    )
}

impl Game<'_> {
    pub(super) fn server_links(&self) -> &[ServerLink] {
        self.remote
            .as_ref()
            .map_or(&[], |r| r.server_links.as_slice())
    }
    pub(super) fn draw_server_links(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        ui::menu_background(batch, atlas, 0.0, 0.0, w, h, true);
        let links = self.server_links().to_vec();
        self.links_ui.scroll = self
            .links_ui
            .scroll
            .clamp(0.0, scroll_limit(h, links.len()));
        let top = list_top(h, links.len()) - self.links_ui.scroll.trunc();
        let title = Text::translate("menu.server_links.title", &[]);
        let tw = atlas.font.width_text(&title);
        let tx = ((w - tw - 30.0) / 2.0).floor();
        batch.text_styled(atlas, &title, tx, 12.0, u32::MAX, true);
        let (wx, wy) = warning_button(w, h, tw);
        let wh = ui::hovered(self.cursor, wx, wy, WARNING_BUTTON, WARNING_BUTTON);
        vise_client::widgets::record(
            "menu.custom_screen_info.tooltip",
            [wx, wy, WARNING_BUTTON, WARNING_BUTTON],
            true,
        );
        batch.sprite_scaled(
            atlas,
            if wh {
                "gui/sprites/dialog/warning_button_highlighted"
            } else {
                "gui/sprites/dialog/warning_button"
            },
            wx,
            wy,
            WARNING_BUTTON,
            WARNING_BUTTON,
            u32::MAX,
        );
        let first = batch.vertices.len();
        let inside = self.cursor.1 >= HEADER && self.cursor.1 < h - FOOTER;
        for (link, (x, y)) in links.iter().zip(link_buttons(w, top, links.len())) {
            let hover = inside && ui::hovered(self.cursor, x, y, LINK_WIDTH, 20.0);
            ui::button(
                batch,
                atlas,
                x,
                y,
                LINK_WIDTH,
                20.0,
                &link_label(link),
                true,
                hover,
            );
        }
        batch.clip_since(first, 0.0, HEADER, w, h - FOOTER);
        ui::scrollbar(
            batch,
            atlas,
            (w / 2.0).floor() + 155.0 + 4.0,
            HEADER,
            h - FOOTER,
            list_height(links.len()),
            self.links_ui.scroll.trunc(),
        );
        let (bx, by) = back_button(w, h);
        let hover = ui::hovered(self.cursor, bx, by, 200.0, 20.0);
        ui::button(
            batch,
            atlas,
            bx,
            by,
            200.0,
            20.0,
            &Text::translate("gui.back", &[]),
            true,
            hover,
        );
        if wh {
            ui::tooltip(
                batch,
                atlas,
                &[Text::translate("menu.custom_screen_info.tooltip", &[])],
                self.cursor,
            );
        }
    }
    pub(super) fn server_links_click(&mut self) -> Result<()> {
        let (w, h) = self.gui_size();
        let links = self.server_links().to_vec();
        let top = list_top(h, links.len()) - self.links_ui.scroll.trunc();
        let inside = self.cursor.1 >= HEADER && self.cursor.1 < h - FOOTER;
        if inside
            && let Some(i) = link_buttons(w, top, links.len())
                .iter()
                .position(|(x, y)| ui::hovered(self.cursor, *x, *y, LINK_WIDTH, 20.0))
        {
            self.audio.play_button_click();
            self.click_link(&links[i].link);
            return Ok(());
        }
        let title = Text::translate("menu.server_links.title", &[]);
        let tw = self.gui.atlas.font.width_text(&title);
        let (wx, wy) = warning_button(w, h, tw);
        if ui::hovered(self.cursor, wx, wy, WARNING_BUTTON, WARNING_BUTTON) {
            self.audio.play_button_click();
            self.screen = Screen::DialogWarning;
            return Ok(());
        }
        let (bx, by) = back_button(w, h);
        if ui::hovered(self.cursor, bx, by, 200.0, 20.0) {
            self.audio.play_button_click();
            self.screen = Screen::Paused;
        }
        Ok(())
    }
    fn click_link(&mut self, url: &str) {
        if !self.settings.chat_links {
            return;
        }
        if self.settings.chat_links_prompt {
            self.links_ui.confirm = Some(url.to_owned());
            self.screen = Screen::ConfirmLink;
        } else {
            open_uri(url);
        }
    }
    pub(super) fn server_links_scroll(&mut self, delta: f32) {
        let (_, h) = self.gui_size();
        self.links_ui.scroll = (self.links_ui.scroll - delta * 10.0)
            .clamp(0.0, scroll_limit(h, self.server_links().len()));
    }
    pub(super) fn draw_confirm_link(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        ui::menu_background(batch, atlas, 0.0, 0.0, w, h, true);
        let url = self.links_ui.confirm.clone().unwrap_or_default();
        let layout = confirm_link_layout(atlas, (w, h), &url);
        for (line, x, y) in &layout.lines {
            batch.text_styled(atlas, line, *x, *y, u32::MAX, true);
        }
        let labels = ["gui.yes", "gui.copy_link_to_clipboard", "gui.no"];
        for ((x, y), key) in layout.buttons.iter().zip(labels) {
            let hover = ui::hovered(self.cursor, *x, *y, 100.0, 20.0);
            ui::button(
                batch,
                atlas,
                *x,
                *y,
                100.0,
                20.0,
                &Text::translate(key, &[]),
                true,
                hover,
            );
        }
    }
    pub(super) fn confirm_link_click(&mut self) {
        let (w, h) = self.gui_size();
        let url = self.links_ui.confirm.clone().unwrap_or_default();
        let layout = confirm_link_layout(&self.gui.atlas, (w, h), &url);
        for (i, (x, y)) in layout.buttons.iter().enumerate() {
            if !ui::hovered(self.cursor, *x, *y, 100.0, 20.0) {
                continue;
            }
            self.audio.play_button_click();
            match i {
                0 => open_uri(&url),
                1 => {
                    self.clipboard.set(&url);
                }
                _ => {}
            }
            self.screen = Screen::ServerLinks;
            return;
        }
    }
    pub(super) fn draw_dialog_warning(&mut self, batch: &mut HudBatch) {
        let atlas = &self.gui.atlas;
        let (w, h) = (batch.width, batch.height);
        ui::menu_background(batch, atlas, 0.0, 0.0, w, h, true);
        let layout = dialog_warning_layout(atlas, (w, h));
        for (line, x, y) in &layout.lines {
            batch.text_styled(atlas, line, *x, *y, u32::MAX, true);
        }
        let quit = if self.external_server() {
            "menu.disconnect"
        } else {
            "menu.returnToMenu"
        };
        for ((x, y), key) in layout.buttons.into_iter().zip([quit, "gui.back"]) {
            let hover = ui::hovered(self.cursor, x, y, 150.0, 20.0);
            ui::button(
                batch,
                atlas,
                x,
                y,
                150.0,
                20.0,
                &Text::translate(key, &[]),
                true,
                hover,
            );
        }
    }
    pub(super) fn dialog_warning_click(&mut self) -> Result<()> {
        let (w, h) = self.gui_size();
        let layout = dialog_warning_layout(&self.gui.atlas, (w, h));
        for (i, (x, y)) in layout.buttons.iter().enumerate() {
            if !ui::hovered(self.cursor, *x, *y, 150.0, 20.0) {
                continue;
            }
            self.audio.play_button_click();
            if i == 0 {
                let reason = self.translate(
                    "menu.custom_screen_info.disconnect",
                    "Custom screen rejected",
                );
                self.leave_server(Some(reason))?;
            } else {
                self.screen = Screen::ServerLinks;
            }
            return Ok(());
        }
        Ok(())
    }
}

struct ConfirmLayout {
    lines: Vec<(Text, f32, f32)>,
    buttons: Vec<(f32, f32)>,
}
fn confirm_layout(
    font: &vise_client::font::Font,
    (w, h): (f32, f32),
    title: &Text,
    message: &Text,
    extra: &[Text],
    button_widths: &[f32],
) -> ConfirmLayout {
    let message: Vec<Text> = font
        .split_lines(message, w - 50.0)
        .into_iter()
        .take(15)
        .map(|(line, _)| line)
        .collect();
    let extra_h = 17.0 * extra.len() as f32;
    let mh = 9.0 * message.len() as f32;
    let total = 9.0 + 8.0 + mh + extra_h + 8.0 + 16.0 + 20.0;
    let mut y = ((h - total) / 2.0).trunc();
    let mut lines = Vec::new();
    let mut push = |line: &Text, at: f32| {
        let lw = font.width_text(line);
        lines.push((line.clone(), ((w - lw) / 2.0).trunc(), at));
    };
    push(title, y);
    y += 17.0;
    for line in &message {
        push(line, y);
        y += 9.0;
    }
    for line in extra {
        y += 8.0;
        push(line, y);
        y += 9.0;
    }
    let row_w: f32 = button_widths.iter().sum::<f32>() + 4.0 * (button_widths.len() - 1) as f32;
    let by = y + 8.0 + 16.0;
    let mut bx = ((w - row_w) / 2.0).trunc();
    let buttons = button_widths
        .iter()
        .map(|bw| {
            let p = (bx, by);
            bx += bw + 4.0;
            p
        })
        .collect();
    ConfirmLayout { lines, buttons }
}
fn confirm_link_layout(
    atlas: &vise_client::hud::HudAtlas,
    size: (f32, f32),
    url: &str,
) -> ConfirmLayout {
    confirm_layout(
        &atlas.font,
        size,
        &Text::translate("chat.link.confirm", &[]),
        &Text::literal(url.to_owned()),
        &[Text::translate("chat.link.warning", &[]).with_color(0xFFCCCC)],
        &[100.0; 3],
    )
}
fn dialog_warning_layout(atlas: &vise_client::hud::HudAtlas, size: (f32, f32)) -> ConfirmLayout {
    confirm_layout(
        &atlas.font,
        size,
        &Text::translate("menu.custom_screen_info.title", &[]),
        &Text::translate("menu.custom_screen_info.contents", &[]),
        &[],
        &[150.0; 2],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_uris_are_http_or_https_with_a_scheme() {
        for ok in [
            "https://example.net/x",
            "http://example.net",
            "HTTPS://EXAMPLE.NET",
        ] {
            assert!(valid_untrusted_uri(ok), "{ok}");
        }
        for bad in [
            "example.net/no-scheme",
            "ftp://example.net",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "https://exa mple.net",
            "://example.net",
            "",
        ] {
            assert!(!valid_untrusted_uri(bad), "{bad}");
        }
    }
}
