use super::{Game, Screen};
use vise_client::{
    Result,
    hud::{EndPortalFill, HudBatch},
    win_screen::{Key, WinScreen},
};
use std::time::Instant;
use winit::{
    event::{ElementState, KeyEvent},
    keyboard::{KeyCode, PhysicalKey},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Finish {
    Respawn,
    RemoteRespawn,
    Options,
    Closed,
}
pub(super) struct Win {
    screen: WinScreen,
    finish: Finish,
    frame: Instant,
}
impl Game<'_> {
    pub(super) fn open_win_screen(&mut self, poem: bool, finish: Finish) {
        let screen = WinScreen::new(
            poem,
            &self.options.resources,
            &self.options.player_name(),
            &self.gui.atlas.font,
        );
        self.win = Some(Win {
            screen,
            finish,
            frame: Instant::now(),
        });
        self.keys.clear();
        self.screen = Screen::Win;
        self.capture(false);
    }
    pub(super) fn show_end_credits(&mut self) -> Result<()> {
        self.level.seen_credits = true;
        self.open_win_screen(true, Finish::Respawn);
        self.end_portal_travel = true;
        let result = self.enter_dimension(super::Dim::Overworld);
        self.end_portal_travel = false;
        result
    }
    pub(super) fn hold_win_screen(&mut self, scroll: f32) {
        if let Some(win) = self.win.as_mut() {
            win.screen.hold(scroll);
        }
    }
    pub(super) fn win_screen_pauses(&self) -> bool {
        self.win.as_ref().is_some_and(|w| !w.screen.poem)
    }
    pub(super) fn win_screen_holds_arrival(&self) -> bool {
        self.win
            .as_ref()
            .is_some_and(|w| w.finish == Finish::Respawn)
    }
    pub(super) fn draw_win_screen(&mut self, batch: &mut HudBatch) {
        let Some(win) = self.win.as_mut() else {
            return;
        };
        let now = Instant::now();
        let delta_ticks =
            now.duration_since(win.frame).as_secs_f32() / self.clock.tick_duration().as_secs_f32();
        win.frame = now;
        let sprite = |n: &str| {
            self.gui
                .atlas
                .sprites
                .get(n)
                .map_or([0.0; 4], |r| [r.u0, r.v0, r.u1 - r.u0, r.v1 - r.v0])
        };
        let fill = EndPortalFill {
            sprites: [
                sprite("environment/end_sky"),
                sprite("entity/end_portal/end_portal"),
            ],
            game_time: (self.world.game_time % 24000) as f32 / 24000.0,
        };
        win.screen.draw(batch, &self.gui.atlas, delta_ticks, fill);
    }
    pub(super) fn win_key(&mut self, event: &KeyEvent) -> Result<()> {
        let PhysicalKey::Code(code) = event.physical_key else {
            return Ok(());
        };
        let pressed = event.state == ElementState::Pressed;
        if code == KeyCode::Escape && pressed {
            return self.close_win_screen();
        }
        let Some(key) = Key::from_code(code) else {
            return Ok(());
        };
        if let Some(win) = self.win.as_mut() {
            win.screen.key(key, pressed);
        }
        Ok(())
    }
    pub(super) fn tick_win_screen(&mut self) -> Result<()> {
        let height = self.gui_size().1;
        if self.win.as_ref().is_some_and(|w| w.screen.finished(height)) {
            self.close_win_screen()?;
        }
        Ok(())
    }
    pub(super) fn close_win_screen(&mut self) -> Result<()> {
        let Some(win) = self.win.take() else {
            return Ok(());
        };
        let credits = vise_client::music::Music::credits();
        self.music
            .stop_playing_music(&credits, None, &mut self.audio);
        match win.finish {
            Finish::Options => self.screen = Screen::Options,
            Finish::Closed if self.title.is_some() => self.screen = Screen::Playing,
            Finish::Respawn | Finish::RemoteRespawn | Finish::Closed => {
                if win.finish == Finish::RemoteRespawn {
                    self.remote_respawn();
                }
                self.screen = Screen::Playing;
                self.capture(true);
            }
        }
        Ok(())
    }
}
