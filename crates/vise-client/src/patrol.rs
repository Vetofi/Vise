use crate::mobs::{
    Species,
    spawn_rules::{self, Reason, SpawnLevel},
};
use vise_core::random::RandomSource;

pub trait PatrolLevel: SpawnLevel {
    fn spawn_patrols(&self) -> bool;
    fn bright_outside(&self) -> bool;
    fn patrol_players(&self) -> Vec<([i32; 3], bool)>;
    fn sections_to_village(&self, pos: [i32; 3]) -> i32;
    fn has_chunk(&self, cx: i32, cz: i32) -> bool;
    fn can_pillager_patrol_spawn(&self, pos: [i32; 3]) -> bool;
    fn effective_difficulty(&self, pos: [i32; 3]) -> f32;
    fn motion_blocking_no_leaves(&self, x: i32, z: i32) -> i32;
}

pub fn close_to_village<L: PatrolLevel + ?Sized>(l: &L, pos: [i32; 3], sections: i32) -> bool {
    sections <= 6 && l.sections_to_village(pos) <= sections
}

pub fn has_chunks_at<L: PatrolLevel + ?Sized>(l: &L, x0: i32, z0: i32, x1: i32, z1: i32) -> bool {
    (x0 >> 4..=x1 >> 4).all(|cx| (z0 >> 4..=z1 >> 4).all(|cz| l.has_chunk(cx, cz)))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    pub pos: [i32; 3],
    pub leader: bool,
}

#[derive(Clone, Debug, Default)]
pub struct PatrolSpawner {
    pub next_tick: i32,
}

impl PatrolSpawner {
    pub fn tick<L: PatrolLevel, R: RandomSource>(
        &mut self,
        level: &L,
        spawn_enemies: bool,
        random: &mut R,
        spawn: &mut dyn FnMut(&L, Member, &mut R) -> bool,
    ) {
        if !spawn_enemies || !level.spawn_patrols() {
            return;
        }
        self.next_tick -= 1;
        if self.next_tick > 0 {
            return;
        }
        self.next_tick += 12000 + random.next_int_bound(1200).unwrap_or(0);
        if !level.bright_outside() || random.next_int_bound(5).unwrap_or(1) != 0 {
            return;
        }
        let players = level.patrol_players();
        if players.is_empty() {
            return;
        }
        let (player, spectator) =
            players[random.next_int_bound(players.len() as i32).unwrap_or(0) as usize];
        if spectator || close_to_village(level, player, 2) {
            return;
        }
        let x = (24 + random.next_int_bound(24).unwrap_or(0))
            * if random.next_boolean() { -1 } else { 1 };
        let z = (24 + random.next_int_bound(24).unwrap_or(0))
            * if random.next_boolean() { -1 } else { 1 };
        let mut pos = [player[0] + x, player[1], player[2] + z];
        if !has_chunks_at(level, pos[0] - 10, pos[2] - 10, pos[0] + 10, pos[2] + 10)
            || !level.can_pillager_patrol_spawn(pos)
        {
            return;
        }
        let group = level.effective_difficulty(pos).ceil() as i32 + 1;
        for i in 0..group {
            pos[1] = level.motion_blocking_no_leaves(pos[0], pos[2]);
            let leader = i == 0;
            if !spawn_member(level, pos, leader, random, spawn) && leader {
                break;
            }
            pos[0] += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
            pos[2] += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
        }
    }
}

fn spawn_member<L: PatrolLevel, R: RandomSource>(
    level: &L,
    pos: [i32; 3],
    leader: bool,
    random: &mut R,
    spawn: &mut dyn FnMut(&L, Member, &mut R) -> bool,
) -> bool {
    let [x, y, z] = pos;
    if !spawn_rules::is_valid_empty_spawn(Species::Pillager, level, x, y, z) {
        return false;
    }
    if !spawn_rules::check_spawn_rules(Species::Pillager, level, Reason::Patrol, random, x, y, z) {
        return false;
    }
    spawn(level, Member { pos, leader }, random)
}

pub fn patrol_target(at: [i32; 3], random: &mut impl RandomSource) -> [i32; 3] {
    let dx = -500 + random.next_int_bound(1000).unwrap_or(0);
    let dz = -500 + random.next_int_bound(1000).unwrap_or(0);
    [at[0] + dx, at[1], at[2] + dz]
}

#[derive(Clone, Debug, Default)]
pub struct GameplayAttributes {
    dimensions: std::collections::HashMap<String, std::collections::HashMap<String, bool>>,
    biomes: std::collections::HashMap<String, std::collections::HashMap<String, bool>>,
    tracks: std::collections::HashMap<String, std::collections::HashMap<String, Vec<BoolTrack>>>,
}
#[derive(Clone, Debug)]
struct BoolTrack {
    keyframes: Vec<(i64, bool)>,
    period: Option<i64>,
    modifier: String,
}
impl BoolTrack {
    fn sample(&self, ticks: i64) -> bool {
        let t = match self.period {
            Some(p) if p > 0 => ticks.rem_euclid(p),
            _ => ticks,
        };
        let mut value = self.keyframes[0].1;
        for (at, v) in &self.keyframes {
            if t >= *at {
                value = *v;
            }
        }
        value
    }
    fn apply(&self, base: bool, ticks: i64) -> bool {
        let arg = self.sample(ticks);
        match self.modifier.as_str() {
            "and" => base && arg,
            "or" => base || arg,
            "xor" => base ^ arg,
            "nand" => !(base && arg),
            "nor" => !(base || arg),
            "xnor" => base == arg,
            _ => arg,
        }
    }
}
pub const CAN_PILLAGER_PATROL_SPAWN: &str = "minecraft:gameplay/can_pillager_patrol_spawn";
pub const CAN_START_RAID: &str = "minecraft:gameplay/can_start_raid";
impl GameplayAttributes {
    pub fn load(root: &std::path::Path) -> Self {
        use serde_json::Value;
        let data = root.join("data/minecraft");
        let read = |p: std::path::PathBuf| -> Option<Value> {
            serde_json::from_slice(&std::fs::read(p).ok()?).ok()
        };
        let list = |dir: &str| -> Vec<(String, Value)> {
            let mut out = Vec::new();
            if let Ok(entries) = std::fs::read_dir(data.join(dir)) {
                for e in entries.flatten() {
                    let path = e.path();
                    if let (Some(stem), Some(v)) = (
                        path.file_stem().and_then(|s| s.to_str()).map(str::to_owned),
                        read(path.clone()),
                    ) {
                        out.push((format!("minecraft:{stem}"), v));
                    }
                }
            }
            out
        };
        let keys = [CAN_PILLAGER_PATROL_SPAWN, CAN_START_RAID];
        let mut out = Self::default();
        let timelines: std::collections::HashMap<String, Value> =
            list("timeline").into_iter().collect();
        fn tag(data: &std::path::Path, name: &str, out: &mut Vec<String>) {
            let file = data.join(format!(
                "tags/timeline/{}.json",
                name.trim_start_matches('#')
                    .trim_start_matches("minecraft:")
            ));
            let Some(v) = std::fs::read(file)
                .ok()
                .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
            else {
                return;
            };
            for item in v["values"].as_array().into_iter().flatten() {
                let s = item.as_str().or_else(|| item["id"].as_str()).unwrap_or("");
                if s.starts_with('#') {
                    tag(data, s, out);
                } else if !s.is_empty() {
                    out.push(s.to_owned());
                }
            }
        }
        for (dim, v) in list("dimension_type") {
            for key in keys {
                if let Some(b) = v["attributes"][key].as_bool() {
                    out.dimensions
                        .entry(key.into())
                        .or_default()
                        .insert(dim.clone(), b);
                }
            }
            let mut names = Vec::new();
            match &v["timelines"] {
                Value::String(s) if s.starts_with('#') => tag(&data, s, &mut names),
                Value::String(s) => names.push(s.clone()),
                Value::Array(a) => {
                    names.extend(a.iter().filter_map(|x| x.as_str()).map(str::to_owned))
                }
                _ => {}
            }
            for name in names {
                let Some(t) = timelines.get(&name) else {
                    continue;
                };
                for key in keys {
                    let track = &t["tracks"][key];
                    let Some(frames) = track["keyframes"].as_array() else {
                        continue;
                    };
                    let keyframes: Vec<(i64, bool)> = frames
                        .iter()
                        .filter_map(|f| Some((f["ticks"].as_i64()?, f["value"].as_bool()?)))
                        .collect();
                    if keyframes.is_empty() {
                        continue;
                    }
                    out.tracks
                        .entry(key.into())
                        .or_default()
                        .entry(dim.clone())
                        .or_default()
                        .push(BoolTrack {
                            keyframes,
                            period: t["period_ticks"].as_i64(),
                            modifier: track["modifier"].as_str().unwrap_or("override").to_owned(),
                        });
                }
            }
        }
        for (biome, v) in list("worldgen/biome") {
            for key in keys {
                if let Some(b) = v["attributes"][key].as_bool() {
                    out.biomes
                        .entry(key.into())
                        .or_default()
                        .insert(biome.clone(), b);
                }
            }
        }
        out
    }
    pub fn value(&self, key: &str, dimension: &str, biome: &str, clock: i64) -> bool {
        let mut v = self
            .dimensions
            .get(key)
            .and_then(|m| m.get(dimension))
            .copied()
            .unwrap_or(true);
        if let Some(b) = self.biomes.get(key).and_then(|m| m.get(biome)) {
            v = *b;
        }
        for t in self
            .tracks
            .get(key)
            .and_then(|m| m.get(dimension))
            .into_iter()
            .flatten()
        {
            v = t.apply(v, clock);
        }
        v
    }
}
