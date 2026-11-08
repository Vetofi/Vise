use super::{Game, Screen};
use vise_client::{
    Result,
    creative::Creative,
    gamepad::{self, Button, MenuNav, Rect},
    hud::{HudBatch, rgba},
    options_ui::{Event, Page},
    osk::{self, Keyboard},
    widgets::{self, Kind, Target},
};
use std::time::Instant;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalPosition,
    event::{DeviceId, ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent},
    event_loop::ActiveEventLoop,
    keyboard::KeyCode,
};

const REPEAT_DELAY: f32 = 0.35;
const REPEAT_EVERY: f32 = 0.09;
const SCROLL_SPEED: f32 = 12.0;
const STICK_STEP: f32 = 0.55;
const SCROLL_TRIES: u8 = 4;

#[derive(Clone)]
struct Pending {
    button: Button,
    from: Rect,
    generation: u64,
    tries: u8,
    before: Vec<Rect>,
}

#[derive(Default)]
pub(super) struct MenuCursor {
    pos: Option<(f32, f32)>,
    last_poll: Option<Instant>,
    moving: bool,
    wheel: [f32; 2],
    repeat: Option<(Button, Instant, bool)>,
    held: [bool; 2],
    os_hidden: bool,
    open_polls: u32,
    focus: Option<Target>,
    stick: Option<Button>,
    swapped: bool,
    pending: Option<Pending>,
    nudge: Option<(Rect, f32)>,
    repeats: u32,
    pub(super) osk: Option<Keyboard>,
    osk_dismissed: bool,
}

impl Game<'_> {
    pub(super) fn pad_in_menu(&self) -> bool {
        self.title.is_some()
            || self.screen != Screen::Playing
            || self.sign_edit.is_some()
            || self.book.is_some()
    }

    pub(super) fn gui_scale(&self) -> Option<HudBatch> {
        self.renderer
            .as_ref()
            .map(|r| HudBatch::new([r.config.width, r.config.height]))
    }

    fn pad_focus_mode(&self) -> bool {
        (self.settings.controller.menu_nav == MenuNav::Focus) != self.pad.menu.swapped
    }

    fn pad_targets(&self) -> Vec<Target> {
        let mut targets = self.pad_all_targets();
        if let Some(batch) = self.gui_scale() {
            let (w, h) = (batch.width + 0.5, batch.height + 0.5);
            targets.retain(|t| {
                let [x, y, tw, th] = t.rect;
                x >= -0.5 && y >= -0.5 && x + tw <= w && y + th <= h
            });
        }
        targets
    }

    fn pad_all_targets(&self) -> Vec<Target> {
        let mut targets: Vec<Target> = widgets::last()
            .iter()
            .filter(|w| w.enabled)
            .map(|w| Target {
                rect: [w.x, w.y, w.w, w.h],
                kind: Kind::Button,
            })
            .collect();
        for t in widgets::last_targets() {
            if let Some(same) = targets.iter_mut().find(|o| o.rect == t.rect) {
                same.kind = t.kind;
            } else {
                targets.push(t);
            }
        }
        if self.title.is_some() || self.sign_edit.is_some() || self.book.is_some() {
            return targets;
        }
        let Some(batch) = self.gui_scale() else {
            return targets;
        };
        let slot = |x: f32, y: f32| Target {
            rect: [x, y, 16.0, 16.0],
            kind: Kind::Slot,
        };
        match self.screen {
            Screen::Inventory => {
                let (ox, oy) = self.inventory_origin(&batch);
                targets.extend(self.slots().into_iter().map(|i| {
                    let (x, y) = self.slot_position(i);
                    slot(ox + x, oy + y)
                }));
                targets.extend(
                    self.station_targets()
                        .into_iter()
                        .map(|[x, y, w, h]| Target {
                            rect: [ox + x, oy + y, w, h],
                            kind: Kind::Button,
                        }),
                );
                targets.extend(self.recipe_book_targets());
            }
            Screen::Creative => {
                let (ox, oy) = Self::creative_origin(&batch);
                if self.creative_inventory_open() {
                    targets.extend(self.slots().into_iter().map(|i| {
                        let (x, y) = self.slot_position(i);
                        slot(ox + x, oy + y)
                    }));
                } else {
                    for r in 0..5 {
                        for c in 0..9 {
                            targets.push(slot(
                                ox + 9.0 + c as f32 * 18.0,
                                oy + 18.0 + r as f32 * 18.0,
                            ));
                        }
                    }
                    for c in 0..9 {
                        targets.push(slot(ox + 9.0 + c as f32 * 18.0, oy + 112.0));
                    }
                }
                for (i, t) in self.creative_screen.tabs.iter().enumerate() {
                    let (x, y) = Creative::tab_position(t);
                    targets.push(Target {
                        rect: [ox + x, oy + y, 26.0, 32.0],
                        kind: Kind::Tab(i == self.creative_screen.selected),
                    });
                }
            }
            _ => {}
        }
        targets
    }

    fn pad_first_target(&self, targets: &[Target]) -> Option<Target> {
        let batch = self.gui_scale()?;
        let at = |x: f32, y: f32| {
            targets
                .iter()
                .find(|t| t.rect[0] == x && t.rect[1] == y)
                .copied()
        };
        if self.title.is_none() {
            match self.screen {
                Screen::Inventory => {
                    let (ox, oy) = self.inventory_origin(&batch);
                    let (x, y) = self.slot_position(self.selected);
                    if let Some(t) = at(ox + x, oy + y) {
                        return Some(t);
                    }
                }
                Screen::Creative if !self.creative_inventory_open() => {
                    let (ox, oy) = Self::creative_origin(&batch);
                    if let Some(t) = at(ox + 9.0 + self.selected as f32 * 18.0, oy + 112.0) {
                        return Some(t);
                    }
                }
                _ => {}
            }
        }
        targets
            .iter()
            .find(|t| t.kind == Kind::Button)
            .or_else(|| targets.iter().find(|t| !matches!(t.kind, Kind::Tab(_))))
            .or(targets.first())
            .copied()
    }

    fn pad_pointer_to(&mut self, event_loop: &ActiveEventLoop, at: (f32, f32)) {
        let (Some(window), Some(batch)) = (self.window.as_ref(), self.gui_scale()) else {
            return;
        };
        let id = window.id();
        let at = (
            at.0.clamp(0.0, batch.width - 1.0),
            at.1.clamp(0.0, batch.height - 1.0),
        );
        let position =
            PhysicalPosition::new(f64::from(at.0 * batch.scale), f64::from(at.1 * batch.scale));
        self.window_event(
            event_loop,
            id,
            WindowEvent::CursorMoved {
                device_id: DeviceId::dummy(),
                position,
            },
        );
        self.pad.menu.pos = Some(self.cursor);
    }

    fn pad_pointer_button(&mut self, event_loop: &ActiveEventLoop, right: bool, pressed: bool) {
        let Some(id) = self.window.as_ref().map(|w| w.id()) else {
            return;
        };
        self.pad.menu.held[usize::from(right)] = pressed;
        let button = if right {
            MouseButton::Right
        } else {
            MouseButton::Left
        };
        let state = if pressed {
            ElementState::Pressed
        } else {
            ElementState::Released
        };
        self.window_event(
            event_loop,
            id,
            WindowEvent::MouseInput {
                device_id: DeviceId::dummy(),
                state,
                button,
            },
        );
    }

    fn pad_click_at(&mut self, event_loop: &ActiveEventLoop, at: (f32, f32), right: bool) {
        self.pad_pointer_to(event_loop, at);
        self.pad_pointer_button(event_loop, right, true);
        self.pad_pointer_button(event_loop, right, false);
    }

    fn pad_wheel(&mut self, event_loop: &ActiveEventLoop, x: f32, y: f32) {
        let Some(id) = self.window.as_ref().map(|w| w.id()) else {
            return;
        };
        self.window_event(
            event_loop,
            id,
            WindowEvent::MouseWheel {
                device_id: DeviceId::dummy(),
                delta: MouseScrollDelta::LineDelta(x, y),
                phase: TouchPhase::Moved,
            },
        );
    }

    fn pad_take_pointer(&mut self, event_loop: &ActiveEventLoop) {
        if self.pad.menu.pos.is_some() {
            return;
        }
        let targets = self.pad_targets();
        let first = self.pad_first_target(&targets);
        if self.pad_focus_mode() && first.is_some() {
            self.pad.menu.focus = first;
        }
        let at = first.map_or(self.cursor, |t| t.center());
        self.pad_pointer_to(event_loop, at);
    }

    fn pad_focusing(&self) -> bool {
        self.pad_focus_mode() && self.pad.menu.focus.is_some()
    }

    pub(super) fn pad_menu_button(
        &mut self,
        event_loop: &ActiveEventLoop,
        button: Button,
        down: bool,
    ) -> Result<()> {
        if !down {
            match button {
                Button::South if self.pad.menu.held[0] => {
                    self.pad_pointer_button(event_loop, false, false)
                }
                Button::West if self.pad.menu.held[1] => {
                    self.pad_pointer_button(event_loop, true, false)
                }
                b if self.pad.menu.repeat.is_some_and(|r| r.0 == b) => self.pad.menu.repeat = None,
                _ => {}
            }
            return Ok(());
        }
        if self.screen == Screen::Options
            && self.ui.options.page() == Page::PadBinds
            && let Some(action) = self.ui.options.selected_pad_action.take()
        {
            if button != Button::Start {
                self.settings.controller.bind(action, Some(button));
            }
            return self.handle_option_events(vec![Event::Changed]);
        }
        if let Some(keyboard) = self.pad.menu.osk.as_mut() {
            if let Some(out) = keyboard.button(button) {
                self.pad_osk_output(out);
            }
            if matches!(
                button,
                Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight
            ) {
                self.pad.menu.repeat = Some((button, Instant::now(), false));
            }
            return Ok(());
        }
        match button {
            Button::East | Button::Start => return self.pad_back(),
            Button::RightStick => {
                self.pad.menu.swapped = !self.pad.menu.swapped;
                self.pad.menu.focus = None;
                self.pad.menu.pending = None;
                self.pad.menu.nudge = None;
                self.pad.menu.pos = None;
                self.pad_take_pointer(event_loop);
            }
            Button::South if self.pad_focusing() => {
                let focus = self.pad.menu.focus.expect("focusing");
                match focus.kind {
                    Kind::Slider { .. } => {}
                    Kind::Field => {
                        self.pad_click_at(event_loop, focus.center(), false);
                        self.pad_open_keyboard();
                    }
                    _ => {
                        self.pad_pointer_to(event_loop, focus.center());
                        self.pad_pointer_button(event_loop, false, true);
                    }
                }
            }
            Button::South | Button::West => {
                self.pad_take_pointer(event_loop);
                self.pad_pointer_button(event_loop, button == Button::West, true);
            }
            Button::North if matches!(self.screen, Screen::Inventory | Screen::Creative) => {
                self.pad_take_pointer(event_loop);
                let held = (
                    self.keys.contains(&KeyCode::ShiftLeft),
                    self.physical_keys.contains(&KeyCode::ShiftLeft),
                );
                self.keys.insert(KeyCode::ShiftLeft);
                self.physical_keys.insert(KeyCode::ShiftLeft);
                self.pad_pointer_button(event_loop, false, true);
                self.pad_pointer_button(event_loop, false, false);
                if !held.0 {
                    self.keys.remove(&KeyCode::ShiftLeft);
                }
                if !held.1 {
                    self.physical_keys.remove(&KeyCode::ShiftLeft);
                }
            }
            Button::Select | Button::LeftStick
                if matches!(self.screen, Screen::Inventory | Screen::Creative)
                    && self.title.is_none() =>
            {
                self.pad_take_pointer(event_loop);
                let all = button == Button::LeftStick;
                let ctrl = self.physical_keys.contains(&KeyCode::ControlLeft);
                if all {
                    self.physical_keys.insert(KeyCode::ControlLeft);
                }
                let result = self.key_pressed(KeyCode::KeyQ);
                if all && !ctrl {
                    self.physical_keys.remove(&KeyCode::ControlLeft);
                }
                result?;
            }
            Button::DPadUp | Button::DPadDown | Button::DPadLeft | Button::DPadRight => {
                self.pad.menu.repeats = 0;
                self.pad_step(event_loop, button);
                self.pad.menu.repeat = Some((button, Instant::now(), false));
            }
            Button::LeftBumper | Button::RightBumper => {
                self.pad_tab(event_loop, button == Button::RightBumper);
            }
            _ => {}
        }
        Ok(())
    }

    fn pad_step(&mut self, event_loop: &ActiveEventLoop, button: Button) {
        if let Some(keyboard) = self.pad.menu.osk.as_mut() {
            keyboard.step(button);
            return;
        }
        if self.pad_focus_mode() {
            let targets = self.pad_targets();
            if !targets.is_empty() {
                self.pad_focus_step(event_loop, button, &targets);
                return;
            }
        }
        self.pad_pointer_step(event_loop, button);
    }

    fn pad_pointer_step(&mut self, event_loop: &ActiveEventLoop, button: Button) {
        if self.pad.menu.pos.is_none() {
            self.pad_take_pointer(event_loop);
            return;
        }
        if self.pad.menu.held[0] {
            let (dx, dy) = match button {
                Button::DPadLeft => (-2.0, 0.0),
                Button::DPadRight => (2.0, 0.0),
                Button::DPadUp => (0.0, -2.0),
                _ => (0.0, 2.0),
            };
            let at = self.cursor;
            self.pad_pointer_to(event_loop, (at.0 + dx, at.1 + dy));
            return;
        }
        let from = self.cursor;
        let centres: Vec<(f32, f32)> = self.pad_targets().iter().map(Target::center).collect();
        if let Some(i) = gamepad::focus_step(from, button, &centres) {
            self.pad_pointer_to(event_loop, centres[i]);
        }
    }

    fn pad_focus_step(&mut self, event_loop: &ActiveEventLoop, button: Button, targets: &[Target]) {
        let Some(focus) = self.pad.menu.focus else {
            self.pad.menu.pos = None;
            self.pad_take_pointer(event_loop);
            return;
        };
        if let Kind::Slider { value, track } = focus.kind
            && matches!(button, Button::DPadLeft | Button::DPadRight)
        {
            let from = match self.pad.menu.nudge {
                Some((rect, x)) if rect == focus.rect => x,
                _ => gamepad::slider_x(track, value),
            };
            let step = gamepad::slider_step(self.pad.menu.repeats);
            let x = (from
                + if button == Button::DPadLeft {
                    -step
                } else {
                    step
                })
            .clamp(track[0], track[0] + track[1]);
            self.pad.menu.nudge = Some((focus.rect, x));
            self.pad_click_at(event_loop, (x, focus.center().1), false);
            return;
        }
        let rects: Vec<Rect> = targets.iter().map(|t| t.rect).collect();
        let vertical = matches!(button, Button::DPadUp | Button::DPadDown);
        let step = if vertical {
            gamepad::focus_step_strict(focus.rect, button, &rects)
        } else {
            gamepad::focus_step_rect(focus.rect, button, &rects)
        };
        match step {
            Some(i) => self.pad_focus_on(event_loop, targets[i]),
            None if vertical => {
                let up = button == Button::DPadUp;
                self.pad_wheel(event_loop, 0.0, if up { 1.0 } else { -1.0 });
                self.pad.menu.pending = Some(Pending {
                    button,
                    from: focus.rect,
                    generation: widgets::generation(),
                    tries: 1,
                    before: rects,
                });
            }
            None => {}
        }
    }

    fn pad_focus_on(&mut self, event_loop: &ActiveEventLoop, target: Target) {
        self.pad.menu.focus = Some(target);
        self.pad.menu.pending = None;
        self.pad.menu.nudge = None;
        if !self.pad.menu.held[0] {
            self.pad_pointer_to(event_loop, target.center());
        }
    }

    fn pad_tab(&mut self, event_loop: &ActiveEventLoop, next: bool) {
        if self.screen == Screen::Creative && self.title.is_none() {
            self.pad_creative_tab(event_loop, next);
            return;
        }
        let tabs: Vec<(Rect, bool)> = self
            .pad_targets()
            .into_iter()
            .filter_map(|t| match t.kind {
                Kind::Tab(selected) => Some((t.rect, selected)),
                _ => None,
            })
            .collect();
        let Some(i) = gamepad::tab_step(&tabs, next) else {
            return;
        };
        let rect = tabs[i].0;
        let back = self.pad.menu.pos;
        self.pad_click_at(
            event_loop,
            (rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0),
            false,
        );
        if self.pad_focus_mode() {
            self.pad.menu.focus = Some(Target {
                rect,
                kind: Kind::Tab(true),
            });
        } else if let Some(at) = back {
            self.pad_pointer_to(event_loop, at);
        }
    }

    fn pad_creative_tab(&mut self, event_loop: &ActiveEventLoop, next: bool) {
        let Some(batch) = self.gui_scale() else {
            return;
        };
        let count = self.creative_screen.tabs.len();
        if count == 0 {
            return;
        }
        let selected = self.creative_screen.selected;
        let tab = if next {
            (selected + 1) % count
        } else {
            (selected + count - 1) % count
        };
        let (ox, oy) = Self::creative_origin(&batch);
        let (x, y) = Creative::tab_position(&self.creative_screen.tabs[tab]);
        let back = self.pad.menu.pos;
        self.pad_click_at(event_loop, (ox + x + 13.0, oy + y + 16.0), false);
        if let Some(at) = back {
            self.pad_pointer_to(event_loop, at);
        }
    }

    pub(super) fn pad_menu_frame(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let seconds = self
            .pad
            .menu
            .last_poll
            .map_or(0.0, |t| now.duration_since(t).as_secs_f32().min(0.05));
        self.pad.menu.last_poll = Some(now);
        let in_menu = self.pad_in_menu() && self.settings.controller.enabled;
        if !in_menu {
            let menu = &mut self.pad.menu;
            menu.open_polls = 0;
            menu.pos = None;
            menu.repeat = None;
            menu.held = [false; 2];
            menu.focus = None;
            menu.swapped = false;
            menu.pending = None;
            menu.nudge = None;
            menu.osk = None;
            menu.osk_dismissed = false;
            self.pad_show_os_pointer(true);
            return;
        }
        self.pad.menu.open_polls = self.pad.menu.open_polls.saturating_add(1);
        if let Some(at) = self.pad.menu.pos
            && ((at.0 - self.cursor.0).abs() > 0.5 || (at.1 - self.cursor.1).abs() > 0.5)
        {
            self.pad.menu.pos = None;
            self.pad.menu.focus = None;
            self.pad.in_use = false;
        }
        if self.pad.in_use && self.pad.menu.pos.is_none() && self.pad.menu.open_polls == 3 {
            self.pad_take_pointer(event_loop);
        }
        let focus_mode = self.pad_focus_mode();
        let targets = if focus_mode {
            self.pad_targets()
        } else {
            Vec::new()
        };
        let focusing = focus_mode && !targets.is_empty();
        if focusing {
            self.pad_follow_focus(event_loop, &targets);
        }
        let (move_deadzone, look_deadzone) = (
            self.settings.controller.move_deadzone,
            self.settings.controller.look_deadzone,
        );
        let state = self.pad.state;
        if focusing || self.pad.menu.osk.is_some() {
            let dir =
                gamepad::stick_direction(gamepad::radial(state.left, move_deadzone), STICK_STEP);
            if dir != self.pad.menu.stick {
                if let Some((b, ..)) = self.pad.menu.repeat
                    && Some(b) == self.pad.menu.stick
                {
                    self.pad.menu.repeat = None;
                }
                self.pad.menu.stick = dir;
                if let Some(button) = dir {
                    if self.pad.menu.pos.is_none() && self.pad.menu.osk.is_none() {
                        self.pad_take_pointer(event_loop);
                    } else {
                        self.pad.menu.repeats = 0;
                        self.pad_step(event_loop, button);
                    }
                    self.pad.menu.repeat = Some((button, now, false));
                }
            }
        } else {
            self.pad.menu.stick = None;
            let (dx, dy) = gamepad::cursor_delta(state.left, move_deadzone, seconds);
            if dx != 0.0 || dy != 0.0 {
                self.pad_take_pointer(event_loop);
                let at = self.cursor;
                self.pad_pointer_to(event_loop, (at.0 + dx, at.1 + dy));
                self.pad.menu.moving = true;
            } else if self.pad.menu.moving {
                self.pad.menu.moving = false;
                let centres: Vec<(f32, f32)> =
                    self.pad_targets().iter().map(Target::center).collect();
                if let Some(i) = gamepad::nearest(self.cursor, &centres, 10.0) {
                    self.pad_pointer_to(event_loop, centres[i]);
                }
            }
        }
        let scroll = gamepad::radial(state.right, look_deadzone);
        let wheel = &mut self.pad.menu.wheel;
        wheel[0] += scroll[0] * SCROLL_SPEED * seconds;
        wheel[1] += scroll[1] * SCROLL_SPEED * seconds;
        if scroll == [0.0, 0.0] {
            *wheel = [0.0, 0.0];
        }
        let lines = [wheel[0].trunc(), wheel[1].trunc()];
        if lines != [0.0, 0.0] {
            wheel[0] -= lines[0];
            wheel[1] -= lines[1];
            self.pad_wheel(event_loop, -lines[0], lines[1]);
        }
        if let Some((button, since, repeating)) = self.pad.menu.repeat {
            let wait = if repeating {
                REPEAT_EVERY
            } else {
                REPEAT_DELAY
            };
            if now.duration_since(since).as_secs_f32() >= wait {
                self.pad.menu.repeat = Some((button, now, true));
                self.pad.menu.repeats = self.pad.menu.repeats.saturating_add(1);
                self.pad_step(event_loop, button);
            }
        }
        self.pad_auto_keyboard();
        self.pad_show_os_pointer(self.pad.menu.pos.is_none());
    }

    fn pad_follow_focus(&mut self, event_loop: &ActiveEventLoop, targets: &[Target]) {
        let rects: Vec<Rect> = targets.iter().map(|t| t.rect).collect();
        let ready = self
            .pad
            .menu
            .pending
            .as_ref()
            .map(|p| widgets::generation() > p.generation);
        if ready == Some(true)
            && let Some(p) = self.pad.menu.pending.take()
        {
            let step = match gamepad::scroll_shift(&p.before, &rects) {
                Some(dy) => {
                    let mut from = p.from;
                    from[1] += dy;
                    self.pad.menu.focus = gamepad::refind(from, &rects, 2.0).map(|i| targets[i]);
                    gamepad::focus_step_strict(from, p.button, &rects).map(|i| (i, Some(from)))
                }
                None => gamepad::focus_step_vague(p.from, p.button, &rects).map(|i| (i, None)),
            };
            match step {
                Some((i, _)) => self.pad_focus_on(event_loop, targets[i]),
                None => {
                    if let Some(dy) = gamepad::scroll_shift(&p.before, &rects)
                        && p.tries < SCROLL_TRIES
                    {
                        let up = p.button == Button::DPadUp;
                        self.pad_wheel(event_loop, 0.0, if up { 1.0 } else { -1.0 });
                        let mut from = p.from;
                        from[1] += dy;
                        self.pad.menu.pending = Some(Pending {
                            from,
                            generation: widgets::generation(),
                            tries: p.tries + 1,
                            before: rects.clone(),
                            ..p
                        });
                    }
                }
            }
        } else if ready.is_none() {
            if let Some(focus) = self.pad.menu.focus {
                self.pad.menu.focus = gamepad::refind(focus.rect, &rects, 12.0).map(|i| targets[i]);
                if self.pad.menu.focus.is_none() && self.pad.menu.pos.is_some() {
                    self.pad.menu.focus = self.pad_first_target(targets);
                }
            } else if self.pad.menu.pos.is_some() {
                let centres: Vec<(f32, f32)> = targets.iter().map(Target::center).collect();
                self.pad.menu.focus =
                    gamepad::nearest(self.cursor, &centres, f32::INFINITY).map(|i| targets[i]);
            }
        }
        let Some(focus) = self.pad.menu.focus else {
            return;
        };
        if self.pad.menu.pos.is_none() || self.pad.menu.held.iter().any(|h| *h) {
            return;
        }
        let at = match self.pad.menu.nudge {
            Some((rect, x)) if rect == focus.rect => (x, focus.center().1),
            _ => focus.center(),
        };
        if (at.0 - self.cursor.0).abs() > 0.5 || (at.1 - self.cursor.1).abs() > 0.5 {
            self.pad_pointer_to(event_loop, at);
        }
    }

    fn pad_show_os_pointer(&mut self, show: bool) {
        if self.pad.menu.os_hidden != show {
            return;
        }
        self.pad.menu.os_hidden = !show;
        if !self.keyboard_mouse {
            return;
        }
        if let Some(window) = &self.window {
            window.set_cursor_visible(show && !self.captured);
        }
    }

    pub(super) fn pad_menu_status(&self) -> serde_json::Value {
        let kind = |k: Kind| match k {
            Kind::Button => "button",
            Kind::Slider { .. } => "slider",
            Kind::Field => "field",
            Kind::Row => "row",
            Kind::Slot => "slot",
            Kind::Tab(true) => "tab*",
            Kind::Tab(false) => "tab",
        };
        let target = |t: &Target| serde_json::json!({ "rect": t.rect, "kind": kind(t.kind) });
        serde_json::json!({
            "mode": if self.pad_focus_mode() { "focus" } else { "pointer" },
            "focus": self.pad.menu.focus.as_ref().map(target),
            "keyboard": self.pad.menu.osk.is_some(),
            "targets": self.pad_targets().iter().map(target).collect::<Vec<_>>(),
        })
    }

    fn pad_typing(&self) -> bool {
        (self.screen == Screen::Chat && self.title.is_none())
            || self.sign_edit.is_some()
            || matches!(self.book, Some(super::books::BookScreen::Edit { .. }))
            || (self.screen == Screen::Creative && self.creative_screen.tab().search)
            || self.recipe_search_focused()
            || self.station_typing()
    }

    fn pad_open_keyboard(&mut self) {
        if gamepad::steam_keyboard(self.settings.controller.keyboard)
            && std::process::Command::new("xdg-open")
                .arg("steam://open/keyboard")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .is_ok()
        {
            return;
        }
        self.pad.menu.osk = Some(Keyboard::default());
    }

    fn pad_auto_keyboard(&mut self) {
        let menu = &self.pad.menu;
        if menu.osk.is_none()
            && !menu.osk_dismissed
            && self.pad.in_use
            && menu.open_polls >= 3
            && self.pad_focus_mode()
            && (self.screen == Screen::Chat || self.sign_edit.is_some())
        {
            self.pad.menu.osk_dismissed = true;
            self.pad_open_keyboard();
        }
    }

    fn pad_osk_output(&mut self, out: osk::Output) {
        use osk::Output;
        let (key, text) = match &out {
            Output::Text(t) => (None, Some(t.as_str())),
            Output::Backspace => (Some(KeyCode::Backspace), None),
            Output::Enter => (Some(KeyCode::Enter), None),
            Output::Close => {
                self.pad.menu.osk = None;
                self.pad.menu.osk_dismissed = true;
                return;
            }
        };
        let backspace = key == Some(KeyCode::Backspace);
        if self.screen == Screen::Chat && self.title.is_none() {
            match text {
                Some(t) => self.chat_type(t),
                None => self.chat_key_code(key.expect("a key"), None),
            }
        } else if self.sign_edit.is_some() {
            self.sign_input(key, text);
        } else if self.book.is_some() {
            self.book_input(key, text);
        } else if self.screen == Screen::Creative && self.creative_screen.tab().search {
            if backspace {
                self.creative_search_backspace(false);
            } else if key != Some(KeyCode::Enter) {
                self.creative_search_edit(text);
            }
        } else if self.recipe_search_focused() {
            if key != Some(KeyCode::Enter) {
                self.recipe_text(text, backspace);
            }
        } else if self.station_typing() {
            if key != Some(KeyCode::Enter) {
                self.station_text(text, backspace);
            }
        } else if self.screen == Screen::WorldOptions {
            if key != Some(KeyCode::Enter) {
                self.world_options_text(text);
            }
        } else if self.screen == Screen::Options {
            let result = match text {
                Some(t) => {
                    self.ui.options.type_text(&self.gui.atlas, t);
                    Ok(())
                }
                None => self.options_key(key.expect("a key"), None),
            };
            if let Err(e) = result {
                log::warn!("Vise: on-screen keyboard: {e}");
            }
        } else if self.title.is_some() {
            match key {
                Some(KeyCode::Enter) => {
                    if let Some(t) = self.title.as_mut() {
                        t.confirm_edit();
                    }
                }
                Some(KeyCode::Backspace) => self.title_edit(None, true),
                _ => self.title_edit(text, false),
            }
        }
        if key == Some(KeyCode::Enter)
            || (!self.pad_typing()
                && self.title.is_none()
                && !matches!(self.screen, Screen::Options | Screen::WorldOptions))
        {
            self.pad.menu.osk = None;
        }
    }

    pub(super) fn draw_pad_cursor(&self, batch: &mut HudBatch) {
        if !self.pad_in_menu() {
            return;
        }
        if let Some(keyboard) = &self.pad.menu.osk {
            let at_top = self
                .pad
                .menu
                .focus
                .is_some_and(|f| f.center().1 > batch.height / 2.0)
                || self.screen == Screen::Chat;
            let mut ui = vise_client::ui_theme::Ui::new(&self.gui.atlas, batch, self.cursor);
            keyboard.draw(&mut ui, at_top, self.ui.options.pad_style);
            ui.finish();
            return;
        }
        let Some((x, y)) = self.pad.menu.pos else {
            return;
        };
        if self.pad_focus_mode()
            && let Some(focus) = self.pad.menu.focus
        {
            self.draw_pad_focus(batch, focus);
            return;
        }
        let atlas = &self.gui.atlas;
        let (x, y) = (x.floor(), y.floor());
        const ROWS: [u8; 10] = [1, 2, 3, 4, 5, 6, 7, 4, 2, 1];
        for (i, &w) in ROWS.iter().enumerate() {
            let row = y + i as f32;
            batch.fill(
                atlas,
                x - 1.0,
                row,
                f32::from(w) + 2.0,
                1.0,
                rgba(0, 0, 0, 255),
            );
        }
        batch.fill(atlas, x - 1.0, y + 10.0, 2.0, 1.0, rgba(0, 0, 0, 255));
        for (i, &w) in ROWS.iter().enumerate().skip(1).take(8) {
            let row = y + i as f32;
            batch.fill(
                atlas,
                x,
                row,
                f32::from(w) - 1.0,
                1.0,
                rgba(255, 255, 255, 255),
            );
        }
    }

    fn draw_pad_focus(&self, batch: &mut HudBatch, focus: Target) {
        let [x, y, w, h] = focus.rect;
        if matches!(focus.kind, Kind::Row | Kind::Field) {
            let atlas = &self.gui.atlas;
            let white = rgba(255, 255, 255, 255);
            batch.fill(atlas, x, y, w, 1.0, white);
            batch.fill(atlas, x, y + h - 1.0, w, 1.0, white);
            batch.fill(atlas, x, y, 1.0, h, white);
            batch.fill(atlas, x + w - 1.0, y, 1.0, h, white);
        }
    }
}
