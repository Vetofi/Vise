use std::{collections::BTreeMap, fs, path::Path};

pub const FILE: &str = "vise-controller.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    South,
    East,
    West,
    North,
    LeftBumper,
    RightBumper,
    LeftTrigger,
    RightTrigger,
    Select,
    Start,
    LeftStick,
    RightStick,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}
impl Button {
    pub const ALL: [Button; 16] = [
        Button::South,
        Button::East,
        Button::West,
        Button::North,
        Button::LeftBumper,
        Button::RightBumper,
        Button::LeftTrigger,
        Button::RightTrigger,
        Button::Select,
        Button::Start,
        Button::LeftStick,
        Button::RightStick,
        Button::DPadUp,
        Button::DPadDown,
        Button::DPadLeft,
        Button::DPadRight,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Button::South => "south",
            Button::East => "east",
            Button::West => "west",
            Button::North => "north",
            Button::LeftBumper => "lb",
            Button::RightBumper => "rb",
            Button::LeftTrigger => "lt",
            Button::RightTrigger => "rt",
            Button::Select => "select",
            Button::Start => "start",
            Button::LeftStick => "ls",
            Button::RightStick => "rs",
            Button::DPadUp => "up",
            Button::DPadDown => "down",
            Button::DPadLeft => "left",
            Button::DPadRight => "right",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.name() == name)
    }
    fn bit(self) -> u32 {
        1 << self as u32
    }
    pub fn label(self, style: GlyphStyle) -> &'static str {
        let i = self as usize;
        match style {
            GlyphStyle::PlayStation => [
                "×", "○", "□", "△", "L1", "R1", "L2", "R2", "Create", "Options", "L3", "R3", "↑",
                "↓", "←", "→",
            ][i],
            GlyphStyle::Nintendo => [
                "B", "A", "Y", "X", "L", "R", "ZL", "ZR", "−", "+", "LS", "RS", "↑", "↓", "←", "→",
            ][i],
            GlyphStyle::Xbox | GlyphStyle::Auto => [
                "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "View", "Menu", "LS", "RS", "↑", "↓",
                "←", "→",
            ][i],
        }
    }
    pub fn color(self, style: GlyphStyle) -> [u8; 3] {
        let face = match self {
            Button::South => 0,
            Button::East => 1,
            Button::West => 2,
            Button::North => 3,
            _ => return [255, 255, 255],
        };
        match style {
            GlyphStyle::PlayStation => [
                [124, 178, 232],
                [235, 110, 110],
                [222, 145, 205],
                [92, 205, 170],
            ][face],
            GlyphStyle::Nintendo => [255, 255, 255],
            GlyphStyle::Xbox | GlyphStyle::Auto => [
                [110, 200, 90],
                [235, 90, 80],
                [80, 150, 240],
                [240, 200, 60],
            ][face],
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadState {
    pub buttons: u32,
    pub left: [f32; 2],
    pub right: [f32; 2],
    pub triggers: [f32; 2],
}
impl PadState {
    pub fn set(&mut self, button: Button, down: bool) {
        if down {
            self.buttons |= button.bit();
        } else {
            self.buttons &= !button.bit();
        }
    }
    pub fn held(&self, button: Button) -> bool {
        self.buttons & button.bit() != 0
    }
    pub fn merge(&self, other: &PadState) -> PadState {
        let add = |a: [f32; 2], b: [f32; 2]| {
            [
                (a[0] + b[0]).clamp(-1.0, 1.0),
                (a[1] + b[1]).clamp(-1.0, 1.0),
            ]
        };
        PadState {
            buttons: self.buttons | other.buttons,
            left: add(self.left, other.left),
            right: add(self.right, other.right),
            triggers: [
                (self.triggers[0] + other.triggers[0]).clamp(0.0, 1.0),
                (self.triggers[1] + other.triggers[1]).clamp(0.0, 1.0),
            ],
        }
    }
    pub fn active(&self, settings: &PadSettings) -> bool {
        self.buttons != 0
            || length(self.left) > settings.move_deadzone
            || length(self.right) > settings.look_deadzone
            || self
                .triggers
                .iter()
                .any(|t| *t >= settings.trigger_threshold)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Edges {
    held: u32,
}
impl Edges {
    pub fn update(&mut self, state: &PadState, threshold: f32) -> Vec<(Button, bool)> {
        let mut now = state.buttons;
        for (trigger, button) in [(0, Button::LeftTrigger), (1, Button::RightTrigger)] {
            let was = self.held & button.bit() != 0;
            let value = state.triggers[trigger];
            let on = if was {
                value >= (threshold - 0.1).max(0.05)
            } else {
                value >= threshold
            };
            if on {
                now |= button.bit();
            } else {
                now &= !button.bit();
            }
        }
        let changed = now ^ self.held;
        self.held = now;
        Button::ALL
            .into_iter()
            .filter(|b| changed & b.bit() != 0)
            .map(|b| (b, now & b.bit() != 0))
            .collect()
    }
    pub fn release_all(&mut self) -> Vec<(Button, bool)> {
        let held = std::mem::take(&mut self.held);
        Button::ALL
            .into_iter()
            .filter(|b| held & b.bit() != 0)
            .map(|b| (b, false))
            .collect()
    }
    pub fn is_held(&self, button: Button) -> bool {
        self.held & button.bit() != 0
    }
}

pub const HOTBAR_PREVIOUS: &str = "pad.hotbarPrevious";
pub const HOTBAR_NEXT: &str = "pad.hotbarNext";

pub const ACTIONS: &[(&str, Option<Button>)] = &[
    ("key.jump", Some(Button::South)),
    ("key.drop", Some(Button::East)),
    ("key.swapOffhand", Some(Button::West)),
    ("key.inventory", Some(Button::North)),
    (HOTBAR_PREVIOUS, Some(Button::LeftBumper)),
    (HOTBAR_NEXT, Some(Button::RightBumper)),
    ("key.use", Some(Button::LeftTrigger)),
    ("key.attack", Some(Button::RightTrigger)),
    ("key.sprint", Some(Button::LeftStick)),
    ("key.sneak", Some(Button::RightStick)),
    ("key.togglePerspective", Some(Button::DPadUp)),
    ("key.pickItem", Some(Button::DPadLeft)),
    ("key.chat", Some(Button::DPadRight)),
    ("key.advancements", Some(Button::DPadDown)),
    ("key.playerlist", Some(Button::Select)),
    ("key.command", None),
    ("key.screenshot", None),
    ("key.toggleGui", None),
    ("key.smoothCamera", None),
];

pub fn action_caption(action: &str) -> &str {
    match action {
        HOTBAR_PREVIOUS => "Hotbar: Previous Slot",
        HOTBAR_NEXT => "Hotbar: Next Slot",
        other => other,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GlyphStyle {
    #[default]
    Auto,
    Xbox,
    PlayStation,
    Nintendo,
}
impl GlyphStyle {
    pub fn name(self) -> &'static str {
        match self {
            GlyphStyle::Auto => "auto",
            GlyphStyle::Xbox => "xbox",
            GlyphStyle::PlayStation => "playstation",
            GlyphStyle::Nintendo => "nintendo",
        }
    }
    pub fn parse(name: &str) -> Self {
        match name {
            "xbox" => GlyphStyle::Xbox,
            "playstation" => GlyphStyle::PlayStation,
            "nintendo" => GlyphStyle::Nintendo,
            _ => GlyphStyle::Auto,
        }
    }
    pub fn resolve(self, detected: GlyphStyle) -> GlyphStyle {
        match self {
            GlyphStyle::Auto => match detected {
                GlyphStyle::Auto => GlyphStyle::Xbox,
                d => d,
            },
            chosen => chosen,
        }
    }
    pub const ALL: [GlyphStyle; 4] = [
        GlyphStyle::Auto,
        GlyphStyle::Xbox,
        GlyphStyle::PlayStation,
        GlyphStyle::Nintendo,
    ];
    pub fn of_vendor(vendor: Option<u16>) -> Self {
        match vendor {
            Some(0x054c) => GlyphStyle::PlayStation,
            Some(0x057e) => GlyphStyle::Nintendo,
            _ => GlyphStyle::Xbox,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MenuNav {
    #[default]
    Focus,
    Pointer,
}
impl MenuNav {
    pub fn name(self) -> &'static str {
        match self {
            MenuNav::Focus => "focus",
            MenuNav::Pointer => "pointer",
        }
    }
    pub fn parse(name: &str) -> Self {
        match name {
            "pointer" => MenuNav::Pointer,
            _ => MenuNav::Focus,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextKeyboard {
    #[default]
    Auto,
    Vise,
    Steam,
}
impl TextKeyboard {
    pub const ALL: [TextKeyboard; 3] = [
        TextKeyboard::Auto,
        TextKeyboard::Vise,
        TextKeyboard::Steam,
    ];
    pub fn name(self) -> &'static str {
        match self {
            TextKeyboard::Auto => "auto",
            TextKeyboard::Vise => "vise",
            TextKeyboard::Steam => "steam",
        }
    }
    pub fn parse(name: &str) -> Self {
        match name {
            "vise" => TextKeyboard::Vise,
            "steam" => TextKeyboard::Steam,
            _ => TextKeyboard::Auto,
        }
    }
}

pub fn steam_keyboard(choice: TextKeyboard) -> bool {
    match choice {
        TextKeyboard::Steam => true,
        TextKeyboard::Vise => false,
        TextKeyboard::Auto => std::env::var("SteamDeck").is_ok_and(|v| v == "1"),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PadSettings {
    pub enabled: bool,
    pub vibration: bool,
    pub adaptive_triggers: bool,
    pub lightbar: Lightbar,
    pub look_sensitivity: f32,
    pub invert_x: bool,
    pub invert_y: bool,
    pub move_deadzone: f32,
    pub look_deadzone: f32,
    pub trigger_threshold: f32,
    pub toggle_sneak: bool,
    pub glyphs: GlyphStyle,
    pub menu_nav: MenuNav,
    pub keyboard: TextKeyboard,
    bindings: BTreeMap<&'static str, Option<Button>>,
}
impl Default for PadSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            vibration: true,
            adaptive_triggers: true,
            lightbar: Lightbar::Health,
            look_sensitivity: 0.5,
            invert_x: false,
            invert_y: false,
            move_deadzone: 0.15,
            look_deadzone: 0.12,
            trigger_threshold: 0.4,
            toggle_sneak: true,
            glyphs: GlyphStyle::Auto,
            menu_nav: MenuNav::Focus,
            keyboard: TextKeyboard::Auto,
            bindings: BTreeMap::new(),
        }
    }
}
fn bool_text(v: bool) -> &'static str {
    if v { "true" } else { "false" }
}
impl PadSettings {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn save(&self, dir: &Path) {
        let path = dir.join(FILE);
        if *self != Self::default() || path.exists() {
            let _ = fs::write(path, self.serialize());
        }
    }
    pub fn parse(text: &str) -> Self {
        let mut s = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            let flag = |d: bool| match value {
                "true" => true,
                "false" => false,
                _ => d,
            };
            let unit = |d: f32| {
                value
                    .parse::<f32>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .map_or(d, |v| v.clamp(0.0, 1.0))
            };
            match key.trim() {
                "enabled" => s.enabled = flag(s.enabled),
                "vibration" => s.vibration = flag(s.vibration),
                "adaptiveTriggers" => s.adaptive_triggers = flag(s.adaptive_triggers),
                "lightbar" => s.lightbar = Lightbar::parse(value),
                "lookSensitivity" => s.look_sensitivity = unit(s.look_sensitivity),
                "invertX" => s.invert_x = flag(s.invert_x),
                "invertY" => s.invert_y = flag(s.invert_y),
                "moveDeadzone" => s.move_deadzone = unit(s.move_deadzone).min(0.9),
                "lookDeadzone" => s.look_deadzone = unit(s.look_deadzone).min(0.9),
                "triggerThreshold" => {
                    s.trigger_threshold = unit(s.trigger_threshold).clamp(0.1, 0.9)
                }
                "toggleSneak" => s.toggle_sneak = flag(s.toggle_sneak),
                "glyphs" => s.glyphs = GlyphStyle::parse(value),
                "menuNavigation" => s.menu_nav = MenuNav::parse(value),
                "keyboard" => s.keyboard = TextKeyboard::parse(value),
                other => {
                    if let Some(action) = other.strip_prefix("bind_") {
                        let button = if value == "none" {
                            Some(None)
                        } else {
                            Button::parse(value).map(Some)
                        };
                        if let Some(button) = button {
                            s.bind(action, button);
                        }
                    }
                }
            }
        }
        s
    }
    pub fn serialize(&self) -> String {
        let mut text = format!(
            "enabled:{}\nvibration:{}\nadaptiveTriggers:{}\nlightbar:{}\nlookSensitivity:{}\ninvertX:{}\ninvertY:{}\nmoveDeadzone:{}\nlookDeadzone:{}\ntriggerThreshold:{}\ntoggleSneak:{}\nglyphs:{}\nmenuNavigation:{}\nkeyboard:{}\n",
            bool_text(self.enabled),
            bool_text(self.vibration),
            bool_text(self.adaptive_triggers),
            self.lightbar.name(),
            self.look_sensitivity,
            bool_text(self.invert_x),
            bool_text(self.invert_y),
            self.move_deadzone,
            self.look_deadzone,
            self.trigger_threshold,
            bool_text(self.toggle_sneak),
            self.glyphs.name(),
            self.menu_nav.name(),
            self.keyboard.name(),
        );
        for (action, button) in &self.bindings {
            text.push_str(&format!(
                "bind_{action}:{}\n",
                button.map_or("none", Button::name)
            ));
        }
        text
    }
    pub fn default_of(action: &str) -> Option<Button> {
        ACTIONS.iter().find(|a| a.0 == action).and_then(|a| a.1)
    }
    pub fn binding(&self, action: &str) -> Option<Button> {
        match self.bindings.get(action) {
            Some(button) => *button,
            None => Self::default_of(action),
        }
    }
    pub fn bind(&mut self, action: &str, button: Option<Button>) {
        let Some((name, default)) = ACTIONS.iter().find(|a| a.0 == action) else {
            return;
        };
        if button == *default {
            self.bindings.remove(name);
        } else {
            self.bindings.insert(name, button);
        }
    }
    pub fn reset_bindings(&mut self) {
        self.bindings.clear();
    }
    pub fn conflicts(&self, action: &str) -> bool {
        self.binding(action)
            .is_some_and(|b| self.actions_of(b).iter().any(|a| *a != action))
    }
    pub fn is_default(&self, action: &str) -> bool {
        !self.bindings.contains_key(action)
    }
    pub fn actions_of(&self, button: Button) -> Vec<&'static str> {
        ACTIONS
            .iter()
            .map(|a| a.0)
            .filter(|a| self.binding(a) == Some(button))
            .collect()
    }
}

fn length(v: [f32; 2]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]).sqrt()
}

pub fn radial(v: [f32; 2], deadzone: f32) -> [f32; 2] {
    let len = length(v);
    if !len.is_finite() || len <= deadzone || len == 0.0 {
        return [0.0, 0.0];
    }
    let scaled = ((len - deadzone) / (1.0 - deadzone)).min(1.0);
    [v[0] / len * scaled, v[1] / len * scaled]
}

pub fn move_vector(stick: [f32; 2], deadzone: f32) -> Option<[f32; 2]> {
    let v = radial(stick, deadzone);
    (v != [0.0, 0.0]).then_some([-v[0], v[1]])
}

pub const YAW_SPEED: f32 = 200.0;
pub const PITCH_SPEED: f32 = 140.0;

pub fn look_delta(
    stick: [f32; 2],
    settings: &PadSettings,
    seconds: f32,
    scoping: bool,
) -> (f32, f32) {
    let v = radial(stick, settings.look_deadzone);
    let len = length(v);
    if len == 0.0 {
        return (0.0, 0.0);
    }
    let curve = len;
    let speed = settings.look_sensitivity * 2.0 * if scoping { 0.25 } else { 1.0 };
    let x = v[0] * curve * if settings.invert_x { -1.0 } else { 1.0 };
    let y = v[1] * curve * if settings.invert_y { -1.0 } else { 1.0 };
    (
        x * YAW_SPEED * speed * seconds,
        -y * PITCH_SPEED * speed * seconds,
    )
}

pub fn hotbar_step(selected: usize, forward: bool) -> usize {
    if forward {
        (selected + 1) % 9
    } else {
        (selected + 8) % 9
    }
}

pub const CURSOR_SPEED: f32 = 300.0;

pub fn cursor_delta(stick: [f32; 2], deadzone: f32, seconds: f32) -> (f32, f32) {
    let v = radial(stick, deadzone);
    let len = length(v);
    if len == 0.0 {
        return (0.0, 0.0);
    }
    let speed = (CURSOR_SPEED * len + 30.0) * seconds;
    (v[0] * speed, -v[1] * speed)
}

pub fn focus_step(from: (f32, f32), button: Button, targets: &[(f32, f32)]) -> Option<usize> {
    let (dx, dy) = match button {
        Button::DPadUp => (0.0, -1.0),
        Button::DPadDown => (0.0, 1.0),
        Button::DPadLeft => (-1.0, 0.0),
        Button::DPadRight => (1.0, 0.0),
        _ => return None,
    };
    targets
        .iter()
        .enumerate()
        .filter_map(|(i, t)| {
            let (x, y) = (t.0 - from.0, t.1 - from.1);
            let along = x * dx + y * dy;
            let across = (x * dy - y * dx).abs();
            (along > 2.0 && across <= along * 2.0).then_some((i, along + 2.0 * across))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

pub type Rect = [f32; 4];

pub fn stick_direction(stick: [f32; 2], threshold: f32) -> Option<Button> {
    let [x, y] = stick;
    if length(stick) < threshold {
        return None;
    }
    Some(if x.abs() > y.abs() {
        if x > 0.0 {
            Button::DPadRight
        } else {
            Button::DPadLeft
        }
    } else if y > 0.0 {
        Button::DPadUp
    } else {
        Button::DPadDown
    })
}

pub fn focus_step_rect(from: Rect, button: Button, targets: &[Rect]) -> Option<usize> {
    focus_step_strict(from, button, targets).or_else(|| focus_step_vague(from, button, targets))
}

fn direction(button: Button) -> Option<(usize, bool)> {
    match button {
        Button::DPadUp => Some((1, false)),
        Button::DPadDown => Some((1, true)),
        Button::DPadLeft => Some((0, false)),
        Button::DPadRight => Some((0, true)),
        _ => None,
    }
}

pub fn focus_step_strict(from: Rect, button: Button, targets: &[Rect]) -> Option<usize> {
    let (axis, positive) = direction(button)?;
    let other = 1 - axis;
    let lo = |r: &Rect, a: usize| r[a];
    let hi = |r: &Rect, a: usize| r[a] + r[a + 2] - 1.0;
    let bound = |r: &Rect, a: usize, pos: bool| if pos { hi(r, a) } else { lo(r, a) };
    let after = |a: f32, b: f32| if positive { a > b + 1e-3 } else { b > a + 1e-3 };
    let mut strip = from;
    strip[axis] = bound(&from, axis, positive) + if positive { 1.0 } else { -1.0 };
    strip[axis + 2] = 1.0;
    let first = bound(&strip, axis, !positive);
    let same = |a: &Rect, b: &Rect| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.5);
    let mut strict: Vec<(usize, f32, f32)> = targets
        .iter()
        .enumerate()
        .filter(|(_, r)| !same(r, &from))
        .filter(|(_, r)| lo(r, other).max(lo(&strip, other)) <= hi(r, other).min(hi(&strip, other)))
        .filter(|(_, r)| {
            let child_first = bound(r, axis, !positive);
            after(child_first, first)
                || ((child_first - first).abs() < 1e-3
                    && after(bound(r, axis, positive), bound(&strip, axis, positive)))
        })
        .map(|(i, r)| (i, bound(r, axis, !positive), lo(r, other)))
        .collect();
    strict.sort_by(|a, b| {
        let primary = if positive {
            a.1.total_cmp(&b.1)
        } else {
            b.1.total_cmp(&a.1)
        };
        primary.then(a.2.total_cmp(&b.2))
    });
    strict.first().map(|s| s.0)
}

pub fn focus_step_vague(from: Rect, button: Button, targets: &[Rect]) -> Option<usize> {
    let (axis, positive) = direction(button)?;
    let other = 1 - axis;
    let lo = |r: &Rect, a: usize| r[a];
    let hi = |r: &Rect, a: usize| r[a] + r[a + 2] - 1.0;
    let bound = |r: &Rect, a: usize, pos: bool| if pos { hi(r, a) } else { lo(r, a) };
    let after = |a: f32, b: f32| if positive { a > b + 1e-3 } else { b > a + 1e-3 };
    let same = |a: &Rect, b: &Rect| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.5);
    let mut strip = from;
    strip[axis] = bound(&from, axis, positive) + if positive { 1.0 } else { -1.0 };
    strip[axis + 2] = 1.0;
    let centre = |r: &Rect, a: usize| (hi(r, a) + lo(r, a)) / 2.0;
    let edge = bound(&strip, axis, positive);
    let mid = centre(&strip, other);
    targets
        .iter()
        .enumerate()
        .filter(|(_, r)| !same(r, &from))
        .filter_map(|(i, r)| {
            let facing = bound(r, axis, !positive);
            after(facing, edge).then(|| {
                let (d1, d2) = (facing - edge, centre(r, other) - mid);
                (i, d1 * d1 + d2 * d2)
            })
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

pub fn scroll_shift(before: &[Rect], after: &[Rect]) -> Option<f32> {
    let same = |a: &Rect, b: &Rect| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 0.5);
    if before.len() == after.len() && before.iter().zip(after).all(|(a, b)| same(a, b)) {
        return None;
    }
    let mut votes: Vec<(f32, usize)> = Vec::new();
    for b in before {
        for a in after {
            let dy = a[1] - b[1];
            if (a[0] - b[0]).abs() < 0.5
                && (a[2] - b[2]).abs() < 0.5
                && (a[3] - b[3]).abs() < 0.5
                && dy.abs() >= 0.5
            {
                match votes.iter_mut().find(|v| (v.0 - dy).abs() < 0.5) {
                    Some(v) => v.1 += 1,
                    None => votes.push((dy, 1)),
                }
            }
        }
    }
    votes
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.abs().total_cmp(&a.0.abs())))
        .map(|v| v.0)
}

pub fn refind(focus: Rect, targets: &[Rect], radius: f32) -> Option<usize> {
    let same = |r: &Rect| r.iter().zip(&focus).all(|(a, b)| (a - b).abs() < 0.5);
    if let Some(i) = targets.iter().position(same) {
        return Some(i);
    }
    let c = |r: &Rect| (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    let centres: Vec<(f32, f32)> = targets.iter().map(c).collect();
    nearest(c(&focus), &centres, radius)
}

pub fn tab_step(tabs: &[(Rect, bool)], forward: bool) -> Option<usize> {
    if tabs.len() < 2 {
        return None;
    }
    let mut order: Vec<usize> = (0..tabs.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (tabs[a].0, tabs[b].0);
        ra[1].total_cmp(&rb[1]).then(ra[0].total_cmp(&rb[0]))
    });
    let n = order.len();
    let at = order.iter().position(|&i| tabs[i].1).unwrap_or(0);
    Some(
        order[if forward {
            (at + 1) % n
        } else {
            (at + n - 1) % n
        }],
    )
}

pub fn slider_x(track: [f32; 2], value: f32) -> f32 {
    track[0] + value.clamp(0.0, 1.0) * track[1]
}

pub fn slider_step(repeats: u32) -> f32 {
    [1.0, 1.0, 2.0, 2.0, 4.0, 4.0, 8.0][repeats.min(6) as usize]
}

pub fn nearest(at: (f32, f32), targets: &[(f32, f32)], radius: f32) -> Option<usize> {
    targets
        .iter()
        .enumerate()
        .map(|(i, t)| (i, (t.0 - at.0).hypot(t.1 - at.1)))
        .filter(|(_, d)| *d <= radius)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HealthWatch {
    last: Option<f32>,
}
impl HealthWatch {
    pub fn observe(&mut self, health: Option<f32>) -> Option<f32> {
        let before = std::mem::replace(&mut self.last, health);
        match (before, health) {
            (Some(before), Some(now)) if now < before => Some(before - now),
            _ => None,
        }
    }
    pub fn forget(&mut self) {
        self.last = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Rumble {
    Hurt(f32),
    BlockBroken,
    Explosion {
        distance: f64,
        radius: f32,
    },
    Totem,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulse {
    pub strong: f32,
    pub weak: f32,
    pub millis: u32,
}
impl Rumble {
    pub fn name(self) -> &'static str {
        match self {
            Rumble::Hurt(_) => "hurt",
            Rumble::BlockBroken => "block",
            Rumble::Explosion { .. } => "explosion",
            Rumble::Totem => "totem",
        }
    }
    pub fn pulse(self) -> Option<Pulse> {
        match self {
            Rumble::Hurt(amount) if amount > 0.0 => Some(Pulse {
                strong: (0.2 + 0.06 * amount).min(0.8),
                weak: 0.3,
                millis: (150.0 + 20.0 * amount).min(400.0) as u32,
            }),
            Rumble::Hurt(_) => None,
            Rumble::BlockBroken => Some(Pulse {
                strong: 0.0,
                weak: 0.25,
                millis: 60,
            }),
            Rumble::Explosion { distance, radius } => {
                let reach = f64::from(radius.max(1.0)) * 4.0;
                let near = (1.0 - distance / reach) as f32;
                (near > 0.05).then_some(Pulse {
                    strong: 0.9 * near,
                    weak: 0.6 * near,
                    millis: 350,
                })
            }
            Rumble::Totem => Some(Pulse {
                strong: 0.7,
                weak: 0.7,
                millis: 500,
            }),
        }
    }

    pub fn shape(self) -> Vec<Pulse> {
        let Some(p) = self.pulse() else {
            return Vec::new();
        };
        let step = |level: f32, millis: u32| Pulse {
            strong: p.strong * level,
            weak: p.weak * level,
            millis,
        };
        match self {
            Rumble::BlockBroken => vec![Pulse { millis: 35, ..p }],
            Rumble::Hurt(_) => {
                let tail = p.millis.saturating_sub(50) / 2;
                vec![step(1.0, 50), step(0.55, tail), step(0.25, tail)]
            }
            Rumble::Explosion { .. } => vec![
                step(1.0, 80),
                step(0.6, 150),
                step(0.35, 150),
                step(0.15, 150),
            ],
            Rumble::Totem => vec![
                step(0.4, 100),
                step(0.7, 100),
                step(1.0, 200),
                step(0.5, 100),
            ],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TriggerFeel {
    Free,
    Draw(f32),
    Click,
    Boost,
    Brace,
    Locked,
    Texture(f32),
}
#[cfg(feature = "controllers")]
impl TriggerFeel {
    pub fn effect(self, threshold: f32) -> vise_dualsense::TriggerEffect {
        use vise_dualsense::trigger;
        let press = ((threshold.clamp(0.0, 1.0) * 10.0) as u8).clamp(3, 8);
        match self {
            TriggerFeel::Free => vise_dualsense::TRIGGER_OFF,
            TriggerFeel::Draw(charge) => {
                trigger::feedback(2, 1 + (5.0 * charge.clamp(0.0, 1.0)).round() as u8)
            }
            TriggerFeel::Click => trigger::weapon((press - 2).max(2), press, 8),
            TriggerFeel::Boost => trigger::weapon((press - 2).max(2), press, 5),
            TriggerFeel::Brace => trigger::feedback(2, 3),
            TriggerFeel::Locked => trigger::feedback(1, 8),
            TriggerFeel::Texture(hardness) if hardness > 0.0 => {
                let amplitude = (1.0 + 1.6 * hardness.sqrt()).round().clamp(1.0, 5.0);
                let hertz = match hardness {
                    h if h < 1.0 => 40,
                    h if h < 2.0 => 30,
                    h if h < 5.0 => 20,
                    _ => 10,
                };
                trigger::vibration(1, amplitude as u8, hertz)
            }
            TriggerFeel::Texture(_) => vise_dualsense::TRIGGER_OFF,
        }
    }
}
#[cfg(feature = "controllers")]
impl PadSettings {
    pub fn trigger_effects(
        &self,
        using: TriggerFeel,
        attacking: TriggerFeel,
    ) -> [vise_dualsense::TriggerEffect; 2] {
        [Button::LeftTrigger, Button::RightTrigger].map(|trigger| {
            let actions = self.actions_of(trigger);
            let feel = if !self.adaptive_triggers {
                TriggerFeel::Free
            } else if actions.contains(&"key.use") {
                using
            } else if actions.contains(&"key.attack") {
                attacking
            } else {
                TriggerFeel::Free
            };
            feel.effect(self.trigger_threshold)
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lightbar {
    Health,
    Dimension,
    Off,
}
impl Lightbar {
    pub const ALL: [Lightbar; 3] = [Lightbar::Health, Lightbar::Dimension, Lightbar::Off];
    pub fn name(self) -> &'static str {
        match self {
            Lightbar::Health => "health",
            Lightbar::Dimension => "dimension",
            Lightbar::Off => "off",
        }
    }
    pub fn parse(name: &str) -> Self {
        match name.trim() {
            "dimension" => Lightbar::Dimension,
            "off" => Lightbar::Off,
            _ => Lightbar::Health,
        }
    }
    pub fn colour(self, health: f32, dimension: &str) -> Option<[u8; 3]> {
        let rgb = match self {
            Lightbar::Off => return None,
            Lightbar::Health => {
                let (green, amber, red) = (
                    [34.0, 197.0, 94.0],
                    [245.0, 158.0, 11.0],
                    [239.0, 68.0, 68.0],
                );
                let h = health.clamp(0.0, 1.0);
                let (from, to, k) = if h >= 0.5 {
                    (amber, green, (h - 0.5) * 2.0)
                } else {
                    (red, amber, h * 2.0)
                };
                [0, 1, 2].map(|i| from[i] + (to[i] - from[i]) * k)
            }
            Lightbar::Dimension => match dimension {
                "minecraft:the_nether" => [234.0, 88.0, 12.0],
                "minecraft:the_end" => [139.0, 92.0, 246.0],
                _ => [14.0, 165.0, 233.0],
            },
        };
        Some(rgb.map(|v| (v * 0.5).round() as u8))
    }
}

pub fn bow_power(ticks: u32) -> f32 {
    let f = ticks as f32 / 20.0;
    ((f * f + f * 2.0) / 3.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_bedrock() {
        let s = PadSettings::default();
        assert_eq!(s.actions_of(Button::South), ["key.jump"]);
        assert_eq!(s.actions_of(Button::RightTrigger), ["key.attack"]);
        assert_eq!(s.actions_of(Button::LeftTrigger), ["key.use"]);
        assert_eq!(s.actions_of(Button::North), ["key.inventory"]);
        assert_eq!(s.actions_of(Button::RightBumper), [HOTBAR_NEXT]);
        assert!(s.actions_of(Button::Start).is_empty());
        for (action, _) in ACTIONS {
            assert!(
                action.starts_with("pad.")
                    || crate::keybinds::MAPPINGS.iter().any(|m| m.0 == *action),
                "{action}"
            );
        }
    }

    #[test]
    fn settings_round_trip_and_store_only_changes() {
        let mut s = PadSettings::default();
        assert!(!s.serialize().contains("bind_"));
        s.bind("key.jump", Some(Button::East));
        s.bind("key.drop", None);
        s.bind("key.sneak", Some(Button::RightStick));
        s.invert_y = true;
        s.look_sensitivity = 0.75;
        s.glyphs = GlyphStyle::PlayStation;
        let text = s.serialize();
        assert!(text.contains("bind_key.jump:east\n"));
        assert!(text.contains("bind_key.drop:none\n"));
        assert!(!text.contains("key.sneak"));
        let back = PadSettings::parse(&text);
        assert_eq!(back, s);
        assert_eq!(back.binding("key.drop"), None);
        assert_eq!(back.actions_of(Button::East), ["key.jump"]);
        let odd =
            PadSettings::parse("lookSensitivity:NaN\nbind_key.nope:south\nbind_key.jump:x9\n");
        assert_eq!(odd, PadSettings::default());
    }

    #[test]
    fn radial_deadzone_rescales() {
        assert_eq!(radial([0.1, 0.05], 0.15), [0.0, 0.0]);
        let full = radial([1.0, 0.0], 0.15);
        assert!((full[0] - 1.0).abs() < 1e-6);
        let half = radial([0.0, 0.575], 0.15);
        assert!((half[1] - 0.5).abs() < 1e-6);
        let over = radial([1.0, 1.0], 0.15);
        assert!((length(over) - 1.0).abs() < 1e-6);
        assert_eq!(move_vector([0.0, 0.0], 0.15), None);
        let m = move_vector([1.0, 0.0], 0.15).unwrap();
        assert!(m[0] < -0.99 && m[1] == 0.0, "right is negative left");
    }

    #[test]
    fn look_turns_right_and_up() {
        let s = PadSettings::default();
        let (yaw, pitch) = look_delta([1.0, 0.0], &s, 1.0, false);
        assert!((yaw - YAW_SPEED).abs() < 1e-3 && pitch == 0.0);
        let (_, pitch) = look_delta([0.0, 1.0], &s, 0.5, false);
        assert!((pitch + PITCH_SPEED * 0.5).abs() < 1e-3, "up looks up");
        let inverted = PadSettings {
            invert_y: true,
            ..PadSettings::default()
        };
        assert!(look_delta([0.0, 1.0], &inverted, 1.0, false).1 > 0.0);
        let (small, _) = look_delta([0.5, 0.0], &s, 1.0, false);
        assert!(
            small < YAW_SPEED * 0.5,
            "the curve is gentler near the centre"
        );
        assert_eq!(look_delta([0.05, 0.05], &s, 1.0, false), (0.0, 0.0));
        assert!(look_delta([1.0, 0.0], &s, 1.0, true).0 < YAW_SPEED * 0.5);
    }

    #[test]
    fn edges_with_trigger_hysteresis() {
        let mut edges = Edges::default();
        let mut state = PadState::default();
        state.set(Button::South, true);
        assert_eq!(edges.update(&state, 0.4), [(Button::South, true)]);
        assert!(edges.update(&state, 0.4).is_empty());
        state.triggers[1] = 0.45;
        assert_eq!(edges.update(&state, 0.4), [(Button::RightTrigger, true)]);
        state.triggers[1] = 0.35;
        assert!(
            edges.update(&state, 0.4).is_empty(),
            "inside the hysteresis"
        );
        state.triggers[1] = 0.2;
        state.set(Button::South, false);
        assert_eq!(
            edges.update(&state, 0.4),
            [(Button::South, false), (Button::RightTrigger, false)]
        );
        state.set(Button::DPadUp, true);
        edges.update(&state, 0.4);
        assert_eq!(edges.release_all(), [(Button::DPadUp, false)]);
    }

    #[test]
    fn merge_and_activity() {
        let s = PadSettings::default();
        let mut a = PadState::default();
        assert!(!a.active(&s));
        a.left = [0.1, 0.0];
        assert!(!a.active(&s), "drift inside the deadzone");
        let mut b = PadState::default();
        b.set(Button::North, true);
        b.left = [0.95, 0.0];
        let m = a.merge(&b);
        assert!(m.held(Button::North) && m.left[0] == 1.0 && m.active(&s));
    }

    #[test]
    fn focus_moves_between_slots() {
        let mut targets: Vec<(f32, f32)> = (0..6)
            .map(|i| (8.0 + (i % 3) as f32 * 18.0, 8.0 + (i / 3) as f32 * 18.0))
            .collect();
        targets.push((26.0, 80.0));
        let from = targets[0];
        assert_eq!(focus_step(from, Button::DPadRight, &targets), Some(1));
        assert_eq!(focus_step(from, Button::DPadDown, &targets), Some(3));
        assert_eq!(focus_step(from, Button::DPadUp, &targets), None);
        assert_eq!(focus_step(from, Button::DPadLeft, &targets), None);
        assert_eq!(focus_step(targets[4], Button::DPadDown, &targets), Some(6));
        assert_eq!(focus_step(targets[2], Button::DPadLeft, &targets), Some(1));
        assert_eq!(nearest((12.0, 10.0), &targets, 9.0), Some(0));
        assert_eq!(nearest((100.0, 100.0), &targets, 9.0), None);
        let (x, y) = cursor_delta([0.0, 1.0], 0.15, 0.1);
        assert!(x == 0.0 && y < -30.0, "up moves up the screen");
        assert_eq!(cursor_delta([0.1, 0.0], 0.15, 0.1), (0.0, 0.0));
    }

    #[test]
    fn focus_follows_vanilla_arrow_navigation() {
        let mut rects: Vec<Rect> = (0..6)
            .map(|i| {
                let (col, row) = (i % 2, i / 2);
                [
                    5.0 + col as f32 * 160.0,
                    40.0 + row as f32 * 24.0,
                    150.0,
                    20.0,
                ]
            })
            .collect();
        rects.push([85.0, 120.0, 150.0, 20.0]);
        assert_eq!(
            focus_step_rect(rects[0], Button::DPadRight, &rects),
            Some(1)
        );
        assert_eq!(focus_step_rect(rects[1], Button::DPadRight, &rects), None);
        assert_eq!(focus_step_rect(rects[0], Button::DPadDown, &rects), Some(2));
        assert_eq!(focus_step_rect(rects[3], Button::DPadUp, &rects), Some(1));
        assert_eq!(focus_step_rect(rects[0], Button::DPadUp, &rects), None);
        assert_eq!(focus_step_rect(rects[5], Button::DPadDown, &rects), Some(6));
        assert_eq!(focus_step_rect(rects[4], Button::DPadDown, &rects), Some(6));
        assert_eq!(focus_step_rect(rects[6], Button::DPadUp, &rects), Some(4));
        let grid: Vec<Rect> = vec![
            [10.0, 10.0, 16.0, 16.0],
            [28.0, 10.0, 16.0, 16.0],
            [10.0, 28.0, 16.0, 16.0],
            [28.0, 28.0, 16.0, 16.0],
            [80.0, 50.0, 16.0, 16.0],
        ];
        assert_eq!(focus_step_rect(grid[1], Button::DPadRight, &grid), Some(4));
        assert_eq!(focus_step_rect(grid[4], Button::DPadLeft, &grid), Some(3));
        assert_eq!(focus_step_rect(grid[0], Button::DPadDown, &grid), Some(2));
        let dup = vec![grid[0], grid[0]];
        assert_eq!(focus_step_rect(grid[0], Button::DPadRight, &dup), None);
        assert_eq!(focus_step_rect(grid[0], Button::South, &grid), None);
    }

    #[test]
    fn refind_tabs_sliders_and_stick_steps() {
        let rects: Vec<Rect> = vec![[0.0, 0.0, 10.0, 10.0], [0.0, 20.0, 10.0, 10.0]];
        assert_eq!(refind([0.0, 20.0, 10.0, 10.0], &rects, 8.0), Some(1));
        assert_eq!(refind([0.0, 24.0, 10.0, 10.0], &rects, 8.0), Some(1));
        assert_eq!(refind([90.0, 90.0, 10.0, 10.0], &rects, 8.0), None);
        let before: Vec<Rect> = (0..5)
            .map(|i| [10.0, 40.0 + 24.0 * i as f32, 100.0, 20.0])
            .chain([[10.0, 200.0, 50.0, 20.0]])
            .collect();
        let after: Vec<Rect> = (0..5)
            .map(|i| [10.0, 16.0 + 24.0 * i as f32, 100.0, 20.0])
            .chain([[10.0, 200.0, 50.0, 20.0]])
            .collect();
        assert_eq!(scroll_shift(&before, &after), Some(-24.0));
        assert_eq!(scroll_shift(&before, &before), None);
        let tabs = [
            ([60.0, 0.0, 26.0, 32.0], false),
            ([0.0, 0.0, 26.0, 32.0], true),
            ([30.0, 0.0, 26.0, 32.0], false),
        ];
        assert_eq!(tab_step(&tabs, true), Some(2));
        assert_eq!(tab_step(&tabs, false), Some(0), "wraps to the last");
        assert_eq!(tab_step(&tabs[..1], true), None);
        assert_eq!(slider_x([14.0, 142.0], 0.0), 14.0);
        assert_eq!(slider_x([14.0, 142.0], 1.0), 156.0);
        assert!(slider_step(0) == 1.0 && slider_step(99) == 8.0);
        assert_eq!(stick_direction([0.9, 0.2], 0.5), Some(Button::DPadRight));
        assert_eq!(stick_direction([-0.2, -0.8], 0.5), Some(Button::DPadDown));
        assert_eq!(stick_direction([0.3, 0.3], 0.5), None);
        let mut s = PadSettings::default();
        assert_eq!(s.menu_nav, MenuNav::Focus);
        s.menu_nav = MenuNav::Pointer;
        s.keyboard = TextKeyboard::Vise;
        let back = PadSettings::parse(&s.serialize());
        assert_eq!(
            (back.menu_nav, back.keyboard),
            (MenuNav::Pointer, TextKeyboard::Vise)
        );
        assert!(steam_keyboard(TextKeyboard::Steam) && !steam_keyboard(TextKeyboard::Vise));
    }

    #[test]
    fn labels_and_conflicts() {
        assert_eq!(Button::South.label(GlyphStyle::Xbox), "A");
        assert_eq!(Button::South.label(GlyphStyle::PlayStation), "×");
        assert_eq!(Button::South.label(GlyphStyle::Nintendo), "B");
        assert_eq!(Button::RightTrigger.label(GlyphStyle::PlayStation), "R2");
        assert_eq!(Button::DPadLeft.label(GlyphStyle::Xbox), "←");
        assert_eq!(
            GlyphStyle::Auto.resolve(GlyphStyle::PlayStation),
            GlyphStyle::PlayStation
        );
        assert_eq!(
            GlyphStyle::Xbox.resolve(GlyphStyle::PlayStation),
            GlyphStyle::Xbox
        );
        assert_eq!(GlyphStyle::Auto.resolve(GlyphStyle::Auto), GlyphStyle::Xbox);
        let mut s = PadSettings::default();
        assert!(!s.conflicts("key.jump"));
        s.bind("key.drop", Some(Button::South));
        assert!(s.conflicts("key.jump") && s.conflicts("key.drop"));
        assert!(!s.is_default("key.drop") && s.is_default("key.jump"));
        assert_eq!(action_caption(HOTBAR_NEXT), "Hotbar: Next Slot");
        assert_eq!(action_caption("key.jump"), "key.jump");
    }

    #[test]
    fn health_watch_only_counts_damage_in_one_life() {
        let mut w = HealthWatch::default();
        assert_eq!(w.observe(Some(20.0)), None);
        assert_eq!(w.observe(Some(15.0)), Some(5.0));
        assert_eq!(w.observe(Some(18.0)), None, "healing");
        w.forget();
        assert_eq!(w.observe(Some(6.0)), None);
        assert_eq!(w.observe(None), None);
        assert_eq!(w.observe(Some(2.0)), None);
        assert_eq!(w.observe(Some(1.0)), Some(1.0));
    }

    #[cfg(feature = "controllers")]
    #[test]
    fn triggers_feel_their_action() {
        use vise_dualsense::{TRIGGER_OFF, trigger};
        let mut s = PadSettings::default();
        let [left, right] = s.trigger_effects(TriggerFeel::Draw(1.0), TriggerFeel::Texture(1.5));
        assert_eq!(left, trigger::feedback(2, 6));
        assert_eq!(right, trigger::vibration(1, 3, 30));
        assert_eq!(
            s.trigger_effects(TriggerFeel::Draw(0.0), TriggerFeel::Free),
            [trigger::feedback(2, 1), TRIGGER_OFF]
        );
        let catch = |strength| [0x25, 0x14, 0x00, strength, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(TriggerFeel::Click.effect(0.4), catch(7));
        assert_eq!(TriggerFeel::Boost.effect(0.4), catch(4));
        assert_eq!(TriggerFeel::Click.effect(0.7)[1..3], [0xa0, 0x00]);
        assert_eq!(TriggerFeel::Click.effect(0.1), trigger::weapon(2, 3, 8));
        assert_eq!(TriggerFeel::Brace.effect(0.4), trigger::feedback(2, 3));
        assert_eq!(TriggerFeel::Locked.effect(0.4), trigger::feedback(1, 8));
        assert_eq!(
            TriggerFeel::Texture(0.5).effect(0.4),
            trigger::vibration(1, 2, 40)
        );
        assert_eq!(
            TriggerFeel::Texture(3.0).effect(0.4),
            trigger::vibration(1, 4, 20)
        );
        assert_eq!(
            TriggerFeel::Texture(50.0).effect(0.4),
            trigger::vibration(1, 5, 10)
        );
        assert_eq!(TriggerFeel::Texture(0.0).effect(0.4), TRIGGER_OFF);
        assert_eq!(TriggerFeel::Texture(-1.0).effect(0.4), TRIGGER_OFF);
        s.adaptive_triggers = false;
        assert_eq!(
            s.trigger_effects(TriggerFeel::Click, TriggerFeel::Boost),
            [TRIGGER_OFF; 2]
        );
        assert!(!PadSettings::parse(&s.serialize()).adaptive_triggers);
    }

    #[test]
    fn lightbar_colours() {
        let full = Lightbar::Health.colour(1.0, "minecraft:overworld").unwrap();
        let empty = Lightbar::Health.colour(0.0, "minecraft:overworld").unwrap();
        assert!(full[1] > full[0], "green at full health: {full:?}");
        assert!(empty[0] > empty[1], "red when dying: {empty:?}");
        assert_eq!(Lightbar::Health.colour(0.5, ""), Some([123, 79, 6]));
        assert_ne!(
            Lightbar::Dimension.colour(1.0, "minecraft:the_nether"),
            Lightbar::Dimension.colour(1.0, "minecraft:overworld")
        );
        assert_eq!(Lightbar::Off.colour(1.0, ""), None);
        for l in Lightbar::ALL {
            assert_eq!(Lightbar::parse(l.name()), l);
        }
    }

    #[test]
    fn bow_draw() {
        assert_eq!(bow_power(0), 0.0);
        assert!((bow_power(10) - 5.0 / 12.0).abs() < 1e-6);
        assert_eq!(bow_power(20), 1.0);
        assert_eq!(bow_power(40), 1.0);
    }

    #[test]
    fn rumble_pulses() {
        let small = Rumble::Hurt(1.0).pulse().unwrap();
        let big = Rumble::Hurt(10.0).pulse().unwrap();
        assert!(big.strong > small.strong && big.millis > small.millis);
        assert!(Rumble::Hurt(100.0).pulse().unwrap().strong <= 0.8);
        assert_eq!(Rumble::Hurt(0.0).pulse(), None);
        let near = Rumble::Explosion {
            distance: 2.0,
            radius: 4.0,
        }
        .pulse()
        .unwrap();
        let far = Rumble::Explosion {
            distance: 12.0,
            radius: 4.0,
        }
        .pulse()
        .unwrap();
        assert!(near.strong > far.strong);
        assert_eq!(
            Rumble::Explosion {
                distance: 20.0,
                radius: 4.0
            }
            .pulse(),
            None
        );
        assert_eq!(Rumble::BlockBroken.pulse().unwrap().strong, 0.0);
        for rumble in [
            Rumble::Hurt(4.0),
            Rumble::Explosion {
                distance: 2.0,
                radius: 4.0,
            },
        ] {
            let (pulse, shape) = (rumble.pulse().unwrap(), rumble.shape());
            assert_eq!(shape[0].strong, pulse.strong);
            assert!(shape.windows(2).all(|w| w[1].strong < w[0].strong));
        }
        let hurt: u32 = Rumble::Hurt(4.0).shape().iter().map(|s| s.millis).sum();
        assert!(hurt.abs_diff(Rumble::Hurt(4.0).pulse().unwrap().millis) <= 1);
        assert_eq!(
            Rumble::Totem.shape().iter().map(|s| s.millis).sum::<u32>(),
            500
        );
        assert_eq!(Rumble::BlockBroken.shape()[0].millis, 35);
        assert!(Rumble::Hurt(0.0).shape().is_empty());
    }

    #[test]
    fn hotbar_wraps_and_glyphs_follow_the_vendor() {
        assert_eq!(hotbar_step(8, true), 0);
        assert_eq!(hotbar_step(0, false), 8);
        assert_eq!(hotbar_step(3, true), 4);
        assert_eq!(GlyphStyle::of_vendor(Some(0x054c)), GlyphStyle::PlayStation);
        assert_eq!(GlyphStyle::of_vendor(Some(0x28de)), GlyphStyle::Xbox);
        assert_eq!(GlyphStyle::of_vendor(None), GlyphStyle::Xbox);
    }
}
