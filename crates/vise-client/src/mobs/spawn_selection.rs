use super::Species;
use super::spawn_rules::{self, Reason, SpawnLevel};
use crate::biome::SpawnEntry;
use crate::fast_hash::FxHashMap;
use vise_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path};

pub use crate::biome::{SpawnCost, parse_costs, parse_entries};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Monster,
    Creature,
    Ambient,
    Axolotls,
    UndergroundWaterCreature,
    WaterCreature,
    WaterAmbient,
    Misc,
}
impl Category {
    pub const ALL: [Self; 8] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
        Self::Misc,
    ];
    pub const SPAWNING: [Self; 7] = [
        Self::Monster,
        Self::Creature,
        Self::Ambient,
        Self::Axolotls,
        Self::UndergroundWaterCreature,
        Self::WaterCreature,
        Self::WaterAmbient,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Monster => "monster",
            Self::Creature => "creature",
            Self::Ambient => "ambient",
            Self::Axolotls => "axolotls",
            Self::UndergroundWaterCreature => "underground_water_creature",
            Self::WaterCreature => "water_creature",
            Self::WaterAmbient => "water_ambient",
            Self::Misc => "misc",
        }
    }
    pub fn from_name(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|c| c.name() == name)
            .unwrap_or(Self::Misc)
    }
    fn index(self) -> usize {
        self as usize
    }
    pub fn max(self) -> i32 {
        match self {
            Self::Monster => 70,
            Self::Creature => 10,
            Self::Ambient => 15,
            Self::Axolotls | Self::UndergroundWaterCreature | Self::WaterCreature => 5,
            Self::WaterAmbient => 20,
            Self::Misc => -1,
        }
    }
    pub fn friendly(self) -> bool {
        self != Self::Monster
    }
    pub fn persistent(self) -> bool {
        matches!(self, Self::Creature | Self::Misc)
    }
    pub fn despawn_distance(self) -> i32 {
        if self == Self::WaterAmbient { 64 } else { 128 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WalkTarget {
    NotPathfinder,
    Zero,
    Animal,
    Mooshroom,
    Turtle,
    Monster,
    Giant,
    Guardian,
    Silverfish,
    Strider,
    Hoglin,
    Bee,
    HappyGhast,
}
fn walk_target(s: Species) -> WalkTarget {
    use Species as S;
    use WalkTarget as W;
    match s {
        S::Bat | S::Slime | S::MagmaCube | S::SulfurCube | S::Ghast | S::Phantom => {
            W::NotPathfinder
        }
        S::Enderman | S::Creaking | S::Pillager | S::Warden => W::Zero,
        S::Giant => W::Giant,
        S::Guardian | S::ElderGuardian => W::Guardian,
        S::Silverfish => W::Silverfish,
        S::Zombie
        | S::Husk
        | S::Drowned
        | S::ZombieVillager
        | S::ZombifiedPiglin
        | S::Skeleton
        | S::Stray
        | S::Bogged
        | S::Parched
        | S::WitherSkeleton
        | S::Creeper
        | S::Spider
        | S::CaveSpider
        | S::Endermite
        | S::Witch
        | S::Vindicator
        | S::Evoker
        | S::Illusioner
        | S::Ravager
        | S::Piglin
        | S::PiglinBrute
        | S::Zoglin
        | S::Blaze
        | S::Vex
        | S::Breeze
        | S::Wither => W::Monster,
        S::Mooshroom => W::Mooshroom,
        S::Turtle => W::Turtle,
        S::Strider => W::Strider,
        S::Hoglin => W::Hoglin,
        S::Bee => W::Bee,
        S::HappyGhast => W::HappyGhast,
        S::Axolotl | S::Nautilus | S::ZombieNautilus => W::Zero,
        S::Pig
        | S::Cow
        | S::Sheep
        | S::Chicken
        | S::Rabbit
        | S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk
        | S::Goat
        | S::Panda
        | S::PolarBear
        | S::Fox
        | S::Wolf
        | S::Cat
        | S::Ocelot
        | S::Parrot
        | S::Armadillo
        | S::Frog
        | S::Sniffer => W::Animal,
        _ => W::Zero,
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Obstruction {
    NoLiquidUnobstructed,
    Unobstructed,
    Ocelot,
    NoLiquid,
}
fn obstruction(s: Species) -> Obstruction {
    use Species as S;
    match s {
        S::Squid
        | S::GlowSquid
        | S::Dolphin
        | S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::Nautilus
        | S::ZombieNautilus
        | S::Guardian
        | S::ElderGuardian
        | S::Strider
        | S::Drowned => Obstruction::Unobstructed,
        S::Ocelot => Obstruction::Ocelot,
        S::Ravager => Obstruction::NoLiquid,
        _ => Obstruction::NoLiquidUnobstructed,
    }
}
fn removed_when_far(s: Species) -> bool {
    use Species as S;
    match s {
        S::Axolotl
        | S::Cod
        | S::Salmon
        | S::TropicalFish
        | S::Pufferfish
        | S::Tadpole
        | S::CamelHusk
        | S::ZombieHorse
        | S::Hoglin
        | S::Nautilus
        | S::ZombieNautilus => true,
        S::Villager
        | S::WanderingTrader
        | S::IronGolem
        | S::SnowGolem
        | S::Shulker
        | S::Allay
        | S::Warden => false,
        _ => !is_animal(s),
    }
}
fn is_animal(s: Species) -> bool {
    use Species as S;
    matches!(walk_target(s), WalkTarget::Animal)
        || matches!(
            s,
            S::Axolotl
                | S::Nautilus
                | S::ZombieNautilus
                | S::Mooshroom
                | S::Turtle
                | S::Strider
                | S::Hoglin
                | S::Bee
                | S::HappyGhast
        )
}
pub fn max_cluster_size(s: Species) -> u32 {
    use Species as S;
    match s {
        S::Horse
        | S::Donkey
        | S::Mule
        | S::Llama
        | S::TraderLlama
        | S::SkeletonHorse
        | S::ZombieHorse
        | S::Camel
        | S::CamelHusk => 6,
        S::Cod | S::TropicalFish | S::Pufferfish | S::Tadpole | S::Wolf => 8,
        S::Salmon => 5,
        S::Ghast | S::HappyGhast | S::Pillager => 1,
        _ => 4,
    }
}
fn spawns_far_from_player(s: Species, category: Category) -> bool {
    matches!(category, Category::Creature | Category::Misc)
        || matches!(s, Species::Pillager | Species::Shulker)
}

pub type MobType = (Species, f32, f32, Category);

pub trait NaturalLevel: SpawnLevel {
    fn min_y(&self) -> i32;
    fn max_y(&self) -> i32;
    fn ambient_light(&self) -> f32;
    fn has_ceiling(&self) -> bool;
    fn mob_type(&self, entity: &str) -> Option<MobType>;
    fn biome_spawns(&self, x: i32, y: i32, z: i32) -> Option<&HashMap<String, Vec<SpawnEntry>>>;
    fn spawn_cost(&self, entity: &str, x: i32, y: i32, z: i32) -> Option<SpawnCost>;
    fn creature_probability(&self, x: i32, y: i32, z: i32) -> f32;
    fn structures_at(
        &self,
        x: i32,
        y: i32,
        z: i32,
        wanted: &dyn Fn(&str) -> bool,
    ) -> Vec<StructureAt>;
    fn structure_spawns(&self) -> &StructureSpawns;
    fn nearest_player_positions(&self) -> &[[f64; 3]];
    fn respawn(&self) -> Option<[i32; 3]>;
    fn can_spawn_in_chunk(&self, cx: i32, cz: i32) -> bool;
    fn no_collision(&self, bounds: Aabb) -> bool;
    fn unobstructed(&self, bounds: Aabb) -> bool;
    fn spawn_mobs(&self) -> bool;
    fn heightmap(&self, kind: Heightmap, x: i32, z: i32) -> i32;
    fn pathfindable_land(&self, x: i32, y: i32, z: i32) -> bool;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Heightmap {
    WorldSurface,
    OceanFloor,
    MotionBlocking,
    MotionBlockingNoLeaves,
}
impl Heightmap {
    pub fn from_name(name: &str) -> Self {
        match name {
            "WORLD_SURFACE" => Self::WorldSurface,
            "OCEAN_FLOOR" => Self::OceanFloor,
            "MOTION_BLOCKING" => Self::MotionBlocking,
            _ => Self::MotionBlockingNoLeaves,
        }
    }
}

pub fn heightmap_blocks<L: SpawnLevel>(l: &L, kind: Heightmap, x: i32, y: i32, z: i32) -> bool {
    let state = l.state(x, y, z);
    let f = l.facts().get(state);
    let fluid = f.fluid != crate::blocks::Fluid::None;
    match kind {
        Heightmap::WorldSurface => !l.facts().is_air(state),
        Heightmap::OceanFloor => l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name),
        Heightmap::MotionBlocking => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap", &f.name)
        }
        Heightmap::MotionBlockingNoLeaves => {
            fluid || l.block_tag("minecraft:blocks_motion_in_heightmap_no_leaves", &f.name)
        }
    }
}
fn magic_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    let raw = l
        .block_light(x, y, z)
        .max(l.sky_light(x, y, z) - l.sky_darken());
    let v = raw as f32 / 15.0;
    let curved = v / (4.0 - 3.0 * v);
    curved + l.ambient_light() * (1.0 - curved)
}
fn cost_from_light<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> f32 {
    magic_light(l, x, y, z) - 0.5
}
fn name_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> &str {
    &l.facts().get(l.state(x, y, z)).name
}
fn fluid_at<L: NaturalLevel>(l: &L, x: i32, y: i32, z: i32) -> crate::blocks::Fluid {
    l.facts().get(l.state(x, y, z)).fluid
}
pub(crate) fn mob_spawn_rules<L: NaturalLevel>(s: Species, l: &L, x: i32, y: i32, z: i32) -> bool {
    use crate::blocks::Fluid;
    let value = match walk_target(s) {
        WalkTarget::NotPathfinder | WalkTarget::Zero => 0.0,
        WalkTarget::Animal => {
            if name_at(l, x, y - 1, z) == "minecraft:grass_block" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Mooshroom => {
            if name_at(l, x, y - 1, z) == "minecraft:mycelium" {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Turtle => {
            if fluid_at(l, x, y, z) == Fluid::Water
                || l.block_tag("minecraft:sand", name_at(l, x, y - 1, z))
            {
                10.0
            } else {
                cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Monster => -cost_from_light(l, x, y, z),
        WalkTarget::Giant => cost_from_light(l, x, y, z),
        WalkTarget::Guardian => {
            if fluid_at(l, x, y, z) == Fluid::Water {
                10.0 + cost_from_light(l, x, y, z)
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Silverfish => {
            if infested_host(name_at(l, x, y - 1, z)) {
                10.0
            } else {
                -cost_from_light(l, x, y, z)
            }
        }
        WalkTarget::Strider => {
            if fluid_at(l, x, y, z) == Fluid::Lava {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::Hoglin => {
            if name_at(l, x, y - 1, z) == "minecraft:crimson_nylium" {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::Bee => {
            if l.facts().is_air(l.state(x, y, z)) {
                10.0
            } else {
                0.0
            }
        }
        WalkTarget::HappyGhast => {
            let air = |dy| l.facts().is_air(l.state(x, y + dy, z));
            if !air(0) {
                0.0
            } else if air(-1) && !air(-2) {
                10.0
            } else {
                5.0
            }
        }
    };
    value >= 0.0
}
pub fn infested_host(block: &str) -> bool {
    matches!(
        block,
        "minecraft:stone"
            | "minecraft:cobblestone"
            | "minecraft:stone_bricks"
            | "minecraft:mossy_stone_bricks"
            | "minecraft:cracked_stone_bricks"
            | "minecraft:chiseled_stone_bricks"
            | "minecraft:deepslate"
    )
}
pub fn contains_any_liquid<L: SpawnLevel>(l: &L, b: Aabb) -> bool {
    let (lo, hi) = (b.min_position(), b.max_position());
    let (x0, y0, z0) = (
        lo.x.floor() as i32,
        lo.y.floor() as i32,
        lo.z.floor() as i32,
    );
    let (x1, y1, z1) = (
        hi.x.floor() as i32,
        hi.y.floor() as i32,
        hi.z.floor() as i32,
    );
    (x0..=x1).any(|x| {
        (y0..=y1).any(|y| {
            (z0..=z1).any(|z| l.facts().get(l.state(x, y, z)).fluid != crate::blocks::Fluid::None)
        })
    })
}
pub fn spawn_box(x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let half = f64::from(width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(height), z + half),
    )
}
fn spawn_dimensions_scale(s: Species) -> f32 {
    match s {
        Species::Slime | Species::MagmaCube => 4.0,
        Species::SulfurCube => 2.0,
        _ => 1.0,
    }
}
pub fn spawn_aabb(s: Species, x: f64, y: f64, z: f64, width: f32, height: f32) -> Aabb {
    let scale = spawn_dimensions_scale(s);
    let half = f64::from(scale * width / 2.0);
    Aabb::new(
        Vec3::new(x - half, y, z - half),
        Vec3::new(x + half, y + f64::from(scale * height), z + half),
    )
}
pub(crate) fn spawn_obstruction<L: NaturalLevel>(
    s: Species,
    l: &L,
    bounds: Aabb,
    spawned: Option<&[Aabb]>,
    x: i32,
    y: i32,
    z: i32,
) -> bool {
    let unobstructed = |l: &L| {
        spawned.is_none_or(|s| l.unobstructed(bounds) && !s.iter().any(|b| b.intersects(bounds)))
    };
    match obstruction(s) {
        Obstruction::NoLiquidUnobstructed => !contains_any_liquid(l, bounds) && unobstructed(l),
        Obstruction::Unobstructed => unobstructed(l),
        Obstruction::NoLiquid => !contains_any_liquid(l, bounds),
        Obstruction::Ocelot => {
            if unobstructed(l) && !contains_any_liquid(l, bounds) {
                if y < l.sea_level() {
                    return false;
                }
                let below = name_at(l, x, y - 1, z);
                if below == "minecraft:grass_block" || l.block_tag("minecraft:leaves", below) {
                    return true;
                }
            }
            false
        }
    }
}

#[derive(Clone, Debug)]
pub struct Override {
    pub piece: bool,
    pub spawns: Vec<SpawnEntry>,
}
#[derive(Clone, Debug, Default)]
pub struct StructureSpawns {
    pub overrides: FxHashMap<String, FxHashMap<String, Override>>,
}
impl StructureSpawns {
    pub fn shared(root: &Path) -> &'static Self {
        static SHARED: std::sync::OnceLock<StructureSpawns> = std::sync::OnceLock::new();
        SHARED.get_or_init(|| Self::load(root))
    }
    pub fn load(root: &Path) -> Self {
        let mut overrides = FxHashMap::default();
        let dir = root.join("data/minecraft/worldgen/structure");
        for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            let Some(v) = fs::read(&path)
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            else {
                continue;
            };
            let map: FxHashMap<String, Override> = v["spawn_overrides"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(category, o)| {
                    (
                        category.clone(),
                        Override {
                            piece: o["bounding_box"].as_str() == Some("piece"),
                            spawns: parse_entries(&o["spawns"]),
                        },
                    )
                })
                .collect();
            if !map.is_empty() {
                overrides.insert(format!("minecraft:{stem}"), map);
            }
        }
        Self { overrides }
    }
    pub fn get(&self, structure: &str, category: &str) -> Option<&Override> {
        self.overrides.get(structure)?.get(category)
    }
}

#[derive(Clone, Debug)]
pub struct StructureAt {
    pub id: String,
    pub in_piece: bool,
    pub in_bounds: bool,
}

pub fn fortress_enemies() -> &'static [SpawnEntry] {
    static ENEMIES: std::sync::LazyLock<Vec<SpawnEntry>> = std::sync::LazyLock::new(|| {
        let e = |entity: &str, weight, min, max, constant| SpawnEntry {
            entity: entity.to_owned(),
            weight,
            min,
            max,
            constant,
        };
        vec![
            e("minecraft:blaze", 10, 2, 3, false),
            e("minecraft:zombified_piglin", 5, 4, 4, true),
            e("minecraft:wither_skeleton", 8, 5, 5, true),
            e("minecraft:skeleton", 2, 5, 5, true),
            e("minecraft:magma_cube", 3, 4, 4, true),
        ]
    });
    &ENEMIES
}

pub fn mobs_at<L: NaturalLevel>(
    l: &L,
    category: Category,
    x: i32,
    y: i32,
    z: i32,
) -> &[SpawnEntry] {
    let overrides = l.structure_spawns();
    let monster = category == Category::Monster;
    let structures = l.structures_at(x, y, z, &|id| {
        (monster && id == "minecraft:fortress") || overrides.get(id, category.name()).is_some()
    });
    if category == Category::Monster
        && name_at(l, x, y - 1, z) == "minecraft:nether_bricks"
        && structures
            .iter()
            .any(|s| s.id == "minecraft:fortress" && s.in_bounds)
    {
        return fortress_enemies();
    }
    for s in &structures {
        if let Some(o) = overrides.get(&s.id, category.name())
            && (if o.piece { s.in_piece } else { s.in_bounds })
        {
            return &o.spawns;
        }
    }
    l.biome_spawns(x, y, z)
        .and_then(|b| b.get(category.name()))
        .map_or(&[], Vec::as_slice)
}

pub fn weighted_pick(list: &[SpawnEntry], random: &mut impl RandomSource) -> Option<SpawnEntry> {
    let total: i64 = list.iter().map(|e| i64::from(e.weight)).sum();
    if total <= 0 {
        return None;
    }
    let mut selection = i64::from(random.next_int_bound(total as i32).ok()?);
    for e in list {
        selection -= i64::from(e.weight);
        if selection < 0 {
            return Some(e.clone());
        }
    }
    None
}

pub fn sample_count(entry: &SpawnEntry, random: &mut impl RandomSource) -> u32 {
    if entry.constant {
        entry.min
    } else {
        let span = entry.max.saturating_sub(entry.min) as i32 + 1;
        entry.min + random.next_int_bound(span).unwrap_or(0) as u32
    }
}
fn same_spawner(a: &SpawnEntry, b: &SpawnEntry) -> bool {
    a.entity == b.entity && a.constant == b.constant && a.min == b.min && a.max == b.max
}

#[derive(Clone, Debug)]
pub struct Counted<'a> {
    pub entity: &'a str,
    pub category: Category,
    pub block: [i32; 3],
    pub is_mob: bool,
    pub persistent: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct SpawnPlayer {
    pub position: [f64; 3],
    pub spectator: bool,
}

#[derive(Clone, Debug)]
pub struct SpawnState {
    spawnable_chunks: i32,
    counts: [i32; 8],
    charges: Vec<([i32; 3], f64)>,
    players: Vec<SpawnPlayer>,
    players_near: FxHashMap<(i32, i32), Vec<usize>>,
    player_counts: FxHashMap<usize, [i32; 8]>,
    last_checked: Option<([i32; 3], String, f64)>,
}
impl SpawnState {
    pub fn new<L: NaturalLevel>(
        l: &L,
        spawnable_chunks: i32,
        players: Vec<SpawnPlayer>,
        entities: &[Counted],
        chunk_loaded: impl Fn(i32, i32) -> bool,
    ) -> Self {
        let mut state = Self {
            spawnable_chunks,
            counts: [0; 8],
            charges: Vec::new(),
            players,
            players_near: FxHashMap::default(),
            player_counts: FxHashMap::default(),
            last_checked: None,
        };
        for e in entities {
            if e.is_mob && e.persistent || e.category == Category::Misc {
                continue;
            }
            let [x, y, z] = e.block;
            if !chunk_loaded(x >> 4, z >> 4) {
                continue;
            }
            if let Some(cost) = l.spawn_cost(e.entity, x, y, z) {
                state.add_charge(e.block, cost.charge);
            }
            if e.is_mob {
                state.add_local(x >> 4, z >> 4, e.category);
            }
            state.counts[e.category.index()] += 1;
        }
        state
    }
    fn add_charge(&mut self, pos: [i32; 3], charge: f64) {
        if charge != 0.0 {
            self.charges.push((pos, charge));
        }
    }
    fn players_near<'a>(
        near: &'a mut FxHashMap<(i32, i32), Vec<usize>>,
        players: &[SpawnPlayer],
        cx: i32,
        cz: i32,
    ) -> &'a [usize] {
        near.entry((cx, cz)).or_insert_with(|| {
            let tracked = players.iter().any(|p| {
                let (px, pz) = (
                    (p.position[0].floor() as i32) >> 4,
                    (p.position[2].floor() as i32) >> 4,
                );
                (px - cx).abs().max((pz - cz).abs()) <= 8
            });
            if !tracked {
                return Vec::new();
            }
            let (x, z) = (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8));
            players
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    let (dx, dz) = (x - p.position[0], z - p.position[2]);
                    !p.spectator && dx * dx + dz * dz < 16384.0
                })
                .map(|(i, _)| i)
                .collect()
        })
    }
    fn add_local(&mut self, cx: i32, cz: i32, category: Category) {
        for &p in Self::players_near(&mut self.players_near, &self.players, cx, cz) {
            self.player_counts.entry(p).or_insert([0; 8])[category.index()] += 1;
        }
    }
    pub fn can_spawn_global(&self, category: Category) -> bool {
        let max = category.max() * self.spawnable_chunks / 289;
        self.counts[category.index()] < max
    }
    pub fn can_spawn_local(&mut self, category: Category, cx: i32, cz: i32) -> bool {
        let near = Self::players_near(&mut self.players_near, &self.players, cx, cz);
        near.iter().any(|p| {
            self.player_counts
                .get(p)
                .is_none_or(|c| c[category.index()] < category.max())
        })
    }
    pub fn filtered_categories(
        &self,
        spawn_enemies: bool,
        spawn_persistent: bool,
    ) -> Vec<Category> {
        Category::SPAWNING
            .into_iter()
            .filter(|&c| {
                (spawn_enemies || c.friendly())
                    && (spawn_persistent || !c.persistent())
                    && self.can_spawn_global(c)
            })
            .collect()
    }
    fn can_spawn<L: NaturalLevel>(&mut self, l: &L, entity: &str, pos: [i32; 3]) -> bool {
        match l.spawn_cost(entity, pos[0], pos[1], pos[2]) {
            None => {
                self.last_checked = Some((pos, entity.to_owned(), 0.0));
                true
            }
            Some(cost) => {
                self.last_checked = Some((pos, entity.to_owned(), cost.charge));
                self.energy_change(pos, cost.charge) <= cost.energy_budget
            }
        }
    }
    fn energy_change(&self, pos: [i32; 3], charge: f64) -> f64 {
        if charge == 0.0 {
            return 0.0;
        }
        let mut change = 0.0;
        for (p, c) in &self.charges {
            let d = [
                f64::from(p[0] - pos[0]),
                f64::from(p[1] - pos[1]),
                f64::from(p[2] - pos[2]),
            ];
            let dist = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
            change += if dist == 0.0 {
                f64::INFINITY
            } else {
                c / dist.sqrt()
            };
        }
        change * charge
    }
    fn after_spawn<L: NaturalLevel>(
        &mut self,
        l: &L,
        entity: &str,
        category: Category,
        pos: [i32; 3],
    ) {
        let charge = match &self.last_checked {
            Some((p, t, c)) if *p == pos && t == entity => *c,
            _ => l
                .spawn_cost(entity, pos[0], pos[1], pos[2])
                .map_or(0.0, |c| c.charge),
        };
        self.add_charge(pos, charge);
        self.counts[category.index()] += 1;
        self.add_local(pos[0] >> 4, pos[2] >> 4, category);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Categories(Vec<Category>),
    Local(Category, bool),
    Start(Category, [i32; 3]),
    Try(i32, i32),
    Test(String, [i32; 3], bool),
    Spawn(String, [f64; 3], f32),
}
#[derive(Clone, Debug)]
pub struct SpawnRequest<'a> {
    pub entity: &'a str,
    pub species: Species,
    pub position: [f64; 3],
    pub yaw: f32,
    pub group_index: u32,
    pub reason: Reason,
}
#[derive(Clone, Copy, Debug)]
pub struct Spawned {
    pub bounds: Aabb,
    pub school: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_for_chunk<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    categories: &[Category],
    chunk: (i32, i32),
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut dyn FnMut(Event),
) {
    for &category in categories {
        let local = state.can_spawn_local(category, chunk.0, chunk.1);
        trace(Event::Local(category, local));
        if local {
            spawn_category_for_chunk(l, random, state, category, chunk, spawn, trace);
        }
    }
}
pub fn spawn_category_for_chunk<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    category: Category,
    chunk: (i32, i32),
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut dyn FnMut(Event),
) {
    let x = chunk.0 * 16 + random.next_int_bound(16).unwrap_or(0);
    let z = chunk.1 * 16 + random.next_int_bound(16).unwrap_or(0);
    let top = l.heightmap(Heightmap::WorldSurface, x, z) + 1;
    let min = l.min_y();
    let y = min + random.next_int_bound(top - min + 1).unwrap_or(0);
    trace(Event::Start(category, [x, y, z]));
    if y > min {
        spawn_category_for_position(l, random, state, category, chunk, [x, y, z], spawn, trace);
    }
}
#[allow(clippy::too_many_arguments)]
pub fn spawn_category_for_position<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    state: &mut SpawnState,
    category: Category,
    chunk: (i32, i32),
    start: [i32; 3],
    spawn: &mut dyn FnMut(&L, &SpawnRequest) -> Option<Spawned>,
    trace: &mut dyn FnMut(Event),
) {
    let [x0, y, z0] = start;
    if l.mob_data().state(l.state(x0, y, z0)).flags & super::data::REDSTONE_CONDUCTOR != 0 {
        return;
    }
    let mut spawned: Vec<Aabb> = Vec::new();
    let mut cluster = 0u32;
    let mut nearest = Nearest::new(start);
    for _ in 0..3 {
        let (mut x, mut z) = (x0, z0);
        let mut current: Option<(SpawnEntry, Option<MobType>, [i32; 3])> = None;
        let mut group_index = 0u32;
        let mut max = (random.next_float() * 4.0).ceil() as i32;
        let mut ll = 0;
        while ll < max {
            ll += 1;
            x += random.next_int_bound(6).unwrap_or(0) - random.next_int_bound(6).unwrap_or(0);
            z += random.next_int_bound(6).unwrap_or(0) - random.next_int_bound(6).unwrap_or(0);
            let (xx, zz) = (f64::from(x) + 0.5, f64::from(z) + 0.5);
            trace(Event::Try(x, z));
            let Some(distance) = nearest.distance_sqr(l, xx, f64::from(y), zz) else {
                continue;
            };
            if !right_distance(l, chunk, [x, y, z], distance) {
                continue;
            }
            if current.is_none() {
                let list = if category == Category::WaterAmbient
                    && l.biome_tag("minecraft:reduce_water_ambient_spawns", x, y, z)
                    && random.next_float() < 0.98
                {
                    None
                } else {
                    weighted_pick(mobs_at(l, category, x, y, z), random)
                };
                let Some(entry) = list else {
                    break;
                };
                max = sample_count(&entry, random) as i32;
                let kind = l.mob_type(&entry.entity);
                current = Some((entry, kind, [x, y, z]));
            }
            let (entry, kind, picked_at) = current.as_ref().unwrap_or_else(|| unreachable!());
            let listed = *picked_at == [x, y, z];
            let valid = valid_position_for_type(
                l,
                random,
                category,
                entry,
                *kind,
                listed,
                [x, y, z],
                distance,
            );
            let Some((species, width, height, type_category)) = (*kind).filter(|_| valid) else {
                continue;
            };
            let passed = state.can_spawn(l, &entry.entity, [x, y, z]);
            trace(Event::Test(entry.entity.clone(), [x, y, z], passed));
            if !passed {
                continue;
            }
            let yaw = random.next_float() * 360.0;
            let bounds = spawn_box(xx, f64::from(y), zz, width, height);
            let despawn = f64::from(type_category.despawn_distance().pow(2));
            let far = distance > despawn && removed_when_far(species);
            if far
                || !mob_spawn_rules(species, l, x, y, z)
                || !spawn_obstruction(species, l, bounds, Some(&spawned), x, y, z)
            {
                continue;
            }
            let request = SpawnRequest {
                entity: &entry.entity,
                species,
                position: [xx, f64::from(y), zz],
                yaw,
                group_index,
                reason: Reason::Natural,
            };
            let Some(result) = spawn(l, &request) else {
                continue;
            };
            group_index += 1;
            spawned.push(result.bounds);
            cluster += 1;
            state.after_spawn(l, &entry.entity, type_category, [x, y, z]);
            trace(Event::Spawn(
                entry.entity.clone(),
                [xx, f64::from(y), zz],
                yaw,
            ));
            if cluster >= max_cluster_size(species) {
                return;
            }
            if species == Species::TropicalFish && !result.school {
                break;
            }
        }
    }
}
struct Nearest {
    nearby: Option<Vec<[f64; 3]>>,
    start: (f64, f64),
}
impl Nearest {
    const REACH: f64 = 128.0;
    const MARGIN: f64 = 64.0;
    fn new(start: [i32; 3]) -> Self {
        Self {
            nearby: None,
            start: (f64::from(start[0]) + 0.5, f64::from(start[2]) + 0.5),
        }
    }
    fn distance_sqr<L: NaturalLevel>(&mut self, l: &L, x: f64, y: f64, z: f64) -> Option<f64> {
        let all = l.nearest_player_positions();
        if all.is_empty() {
            return None;
        }
        let distance = |p: &[f64; 3]| (p[0] - x).powi(2) + (p[1] - y).powi(2) + (p[2] - z).powi(2);
        let (sx, sz) = self.start;
        if (x - sx).abs() > Self::MARGIN || (z - sz).abs() > Self::MARGIN {
            return all.iter().map(distance).reduce(f64::min);
        }
        let nearby = self.nearby.get_or_insert_with(|| {
            all.iter()
                .filter(|p| {
                    let dx = ((p[0] - sx).abs() - Self::MARGIN).max(0.0);
                    let dz = ((p[2] - sz).abs() - Self::MARGIN).max(0.0);
                    dx * dx + dz * dz <= Self::REACH * Self::REACH
                })
                .copied()
                .collect()
        });
        Some(nearby.iter().map(distance).fold(f64::INFINITY, f64::min))
    }
}
fn right_distance<L: NaturalLevel>(l: &L, chunk: (i32, i32), pos: [i32; 3], distance: f64) -> bool {
    if distance <= 576.0 {
        return false;
    }
    if let Some(r) = l.respawn() {
        let d = [
            f64::from(r[0]) + 0.5 - (f64::from(pos[0]) + 0.5),
            f64::from(r[1]) + 0.5 - f64::from(pos[1]),
            f64::from(r[2]) + 0.5 - (f64::from(pos[2]) + 0.5),
        ];
        if d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < 24.0 * 24.0 {
            return false;
        }
    }
    let (cx, cz) = (pos[0] >> 4, pos[2] >> 4);
    (cx, cz) == chunk || l.can_spawn_in_chunk(cx, cz)
}
#[allow(clippy::too_many_arguments)]
fn valid_position_for_type<L: NaturalLevel, R: RandomSource>(
    l: &L,
    random: &mut R,
    category: Category,
    entry: &SpawnEntry,
    kind: Option<MobType>,
    listed: bool,
    pos: [i32; 3],
    distance: f64,
) -> bool {
    let Some((species, width, height, type_category)) = kind else {
        return false;
    };
    if type_category == Category::Misc {
        return false;
    }
    let despawn = f64::from(type_category.despawn_distance().pow(2));
    if !spawns_far_from_player(species, type_category) && distance > despawn {
        return false;
    }
    let [x, y, z] = pos;
    if !listed
        && !mobs_at(l, category, x, y, z)
            .iter()
            .any(|e| same_spawner(e, entry))
    {
        return false;
    }
    if !spawn_rules::is_spawn_position_ok(species, l, x, y, z) {
        return false;
    }
    if !spawn_rules::check_spawn_rules(species, l, Reason::Natural, random, x, y, z) {
        return false;
    }
    l.no_collision(spawn_aabb(
        species,
        f64::from(x) + 0.5,
        f64::from(y),
        f64::from(z) + 0.5,
        width,
        height,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenEvent {
    Column(i32, i32),
    Spawn(String, [f64; 3], f32),
}
#[allow(clippy::too_many_arguments)]
pub fn spawn_mobs_for_chunk_generation<L: NaturalLevel, R: RandomSource, Q: RandomSource>(
    l: &L,
    source: [i32; 3],
    chunk: (i32, i32),
    random: &mut R,
    rules: &mut Q,
    spawn: &mut dyn FnMut(&L, &SpawnRequest, &mut Q) -> bool,
    trace: &mut dyn FnMut(GenEvent),
) {
    if !l.spawn_mobs() {
        return;
    }
    let [sx, sy, sz] = source;
    let mobs = l
        .biome_spawns(sx, sy, sz)
        .and_then(|b| b.get("creature"))
        .cloned()
        .unwrap_or_default();
    if mobs.iter().map(|e| e.weight).sum::<u32>() == 0 {
        return;
    }
    let probability = l.creature_probability(sx, sy, sz);
    let (xo, zo) = (chunk.0 * 16, chunk.1 * 16);
    while random.next_float() < probability {
        let Some(entry) = weighted_pick(&mobs, random) else {
            continue;
        };
        let count = sample_count(&entry, random);
        let mut group_index = 0u32;
        let mut x = xo + random.next_int_bound(16).unwrap_or(0);
        let mut z = zo + random.next_int_bound(16).unwrap_or(0);
        let (start_x, start_z) = (x, z);
        let kind = l.mob_type(&entry.entity);
        for _ in 0..count {
            let mut success = false;
            let mut attempts = 0;
            while !success && attempts < 4 {
                attempts += 1;
                let Some((species, width, height, _)) = kind else {
                    break;
                };
                let pos = top_non_colliding(l, &entry.entity, x, z);
                trace(GenEvent::Column(x, z));
                if spawn_rules::is_spawn_position_ok(species, l, pos[0], pos[1], pos[2]) {
                    let fx = f64::from(x).clamp(
                        f64::from(xo) + f64::from(width),
                        f64::from(xo) + 16.0 - f64::from(width),
                    );
                    let fz = f64::from(z).clamp(
                        f64::from(zo) + f64::from(width),
                        f64::from(zo) + 16.0 - f64::from(width),
                    );
                    let fy = f64::from(pos[1]);
                    let (bx, bz) = (fx.floor() as i32, fz.floor() as i32);
                    if !l.no_collision(spawn_aabb(species, fx, fy, fz, width, height))
                        || !spawn_rules::check_spawn_rules(
                            species,
                            l,
                            Reason::ChunkGeneration,
                            rules,
                            bx,
                            pos[1],
                            bz,
                        )
                    {
                        continue;
                    }
                    let yaw = random.next_float() * 360.0;
                    let bounds = spawn_box(fx, fy, fz, width, height);
                    if mob_spawn_rules(species, l, bx, pos[1], bz)
                        && spawn_obstruction(species, l, bounds, None, bx, pos[1], bz)
                    {
                        let request = SpawnRequest {
                            entity: &entry.entity,
                            species,
                            position: [fx, fy, fz],
                            yaw,
                            group_index,
                            reason: Reason::ChunkGeneration,
                        };
                        if spawn(l, &request, rules) {
                            group_index += 1;
                            trace(GenEvent::Spawn(entry.entity.clone(), [fx, fy, fz], yaw));
                            success = true;
                        }
                    }
                }
                x += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                z += random.next_int_bound(5).unwrap_or(0) - random.next_int_bound(5).unwrap_or(0);
                while x < xo || x >= xo + 16 || z < zo || z >= zo + 16 {
                    x = start_x + random.next_int_bound(5).unwrap_or(0)
                        - random.next_int_bound(5).unwrap_or(0);
                    z = start_z + random.next_int_bound(5).unwrap_or(0)
                        - random.next_int_bound(5).unwrap_or(0);
                }
            }
        }
    }
}
fn top_non_colliding<L: NaturalLevel>(l: &L, entity: &str, x: i32, z: i32) -> [i32; 3] {
    let heightmap = Heightmap::from_name(l.mob_data().heightmap(entity));
    let mut y = l.heightmap(heightmap, x, z);
    if l.has_ceiling() {
        loop {
            y -= 1;
            if l.facts().is_air(l.state(x, y, z)) {
                break;
            }
        }
        loop {
            y -= 1;
            if !(l.facts().is_air(l.state(x, y, z)) && y > l.min_y()) {
                break;
            }
        }
    }
    if l.mob_data().placement(entity) == super::data::Placement::OnGround
        && l.pathfindable_land(x, y - 1, z)
    {
        y -= 1;
    }
    [x, y, z]
}
