use super::{Game, Target};
use vise_client::{command::args::Selector, lan::guest::Guest, text::Text};
use vise_protocol::packets::play::{self, Clientbound as C};

impl Target {
    pub(in crate::game) fn is_player(self) -> bool {
        matches!(self, Target::Player | Target::Guest(_))
    }
}

fn mode_name(id: i32) -> &'static str {
    match id {
        1 => "creative",
        2 => "adventure",
        3 => "spectator",
        _ => "survival",
    }
}

impl Game<'_> {
    pub(in crate::game) fn command_guest(&self, conn: u32) -> Option<&Guest> {
        self.lan.as_ref()?.hub.guest(conn)
    }

    fn command_players(&self) -> impl Iterator<Item = &Guest> {
        let acting = self.lan_acting;
        self.lan
            .iter()
            .flat_map(|h| h.hub.guests.iter())
            .filter(move |g| g.placed && Some(g.conn) != acting)
    }

    pub(in crate::game) fn source_entity(&self) -> Option<Target> {
        match self.command_source {
            Some(s) => s.entity,
            None => self.player.is_some().then_some(Target::Player),
        }
    }

    pub(in crate::game) fn implied_player(&self) -> Target {
        self.source_entity()
            .filter(|t| t.is_player())
            .unwrap_or(Target::Player)
    }

    pub(super) fn player_named(&self, name: &str) -> Option<Target> {
        if self.player.is_some() && name.eq_ignore_ascii_case(self.acting_player_name()) {
            return Some(Target::Player);
        }
        self.command_players()
            .find(|g| g.name.eq_ignore_ascii_case(name))
            .map(|g| Target::Guest(g.conn))
    }

    pub(in crate::game) fn target_by_uuid(&self, uuid: u128) -> Option<Target> {
        let own = self.lan_acting_uuid.or_else(|| {
            self.level.player_uuid.map(|[a, b, c, d]| {
                (u128::from(a as u32) << 96)
                    | (u128::from(b as u32) << 64)
                    | (u128::from(c as u32) << 32)
                    | u128::from(d as u32)
            })
        });
        if self.player.is_some() && own == Some(uuid) {
            return Some(Target::Player);
        }
        if let Some(g) = self.command_players().find(|g| g.uuid == uuid) {
            return Some(Target::Guest(g.conn));
        }
        let words = [96, 64, 32, 0].map(|shift| (uuid >> shift) as u32 as i32);
        self.target_with_uuid(words)
    }

    pub(super) fn player_candidates(&self, s: &Selector) -> Vec<Target> {
        let limited = s.distance.is_some()
            || s.pos.iter().any(Option::is_some)
            || s.delta.iter().any(Option::is_some);
        let alive = matches!(s.kind, 'e' | 'n');
        let here = self.dim.id();
        let in_place = self.lan_acting.filter(|_| self.player.is_some());
        self.lan
            .iter()
            .flat_map(|h| h.hub.guests.iter())
            .filter(|g| g.placed)
            .filter(|g| !limited || g.dimension == here || Some(g.conn) == in_place)
            .filter(|g| !alive || (!g.dead && g.health > 0.0))
            .map(|g| {
                if Some(g.conn) == in_place {
                    Target::Player
                } else {
                    Target::Guest(g.conn)
                }
            })
            .collect()
    }

    pub(super) fn guest_level_and_mode(&self, conn: u32) -> Option<(i32, &'static str)> {
        self.command_guest(conn)
            .map(|g| (g.xp_level, mode_name(g.game_mode)))
    }

    pub(in crate::game) fn player_text(&self, name: &str, uuid: u128) -> Text {
        let shown = match self.score.board.players_team(name) {
            Some(team) => team.formatted_name(Text::literal(name)),
            None => Text::literal(name),
        };
        shown.with_component_fields(vec![
            (
                "click_event".into(),
                serde_json::json!({"action": "suggest_command", "command": format!("/tell {name} ")}),
            ),
            (
                "hover_event".into(),
                serde_json::json!({"action": "show_entity", "id": "minecraft:player",
                    "uuid": vise_protocol::crypto::uuid_string(uuid), "name": name}),
            ),
            ("insertion".into(), name.into()),
        ])
    }

    pub(in crate::game) fn for_player<T>(
        &mut self,
        t: Target,
        f: impl FnOnce(&mut Self) -> T,
    ) -> Option<T> {
        let conn = match t {
            Target::Player => return Some(f(self)),
            Target::Guest(conn) if self.lan_acting == Some(conn) => return Some(f(self)),
            Target::Guest(conn) => conn,
            _ => return None,
        };
        let mut host = self.lan.take()?;
        let back = self.dim;
        let theirs = host
            .hub
            .guest(conn)
            .map(|g| super::super::Dim::from_id(&g.dimension));
        if let Some(dim) = theirs.filter(|d| *d != back)
            && let Err(e) = self.enter_level(Some(&mut host), dim)
        {
            log::error!("could not open {}: {e}", dim.id());
        }
        let Some(swap) = self.enter_guest(&mut host, conn) else {
            if self.dim != back
                && let Err(e) = self.enter_level(Some(&mut host), back)
            {
                log::error!("could not reopen {}: {e}", back.id());
            }
            self.lan = Some(host);
            return None;
        };
        self.lan = Some(host);
        let source = self.chat_capture.replace(Vec::new());
        let out = f(self);
        let told = std::mem::replace(&mut self.chat_capture, source).unwrap_or_default();
        let mut host = self
            .lan
            .take()
            .expect("the hub is back after a player's turn");
        self.leave_guest(&mut host, swap);
        for line in told {
            host.hub.send_to(
                conn,
                &C::SystemChat(play::SystemChat {
                    content: vise_client::lan::data::component_nbt(&line.component()),
                    overlay: false,
                }),
            );
        }
        if self.dim != back
            && let Err(e) = self.enter_level(Some(&mut host), back)
        {
            log::error!("could not reopen {}: {e}", back.id());
        }
        self.lan = Some(host);
        Some(out)
    }
}
