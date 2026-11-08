use super::{Game, lan::LanHost};
use vise_client::{
    items::ItemStack,
    lan::{
        hub::HostState,
        waypoints::{self, HIDING_HEADS, Icon, PLAYER_RANGE},
    },
};

const HOST_CONN: u32 = u32::MAX;

fn transmit_range(crouching: bool, invisible: bool, head: Option<&ItemStack>) -> f64 {
    let hiding_head = head.is_some_and(|s| HIDING_HEADS.contains(&&*s.item));
    if crouching || invisible || hiding_head {
        0.0
    } else {
        PLAYER_RANGE
    }
}

impl Game<'_> {
    fn waypoint_team(&self, name: &str) -> Option<(String, Option<i32>)> {
        let team = self.score.board.players_team(name)?;
        let color = team.color.as_deref().and_then(waypoints::team_rgb);
        Some((team.name.clone(), color))
    }

    pub(super) fn lan_update_waypoints(&mut self, host: &mut LanHost, state: Option<&HostState>) {
        let mut players: Vec<(&str, waypoints::Player)> = Vec::new();
        if let Some(h) = state {
            let icon = self
                .cmds
                .waypoint_icons
                .get(&super::commands::Target::Player)
                .map(|i| Icon {
                    style: i.style.clone(),
                    color: i.color,
                })
                .unwrap_or_default();
            players.push((
                self.dim.id(),
                waypoints::Player {
                    uuid: h.uuid,
                    conn: Some(HOST_CONN),
                    position: h.position,
                    spectator: self.spectator(),
                    transmit_range: transmit_range(
                        h.sneaking,
                        self.potions.effects.get("invisibility").is_some(),
                        h.equipment[5].as_ref(),
                    ),
                    receive_range: PLAYER_RANGE,
                    first_tick: false,
                    view_distance: host.hub.config.view_distance,
                    team: self.waypoint_team(&h.name),
                    icon,
                },
            ));
        }
        let mut respawned = Vec::new();
        for g in host.hub.guests.iter().filter(|g| g.placed) {
            if g.dead {
                host.waypoint_dead.insert(g.uuid);
            } else if host.waypoint_dead.remove(&g.uuid) {
                respawned.push(g.uuid);
            }
            let crouching = g.sneaking && !g.flying && !g.fall_flying && !g.sleeping;
            players.push((
                &g.dimension,
                waypoints::Player {
                    uuid: g.uuid,
                    conn: Some(g.conn),
                    position: g.position,
                    spectator: g.spectator(),
                    transmit_range: transmit_range(
                        crouching,
                        g.effects.get("invisibility").is_some(),
                        g.armor[0].as_ref(),
                    ),
                    receive_range: PLAYER_RANGE,
                    first_tick: g.ticks == 0,
                    view_distance: g.view_distance,
                    team: self.waypoint_team(&g.name),
                    icon: Icon::default(),
                },
            ));
        }
        host.waypoint_dead
            .retain(|uuid| players.iter().any(|(_, p)| p.uuid == *uuid));
        let locator_bar = self.ui.rules.bool("locator_bar");
        let mut levels: Vec<String> = host.waypoints.keys().cloned().collect();
        for (level, _) in &players {
            if !levels.iter().any(|l| l == level) {
                levels.push((*level).to_owned());
            }
        }
        levels.sort();
        let mut out = Vec::new();
        for level in levels {
            let here: Vec<waypoints::Player> = players
                .iter()
                .filter(|(l, _)| *l == level)
                .map(|(_, p)| p.clone())
                .collect();
            let away: Vec<(u128, u32)> = players
                .iter()
                .filter(|(l, _)| *l != level)
                .filter_map(|(_, p)| Some((p.uuid, p.conn?)))
                .collect();
            let manager = host.waypoints.entry(level.clone()).or_default();
            out.extend(manager.tick_level(&here, &away, &respawned, locator_bar));
            if here.is_empty() {
                host.waypoints.remove(&level);
            }
        }
        for (conn, packet) in out {
            if conn == HOST_CONN {
                if let vise_protocol::packets::play::Clientbound::TrackedWaypoint(w) = &packet {
                    self.handle_waypoint(w);
                }
            } else if let Some(g) = host.hub.guest(conn) {
                g.send(&packet);
            }
        }
    }
}
