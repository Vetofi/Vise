use super::{Game, Vertex};
use vise_client::{mobs::Mob, scoreboard as sb};

pub(super) fn team_color(team: Option<&sb::Team>) -> u32 {
    team.and_then(|t| t.color.as_deref())
        .and_then(sb::team_color_rgb)
        .unwrap_or(0xFF_FFFF)
}

impl Game<'_> {
    pub(super) fn entity_glowing(&self, id: u32) -> bool {
        self.remote_glowing(id).unwrap_or_else(|| {
            self.persist_raw(id)
                .and_then(|t| vise_client::save::number(t.get("Glowing")))
                .is_some_and(|b| b != 0)
        })
    }

    pub(super) fn mob_outline(&self, m: &Mob) -> Option<u32> {
        if !m.is_currently_glowing() {
            return None;
        }
        let team = if &*m.kind.id == "minecraft:player" {
            let names = self.remote_player_names();
            let name = names.into_iter().find(|(id, _)| *id == m.id);
            name.and_then(|(_, name)| self.score.board.players_team(&name))
        } else {
            self.mob_team(m.id)
        };
        Some(team_color(team))
    }

    pub(super) fn mark_outline(&self, vertices: &[Vertex], rgb: u32) {
        let color = vise_client::mobs::render::abgr(rgb);
        self.glowing_vertices
            .borrow_mut()
            .extend(vertices.iter().map(|v| Vertex {
                color,
                normal: [0; 4],
                ..*v
            }));
    }
}
