use std::fmt::Write as _;
use std::time::Duration;

pub const FONT: &str = "Geist";
pub const MONO: &str = "Geist Mono";
pub mod night;

pub const PIXEL: &str = "Pixelify Sans";

pub mod fonts {
    pub const GEIST_REGULAR: &[u8] = include_bytes!("../fonts/Geist-Regular.ttf");
    pub const GEIST_MEDIUM: &[u8] = include_bytes!("../fonts/Geist-Medium.ttf");
    pub const GEIST_SEMIBOLD: &[u8] = include_bytes!("../fonts/Geist-SemiBold.ttf");
    pub const GEIST_MONO: &[u8] = include_bytes!("../fonts/GeistMono-Regular.ttf");
    pub const PIXEL_MEDIUM: &[u8] = include_bytes!("../fonts/PixelifySans-Medium.ttf");
    pub const PIXEL_SEMIBOLD: &[u8] = include_bytes!("../fonts/PixelifySans-SemiBold.ttf");
    pub const PIXEL_BOLD: &[u8] = include_bytes!("../fonts/PixelifySans-Bold.ttf");
    pub const BRICOLAGE_REGULAR: &[u8] = include_bytes!("../fonts/BricolageGrotesque-Regular.ttf");
    pub const BRICOLAGE_MEDIUM: &[u8] = include_bytes!("../fonts/BricolageGrotesque-Medium.ttf");
    pub const BRICOLAGE_SEMIBOLD: &[u8] =
        include_bytes!("../fonts/BricolageGrotesque-SemiBold.ttf");
    pub const BRICOLAGE_BOLD: &[u8] = include_bytes!("../fonts/BricolageGrotesque-Bold.ttf");
    pub const BRICOLAGE_DISPLAY_BOLD: &[u8] =
        include_bytes!("../fonts/BricolageGrotesqueDisplay-Bold.ttf");
}

pub mod icons {
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            pub const ALL: &[(&str, &[u8])] = &[
                $(($name, include_bytes!(concat!("../icons/pixel/", $name, ".svg")))),*
            ];
        };
    }
    icons!(
        "archive",
        "arrow-left",
        "book-open",
        "box",
        "bug",
        "check",
        "chevron-down",
        "chevron-right",
        "clock",
        "close",
        "download",
        "external-link",
        "folder",
        "gamepad",
        "globe",
        "heart",
        "home",
        "image",
        "info-box",
        "message",
        "monitor",
        "notes",
        "package",
        "play",
        "plus",
        "reload",
        "search",
        "server",
        "sliders",
        "star",
        "terminal",
        "trash",
        "user",
        "zap",
    );

    pub fn get(name: &str) -> Option<&'static [u8]> {
        ALL.iter().find(|(n, _)| *n == name).map(|(_, svg)| *svg)
    }
}

pub const ICON_PNG: &[u8] = include_bytes!("../icons/vise-64.png");

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Web {
    Fixed,
    Fluid(f32, f32),
    Viewport(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub name: &'static str,
    pub px: f32,
    pub web: Web,
    pub line: f32,
    pub tracking: f32,
    pub weight: u16,
    pub mono: bool,
}

impl Step {
    pub fn line_px(&self) -> f32 {
        (self.px * self.line).round()
    }
}

const fn step(
    name: &'static str,
    px: f32,
    web: Web,
    line: f32,
    tracking: f32,
    weight: u16,
    mono: bool,
) -> Step {
    Step {
        name,
        px,
        web,
        line,
        tracking,
        weight,
        mono,
    }
}

pub const MICRO: Step = step("micro", 11., Web::Fixed, 1.5, 0.04, 400, true);
pub const CAPTION: Step = step("caption", 12., Web::Fixed, 1.5, 0., 400, false);
pub const LABEL: Step = step("label", 13., Web::Fixed, 1.5, 0., 400, false);
pub const UI: Step = step("ui", 14., Web::Fixed, 1.5, 0., 400, false);
pub const BODY: Step = step("body", 15., Web::Fluid(15., 17.), 1.6, 0., 400, false);
pub const HEADING: Step = step("heading", 20., Web::Fluid(20., 24.), 1.3, -0.01, 500, false);
pub const TITLE: Step = step("title", 28., Web::Fluid(32., 48.), 1.05, -0.02, 500, false);
pub const DISPLAY: Step = step(
    "display",
    44.,
    Web::Fluid(34., 64.),
    1.05,
    -0.025,
    500,
    false,
);
pub const WORDMARK: Step = step("wordmark", 220., Web::Viewport(18.), 0.8, -0.04, 500, false);

pub const TYPE: [Step; 9] = [
    MICRO, CAPTION, LABEL, UI, BODY, HEADING, TITLE, DISPLAY, WORDMARK,
];

pub const SPACE: [f32; 9] = [4., 8., 12., 16., 24., 32., 48., 64., 96.];
pub const GUTTER: f32 = 64.;
pub const GUTTER_NARROW: f32 = 16.;
pub const MEASURE: f32 = 56.;

pub const RADIUS: f32 = 8.;
pub const RADIUS_PLATE: f32 = 12.;
pub const RADIUS_PILL: f32 = 999.;
pub const RULE: f32 = 1.;

pub const CONTROL: f32 = 32.;
pub const CONTROL_LARGE: f32 = 40.;
pub const SEGMENT: f32 = 28.;
pub const FIELD: f32 = 36.;
pub const ROW: f32 = 64.;
pub const ROW_WEB: f32 = 86.;
pub const INDEX: f32 = 40.;
pub const PLATE_BLEED: f32 = 16.;
pub const PRESSED: f32 = 0.85;
pub const DISABLED: f32 = 0.4;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub name: &'static str,
    pub ms: u64,
    pub curve: [f32; 4],
}

impl Motion {
    pub const fn duration(&self) -> Duration {
        Duration::from_millis(self.ms)
    }
    pub fn secs(&self) -> f32 {
        self.ms as f32 / 1000.
    }
    pub fn ease(&self, t: f32) -> f32 {
        bezier(self.curve, t)
    }
}

pub const QUICK: Motion = Motion {
    name: "quick",
    ms: 150,
    curve: [0.4, 0., 0.2, 1.],
};
pub const MOVE: Motion = Motion {
    name: "move",
    ms: 240,
    curve: [0.23, 1., 0.32, 1.],
};
pub const ENTER: Motion = Motion {
    name: "enter",
    ms: 600,
    curve: [0.23, 1., 0.32, 1.],
};
pub const MOTION: [Motion; 3] = [QUICK, MOVE, ENTER];
pub const STAGGER: Duration = Duration::from_millis(40);
pub const RISE: f32 = 8.;

pub const OPENING_HOLD: Duration = Duration::from_millis(1400);
pub const OPENING_MAX: Duration = Duration::from_millis(2500);
pub const OPENING_REVEAL: Duration = Duration::from_millis(800);

pub fn bezier([x1, y1, x2, y2]: [f32; 4], t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    if t == 0. || t == 1. {
        return t;
    }
    let at = |a: f32, b: f32, s: f32| {
        let u = 1. - s;
        3. * u * u * s * a + 3. * u * s * s * b + s * s * s
    };
    let (mut lo, mut hi) = (0f32, 1f32);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if at(x1, x2, mid) < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at(y1, y2, (lo + hi) / 2.)
}

pub fn ease_out_quint(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(5)
}
pub fn ease_out_cubic(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub page: u32,
    pub plate_soft: u32,
    pub plate: u32,
    pub surface: u32,
    pub rule: u32,
    pub ink: u32,
    pub ink_soft: u32,
    pub muted: u32,
    pub accent: u32,
    pub on_accent: u32,
    pub danger: u32,
    pub success: u32,
    pub dark: bool,
}

pub const LIGHT: Palette = Palette {
    page: 0xfafafc,
    plate_soft: 0xf3f3f8,
    plate: 0xebecf4,
    surface: 0xffffff,
    rule: 0xe2e3ea,
    ink: 0x16181f,
    ink_soft: 0x3b3e49,
    muted: 0x646875,
    accent: 0x5b3fd6,
    on_accent: 0xffffff,
    danger: 0xb4372a,
    success: 0x1d7a48,
    dark: false,
};

pub const DARK: Palette = Palette {
    page: 0x0d0e13,
    plate_soft: 0x181a22,
    plate: 0x20222c,
    surface: 0x16181f,
    rule: 0x24262f,
    ink: 0xecedf3,
    ink_soft: 0xc5c7d1,
    muted: 0x8b8f9c,
    accent: 0x9a86ff,
    on_accent: 0x0d0e13,
    danger: 0xf07a68,
    success: 0x45c27a,
    dark: true,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blocks {
    pub page: u32,
    pub side: u32,
    pub panel: u32,
    pub raised: u32,
    pub button: u32,
    pub button_bevel: u32,
    pub well: u32,
    pub bevel: u32,
    pub lip: u32,
    pub outline: u32,
    pub rule: u32,
    pub ink: u32,
    pub ink_soft: u32,
    pub muted: u32,
    pub accent: u32,
    pub face: u32,
    pub face_bevel: u32,
    pub face_lip: u32,
    pub on_face: u32,
    pub danger: u32,
    pub success: u32,
    pub warn: u32,
    pub dark: bool,
}

pub const BLOCKS_DARK: Blocks = Blocks {
    page: 0x0f0d17,
    side: 0x0b0a11,
    panel: 0x1a1727,
    raised: 0x221e34,
    button: 0x2b2641,
    button_bevel: 0x3d3560,
    well: 0x0c0b12,
    bevel: 0x342d52,
    lip: 0x07060b,
    outline: 0x05040a,
    rule: 0x120f1c,
    ink: 0xecedf3,
    ink_soft: 0xc5c7d1,
    muted: 0xa29cb8,
    accent: 0x9a86ff,
    face: 0x5b3fd6,
    face_bevel: 0x8a74ff,
    face_lip: 0x3a2694,
    on_face: 0xffffff,
    danger: 0xf07a68,
    success: 0x45c27a,
    warn: 0xf0b84f,
    dark: true,
};

pub const BLOCKS_LIGHT: Blocks = Blocks {
    page: 0xece7f7,
    side: 0xe2dbf2,
    panel: 0xf8f6fd,
    raised: 0xe9e3f8,
    button: 0xddd4f3,
    button_bevel: 0xf4f0fe,
    well: 0xf3f0fb,
    bevel: 0xffffff,
    lip: 0xc3b8e2,
    outline: 0x2b2244,
    rule: 0xe4ddf4,
    ink: 0x1b1530,
    ink_soft: 0x3b3354,
    muted: 0x585071,
    accent: 0x5333c9,
    face: 0x5b3fd6,
    face_bevel: 0x8069ef,
    face_lip: 0x3a2694,
    on_face: 0xffffff,
    danger: 0xa3301d,
    success: 0x136336,
    warn: 0x7a4f00,
    dark: false,
};

pub const BLOCK_OUTLINE: f32 = 2.;
pub const BLOCK_BEVEL: f32 = 2.;
pub const BLOCK_LIP: f32 = 4.;
pub const TILE_TEXELS: usize = 16;
pub const TILE_TEXEL: f32 = 4.;
pub const TILE_SHADES: [i16; 5] = [0, 2, -2, 5, -4];

pub const VEIL: f32 = 0.90;
pub const HOME_VEIL: f32 = 0.80;
pub const BACKDROP_MUTED: u32 = 0x9fa3ae;
pub const BRIGHT_BACKDROP: u32 = 0xe3e3e3;

impl Palette {
    pub fn named(&self) -> [(&'static str, u32); 12] {
        [
            ("page", self.page),
            ("plate-soft", self.plate_soft),
            ("plate", self.plate),
            ("surface", self.surface),
            ("rule", self.rule),
            ("ink", self.ink),
            ("ink-soft", self.ink_soft),
            ("muted", self.muted),
            ("accent", self.accent),
            ("on-accent", self.on_accent),
            ("danger", self.danger),
            ("success", self.success),
        ]
    }
}

impl Blocks {
    pub fn named(&self) -> [(&'static str, u32); 25] {
        [
            ("page", self.page),
            ("plate-soft", self.raised),
            ("plate", self.button),
            ("surface", self.panel),
            ("rule", self.rule),
            ("ink", self.ink),
            ("ink-soft", self.ink_soft),
            ("muted", self.muted),
            ("accent", self.accent),
            ("on-accent", self.on_face),
            ("danger", self.danger),
            ("success", self.success),
            ("warn", self.warn),
            ("side", self.side),
            ("panel", self.panel),
            ("raised", self.raised),
            ("button", self.button),
            ("button-bevel", self.button_bevel),
            ("well", self.well),
            ("bevel", self.bevel),
            ("lip", self.lip),
            ("outline", self.outline),
            ("face", self.face),
            ("face-bevel", self.face_bevel),
            ("face-lip", self.face_lip),
        ]
    }
}

pub fn contrast(a: u32, b: u32) -> f32 {
    fn luminance(c: u32) -> f32 {
        let ch = |s: u32| {
            let v = ((c >> s) & 255) as f32 / 255.;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * ch(16) + 0.7152 * ch(8) + 0.0722 * ch(0)
    }
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

pub fn css() -> String {
    let mut s = String::new();
    let num = |v: f32| {
        let t = format!("{v:.3}");
        t.trim_end_matches('0').trim_end_matches('.').to_string()
    };
    let colors = |s: &mut String, p: &Blocks| {
        for (name, c) in p.named() {
            let _ = writeln!(s, "  --{name}: #{c:06x};");
        }
        let _ = writeln!(s, "  --on-face: #{:06x};", p.on_face);
        let _ = writeln!(
            s,
            "  color-scheme: {};",
            if p.dark { "dark" } else { "light" }
        );
        s.push_str(&if p.dark { night::NIGHT } else { night::DAWN }.css());
    };
    s.push_str(
        ":root {\n",
    );
    colors(&mut s, &BLOCKS_LIGHT);
    let _ = writeln!(
        s,
        "  --font: \"{FONT}\", ui-sans-serif, system-ui, sans-serif;\n  --mono: \"{MONO}\", ui-monospace, monospace;\n  --pixel: \"{PIXEL}\", \"{FONT}\", ui-sans-serif, sans-serif;\n  --night-font: \"{}\", \"{FONT}\", ui-sans-serif, sans-serif;\n  --night-display-font: \"{}\", \"{}\", ui-sans-serif, sans-serif;",
        night::FONT,
        night::DISPLAY_FONT,
        night::FONT
    );
    for (name, v) in [
        ("block-outline", BLOCK_OUTLINE),
        ("block-bevel", BLOCK_BEVEL),
        ("block-lip", BLOCK_LIP),
    ] {
        let _ = writeln!(s, "  --{name}: {}px;", num(v));
    }
    for step in TYPE.iter().chain(&night::TYPE) {
        let size = match step.web {
            Web::Fixed => format!("{}px", num(step.px)),
            Web::Fluid(min, max) => format!(
                "clamp({}px, calc({}px + {} * (100vw - 375px) / 1065), {}px)",
                num(min),
                num(min),
                num(max - min),
                num(max)
            ),
            Web::Viewport(vw) => format!("{}vw", num(vw)),
        };
        let n = step.name;
        let _ = writeln!(s, "  --text-{n}: {size};");
        let _ = writeln!(s, "  --text-{n}-line: {};", num(step.line));
        let _ = writeln!(s, "  --text-{n}-tracking: {}em;", num(step.tracking));
        let _ = writeln!(s, "  --text-{n}-weight: {};", step.weight);
    }
    for (i, v) in SPACE.iter().enumerate() {
        let _ = writeln!(s, "  --space-{}: {}px;", i + 1, num(*v));
    }
    for (name, v) in [
        ("gutter", GUTTER),
        ("gutter-narrow", GUTTER_NARROW),
        ("radius", RADIUS),
        ("radius-plate", RADIUS_PLATE),
        ("radius-pill", RADIUS_PILL),
        ("rule-width", RULE),
        ("control", CONTROL),
        ("control-large", CONTROL_LARGE),
        ("segment", SEGMENT),
        ("field", FIELD),
        ("row", ROW_WEB),
        ("index", INDEX),
        ("plate-bleed", PLATE_BLEED),
        ("rise", RISE),
        ("night-pixel-corner", night::PIXEL_CORNER),
        ("night-radius-control", night::RADIUS_CONTROL),
        ("night-play", night::PLAY),
    ] {
        let _ = writeln!(s, "  --{name}: {}px;", num(v));
    }
    let _ = writeln!(s, "  --measure: {}ch;", num(MEASURE));
    for m in MOTION {
        let [a, b, c, d] = m.curve;
        let _ = writeln!(s, "  --{}: {}ms;", m.name, m.ms);
        let _ = writeln!(
            s,
            "  --ease-{}: cubic-bezier({}, {}, {}, {});",
            m.name,
            num(a),
            num(b),
            num(c),
            num(d)
        );
    }
    let _ = writeln!(s, "  --stagger: {}ms;", STAGGER.as_millis());
    s.push_str(
        "}\n\n@media (prefers-color-scheme: dark) {\n  :root:not([data-theme=\"light\"]) {\n",
    );
    let mut dark = String::new();
    colors(&mut dark, &BLOCKS_DARK);
    for line in dark.lines() {
        let _ = writeln!(s, "  {line}");
    }
    s.push_str("  }\n}\n\n:root[data-theme=\"dark\"] {\n");
    s.push_str(&dark);
    s.push_str("}\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_meets_wcag_aa_in_both_palettes() {
        for p in [LIGHT, DARK] {
            let backgrounds = [
                ("page", p.page),
                ("plate_soft", p.plate_soft),
                ("plate", p.plate),
                ("surface", p.surface),
            ];
            let texts = [
                ("ink", p.ink),
                ("ink_soft", p.ink_soft),
                ("muted", p.muted),
                ("accent", p.accent),
                ("danger", p.danger),
                ("success", p.success),
            ];
            for (bn, bg) in backgrounds {
                for (tn, fg) in texts {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{} {tn} on {bn}: {ratio:.2}",
                        if p.dark { "dark" } else { "light" }
                    );
                }
            }
            assert!(contrast(p.on_accent, p.accent) >= 4.5);
            assert!(contrast(p.page, p.ink) >= 4.5);
        }
    }

    #[test]
    fn blocks_text_meets_wcag_aa_in_both_appearances() {
        for p in [
            BLOCKS_DARK,
            BLOCKS_LIGHT,
            night::NIGHT.blocks(),
            night::DAWN.blocks(),
        ] {
            let theme = if p.dark { "dark" } else { "light" };
            let backgrounds = [
                ("page", p.page),
                ("side", p.side),
                ("panel", p.panel),
                ("raised", p.raised),
                ("button", p.button),
                ("well", p.well),
            ];
            let texts = [
                ("ink", p.ink),
                ("ink_soft", p.ink_soft),
                ("muted", p.muted),
                ("accent", p.accent),
                ("danger", p.danger),
                ("success", p.success),
                ("warn", p.warn),
            ];
            for (bn, bg) in backgrounds {
                for (tn, fg) in texts {
                    let ratio = contrast(fg, bg);
                    assert!(ratio >= 4.5, "{theme} {tn} on {bn}: {ratio:.2}");
                }
                let lifted = TILE_SHADES.iter().max().copied().unwrap_or(0) as i32;
                let shade = [16, 8, 0].iter().fold(0, |out, s| {
                    let c = (((p.page >> s) & 255) as i32 + lifted).clamp(0, 255) as u32;
                    out | c << s
                });
                for (tn, fg) in texts {
                    let ratio = contrast(fg, shade);
                    assert!(
                        ratio >= 4.5,
                        "{theme} {tn} on the stone texture: {ratio:.2}"
                    );
                }
            }
            let label = contrast(p.on_face, p.face);
            assert!(label >= 4.5, "{theme} the main action's label: {label:.2}");
        }
    }

    #[test]
    fn every_icon_is_a_pixel_svg() {
        for (name, svg) in icons::ALL {
            let svg = std::str::from_utf8(svg).unwrap();
            assert!(svg.contains("viewBox=\"0 0 24 24\""), "{name}");
            assert!(svg.contains("currentColor"), "{name}");
        }
        assert!(icons::get("play").is_some());
    }

    #[test]
    fn menu_text_meets_wcag_aa_over_a_bright_backdrop() {
        let mix = |a: u32, b: u32, t: f32| {
            [16, 8, 0].iter().fold(0, |out, s| {
                let (x, y) = (((a >> s) & 255) as f32, ((b >> s) & 255) as f32);
                out | ((x * (1. - t) + y * t).round() as u32) << s
            })
        };
        let p = DARK;
        for (veil, texts) in [
            (
                VEIL,
                &[p.ink, p.ink_soft, BACKDROP_MUTED, p.danger, p.success][..],
            ),
            (HOME_VEIL, &[p.ink, p.ink_soft][..]),
        ] {
            let bg = mix(BRIGHT_BACKDROP, p.page, veil);
            let plate = mix(bg, p.ink, 0.08);
            for &text in texts {
                for under in [bg, plate] {
                    let ratio = contrast(text, under);
                    assert!(ratio >= 4.5, "#{text:06x} over veil {veil}: {ratio:.2}");
                }
            }
        }
        assert!(contrast(BACKDROP_MUTED, p.page) >= 4.5);
    }

    #[test]
    fn contrast_matches_known_values() {
        assert!((contrast(0x000000, 0xffffff) - 21.).abs() < 0.01);
        assert!((contrast(0x777777, 0xffffff) - 4.48).abs() < 0.01);
    }

    #[test]
    fn bezier_eases_out() {
        assert_eq!(bezier(MOVE.curve, 0.), 0.);
        assert!((bezier(MOVE.curve, 1.) - 1.).abs() < 1e-4);
        assert!(MOVE.ease(0.25) > 0.6, "{}", MOVE.ease(0.25));
        assert!((bezier([0., 0., 1., 1.], 0.3) - 0.3).abs() < 1e-3);
    }

    #[test]
    fn tokens_css_is_up_to_date() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tokens.css");
        let written = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            written == css(),
            "crates/vise-theme/tokens.css is out of date: run `cargo run -p xtask -- tokens`"
        );
    }
}
