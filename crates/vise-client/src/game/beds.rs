use super::{Dim, Game};
use vise_client::Result;

impl Game<'_> {
    pub(super) fn set_bed_occupied(&mut self, head: [i32; 3], occupied: bool) {
        let registry = &self.pack.assets.registry;
        let Some(mut state) = registry
            .state(self.world.block(head[0], head[1], head[2]))
            .cloned()
        else {
            return;
        };
        let value = if occupied { "true" } else { "false" };
        if state.properties.get("occupied").is_none_or(|v| v == value) {
            return;
        }
        state.properties.insert("occupied".into(), value.into());
        let Some(id) = registry.id(&state) else {
            return;
        };
        if let Err(e) = self.level_set(head, id) {
            log::warn!("could not mark the bed at {head:?} occupied={value}: {e}");
        }
    }

    pub(super) fn occupy_bed(&mut self, head: [i32; 3]) {
        self.set_bed_occupied(head, true);
        if !self.player_beds.contains(&(self.dim, head)) {
            self.player_beds.push((self.dim, head));
        }
    }

    fn player_in_bed(&self, head: [i32; 3]) -> bool {
        self.sleeping == Some(head)
            || self.lan.as_ref().is_some_and(|h| {
                h.hub
                    .guests
                    .iter()
                    .any(|g| g.bed == Some(head) && g.dimension == self.dim.id())
            })
    }

    pub(super) fn free_left_beds(&mut self) {
        let dim = self.dim;
        let left: Vec<[i32; 3]> = self
            .player_beds
            .iter()
            .filter(|(d, head)| *d == dim && !self.player_in_bed(*head))
            .map(|(_, head)| *head)
            .collect();
        for head in left {
            self.player_beds.retain(|b| *b != (dim, head));
            self.set_bed_occupied(head, false);
            let bed = self.world.block(head[0], head[1], head[2]);
            let straw = &*self.res.facts().get(bed).name == "minecraft:straw_bed";
            if dim == Dim::Overworld
                && straw
                && let Err(e) = self.destroy_straw_bed(head)
            {
                log::warn!("could not break the straw bed at {head:?}: {e}");
            }
        }
    }

    pub(super) fn player_bed_levels(&self) -> impl Iterator<Item = Dim> + '_ {
        self.player_beds.iter().map(|(d, _)| *d)
    }

    pub(super) fn free_player_beds(&mut self) -> Result<()> {
        if self.player_beds.is_empty() {
            return Ok(());
        }
        let back = self.dim;
        for (dim, head) in std::mem::take(&mut self.player_beds) {
            self.enter_level(None, dim)?;
            if self.dim == dim {
                self.set_bed_occupied(head, false);
            }
        }
        self.enter_level(None, back)
    }
}
