use super::{Game, lan::LanSettings};
use vise_client::{
    Result,
    friends_screen::{self, Action, FriendsView},
    hud::HudBatch,
    text::Text,
};
use vise_friends::{Host, HostConfig, HostEvent, PathMode, host::DEFAULT_PORT, reach, stun};
use vise_protocol::{GAME_VERSION, PROTOCOL_VERSION};
use std::time::{Duration, Instant};

pub(super) struct Sharing {
    host: Host,
    opened_lan: bool,
    code: Option<String>,
    wrong: u32,
    notice: Option<String>,
    copied: Option<Instant>,
    paths: (u32, u32),
}

pub const HELP: &[&str] = &[
    "friends host                   share the open world behind an invite code",
    "friends stop                   stop sharing",
    "friends status                 the code, how friends reach this host, the guests",
];

fn stun_servers() -> Vec<String> {
    if std::env::var_os("VISE_STUN_SERVERS").is_some() {
        return stun::servers_from_env();
    }
    let dir = std::env::current_dir().unwrap_or_default();
    match vise_client::ui_theme::preference(&dir, "friendsStun") {
        Some(list) => stun::parse_servers(&list),
        None => stun::servers_from_env(),
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name).map_or(default, |v| v != "0")
}

impl Game<'_> {
    pub(super) fn friends_start(&mut self) -> Result<()> {
        if self.friends.is_some() {
            return Ok(());
        }
        if self.title.is_some() || (self.remote.is_some() && self.integrated.is_none()) {
            return Err("open a singleplayer world first".into());
        }
        let opened_lan = self.lan_port().is_none();
        if opened_lan {
            self.lan_open(LanSettings {
                port: vise_client::lan::available_port(),
                guest_commands: false,
                force_game_mode: true,
            })?;
        }
        let port = self.lan_port().ok_or("the world isn't open to LAN")?;
        let host = match Host::start(HostConfig {
            lan: std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            protocol: PROTOCOL_VERSION,
            game: GAME_VERSION.to_owned(),
            relay: vise_client::net::friends_relay_setting(),
            path: PathMode::from_env(),
            port: std::env::var("VISE_FRIENDS_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(DEFAULT_PORT),
            reach: reach::Options {
                map_port: env_flag("VISE_FRIENDS_UPNP", true),
                stun: stun_servers(),
                loopback: env_flag("VISE_FRIENDS_LOOPBACK", false),
            },
        }) {
            Ok(h) => h,
            Err(e) => {
                if opened_lan {
                    self.lan_close(true);
                }
                return Err(e.into());
            }
        };
        self.friends = Some(Sharing {
            host,
            opened_lan,
            code: None,
            wrong: 0,
            notice: None,
            copied: None,
            paths: (0, 0),
        });
        Ok(())
    }

    pub(super) fn friends_stop(&mut self) {
        let Some(mut sharing) = self.friends.take() else {
            return;
        };
        sharing.host.stop();
        if sharing.opened_lan {
            self.lan_close(true);
        } else {
            self.system_message(Text::literal("Stopped sharing the invite code."));
        }
    }

    pub(super) fn friends_lan_closed(&mut self) {
        if let Some(mut sharing) = self.friends.take() {
            sharing.host.stop();
        }
    }

    pub(super) fn tick_friends(&mut self) {
        let Some(sharing) = self.friends.as_mut() else {
            return;
        };
        let mut announce = false;
        for event in sharing.host.poll() {
            match event {
                HostEvent::Code(code) => {
                    if sharing.code.is_some() && sharing.wrong >= vise_friends::host::MAX_WRONG {
                        sharing.notice = Some(
                            "Someone kept trying a wrong short code, so it changed.".to_owned(),
                        );
                        sharing.wrong = 0;
                    }
                    announce = sharing.code.is_none();
                    sharing.code = Some(code);
                }
                HostEvent::WrongCode { count } => {
                    sharing.wrong = count;
                    sharing.notice = Some("Someone tried a wrong code.".to_owned());
                }
                HostEvent::Connected { direct: true, .. } => sharing.paths.0 += 1,
                HostEvent::Connected { direct: false, .. } => sharing.paths.1 += 1,
                HostEvent::Disconnected { .. } => {}
            }
        }
        if announce {
            self.system_message(Text::literal(
                "Your invite code is ready: Game Menu, Play with Friends. Copy it and send it to your friends.",
            ));
        }
    }

    pub(super) fn friends_copy(&mut self) {
        let Some(code) = self.friends.as_ref().and_then(|s| s.code.clone()) else {
            return;
        };
        use vise_client::clipboard::Clipboard;
        self.clipboard.set(&code);
        if let Some(s) = self.friends.as_mut() {
            s.copied = Some(Instant::now());
        }
    }

    pub(super) fn draw_friends_screen(&mut self, batch: &mut HudBatch) {
        self.tick_friends();
        let view = self.friends_view();
        friends_screen::draw(batch, &self.gui.atlas, self.cursor, &view, true);
    }
    pub(super) fn friends_screen_click(&mut self) {
        let view = self.friends_view();
        let Some(action) =
            friends_screen::click(&self.gui.atlas, self.gui_size(), self.cursor, &view)
        else {
            return;
        };
        self.audio.play_button_click();
        match action {
            Action::Share => {
                if let Err(e) = self.friends_start() {
                    self.system_message(Text::literal(format!("Couldn't share the world: {e}")));
                }
            }
            Action::Copy => self.friends_copy(),
            Action::Stop => self.friends_stop(),
            Action::Help => {
                if let Some(url) = &view.help {
                    super::packs::open_folder(std::path::Path::new(url));
                }
            }
            Action::Done => self.screen = super::Screen::Paused,
        }
    }

    pub(super) fn friends_view(&self) -> FriendsView {
        let guests = match &self.lan {
            Some(h) => h.hub.guests.iter().map(|g| g.name.clone()).collect(),
            None if self.integrated.is_some() => {
                let me = self.options.player_name();
                let mut names = self.remote_online_names().unwrap_or_default();
                names.retain(|n| *n != me);
                names
            }
            None => Vec::new(),
        };
        let Some(s) = &self.friends else {
            return FriendsView {
                shown: self.remote.is_none() || self.integrated.is_some(),
                ..FriendsView::default()
            };
        };
        let state = s.host.state();
        let mut status = if s.code.is_none() {
            "Finding how friends can reach you…".to_owned()
        } else {
            state.summary.clone()
        };
        if let Some(short) = &state.short {
            status.push_str(&format!(" Short code for the relay: {short}."));
        }
        if let Some(problem) = &state.relay_problem {
            status.push(' ');
            status.push_str(problem);
        }
        let reachable = matches!(
            state.verdict,
            Some(reach::Verdict::Mapped(_) | reach::Verdict::Public)
        ) || state.short.is_some();
        FriendsView {
            shown: true,
            sharing: true,
            code: s.code.clone(),
            status,
            reachable,
            help: (!reachable && s.code.is_some()).then(|| reach::HELP_URL.to_owned()),
            guests,
            notice: s.notice.clone(),
            copied: s
                .copied
                .is_some_and(|t| t.elapsed() < Duration::from_secs(2)),
        }
    }

    pub(super) fn friends_control(&mut self, words: &[&str]) -> Result<Option<serde_json::Value>> {
        use serde_json::json;
        if words[0] != "friends" {
            return Ok(None);
        }
        match words.get(1).copied() {
            Some("host") => {
                self.friends_start()?;
                Ok(Some(json!({ "ok": true, "lan_port": self.lan_port() })))
            }
            Some("stop") => {
                self.friends_stop();
                Ok(Some(json!({ "ok": true })))
            }
            Some("status") => {
                self.tick_friends();
                let view = self.friends_view();
                let state = self.friends.as_ref().map(|s| s.host.state());
                Ok(Some(json!({
                    "sharing": view.sharing,
                    "code": view.code,
                    "short": state.as_ref().and_then(|s| s.short.as_ref().map(ToString::to_string)),
                    "summary": view.status,
                    "reachable": view.reachable,
                    "verdict": state.as_ref().and_then(|s| s.verdict.as_ref().map(|v| format!("{v:?}"))),
                    "direct_port": state.as_ref().map(|s| s.port),
                    "sessions": self.friends.as_ref().map_or(0, |s| s.host.sessions()),
                    "direct": self.friends.as_ref().map_or(0, |s| s.paths.0),
                    "relayed": self.friends.as_ref().map_or(0, |s| s.paths.1),
                    "guests": view.guests,
                    "lan_port": self.lan_port(),
                })))
            }
            _ => Err("friends host|stop|status".into()),
        }
    }
}
