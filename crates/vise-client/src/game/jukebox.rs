use super::Game;
use vise_client::{
    containers::{BlockEntity, Jukebox},
    first_person::Hand,
    items::ItemStack,
    mobs::Species,
    particles::{ParticleOptions, ParticleType},
    text::Text,
};
use vise_core::{
    geometry::{Aabb, Vec3},
    position::BlockPos,
    random::RandomSource,
};
use vise_world::level::flags as level_flags;

impl Game<'_> {
    fn jukebox(&self, pos: [i32; 3]) -> Option<&Jukebox> {
        match self.world.block_entities.get(&(pos[0], pos[1], pos[2])) {
            Some(BlockEntity::Jukebox(j)) => Some(j),
            _ => None,
        }
    }
    fn set_has_record(&mut self, pos: [i32; 3], has: bool) {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let registry = self.pack.assets.registry;
        let Some(mut s) = registry.state(state).cloned() else {
            return;
        };
        s.properties.insert(
            "has_record".into(),
            if has { "true" } else { "false" }.into(),
        );
        if let Some(new) = registry.id(&s) {
            let p = BlockPos::new(pos[0], pos[1], pos[2]);
            self.with_level(|b, level, ticks| {
                b.set_block(level, ticks, p, new, level_flags::UPDATE_CLIENTS)
            });
        }
    }
    fn jukebox_changed(&mut self, pos: [i32; 3]) {
        let p = BlockPos::new(pos[0], pos[1], pos[2]);
        let state = self.world.block(pos[0], pos[1], pos[2]);
        self.with_level(|b, level, ticks| {
            let block = b.block(state);
            b.update_neighbors_at(level, ticks, p, block);
        });
        self.container_changed((pos[0], pos[1], pos[2]));
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
    pub(super) fn use_jukebox(&mut self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        if &*self
            .res
            .facts()
            .get(self.world.block(pos[0], pos[1], pos[2]))
            .name
            != "minecraft:jukebox"
        {
            return false;
        }
        if self.jukebox(pos).is_some_and(|j| j.record.is_some()) {
            if self.hand != Hand::Main {
                return false;
            }
            self.eject_record(pos);
            return true;
        }
        let Some(stack) = held else { return false };
        let Some(song) = self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.jukebox_song.clone())
        else {
            return false;
        };
        let record = ItemStack {
            count: 1,
            ..stack.clone()
        };
        if !self.creative {
            self.consume_held();
        }
        self.stats.custom("play_record", 1);
        self.world.block_entities.insert(
            (pos[0], pos[1], pos[2]),
            BlockEntity::Jukebox(Jukebox {
                record: Some(record),
                ticks: 0,
                playing: true,
            }),
        );
        self.set_has_record(pos, true);
        self.start_song(pos, &song);
        self.jukebox_changed(pos);
        self.trigger_block("minecraft:item_used_on_block", pos, stack);
        true
    }
    pub(super) fn jukebox_takes(&self, pos: [i32; 3], held: Option<&ItemStack>) -> bool {
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if &*self.res.facts().get(state).name != "minecraft:jukebox" {
            return false;
        }
        let has_record = self
            .pack
            .assets
            .registry
            .state(state)
            .and_then(|s| s.properties.get("has_record"))
            .is_some_and(|v| v == "true");
        has_record
            || held.is_some_and(|stack| {
                self.res
                    .items()
                    .get(&stack.item)
                    .is_some_and(|i| i.jukebox_song.is_some())
            })
    }
    fn start_song(&mut self, pos: [i32; 3], song: &str) {
        self.stop_sound(pos);
        let Some(s) = self.items().songs.get(song).cloned() else {
            return;
        };
        let id = self.audio.play_jukebox_song(&s.sound, pos);
        self.jukebox_sounds.insert((pos[0], pos[1], pos[2]), id);
        let description =
            vise_client::component::to_text(&s.description, &mut |k| Text::literal(k));
        self.titles
            .set_overlay(Text::translate("record.nowPlaying", &[description]), true);
        self.update_song_volumes();
        self.notify_record_playing(pos, true);
        if self.audio.capture.is_some() {
            self.lan_songs.push((pos, Some(song.to_owned())));
        }
    }
    pub(super) fn jukebox_song_event(&mut self, pos: [i32; 3], data: i32) {
        if let Some(song) = jukebox_song_name(self.remote.as_ref().map(|r| &*r.registries), data) {
            self.start_song(pos, &song);
        }
    }
    pub(super) fn jukebox_stop_event(&mut self, pos: [i32; 3]) {
        self.stop_sound(pos);
        self.notify_record_playing(pos, false);
    }
    fn stop_sound(&mut self, pos: [i32; 3]) {
        if let Some(id) = self.jukebox_sounds.remove(&(pos[0], pos[1], pos[2])) {
            self.audio.stop_and_release(id);
        }
    }
    fn stop_song(&mut self, pos: [i32; 3]) {
        self.stop_sound(pos);
        self.notify_record_playing(pos, false);
        if self.audio.capture.is_some() {
            self.lan_songs.push((pos, None));
        }
    }
    pub(super) fn notify_record_playing(&mut self, pos: [i32; 3], playing: bool) {
        let min = Vec3::new(f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2]));
        let area = Aabb::new(min, min.add_vector(Vec3::new(1.0, 1.0, 1.0)))
            .inflate(Vec3::new(3.0, 3.0, 3.0));
        for m in self
            .mobs
            .iter_mut()
            .filter(|m| m.species() == Species::Parrot && m.bounds().intersects(area))
        {
            m.extra.party_jukebox = playing.then_some(pos);
        }
    }
    fn eject_record(&mut self, pos: [i32; 3]) {
        let key = (pos[0], pos[1], pos[2]);
        let record = match self.world.block_entities.get_mut(&key) {
            Some(BlockEntity::Jukebox(j)) => {
                j.playing = false;
                j.ticks = 0;
                j.record.take()
            }
            _ => None,
        };
        self.stop_song(pos);
        if let Some(record) = record {
            let at = vise_world::level::removal::jukebox_pop_position(
                &mut self.world.random,
                vise_core::position::BlockPos::new(pos[0], pos[1], pos[2]),
            );
            self.spawn_item(record, at, super::dispenser::new_item_motion(), 10);
        }
        self.set_has_record(pos, false);
        self.jukebox_changed(pos);
    }
    pub(super) fn jukebox_removed(&mut self, pos: [i32; 3]) {
        self.stop_song(pos);
    }
    pub(super) fn tick_jukebox(&mut self, key: (i32, i32, i32)) {
        let playing = match self.world.block_entities.get(&key) {
            Some(BlockEntity::Jukebox(j)) if j.playing => j.record.as_ref().and_then(|r| {
                let song = self.items().get(&r.item)?.jukebox_song.clone()?;
                Some((song, j.ticks))
            }),
            _ => None,
        };
        let Some((song, ticks)) = playing else {
            return;
        };
        let pos = [key.0, key.1, key.2];
        let Some(s) = self.items().songs.get(&song).cloned() else {
            return;
        };
        if s.finished(ticks) {
            if let Some(BlockEntity::Jukebox(j)) = self.world.block_entities.get_mut(&key) {
                j.playing = false;
                j.ticks = 0;
            }
            self.stop_song(pos);
            self.jukebox_changed(pos);
            return;
        }
        if !self.jukebox_sounds.contains_key(&key) && ticks == 0 {
            self.start_song(pos, &song);
        }
        if ticks % 20 == 0 {
            let color = f64::from(self.world.random.next_int_bound(4).unwrap_or(0)) / 24.0;
            let at = [
                f64::from(pos[0]) + 0.5,
                f64::from(pos[1]) + 1.2,
                f64::from(pos[2]) + 0.5,
            ];
            self.particles.add(
                &ParticleOptions::simple(ParticleType::Note),
                at,
                [color, 0.0, 0.0],
            );
        }
        if let Some(BlockEntity::Jukebox(j)) = self.world.block_entities.get_mut(&key) {
            j.ticks += 1;
        }
    }
    pub(super) fn update_song_volumes(&mut self) {
        let sounds: Vec<_> = self
            .jukebox_sounds
            .iter()
            .map(|(k, id)| (*k, *id))
            .collect();
        for (k, id) in sounds {
            let playing = matches!(self.world.block_entities.get(&k), Some(BlockEntity::Jukebox(j)) if j.playing);
            if !keeps_song(
                self.audio.engine.is_active(id),
                self.remote.is_some(),
                playing,
            ) {
                self.audio.stop_and_release(id);
                self.jukebox_sounds.remove(&k);
            }
        }
    }
}
fn jukebox_song_name(
    registries: Option<&vise_protocol::registry::Dynamic>,
    id: i32,
) -> Option<String> {
    registries?
        .name("minecraft:jukebox_song", id)
        .map(str::to_owned)
}
fn keeps_song(active: bool, server_client: bool, jukebox_playing: bool) -> bool {
    active && (server_client || jukebox_playing)
}
pub(super) fn jukebox_signals(
    world: &vise_client::world::World,
    items: &vise_client::items::Items,
    pos: BlockPos,
) -> Option<(i32, i32)> {
    match world.block_entities.get(&(pos.x, pos.y, pos.z)) {
        Some(BlockEntity::Jukebox(j)) => {
            let comparator = j
                .record
                .as_ref()
                .and_then(|r| items.get(&r.item)?.jukebox_song.as_ref())
                .and_then(|s| items.songs.get(s))
                .map_or(0, |s| s.comparator);
            Some((if j.playing { 15 } else { 0 }, comparator))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{jukebox_song_name, keeps_song};
    use vise_protocol::registry::Dynamic;

    #[test]
    fn a_server_clients_jukebox_song_comes_from_level_event_1010() {
        let mut registries = Dynamic::default();
        registries.registries.insert(
            "minecraft:jukebox_song".into(),
            ["minecraft:11", "minecraft:13", "minecraft:cat"]
                .map(|n| (n.to_owned(), None))
                .to_vec(),
        );
        assert_eq!(
            jukebox_song_name(Some(&registries), 2).as_deref(),
            Some("minecraft:cat")
        );
        assert_eq!(jukebox_song_name(Some(&registries), 3), None);
        assert_eq!(jukebox_song_name(Some(&registries), -1), None);
        assert_eq!(jukebox_song_name(None, 2), None);
        assert!(keeps_song(true, true, false));
        assert!(!keeps_song(false, true, false));
        assert!(keeps_song(true, false, true));
        assert!(!keeps_song(true, false, false));
    }
}
