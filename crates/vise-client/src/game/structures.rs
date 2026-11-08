use super::Game;

impl Game<'_> {
    pub(super) fn structures_reaching(&self, pos: [i32; 3]) -> Vec<(String, bool)> {
        self.structures_reaching_where(pos, &|_| true)
    }
    pub(super) fn structures_reaching_where(
        &self,
        pos: [i32; 3],
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<(String, bool)> {
        let Some(chunk) = self.world.chunks.get(&(pos[0] >> 4, pos[2] >> 4)) else {
            return Vec::new();
        };
        if chunk.structure_refs.is_empty() {
            return Vec::new();
        }
        let Some(locator) = self.end.locator() else {
            return Vec::new();
        };
        let mut out: Vec<(String, bool)> = Vec::new();
        for (id, (sx, sz)) in &chunk.structure_refs {
            if !wanted(id) {
                continue;
            }
            for start in locator.starts_at(*sx, *sz) {
                if *start.name != **id || !start.bounding_box.is_inside(pos) {
                    continue;
                }
                let piece = start.pieces.iter().any(|p| p.bounding_box().is_inside(pos));
                match out.iter_mut().find(|(n, _)| *n == start.name) {
                    Some(e) => e.1 |= piece,
                    None => out.push((start.name.clone(), piece)),
                }
            }
        }
        out
    }
    pub(super) fn structures_at(&self, pos: [i32; 3]) -> Vec<String> {
        self.structures_reaching(pos)
            .into_iter()
            .filter(|(_, piece)| *piece)
            .map(|(n, _)| n)
            .collect()
    }
}
