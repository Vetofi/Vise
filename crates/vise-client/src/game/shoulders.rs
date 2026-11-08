use super::Game;
use vise_client::mobs::Species;
use vise_core::{geometry::Vec3, random::RandomSource};
use vise_world::nbt::Tag;

const KEYS: [&str; 2] = ["ShoulderEntityLeft", "ShoulderEntityRight"];

fn occupied(extra: &Tag, key: &str) -> bool {
    matches!(extra.get(key), Some(Tag::Compound(c)) if !c.is_empty())
}

impl Game<'_> {
    pub(super) fn parrot_to_shoulder(&mut self, mob: u32) {
        let Some(i) = self
            .mobs
            .iter()
            .position(|m| m.id == mob && m.alive() && !m.removed && m.species() == Species::Parrot)
        else {
            return;
        };
        let Some(p) = self.player.as_ref() else {
            return;
        };
        if !vise_client::mobs::tame::owned_by(self.mobs[i].extra.owner, self.acting_uuid()) {
            return;
        }
        if self.riding.is_some()
            || !p.ground.on_ground
            || p.fluids.was_touching_water
            || p.in_powder_snow
            || p.flying
        {
            return;
        }
        let extra = self
            .level
            .player_extra
            .get_or_insert_with(|| Tag::Compound(Default::default()));
        let Some(key) = KEYS.into_iter().find(|k| !occupied(extra, k)) else {
            return;
        };
        let tag = self.mob_tree_nbt(i);
        if let Some(Tag::Compound(map)) = self.level.player_extra.as_mut() {
            map.insert(key.into(), tag);
        }
        let (uuid, now) = (self.acting_uuid(), self.world.game_time);
        self.creatures.shoulder_time.insert(uuid, now);
        self.mobs[i].removed = true;
    }

    pub(super) fn handle_shoulder_entities(&mut self) {
        for key in KEYS {
            self.play_shoulder_entity_ambient_sound(key);
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let any = self
            .level
            .player_extra
            .as_ref()
            .is_some_and(|e| KEYS.iter().any(|k| occupied(e, k)));
        if any
            && (p.fall_distance > 0.5
                || p.fluids.was_touching_water
                || p.flying
                || p.spectator
                || p.in_powder_snow
                || p.auto_spin_attack_ticks > 0
                || p.health.hurt.hurt_time > 0
                || p.health.dead_or_dying()
                || self.sleeping.is_some())
        {
            self.remove_entities_on_shoulder();
        }
    }

    fn play_shoulder_entity_ambient_sound(&mut self, key: &str) {
        let Some(Tag::Compound(tag)) = self.level.player_extra.as_ref().and_then(|e| e.get(key))
        else {
            return;
        };
        let silent = matches!(tag.get(&"Silent".into()), Some(Tag::Byte(b)) if *b != 0);
        let parrot = matches!(
            tag.get(&"id".into()),
            Some(Tag::String(id)) if id.to_string_lossy() == "minecraft:parrot"
        );
        if tag.is_empty() || silent || self.world.random.next_int_bound(200).unwrap_or(1) != 0 {
            return;
        }
        let Some(p) = self.player.as_ref() else {
            return;
        };
        if !parrot {
            return;
        }
        let area = p.bounds().inflate(Vec3::new(20.0, 20.0, 20.0));
        let at = [p.position.x, p.position.y, p.position.z];
        let alive = !p.health.dead_or_dying();
        let nearby: Vec<_> = self
            .mobs
            .iter()
            .filter(|m| m.alive() && !m.removed && m.bounds().intersects(area))
            .map(|m| (m.species(), m.silent))
            .collect();
        let peaceful = self.ui.difficulty == 0;
        if let Some((sound, volume, pitch)) = vise_client::mobs::parrot::shoulder_sound(
            &mut self.world.random,
            alive,
            false,
            peaceful,
            &nearby,
        ) {
            self.audio
                .play(&format!("minecraft:{sound}"), Some(at), volume, pitch);
        }
    }

    fn remove_entities_on_shoulder(&mut self) {
        let (uuid, now) = (self.acting_uuid(), self.world.game_time);
        if self
            .creatures
            .shoulder_time
            .get(&uuid)
            .is_some_and(|t| t + 20 >= now)
        {
            return;
        }
        let Some(p) = self.player.as_ref().map(|p| p.position) else {
            return;
        };
        let Some(Tag::Compound(map)) = self.level.player_extra.as_mut() else {
            return;
        };
        let tags: Vec<Tag> = KEYS
            .iter()
            .filter_map(|k| map.remove(&(*k).into()))
            .collect();
        let pos = [p.x, p.y + f64::from(0.7_f32), p.z];
        for mut tag in tags {
            let Tag::Compound(c) = &mut tag else {
                continue;
            };
            if c.is_empty() {
                continue;
            }
            c.insert(
                "Pos".into(),
                Tag::List(pos.iter().map(|v| Tag::Double(v.to_bits())).collect()),
            );
            c.insert("Owner".into(), Tag::IntArray(uuid.to_vec()));
            self.load_mob(tag, None);
        }
    }
}
