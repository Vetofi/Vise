
use serde::{Deserialize, Serialize};

pub mod timer;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Actions {
    pub setback: f32,
    pub alert: f32,
    pub kick: f32,
    pub decay: f32,
}

macro_rules! checks {
    ($(
        $(#[$doc:meta])*
        $field:ident: $Check:ident => $Config:ident [$setback:expr, $alert:expr, $kick:expr, $decay:expr] {
            $( $(#[$xdoc:meta])* $extra:ident: $ty:ty = $default:expr, )*
        }
    )*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Check { $($Check,)* }

        impl Check {
            pub const ALL: &[Check] = &[$(Check::$Check,)*];
            pub fn name(self) -> &'static str {
                match self { $(Check::$Check => stringify!($field),)* }
            }
        }

        $(
            $(#[$doc])*
            #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
            #[serde(default, deny_unknown_fields)]
            pub struct $Config {
                pub enabled: bool,
                pub setback: f32,
                pub alert: f32,
                pub kick: f32,
                pub decay: f32,
                $( $(#[$xdoc])* pub $extra: $ty, )*
            }

            impl Default for $Config {
                fn default() -> Self {
                    Self {
                        enabled: true,
                        setback: $setback,
                        alert: $alert,
                        kick: $kick,
                        decay: $decay,
                        $($extra: $default,)*
                    }
                }
            }
        )*

        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[serde(default, deny_unknown_fields)]
        pub struct Config {
            pub enabled: bool,
            pub alerts: bool,
            pub alert_level: u8,
            pub log: String,
            $(pub $field: $Config,)*
        }

        impl Default for Config {
            fn default() -> Self {
                Self {
                    enabled: false,
                    alerts: true,
                    alert_level: 2,
                    log: "logs/anticheat.log".into(),
                    $($field: $Config::default(),)*
                }
            }
        }

        impl Config {
            pub fn runs(&self, check: Check) -> bool {
                self.enabled && match check { $(Check::$Check => self.$field.enabled,)* }
            }

            pub fn actions(&self, check: Check) -> Actions {
                match check {
                    $(Check::$Check => {
                        let c = &self.$field;
                        Actions { setback: c.setback, alert: c.alert, kick: c.kick, decay: c.decay }
                    })*
                }
            }
        }
    };
}

checks! {
    timer: Timer => TimerConfig [2.0, 5.0, 20.0, 0.02] {
        tolerance_ms: f64 = 300.0,
        max_lag_ms: f64 = 1000.0,
    }
}

pub const CHECKS: usize = Check::ALL.len();

impl Config {
    pub fn from_server_toml(text: &str) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct File {
            #[serde(default)]
            anticheat: Config,
        }
        let mut table: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut only = toml::Table::new();
        if let Some(section) = table.remove("anticheat") {
            only.insert("anticheat".into(), section);
        }
        let file: File = only
            .try_into()
            .map_err(|e: toml::de::Error| format!("[anticheat]: {e}"))?;
        file.anticheat.check()?;
        Ok(file.anticheat)
    }

    pub fn check(&self) -> Result<(), String> {
        for &c in Check::ALL {
            let a = self.actions(c);
            if [a.setback, a.alert, a.kick, a.decay]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.0)
            {
                return Err(format!(
                    "anticheat.{}: setback, alert, kick and decay are numbers from 0 up",
                    c.name()
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Verdict {
    pub vl: f32,
    pub setback: bool,
    pub alert: bool,
    pub kick: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Player {
    vl: [f32; CHECKS],
    flagged: [u64; CHECKS],
    alerted: [Option<u64>; CHECKS],
    pub timer: timer::Balance,
}

impl Player {
    pub fn vl(&self, config: &Config, check: Check, tick: u64) -> f32 {
        let i = check as usize;
        let seconds = tick.saturating_sub(self.flagged[i]) as f32 / 20.0;
        (self.vl[i] - config.actions(check).decay * seconds).max(0.0)
    }

    pub fn flag(&mut self, config: &Config, check: Check, weight: f32, tick: u64) -> Verdict {
        let i = check as usize;
        let a = config.actions(check);
        let vl = self.vl(config, check, tick) + weight;
        self.vl[i] = vl;
        self.flagged[i] = tick;
        let reached = |threshold: f32| threshold > 0.0 && vl >= threshold;
        let alert = reached(a.alert) && self.alerted[i].is_none_or(|t| tick >= t + 20);
        if alert {
            self.alerted[i] = Some(tick);
        }
        Verdict {
            vl,
            setback: reached(a.setback),
            alert,
            kick: reached(a.kick),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> Config {
        Config {
            enabled: true,
            ..Config::default()
        }
    }

    #[test]
    fn off_by_default_and_without_a_section() {
        assert!(!Config::default().enabled);
        let c = Config::from_server_toml("[server]\nport = 25565\n").unwrap();
        assert_eq!(c, Config::default());
        assert!(!c.runs(Check::Timer));
    }

    #[test]
    fn reads_its_section() {
        let text = "[server]\nport = 1\n[anticheat]\nenabled = true\n[anticheat.timer]\nkick = 0\ntolerance_ms = 500\n";
        let c = Config::from_server_toml(text).unwrap();
        assert!(c.runs(Check::Timer));
        assert_eq!(c.timer.kick, 0.0);
        assert_eq!(c.timer.tolerance_ms, 500.0);
        assert_eq!(c.timer.alert, TimerConfig::default().alert);
        let off = "[anticheat]\nenabled = true\n[anticheat.timer]\nenabled = false\n";
        assert!(!Config::from_server_toml(off).unwrap().runs(Check::Timer));
    }

    #[test]
    fn refuses_unknown_keys_and_negative_levels() {
        assert!(Config::from_server_toml("[anticheat]\nenable = true\n").is_err());
        assert!(Config::from_server_toml("[anticheat.timer]\nkick = -1\n").is_err());
    }

    #[test]
    fn levels_rise_decay_and_act_in_order() {
        let mut c = on();
        c.timer = TimerConfig {
            setback: 2.0,
            alert: 3.0,
            kick: 5.0,
            decay: 1.0,
            ..TimerConfig::default()
        };
        let mut p = Player::default();
        let v = p.flag(&c, Check::Timer, 1.0, 100);
        assert_eq!((v.setback, v.alert, v.kick), (false, false, false));
        let v = p.flag(&c, Check::Timer, 1.0, 100);
        assert_eq!((v.vl, v.setback, v.alert), (2.0, true, false));
        let v = p.flag(&c, Check::Timer, 1.0, 100);
        assert!(v.alert && !v.kick);
        assert!(!p.flag(&c, Check::Timer, 0.5, 110).alert);
        assert_eq!(p.vl(&c, Check::Timer, 150), 1.0);
        assert_eq!(p.vl(&c, Check::Timer, 10_000), 0.0);
        let v = p.flag(&c, Check::Timer, 5.0, 10_000);
        assert!(v.kick && v.alert);
    }

    #[test]
    fn zero_turns_an_action_off() {
        let mut c = on();
        c.timer.kick = 0.0;
        let mut p = Player::default();
        assert!(!p.flag(&c, Check::Timer, 1000.0, 1).kick);
    }
}
