use super::{Game, Screen};
use vise_client::{
    Result,
    gamepad::{self, Button, Edges, GlyphStyle, HealthWatch, PadState, Rumble},
    keybinds::{KeyBinds, key_code, mouse_button},
};
use serde_json::{Value, json};
use std::time::Instant;
use winit::{event_loop::ActiveEventLoop, keyboard::KeyCode};

pub(super) const HELP: &[&str] = &[
    "pad BUTTON down|up|tap         a virtual controller button (south east west north lb rb lt rt select start ls rs up down left right)",
    "pad stick left|right X Y       tilt a virtual stick (-1..1, up positive); 0 0 centres it",
    "pad trigger left|right VALUE   pull a virtual trigger (0..1)",
    "pad release                    centre and release the virtual controller",
    "pad status                     connected controllers, the active one and its state",
];

#[derive(Default)]
pub(super) struct Pads {
    #[cfg(feature = "controllers")]
    devices: hardware::Devices,
    injected: PadState,
    virtual_input: bool,
    taps: Vec<Button>,
    edges: Edges,
    pressed_at: [Option<u64>; 16],
    late_releases: Vec<Button>,
    pub(super) state: PadState,
    pub(super) in_use: bool,
    pub(super) style: GlyphStyle,
    pub(super) menu: super::pad_menu::MenuCursor,
    last_rumble: Option<(&'static str, gamepad::Pulse)>,
    health: HealthWatch,
}

#[cfg(feature = "controllers")]
mod hardware;

#[cfg(not(feature = "controllers"))]
mod hardware {
    use super::{super::Game, Pads};
    use vise_client::gamepad::{PadState, Pulse, Rumble};
    use serde_json::{Value, json};

    impl Pads {
        pub(super) fn read(&mut self) -> PadState {
            PadState::default()
        }
    }

    impl Game<'_> {
        pub(super) fn pad_output_off(&mut self) {}
        pub(super) fn pad_output_frame(&mut self) {}
        pub(in crate::game) fn pad_close_output(&mut self) {}
        pub(super) fn pad_vibrate(&mut self, _: Rumble, _: Pulse) {}
        pub(super) fn pad_devices_status(&self) -> Value {
            json!({ "backend": false, "controllers": [] })
        }
    }
}

impl Pads {
    pub(super) fn player_replaced(&mut self) {
        self.health.forget();
    }
}

impl Game<'_> {
    pub(super) fn poll_pads(&mut self, event_loop: &ActiveEventLoop) {
        let settings = &self.settings.controller;
        if !settings.enabled {
            self.pad_output_off();
            if self.pad.state != PadState::default() {
                self.pad.state = PadState::default();
                for (button, down) in self.pad.edges.release_all() {
                    self.pad_edge(event_loop, button, down);
                }
            }
            return;
        }
        let hardware = self.pad.read();
        self.ui.options.pad_style = self.settings.controller.glyphs.resolve(self.pad.style);
        let state = hardware.merge(&self.pad.injected);
        self.pad.virtual_input = self.pad.injected != PadState::default();
        for button in std::mem::take(&mut self.pad.taps) {
            self.pad.injected.set(button, false);
        }
        let threshold = self.settings.controller.trigger_threshold;
        let edges = self.pad.edges.update(&state, threshold);
        if state.active(&self.settings.controller) {
            self.last_input = Instant::now();
            self.pad.in_use = true;
        }
        self.pad.state = state;
        self.pad_output_frame();
        let health = self
            .player
            .as_ref()
            .filter(|_| self.title.is_none())
            .map(|p| p.health.health());
        if let Some(damage) = self.pad.health.observe(health) {
            self.pad_rumble(Rumble::Hurt(damage));
        }
        let ticking = self.title.is_none() && self.screen == Screen::Playing && !self.pause;
        for button in std::mem::take(&mut self.pad.late_releases) {
            if !ticking || self.pad.pressed_at[button as usize] != Some(self.ticks) {
                self.pad_button_logged(event_loop, button, false);
            } else {
                self.pad.late_releases.push(button);
            }
        }
        for (button, down) in edges {
            self.pad_edge(event_loop, button, down);
        }
        self.pad_menu_frame(event_loop);
    }

    fn pad_edge(&mut self, event_loop: &ActiveEventLoop, button: Button, down: bool) {
        let ticking = self.title.is_none() && self.screen == Screen::Playing && !self.pause;
        if down {
            if let Some(i) = self.pad.late_releases.iter().position(|b| *b == button) {
                self.pad.late_releases.remove(i);
                self.pad_button_logged(event_loop, button, false);
            }
            self.pad.pressed_at[button as usize] = Some(self.ticks);
        } else if ticking && self.pad.pressed_at[button as usize] == Some(self.ticks) {
            self.pad.late_releases.push(button);
            return;
        }
        self.pad_button_logged(event_loop, button, down);
    }

    fn pad_button_logged(&mut self, event_loop: &ActiveEventLoop, button: Button, down: bool) {
        if let Err(e) = self.pad_button(event_loop, button, down) {
            log::warn!("Vise: controller {}: {e}", button.name());
        }
    }

    fn pad_focused(&self) -> bool {
        self.focused || self.pad.virtual_input
    }

    fn pad_button(
        &mut self,
        event_loop: &ActiveEventLoop,
        button: Button,
        down: bool,
    ) -> Result<()> {
        if down && !self.pad_focused() {
            return Ok(());
        }
        let playing = !self.pad_in_menu();
        if down && playing && !self.captured && self.focused {
            self.capture(true);
        }
        if !playing {
            self.pad_menu_button(event_loop, button, down)?;
            if down {
                return Ok(());
            }
        } else if button == Button::Start {
            return if down { self.pad_back() } else { Ok(()) };
        }
        for action in self.settings.controller.actions_of(button) {
            self.pad_action(action, down)?;
        }
        Ok(())
    }

    fn pad_action(&mut self, action: &str, down: bool) -> Result<()> {
        match action {
            gamepad::HOTBAR_PREVIOUS | gamepad::HOTBAR_NEXT => {
                if down && self.screen == Screen::Playing {
                    self.selected =
                        gamepad::hotbar_step(self.selected, action == gamepad::HOTBAR_NEXT);
                }
                Ok(())
            }
            "key.sneak"
                if self.settings.controller.toggle_sneak && !self.settings.toggle_crouch =>
            {
                if down {
                    let sneaking = self.keys.contains(&KeyCode::ShiftLeft);
                    self.mapped_key(KeyCode::ShiftLeft, !sneaking, false)?;
                }
                Ok(())
            }
            _ => {
                let default = KeyBinds::default_of(action);
                if let Some(button) = mouse_button(default) {
                    if self.title.is_none() && self.screen == Screen::Playing {
                        self.play_mouse(button, down)?;
                    }
                    Ok(())
                } else if let Some(key) = key_code(default) {
                    self.mapped_key(key, down, false)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub(super) fn pad_back(&mut self) -> Result<()> {
        if self.screen == Screen::Options {
            return self.options_key(KeyCode::Escape, None);
        }
        if let Some(title) = self.title.as_mut() {
            title.escape();
            self.save_title_options();
            return Ok(());
        }
        if self.sign_edit.is_some() {
            self.close_sign_editor();
            return Ok(());
        }
        if self.book.is_some() {
            self.close_book();
            return Ok(());
        }
        self.key_pressed(KeyCode::Escape)
    }

    pub(super) fn pad_move(&self) -> Option<[f32; 2]> {
        let settings = &self.settings.controller;
        if !settings.enabled || self.pad_in_menu() || !self.pad_focused() {
            return None;
        }
        gamepad::move_vector(self.pad.state.left, settings.move_deadzone)
    }

    pub(super) fn pad_look(&mut self, seconds: f32) {
        let settings = &self.settings.controller;
        if !settings.enabled || self.pad_in_menu() || self.player.is_none() || !self.pad_focused() {
            return;
        }
        let (yaw, pitch) = gamepad::look_delta(
            self.pad.state.right,
            settings,
            seconds.min(0.1),
            self.scoping(),
        );
        if yaw == 0.0 && pitch == 0.0 {
            return;
        }
        self.turn_player(f64::from(yaw / 0.15), f64::from(pitch / 0.15));
    }

    pub(super) fn pad_rumble(&mut self, rumble: Rumble) {
        let settings = &self.settings.controller;
        if !settings.enabled || !settings.vibration || !self.pad.in_use {
            return;
        }
        let Some(pulse) = rumble.pulse() else {
            return;
        };
        self.pad.last_rumble = Some((rumble.name(), pulse));
        self.pad_vibrate(rumble, pulse);
    }

    pub(super) fn pad_command(&mut self, words: &[&str]) -> Result<Value> {
        let arg = |i: usize| -> Result<&str> {
            words
                .get(i)
                .copied()
                .ok_or_else(|| format!("usage: {}", HELP[0]).into())
        };
        let number = |i: usize| -> Result<f32> {
            let v: f32 = arg(i)?.parse()?;
            if v.is_finite() {
                Ok(v)
            } else {
                Err("not a number".into())
            }
        };
        match arg(1)? {
            "status" => {
                let s = &self.pad.state;
                let held: Vec<&str> = Button::ALL
                    .into_iter()
                    .filter(|b| self.pad.edges.is_held(*b))
                    .map(Button::name)
                    .collect();
                let mut status = json!({
                    "enabled": self.settings.controller.enabled,
                    "glyphs": self.pad.style.name(),
                    "last_rumble": self.pad.last_rumble.map(|(why, p)| json!({
                        "reason": why, "strong": p.strong, "weak": p.weak, "ms": p.millis,
                    })),
                    "in_use": self.pad.in_use,
                    "menu": self.pad_menu_status(),
                    "held": held,
                    "left": s.left,
                    "right": s.right,
                    "triggers": s.triggers,
                });
                if let (Some(status), Value::Object(devices)) =
                    (status.as_object_mut(), self.pad_devices_status())
                {
                    status.extend(devices);
                }
                Ok(status)
            }
            "release" => {
                self.pad.injected = PadState::default();
                self.pad.taps.clear();
                Ok(json!({ "ok": true }))
            }
            "stick" => {
                let v = [number(3)?.clamp(-1.0, 1.0), number(4)?.clamp(-1.0, 1.0)];
                match arg(2)? {
                    "left" => self.pad.injected.left = v,
                    "right" => self.pad.injected.right = v,
                    other => return Err(format!("stick must be left or right, not {other}").into()),
                }
                Ok(json!({ "ok": true }))
            }
            "trigger" => {
                let v = number(3)?.clamp(0.0, 1.0);
                match arg(2)? {
                    "left" => self.pad.injected.triggers[0] = v,
                    "right" => self.pad.injected.triggers[1] = v,
                    other => {
                        return Err(format!("trigger must be left or right, not {other}").into());
                    }
                }
                Ok(json!({ "ok": true }))
            }
            name => {
                let button = Button::parse(name)
                    .ok_or_else(|| format!("unknown pad button {name}; {}", HELP[0]))?;
                match arg(2)? {
                    "down" => self.pad.injected.set(button, true),
                    "up" => self.pad.injected.set(button, false),
                    "tap" => {
                        self.pad.injected.set(button, true);
                        self.pad.taps.push(button);
                    }
                    other => {
                        return Err(
                            format!("pad action must be down, up or tap, not {other}").into()
                        );
                    }
                }
                Ok(json!({ "ok": true }))
            }
        }
    }
}
