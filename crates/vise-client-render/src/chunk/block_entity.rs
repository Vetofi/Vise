use vise_client_core::{text::Text, who::Who};
use vise_items::{banner_layer::BannerLayer, items::ItemStack};
use vise_world::{
    level::sculk::SculkSpreader,
    nbt::{Compound, Tag},
};
use std::{collections::HashMap, sync::Arc};

pub type Uuid = [i32; 4];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FurnaceKind {
    Furnace,
    BlastFurnace,
    Smoker,
}
#[derive(Clone, Debug)]
pub struct Furnace {
    pub kind: FurnaceKind,
    pub items: [Option<ItemStack>; 3],
    pub lit_remaining: i32,
    pub lit_total: i32,
    pub cooking_timer: i32,
    pub cooking_total: i32,
    pub speed: f32,
    pub recipes_used: HashMap<String, u32>,
}
#[derive(Clone, Debug)]
pub enum BlockEntity {
    Chest(Vec<Option<ItemStack>>),
    Furnace(Furnace),
    Brewing(Box<BrewingStand>),
    Sign(SignData),
    Jukebox(Jukebox),
    Beacon(Box<Beacon>),
    Lectern(Lectern),
    Sculk(Box<SculkEntity>),
    CreakingHeart(Box<CreakingHeart>),
    Banner(Banner),
    Crafter(Crafter),
    Spawner(Box<Spawner>),
    TrialSpawner(Box<TrialSpawner>),
    Vault(Box<Vault>),
    Beehive(Box<Beehive>),
    CommandBlock(Box<CommandBlock>),
    Campfire(Box<Campfire>),
    Skull(Skull),
    EndGateway(EndGateway),
    DaylightDetector,
    Brushable(Box<Brushable>),
    Shelf(Box<Shelf>),
    ChiseledBookshelf(Box<ChiseledBookshelf>),
    Mod(Box<ModBlockEntity>),
    PotentSulfur(PotentSulfur),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModBlockEntity {
    pub id: String,
    pub nbt: Compound,
    pub items: Vec<Option<ItemStack>>,
    pub data: Vec<i32>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModContainer {
    pub slots: usize,
    pub faces: [Vec<usize>; 6],
    pub inputs: Vec<usize>,
    pub outputs: Vec<usize>,
    pub menu: Option<String>,
}
pub const MOD_MENUS: [(&str, usize); 11] = [
    ("minecraft:furnace", 3),
    ("minecraft:blast_furnace", 3),
    ("minecraft:smoker", 3),
    ("minecraft:hopper", 5),
    ("minecraft:generic_3x3", 9),
    ("minecraft:generic_9x1", 9),
    ("minecraft:generic_9x2", 18),
    ("minecraft:generic_9x3", 27),
    ("minecraft:generic_9x4", 36),
    ("minecraft:generic_9x5", 45),
    ("minecraft:generic_9x6", 54),
];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PotentSulfur {
    pub waiting_countdown: i32,
    pub eruption_tick: i64,
}
impl Default for PotentSulfur {
    fn default() -> Self {
        Self {
            waiting_countdown: -1,
            eruption_tick: -1,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Brushable {
    pub loot_table: Option<vise_world::nbt::Tag>,
    pub loot_seed: Option<vise_world::nbt::Tag>,
    pub item: Option<ItemStack>,
    pub hit_direction: Option<i32>,
}
#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub items: [Option<ItemStack>; 3],
    pub align_to_bottom: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ChiseledBookshelf {
    pub items: [Option<ItemStack>; 6],
    pub last_slot: i32,
}
impl Default for ChiseledBookshelf {
    fn default() -> Self {
        Self {
            items: Default::default(),
            last_slot: -1,
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndGateway {
    pub age: i64,
    pub exit_portal: Option<[i32; 3]>,
    pub exact_teleport: bool,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Campfire {
    pub items: [Option<ItemStack>; 4],
    pub progress: [i32; 4],
    pub total: [i32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct Skull {
    pub profile: Option<vise_world::nbt::Tag>,
    pub note_block_sound: Option<String>,
    pub custom_name: Option<vise_world::nbt::Tag>,
    pub skin: Option<SkullSkin>,
}
#[derive(Clone, Debug)]
pub struct SkullSkin {
    pub download: Option<(String, String)>,
    pub dynamic: Option<NameOrId>,
    pub fallback: &'static str,
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NameOrId {
    Name(String),
    Id(u128),
}
#[derive(Clone, Debug, Default)]
pub struct Crafter {
    pub items: [Option<ItemStack>; 9],
    pub disabled: [bool; 9],
    pub triggered: bool,
    pub crafting_ticks: i32,
    pub state_triggered: Option<bool>,
}
#[derive(Clone, Debug, Default)]
pub struct Banner {
    pub layers: Vec<BannerLayer>,
    pub name: Option<String>,
    pub components: vise_world::nbt::Compound,
}
#[derive(Clone, Debug, Default)]
pub struct CreakingHeart {
    pub creaking: Option<(Option<u32>, [i32; 4])>,
    pub ticks_existed: i64,
    pub ticker: i32,
    pub emitter: i32,
    pub emitter_target: Option<[f64; 3]>,
    pub output_signal: i32,
}
#[derive(Clone, Debug, Default)]
pub struct Lectern {
    pub book: Option<ItemStack>,
    pub page: i32,
}
#[derive(Clone, Debug, Default)]
pub struct Beacon {
    pub levels: i32,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub sections: Vec<(u32, i32)>,
    pub checking: Vec<(u32, i32)>,
    pub last_check_y: i32,
}
#[derive(Clone, Debug, Default)]
pub struct Jukebox {
    pub record: Option<ItemStack>,
    pub ticks: i64,
    pub playing: bool,
}
#[derive(Clone, Debug, Default)]
pub struct BrewingStand {
    pub items: [Option<ItemStack>; 5],
    pub brew_time: i32,
    pub total_brew_time: i32,
    pub fuel: i32,
    pub total_fuel: i32,
    pub speed: f32,
    pub ingredient: Option<Arc<str>>,
    pub last_bottles: Option<[bool; 3]>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignText {
    pub lines: [String; 4],
    pub color: u8,
    pub glowing: bool,
    pub click: [Option<Tag>; 4],
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SignData {
    pub hanging: bool,
    pub front: SignText,
    pub back: SignText,
    pub waxed: bool,
    pub allow_op_features: bool,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Info {
    pub event: &'static str,
    pub distance: f32,
    pub pos: [f64; 3],
    pub source: Option<Who>,
    pub owner: Option<Who>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selector {
    pub current: Option<(Info, i64)>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    pub current: Option<Info>,
    pub travel: i32,
    pub selector: Selector,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SculkKind {
    Sensor,
    Calibrated,
    Shrieker,
    Catalyst,
}
#[derive(Clone, Debug)]
pub struct SculkEntity {
    pub kind: SculkKind,
    pub vibration: Data,
    pub last_frequency: i32,
    pub warning_level: i32,
    pub spreader: SculkSpreader,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CustomSpawnRules {
    pub block: (i32, i32),
    pub sky: (i32, i32),
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpawnData {
    pub entity: Compound,
    pub custom_spawn_rules: Option<CustomSpawnRules>,
    pub equipment: Option<Tag>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Next {
    Potential(usize),
    Own(SpawnData),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Spawner {
    pub spawn_delay: i32,
    pub potentials: Vec<(SpawnData, i32)>,
    pub next: Option<Next>,
    pub min_spawn_delay: i32,
    pub max_spawn_delay: i32,
    pub spawn_count: i32,
    pub max_nearby_entities: i32,
    pub required_player_range: i32,
    pub spawn_range: i32,
    pub client_delay: i32,
    pub spin: f64,
    pub o_spin: f64,
}
impl Default for Spawner {
    fn default() -> Self {
        Self {
            spawn_delay: 20,
            potentials: Vec::new(),
            next: None,
            min_spawn_delay: 200,
            max_spawn_delay: 800,
            spawn_count: 4,
            max_nearby_entities: 6,
            required_player_range: 16,
            spawn_range: 4,
            client_delay: 20,
            spin: 0.0,
            o_spin: 0.0,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TrialConfig {
    pub spawn_range: i32,
    pub total_mobs: f32,
    pub simultaneous_mobs: f32,
    pub total_mobs_added_per_player: f32,
    pub simultaneous_mobs_added_per_player: f32,
    pub ticks_between_spawn: i32,
    pub spawn_potentials: Vec<(SpawnData, i32)>,
    pub loot_tables_to_eject: Vec<(String, i32)>,
    pub items_to_drop_when_ominous: String,
}
impl Default for TrialConfig {
    fn default() -> Self {
        Self {
            spawn_range: 4,
            total_mobs: 6.0,
            simultaneous_mobs: 2.0,
            total_mobs_added_per_player: 2.0,
            simultaneous_mobs_added_per_player: 1.0,
            ticks_between_spawn: 40,
            spawn_potentials: Vec::new(),
            loot_tables_to_eject: vec![
                ("minecraft:spawners/trial_chamber/consumables".into(), 1),
                ("minecraft:spawners/trial_chamber/key".into(), 1),
            ],
            items_to_drop_when_ominous:
                "minecraft:spawners/trial_chamber/items_to_drop_when_ominous".into(),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConfigSlot {
    pub saved: Option<Tag>,
    pub config: TrialConfig,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TrialSpawner {
    pub normal: ConfigSlot,
    pub ominous: ConfigSlot,
    pub target_cooldown_length: i32,
    pub required_player_range: i32,
    pub detected_players: Vec<Uuid>,
    pub current_mobs: Vec<Uuid>,
    pub cooldown_ends_at: i64,
    pub next_mob_spawns_at: i64,
    pub total_mobs_spawned: i32,
    pub next_spawn_data: Option<SpawnData>,
    pub ejecting_loot_table: Option<String>,
    pub is_ominous: bool,
    pub has_display: bool,
    pub spin: f64,
    pub o_spin: f64,
    pub dispensing: Option<Vec<ItemStack>>,
}
impl Default for TrialSpawner {
    fn default() -> Self {
        Self {
            normal: ConfigSlot::default(),
            ominous: ConfigSlot::default(),
            target_cooldown_length: 36000,
            required_player_range: 14,
            detected_players: Vec::new(),
            current_mobs: Vec::new(),
            cooldown_ends_at: 0,
            next_mob_spawns_at: 0,
            total_mobs_spawned: 0,
            next_spawn_data: None,
            ejecting_loot_table: None,
            is_ominous: false,
            has_display: false,
            spin: 0.0,
            o_spin: 0.0,
            dispensing: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct VaultConfig {
    pub loot_table: String,
    pub activation_range: f64,
    pub deactivation_range: f64,
    pub key_item: Option<ItemStack>,
    pub override_loot_table_to_display: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Vault {
    pub config: VaultConfig,
    pub rewarded_players: Vec<Uuid>,
    pub state_updating_resumes_at: i64,
    pub items_to_eject: Vec<ItemStack>,
    pub total_ejections_needed: i32,
    pub last_insert_fail_timestamp: i64,
    pub display_item: Option<ItemStack>,
    pub connected_players: Vec<Uuid>,
    pub connected_particles_range: f64,
    pub shared_dirty: bool,
    pub spin: f32,
    pub o_spin: f32,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Beehive {
    pub bees: Vec<Occupant>,
    pub flower: Option<[i32; 3]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Occupant {
    pub entity_data: Tag,
    pub ticks_in_hive: i32,
    pub min_ticks_in_hive: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandBlock {
    pub command: String,
    pub success_count: i32,
    pub custom_name: Option<Tag>,
    pub track_output: bool,
    pub last_output: Option<Text>,
    pub update_last_execution: bool,
    pub last_execution: i64,
    pub powered: bool,
    pub auto: bool,
    pub condition_met: bool,
}
