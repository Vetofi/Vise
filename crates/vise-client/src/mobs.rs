pub mod ai;
pub mod allay;
pub mod ambient;
pub mod anim;
pub mod animal_brain;
pub mod aquatic;
pub mod armadillo;
pub mod axolotl;
pub mod bat;
pub mod bee;
pub mod body_anim;
pub mod brain;
pub mod breeze;
pub mod camel;
pub mod caravan;
pub mod combat;
pub mod control;
pub mod conversion;
pub mod copper_golem;
pub mod creaking;
pub mod creaking_heart;
pub mod cube;
pub mod data;
pub mod dolphin;
pub mod doors;
pub mod drops;
pub mod enderman;
pub mod equine;
pub mod farm;
pub mod finalize;
pub mod fox;
pub mod frog;
pub mod gear;
pub mod goals;
pub mod goat;
pub mod golem;
pub mod guardian;
pub mod happy_ghast;
pub mod hostile;
pub mod illager;
pub mod illusioner;
pub mod item_break;
pub mod keyframes;
pub mod leash;
pub mod living;
pub mod llama;
pub mod monster_brain;
pub mod nautilus;
pub mod nav;
pub mod nether;
pub mod panda;
pub mod parrot;
pub mod path;
pub mod phantom;
pub mod piglin;
pub mod polar_bear;
pub mod rabbit;
pub mod raider;
pub mod ravager;
pub mod render;
pub mod riders;
pub mod seen_grid;
pub mod shulker;
pub mod sniffer;
pub mod spawn_rules;
pub mod spawn_selection;
pub mod spawner;
pub mod spawning;
pub mod spear_use;
pub mod species;
pub mod strider;
pub mod sulfur_cube;
pub mod sync;
pub mod tame;
pub mod trader;
pub mod transport_items;
pub mod trial_spawner;
pub mod turtle;
pub mod undead;
pub mod vex;
pub mod villager;
pub mod villager_brain;
pub mod villager_hats;
pub mod warden;
pub mod warden_brain;
pub mod witch;
pub mod wither;
mod world;
pub use world::visual_clip;
pub mod zombie_villager;

use crate::{blocks::BlockFacts, mesher::Vertex, world::World};
pub use data::MobData;
use vise_core::{
    geometry::{Aabb, Vec3},
    random::{LegacyRandom, RandomSource},
};
use vise_world::block_collision::BlockCollisionRegistry;
use serde_json::Value;
use std::{collections::HashMap, fs, path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub struct Polygon {
    pub normal: [f32; 3],
    pub vertices: [[f32; 5]; 4],
}
impl Polygon {
    fn vertex_normal(&self, pose: ModelNormals) -> [i8; 4] {
        let n = pose.matrix * glam::Vec3::from_array(self.normal);
        let n = if pose.trusted {
            n
        } else {
            n.normalize_or_zero()
        };
        let packed = n.to_array().map(|v| (v.clamp(-1.0, 1.0) * 127.0) as i8);
        [packed[0], packed[1], packed[2], -127]
    }
}

#[derive(Clone, Copy)]
struct ModelNormals {
    matrix: glam::Mat3,
    trusted: bool,
}
impl ModelNormals {
    fn new(base: glam::Mat4) -> Self {
        let m = glam::Mat3::from_mat4(base);
        let lengths = glam::Vec3::new(m.x_axis.length(), m.y_axis.length(), m.z_axis.length());
        let uniform =
            lengths.min_element() > 0.0 && lengths.max_element() - lengths.min_element() < 1.0e-6;
        Self {
            matrix: if uniform {
                m / lengths.x
            } else {
                m.inverse().transpose()
            },
            trusted: uniform,
        }
    }
    fn rotation(x: f32, y: f32, z: f32) -> glam::Mat3 {
        let sin_cos = |angle: f32| {
            let angle = angle * 0.5;
            let sin = f64::from(angle).sin() as f32;
            let cos = (1.0 - sin * sin).sqrt();
            let a = angle + std::f32::consts::FRAC_PI_2;
            let tau = std::f32::consts::TAU;
            let mut quadrant = a - (a / tau) as i32 as f32 * tau;
            if quadrant < 0.0 {
                quadrant += tau;
            }
            (
                sin,
                if quadrant >= std::f32::consts::PI {
                    -cos
                } else {
                    cos
                },
            )
        };
        let (sx, cx) = sin_cos(x);
        let (sy, cy) = sin_cos(y);
        let (sz, cz) = sin_cos(z);
        let (cycz, sysz, sycz, cysz) = (cy * cz, sy * sz, sy * cz, cy * sz);
        let (w, x, y, z) = (
            cx * cycz + sx * sysz,
            sx * cycz - cx * sysz,
            cx * sycz + sx * cysz,
            cx * cysz - sx * sycz,
        );
        let (ww, xx, yy, zz) = (w * w, x * x, y * y, z * z);
        let (xy, xz, xw, yz, yw, zw) = (x * y, x * z, x * w, y * z, y * w, z * w);
        glam::Mat3::from_cols_array(&[
            ww + xx - zz - yy,
            (xy + xy) + (zw + zw),
            (xz + xz) - (yw + yw),
            (xy + xy) - (zw + zw),
            yy - zz + ww - xx,
            (yz + yz) + (xw + xw),
            (yw + yw) + (xz + xz),
            (yz + yz) - (xw + xw),
            zz - yy - xx + ww,
        ])
    }
    fn part(mut self, pose: [f32; 9]) -> Self {
        self.matrix *= Self::rotation(pose[3], pose[4], pose[5]);
        let scale = glam::Vec3::new(pose[6], pose[7], pose[8]);
        if scale.x.abs() == scale.y.abs() && scale.x.abs() == scale.z.abs() {
            if scale.min_element() < 0.0 {
                self.matrix *= glam::Mat3::from_diagonal(scale.signum());
            }
        } else {
            self.matrix *= glam::Mat3::from_diagonal(scale.recip());
            self.trusted = false;
        }
        self
    }
}

#[derive(Clone, Debug)]
pub struct Part {
    pub name: String,
    pub pose: [f32; 9],
    pub polygons: Vec<Polygon>,
    pub children: Vec<Part>,
}
impl Part {
    fn parse(name: &str, v: &Value) -> Self {
        let pose: Vec<f32> = v["pose"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect();
        let mut p = [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        for (i, x) in pose.iter().enumerate().take(9) {
            p[i] = *x;
        }
        let polygons = v["polygons"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, poly)| {
                let verts: Vec<[f32; 5]> = poly
                    .as_array()?
                    .iter()
                    .map(|vx| {
                        let a: Vec<f32> = vx
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
                            .collect();
                        [a[0], a[1], a[2], a[3], a[4]]
                    })
                    .collect();
                (verts.len() == 4).then(|| Polygon {
                    normal: v["normals"][index].as_array().map_or_else(
                        || {
                            let p = verts
                                .iter()
                                .map(|v| glam::Vec3::new(v[0], v[1], v[2]))
                                .collect::<Vec<_>>();
                            (p[1] - p[0])
                                .cross(p[2] - p[0])
                                .normalize_or_zero()
                                .to_array()
                        },
                        |n| std::array::from_fn(|i| n[i].as_f64().unwrap_or(0.0) as f32),
                    ),
                    vertices: [verts[0], verts[1], verts[2], verts[3]],
                })
            })
            .collect();
        let mut children: Vec<Part> = v["children"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, c)| Part::parse(k, c))
            .collect();
        children.sort_by(|a, b| a.name.cmp(&b.name));
        let name = if name == "root" && children.iter().any(|c| c.name == "root") {
            ""
        } else {
            name
        };
        Self {
            name: name.to_owned(),
            pose: p,
            polygons,
            children,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brain {
    Passive,
    Chicken,
    Zombie,
    Skeleton,
    Spider,
    Creeper,
}
pub use anim::Anim;
pub use species::Species;

#[derive(Clone, Copy, Debug, Default)]
pub struct Flags {
    pub aquatic_swimmer: bool,
    pub fall_immune: bool,
    pub flying: bool,
    pub climber: bool,
    pub lava_walker: bool,
    pub omnidirectional: bool,
    pub fire_immune: bool,
    pub burns_in_day: bool,
    pub undead: bool,
    pub breathes_water: bool,
    pub needs_water: bool,
    pub water_sensitive: bool,
    pub animal: bool,
}

#[derive(Clone, Debug)]
pub struct MobType {
    pub id: String,
    pub species: Species,
    pub width: f64,
    pub height: f64,
    pub eye_height: f64,
    pub category: String,
    pub max_health: f32,
    pub speed: f64,
    pub flying_speed: f64,
    pub attack_damage: f32,
    pub attack_knockback: f64,
    pub follow_range: f64,
    pub step_height: f64,
    pub armor: f32,
    pub armor_toughness: f32,
    pub knockback_resistance: f64,
    pub jump_strength: f64,
    pub attributes: Arc<HashMap<String, f64>>,
    pub gravity: f64,
    pub safe_fall_distance: f64,
    pub fall_damage_multiplier: f64,
    pub water_efficiency: f64,
    pub water_slowdown: f32,
    pub air_speed: f32,
    pub baby_scale: f64,
    pub head_rot_speed: f32,
    pub max_head_x_rot: f32,
    pub max_head_y_rot: f32,
    pub ambient_interval: i32,
    pub flags: Flags,
    pub model: Arc<Part>,
    pub layers: HashMap<String, Arc<Part>>,
    pub texture: String,
    pub brain: Brain,
    pub anim: Anim,
    pub sounds: [&'static str; 3],
    pub step_sound: Option<&'static str>,
    pub food: Arc<[String]>,
    pub tempt: Arc<[String]>,
}
impl MobType {
    pub fn layer(&self, key: &str) -> Option<&Arc<Part>> {
        self.layers.get(key)
    }
}
fn exact_attributes(bytes: &[u8]) -> HashMap<String, HashMap<String, f64>> {
    use serde_json::value::RawValue;
    type Raw = HashMap<String, Box<RawValue>>;
    let Ok(types) = serde_json::from_slice::<Raw>(bytes) else {
        return HashMap::new();
    };
    types
        .into_iter()
        .filter_map(|(id, t)| {
            let t: Raw = serde_json::from_str(t.get()).ok()?;
            let attrs: Raw = serde_json::from_str(t.get("attributes")?.get()).ok()?;
            let values = attrs
                .into_iter()
                .filter_map(|(k, v)| v.get().trim().parse::<f64>().ok().map(|v| (k, v)))
                .collect();
            Some((id, values))
        })
        .collect()
}
pub struct MobTypes {
    pub types: HashMap<String, MobType>,
}
impl MobTypes {
    pub fn load(root: &Path) -> crate::Result<Self> {
        let bytes = fs::read(root.join("entity-types.json"))?;
        let types: Value = serde_json::from_slice(&bytes)?;
        let exact = exact_attributes(&bytes);
        let models: Value = serde_json::from_slice(&fs::read(root.join("entity-models.json"))?)?;
        let item_tags =
            crate::items::load_tags(root, "item", &vise_world::loot::PackOverlay::NONE)
                .unwrap_or_default();
        sulfur_cube::load_archetype_items(&item_tags);
        let tag = |name: &str| -> Arc<[String]> {
            let mut v: Vec<String> = item_tags
                .get(&format!("minecraft:{name}"))
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            v.sort();
            v.into()
        };
        let mut out = HashMap::new();
        for def in species::ALL {
            let id = format!("minecraft:{}", def.name);
            let t = &types[&id];
            if t.is_null() {
                continue;
            }
            let model_key = format!("minecraft:{}", def.model);
            let Some(model) = models.get(&model_key) else {
                continue;
            };
            let defaults = exact.get(&id).cloned().unwrap_or_default();
            let attr = |k: &str| defaults.get(&format!("minecraft:{k}")).copied();
            let mut layers = HashMap::new();
            if let Some(all) = models.as_object() {
                for prefix in def.layer_prefixes() {
                    let start = format!("minecraft:{prefix}#");
                    for (k, v) in all.iter().filter(|(k, _)| k.starts_with(&start)) {
                        layers.insert(
                            k.trim_start_matches("minecraft:").to_owned(),
                            Arc::new(Part::parse("root", v)),
                        );
                    }
                }
            }
            let category = t["category"].as_str().unwrap_or("creature").to_owned();
            let speed = attr("movement_speed").unwrap_or(0.25);
            let flags = def.flags(t["fire_immune"].as_bool().unwrap_or(false));
            out.insert(
                id.clone(),
                MobType {
                    species: def.species,
                    width: t["width"].as_f64().unwrap_or(0.6),
                    height: t["height"].as_f64().unwrap_or(1.8),
                    eye_height: t["eye_height"].as_f64().unwrap_or(1.6),
                    max_health: attr("max_health").unwrap_or(10.0) as f32,
                    speed,
                    flying_speed: attr("flying_speed").unwrap_or(0.4),
                    attack_damage: attr("attack_damage").unwrap_or(0.0) as f32,
                    attack_knockback: attr("attack_knockback").unwrap_or(0.0),
                    follow_range: attr("follow_range").unwrap_or(16.0),
                    step_height: attr("step_height").unwrap_or(0.6),
                    armor: attr("armor").unwrap_or(0.0) as f32,
                    armor_toughness: attr("armor_toughness").unwrap_or(0.0) as f32,
                    knockback_resistance: attr("knockback_resistance").unwrap_or(0.0),
                    jump_strength: attr("jump_strength").unwrap_or(0.42),
                    attributes: Arc::new(defaults.clone()),
                    gravity: attr("gravity").unwrap_or(0.08),
                    safe_fall_distance: attr("safe_fall_distance").unwrap_or(3.0),
                    fall_damage_multiplier: attr("fall_damage_multiplier").unwrap_or(1.0),
                    water_efficiency: attr("water_movement_efficiency").unwrap_or(0.0),
                    water_slowdown: def.water_slowdown(),
                    air_speed: def.air_speed(),
                    baby_scale: def.baby_scale(),
                    head_rot_speed: def.head_rot_speed(),
                    max_head_x_rot: def.max_head_x_rot(),
                    max_head_y_rot: def.max_head_y_rot(),
                    ambient_interval: def.ambient_interval(),
                    flags,
                    model: Arc::new(Part::parse("root", model)),
                    layers,
                    texture: def.texture.to_owned(),
                    brain: def.brain(),
                    anim: def.anim,
                    sounds: def.sounds,
                    step_sound: def.step_sound,
                    food: def
                        .food_tag()
                        .map(tag)
                        .unwrap_or_else(|| Arc::from(Vec::new())),
                    tempt: def
                        .tempt_tag()
                        .or(def.food_tag())
                        .map(tag)
                        .unwrap_or_else(|| Arc::from(Vec::new())),
                    category,
                    id,
                },
            );
        }
        Ok(Self { types: out })
    }
}
pub use vise_client_core::who::Who;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DamageKind {
    PlayerAttack,
    MaceSmash,
    Spear,
    MobAttack,
    Arrow,
    Thrown,
    Fireball,
    Explosion,
    Fall,
    FallingBlock,
    FallingStalactite,
    OnFire,
    InFire,
    Campfire,
    Lava,
    Drown,
    DryOut,
    Magic,
    IndirectMagic,
    Sting,
    Cactus,
    SweetBerryBush,
    Generic,
    InWall,
    Thorns,
    SonicBoom,
    Freeze,
    OutOfWorld,
    Cramming,
    FallingAnvil,
    Fireworks,
    GenericKill,
    LightningBolt,
    OutsideBorder,
    Spit,
    Stalagmite,
    Starve,
    HotFloor,
    SulfurCubeHot,
    WindCharge,
    Wither,
}
impl DamageKind {
    pub fn bypasses_invulnerability(self) -> bool {
        matches!(self, DamageKind::OutOfWorld | DamageKind::GenericKill)
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Damage {
    pub kind: DamageKind,
    pub attacker: Option<Who>,
    pub from: Option<[f64; 3]>,
}
impl Damage {
    pub fn fall() -> Self {
        Self {
            kind: DamageKind::Fall,
            attacker: None,
            from: None,
        }
    }
    pub fn of(kind: DamageKind) -> Self {
        Self {
            kind,
            attacker: None,
            from: None,
        }
    }
    pub fn player(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::PlayerAttack,
            attacker: Some(Who::Player),
            from: Some(from),
        }
    }
    pub fn mace(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::MaceSmash,
            attacker: Some(Who::Player),
            from: Some(from),
        }
    }
    pub fn spear(from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::Spear,
            attacker: Some(Who::Player),
            from: Some(from),
        }
    }
    pub fn indirect_magic(attacker: Option<Who>, from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::IndirectMagic,
            attacker,
            from: Some(from),
        }
    }
    pub fn mob(id: u32, from: [f64; 3]) -> Self {
        Self {
            kind: DamageKind::MobAttack,
            attacker: Some(Who::Mob(id)),
            from: Some(from),
        }
    }
    pub fn causes_panic(&self) -> bool {
        !matches!(
            self.kind,
            DamageKind::Fall
                | DamageKind::Drown
                | DamageKind::DryOut
                | DamageKind::Generic
                | DamageKind::InWall
                | DamageKind::SweetBerryBush
                | DamageKind::OutOfWorld
                | DamageKind::FallingBlock
                | DamageKind::FallingAnvil
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Spit
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::Campfire
        )
    }
    pub fn bypasses_armor(&self) -> bool {
        matches!(
            self.kind,
            DamageKind::Fall
                | DamageKind::OnFire
                | DamageKind::Drown
                | DamageKind::Magic
                | DamageKind::IndirectMagic
                | DamageKind::InWall
                | DamageKind::Generic
                | DamageKind::SonicBoom
                | DamageKind::Freeze
                | DamageKind::OutOfWorld
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::Wither
        )
    }
    pub fn is_fire(&self) -> bool {
        matches!(
            self.kind,
            DamageKind::OnFire
                | DamageKind::InFire
                | DamageKind::Campfire
                | DamageKind::HotFloor
                | DamageKind::Lava
                | DamageKind::Fireball
                | DamageKind::SulfurCubeHot
        )
    }
    pub fn no_knockback(&self) -> bool {
        matches!(
            self.kind,
            DamageKind::Explosion
                | DamageKind::Fall
                | DamageKind::OnFire
                | DamageKind::InFire
                | DamageKind::Campfire
                | DamageKind::HotFloor
                | DamageKind::Lava
                | DamageKind::Drown
                | DamageKind::DryOut
                | DamageKind::Magic
                | DamageKind::Cactus
                | DamageKind::SweetBerryBush
                | DamageKind::Generic
                | DamageKind::InWall
                | DamageKind::Freeze
                | DamageKind::OutOfWorld
                | DamageKind::LightningBolt
                | DamageKind::Cramming
                | DamageKind::GenericKill
                | DamageKind::OutsideBorder
                | DamageKind::Stalagmite
                | DamageKind::Starve
                | DamageKind::SulfurCubeHot
                | DamageKind::Wither
        )
    }
    pub fn is_projectile(&self) -> bool {
        matches!(
            self.kind,
            DamageKind::Arrow | DamageKind::Thrown | DamageKind::Fireball | DamageKind::WindCharge
        )
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Attrs {
    pub speed: f64,
    pub attack_damage: f32,
    pub max_health: f32,
    pub follow_range: f64,
    pub knockback_resistance: f64,
    pub spawn_reinforcements: f64,
    pub jump_strength: f64,
}
pub const WOLF_ARMOR_DURABILITY: i32 = 64;
pub fn anger_left(anger_end: i64, game_time: i64) -> i32 {
    if anger_end <= 0 {
        return 0;
    }
    (anger_end - game_time).clamp(0, i64::from(i32::MAX)) as i32
}

#[derive(Clone, Debug, Default)]
pub struct Extra {
    pub model_scale: f32,
    pub crouching: bool,
    pub avatar_pose: crate::pose::Pose,
    pub sprinting: bool,
    pub passenger: bool,
    pub age: i32,
    pub age_locked: bool,
    pub age_lock_particle_timer: i32,
    pub forced_age: i32,
    pub forced_age_timer: i32,
    pub in_love: i32,
    pub love_player: bool,
    pub variant: u8,
    pub variant2: u8,
    pub variant3: u8,
    pub color: u8,
    pub sheared: bool,
    pub pick_up_loot: bool,
    pub saddled: bool,
    pub body_damage: i32,
    pub body_extra: Option<Box<crate::item_data::StackData>>,
    pub relax_one: bool,
    pub on_sleeper: bool,
    pub saddle_stack: Option<crate::items::ItemStack>,
    pub chested: bool,
    pub items: Vec<Option<crate::items::ItemStack>>,
    pub inventory: Vec<crate::items::ItemStack>,
    pub tame: bool,
    pub sitting: bool,
    pub party_jukebox: Option<[i32; 3]>,
    pub owner: Option<[i32; 4]>,
    pub size: i32,
    pub anim_ticks: i32,
    pub egg_time: i32,
    pub anger: i32,
    pub anger_end: i64,
    pub angry_at: Option<Who>,
    pub player_created: bool,
    pub carried: Option<vise_world::registry::BlockStateId>,
    pub charged: bool,
    pub main_hand: Option<Arc<str>>,
    main_hand_stack: Option<crate::items::ItemStack>,
    pub off_hand: Option<Arc<str>>,
    pub head: Option<Arc<str>>,
    pub chest: Option<Arc<str>>,
    pub legs: Option<Arc<str>>,
    pub feet: Option<Arc<str>>,
    pub armor: [Option<crate::items::ItemStack>; 4],
    pub using_item: i32,
    pub using_off_hand: bool,
    pub stabbed: Option<Vec<(Who, i64)>>,
    pub blocking: bool,
    pub fall_flying: bool,
    pub fall_fly_ticks: i32,
    pub synced_pose: u8,
    pub auto_spin_attack: bool,
    pub main_hand_left: bool,
    pub hidden_model_parts: u8,
    pub elytra: ElytraAnimation,
    pub swim: SwimAnimation,
    pub avatar: crate::camera::AvatarBob,
    pub swing_right_arm: Option<bool>,
    pub swing_stab: bool,
    pub flag: bool,
    pub flag2: bool,
    pub stand_sliding: bool,
    pub ints: [i32; 6],
    pub counter: i32,
    pub counter2: i32,
    pub home: Option<[i32; 3]>,
    pub opened_chest: Option<[i32; 3]>,
    pub point: Option<[f64; 3]>,
    pub repellent: Option<[i32; 3]>,
    pub vector: [f64; 3],
    pub effects: Vec<(&'static str, i32, i32)>,
    pub hidden_effects: Vec<&'static str>,
    pub effect_particles: Vec<crate::particles::ParticleOptions>,
    pub effect_ambient: bool,
    pub illusions: Option<illusioner::Illusions>,
    pub bed: Option<[i32; 3]>,
    pub meeting: Option<[i32; 3]>,
    pub trade: Option<Box<crate::merchant::Trader>>,
    pub leash: Option<leash::Holder>,
    pub leash_pending: Option<leash::Pending>,
    pub leash_player: Option<[i32; 4]>,
    pub leash_spin: f64,
    pub scratch: [i32; 4],
    pub positions: Vec<[i32; 3]>,
    pub warden: Option<Box<warden::Warden>>,
    pub allay: Option<Box<allay::State>>,
    pub raider: Option<Box<raider::State>>,
    pub villager: Option<Box<villager::Ext>>,
    pub animal_brain: Option<Box<animal_brain::Brain>>,
    pub axolotl: axolotl::Animation,
    pub mount: Option<u32>,
    pub rider: Option<u32>,
    pub boarding_cooldown: i32,
    pub jockey: bool,
    pub trap: bool,
    pub trap_time: i32,
    pub gear: Option<Box<gear::Gear>>,
    pub drop_chances: Option<Box<[f32; 8]>>,
    pub left_handed: bool,
    pub sound_variant: Option<u8>,
    pub can_break_doors: bool,
    pub screaming: bool,
    pub lost_horns: [bool; 2],
    pub goat: goat::State,
    pub attributes: Option<Box<finalize::Attributes>>,
    pub memories: Vec<(&'static str, i64, i64)>,
    pub pose_change_tick: i64,
    pub camel_sitting: bool,
    pub pose: u8,
    pub leader: Option<u32>,
    pub lone: bool,
    pub followers: i32,
    pub brain: Option<Box<monster_brain::Brain>>,
    pub bee: Option<Box<bee::Bee>>,
    pub armadillo: armadillo::State,
    pub frog: frog::State,
    pub panda: Option<Box<panda::State>>,
    pub piglin: piglin::Memories,
}
impl Extra {
    pub fn set_main_hand(&mut self, item: Option<Arc<str>>) {
        self.main_hand = item;
        self.main_hand_stack = None;
    }
    pub fn set_main_hand_stack(&mut self, stack: Option<crate::items::ItemStack>) {
        self.main_hand = stack.as_ref().map(|s| s.item.clone());
        self.main_hand_stack = stack;
    }
    pub fn main_hand_stack(&self) -> Option<&crate::items::ItemStack> {
        self.main_hand_stack
            .as_ref()
            .filter(|s| self.main_hand.as_ref() == Some(&s.item))
    }
    pub fn take_main_hand(&mut self) -> Option<Arc<str>> {
        self.main_hand_stack = None;
        self.main_hand.take()
    }
}

#[derive(Clone, Debug)]
pub struct Mob {
    pub id: u32,
    pub kind: Arc<MobType>,
    pub position: Vec3,
    pub previous: Vec3,
    pub velocity: Vec3,
    pub yaw: f32,
    pub previous_yaw: f32,
    pub body_yaw: f32,
    pub previous_body_yaw: f32,
    pub head_yaw: f32,
    pub previous_head_yaw: f32,
    pub head_pitch: f32,
    pub previous_head_pitch: f32,
    pub on_ground: bool,
    pub horizontal_collision: bool,
    pub vertical_collision_below: bool,
    pub needs_sync: bool,
    pub health: f32,
    pub hurt_time: i32,
    pub invulnerable: i32,
    pub death_time: i32,
    pub age: u32,
    pub limb_position: f32,
    pub limb_speed: f32,
    pub previous_limb_speed: f32,
    pub swell: i32,
    pub previous_swell: i32,
    pub ambient_timer: i32,
    pub fall_distance: f64,
    pub stuck_multiplier: Vec3,
    pub piston: vise_world::piston_movement::PistonMovement,
    pub persistent: bool,
    pub custom_name: Option<String>,
    pub custom_name_text: Option<crate::text::Text>,
    pub name_visible: bool,
    pub invisible: bool,
    pub random: LegacyRandom,
    pub ai: Box<ai::Ai>,
    pub extra: Extra,
    pub fluids: living::Fluids,
    pub was_eye_in_water: bool,
    pub first_tick: bool,
    pub no_gravity: bool,
    pub supporting: Option<[i32; 3]>,
    pub no_ai: bool,
    pub silent: bool,
    pub invulnerable_flag: bool,
    pub climbing: bool,
    pub fire_ticks: i32,
    pub ticks_frozen: i32,
    pub glowing: bool,
    pub air: i32,
    pub last_hurt: f32,
    pub last_damage: Option<(Damage, i64)>,
    pub combat: combat::Tracker,
    pub removed: bool,
    pub anim: anim::AnimState,
    pub attrs: Attrs,
    pub rider: Option<equine::Rider>,
    pub outbox: Vec<MobEvent>,
    pub skin: Option<String>,
    pub slim: bool,
    pub cape: Option<String>,
    pub profile: Option<vise_protocol::entity_data::Profile>,
}
#[derive(Clone, Debug)]
pub enum MobEvent {
    SquidCycle {
        mob: u32,
        at: [f64; 3],
    },
    RabbitHop {
        mob: u32,
        at: [f64; 3],
    },
    Drown {
        mob: u32,
        at: [f64; 3],
    },
    EntityEvent {
        mob: u32,
        event: i8,
        at: [f64; 3],
    },
    LevelParticles {
        kind: &'static str,
        at: [f64; 3],
        spread: [f64; 3],
        speed: f64,
        count: u32,
    },
    Sound(&'static str, [f64; 3], f32, f32),
    Bonemeal {
        pos: [i32; 3],
    },
    RingBell {
        bell: [i32; 3],
    },
    ChestOpen {
        chest: [i32; 3],
        open: bool,
    },
    EnterHive {
        mob: u32,
        hive: [i32; 3],
    },
    ParrotToShoulder {
        mob: u32,
    },
    BecomeStatue {
        mob: u32,
    },
    ChestSlot {
        chest: [i32; 3],
        slot: usize,
        stack: Option<crate::items::ItemStack>,
    },
    Infested {
        mob: u32,
    },
    Gift {
        target: u32,
        item: &'static str,
    },
    BreedTarget {
        mob: u32,
        partner: u32,
    },
    LockGaze {
        mob: u32,
        partner: u32,
    },
    StartFollowing {
        fish: u32,
        leader: u32,
    },
    SchoolSize {
        leader: u32,
        by: i32,
    },
    SoundId(String, [f64; 3], f32, f32),
    LocalSound(&'static str, [f64; 3], f32, f32),
    LevelEventSound {
        sound: &'static str,
        at: [f64; 3],
        volume: f32,
        spread: f32,
    },
    GameEvent {
        event: &'static str,
        at: [f64; 3],
        source: u32,
        state: vise_world::registry::BlockStateId,
    },
    AttackPlayer(f32, [f64; 3]),
    AttackPlayerWithEffect {
        amount: f32,
        from: [f64; 3],
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    },
    DamagePlayer {
        amount: f32,
        from: [f64; 3],
        kind: &'static str,
    },
    ElderCurse {
        at: [f64; 3],
    },
    Push {
        target: Who,
        velocity: [f64; 3],
    },
    SonicBoom {
        target: Who,
        attacker: u32,
        from: [f64; 3],
        direction: [f64; 3],
    },
    Shift {
        target: Who,
        delta: [f64; 3],
    },
    Wololo {
        mob: u32,
    },
    Fangs {
        at: [f64; 3],
        yaw: f32,
        delay: i32,
        owner: u32,
    },
    Shoot([f64; 3], [f64; 3]),
    ExplosionSource {
        mob: u32,
        charged: bool,
        name: crate::text::Text,
    },
    Explode([f64; 3], f32, Species),
    AttackMob {
        target: u32,
        amount: f32,
        source: Damage,
    },
    CubeHurtMob {
        cube: u32,
        target: u32,
        amount: f32,
        source: Damage,
    },
    StabDismount {
        target: Who,
    },
    Knockback {
        target: Who,
        power: f64,
        x: f64,
        z: f64,
    },
    SetTarget {
        mob: u32,
        target: Who,
    },
    HuntHoglin {
        mob: u32,
        target: u32,
        ticks: i32,
    },
    Retreat {
        mob: u32,
        from: Who,
        at: [f64; 3],
    },
    StartRiding {
        rider: u32,
        vehicle: u32,
    },
    Projectile(crate::projectiles::Projectile),
    Spawn(Box<Mob>),
    SpawnKind {
        kind: String,
        at: [f64; 3],
        setup: SpawnSetup,
    },
    Convert {
        from: Box<Mob>,
        kind: &'static str,
        params: conversion::Params,
    },
    DropItem {
        item: Arc<str>,
        count: u32,
        at: [f64; 3],
    },
    DropGift {
        table: &'static str,
        at: [f64; 3],
    },
    SetBlock {
        pos: [i32; 3],
        state: vise_world::registry::BlockStateId,
    },
    BedOccupied {
        pos: [i32; 3],
        occupied: bool,
    },
    SetDoorOpen {
        pos: [i32; 3],
        open: bool,
    },
    BreakDoor {
        pos: [i32; 3],
    },
    DestroyProgress {
        mob: u32,
        pos: [i32; 3],
        progress: i32,
    },
    LevelEvent {
        id: i32,
        pos: [i32; 3],
        data: i32,
    },
    Infest {
        pos: [i32; 3],
        infested: bool,
        poof: Option<u32>,
    },
    BreakBlock {
        pos: [i32; 3],
        drop: bool,
    },
    Particles {
        kind: &'static str,
        at: [f64; 3],
        count: u32,
    },
    AroundSelf {
        mob: u32,
        event: i8,
        kind: &'static str,
        feet: [f64; 3],
        width: f64,
        height: f64,
        burst: crate::particles::Burst,
    },
    ParticleAt {
        kind: &'static str,
        at: [f64; 3],
        velocity: [f64; 3],
    },
    Experience {
        at: [f64; 3],
        amount: u32,
    },
    AxolotlSupportPlayer {
        mob: u32,
    },
    DolphinTreasure {
        mob: u32,
    },
    EffectPlayer {
        effect: &'static str,
        ticks: i32,
        amplifier: i32,
    },
    Ignite {
        target: Who,
        seconds: i32,
    },
    Breed {
        parent: u32,
        partner: u32,
        llama: Option<Box<super::mobs::llama::Offspring>>,
    },
    SetBlockName {
        pos: [i32; 3],
        name: &'static str,
        effect: bool,
    },
    Hiss {
        mob: u32,
    },
    Tamed {
        mob: u32,
    },
    Buck {
        mob: u32,
    },
    FinishAdmiring {
        mob: u32,
    },
    Launch {
        target: Who,
        up: f64,
    },
    HurtSelf {
        mob: u32,
        kind: DamageKind,
        amount: f32,
    },
    VillagerMeet {
        mob: u32,
        other: u32,
    },
    VillagerBirth {
        parent: u32,
        partner: u32,
        child: Option<([i32; 3], Option<u8>)>,
    },
    WorkAtComposter {
        mob: u32,
        pos: [i32; 3],
    },
    GolemCheck {
        mob: u32,
        needed: usize,
    },
    Firework {
        at: [f64; 3],
        color: usize,
        flight: u8,
    },
    ThrowGift {
        table: String,
        from: [f64; 3],
        to: [f64; 3],
    },
    PatrolTarget {
        mob: u32,
        target: [i32; 3],
    },
    RaiderTarget {
        mob: u32,
        target: Who,
        aggressive: bool,
    },
    GiftLoot {
        mob: u32,
        table: &'static str,
        at: [f64; 3],
    },
    ShedScute {
        mob: u32,
        at: [f64; 3],
        pitch: f32,
    },
    LeashBreak {
        mob: u32,
        drop: bool,
        sound: bool,
    },
    CureZombieVillager {
        mob: u32,
    },
    Vibration {
        from: [f64; 3],
        to: [f64; 3],
        ticks: i32,
    },
    DigParticles {
        at: [f64; 3],
    },
    Reinforcements {
        mob: u32,
        target: Who,
    },
    BlockBreakEffect {
        pos: [i32; 3],
        block: [i32; 3],
    },
    Darkness {
        at: [f64; 3],
        radius: f64,
    },
    CreakingHurt {
        home: [i32; 3],
        by_player: bool,
        id: u32,
    },
    CreakingTearDown {
        center: [f64; 3],
        spread: [f64; 3],
    },
    PandaSneezed {
        mob: u32,
    },
    FallToPassengers {
        mob: u32,
        distance: f64,
        modifier: f32,
    },
}
#[derive(Clone, Debug, Default)]
pub struct SpawnSetup {
    pub baby: bool,
    pub size: i32,
    pub yaw: f32,
    pub velocity: [f64; 3],
    pub health: Option<f32>,
    pub persistent: bool,
    pub variant: Option<u8>,
    pub nausea: bool,
    pub owner_mob: Option<u32>,
    pub home: Option<[i32; 3]>,
    pub life: Option<i32>,
}
#[derive(Clone, Debug)]
pub struct Seen {
    pub in_water: bool,
    pub playing_dead: bool,
    pub last_damage: Option<Damage>,
    pub id: u32,
    pub species: Species,
    pub position: Vec3,
    pub bounds: Aabb,
    pub eye_y: f64,
    pub alive: bool,
    pub baby: bool,
    pub in_love: bool,
    pub tame: bool,
    pub owner: Option<[i32; 4]>,
    pub hostile: bool,
    pub target: Option<Who>,
    pub health: f32,
    pub variant: u8,
    pub size: i32,
    pub color: u8,
    pub job: Option<[i32; 3]>,
    pub bed: Option<[i32; 3]>,
    pub yaw: f32,
    pub velocity: Vec3,
    pub known_speed: Vec3,
    pub leash: Option<leash::Holder>,
    pub caravan: u32,
    pub school: (Option<u32>, i32),
    pub delay: i32,
    pub hurt_by: Option<(Who, i32)>,
    pub active_raid: bool,
    pub opened_chest: Option<[i32; 3]>,
    pub passenger: bool,
    pub vehicle: Option<u32>,
    pub player_rep: i32,
    pub pushable: bool,
    pub cold_strider: bool,
    pub category: &'static str,
    pub potential_job: Option<[i32; 3]>,
    pub meeting: Option<[i32; 3]>,
    pub profession: u8,
    pub xp: i32,
    pub villager: Option<Arc<villager::Seen>>,
    pub stats: [f64; 3],
    pub sitting: bool,
    pub silent: bool,
    pub lying: bool,
    pub ridden: bool,
    pub knockback_resistance: f64,
    pub effects: Vec<&'static str>,
    pub hunted_recently: bool,
    pub hurt_recently: bool,
}
impl Seen {
    pub fn has_effect(&self, effect: &str) -> bool {
        self.effects.contains(&effect)
    }
}
impl Mob {
    pub fn burning_time(&self) -> f64 {
        let base = finalize::value(self, finalize::Attr::BurningTime);
        let protection: u32 = gear::ARMOR
            .into_iter()
            .map(|slot| gear::level(self, slot, "minecraft:fire_protection"))
            .sum();
        (base - 0.15 * f64::from(protection)).max(0.0)
    }
    pub fn is_on_fire(&self) -> bool {
        !self.kind.flags.fire_immune && self.fire_ticks > 0
    }
    pub fn ignite_for_ticks(&mut self, ticks: i32) {
        self.fire_ticks = self
            .fire_ticks
            .max((f64::from(ticks) * self.burning_time()).ceil() as i32);
        self.ticks_frozen = 0;
    }
    pub fn extinguish_fire(&mut self) {
        if self.is_on_fire() {
            let pitch = 1.6 + (self.random.next_float() - self.random.next_float()) * 0.4;
            let at = [self.position.x, self.position.y, self.position.z];
            self.outbox.push(MobEvent::Sound(
                "entity.generic.extinguish_fire",
                at,
                0.7,
                pitch,
            ));
        }
        self.fire_ticks = self.fire_ticks.min(0);
    }
    pub fn is_fully_frozen(&self) -> bool {
        self.ticks_frozen >= crate::motion::PlayerMotion::TICKS_REQUIRED_TO_FREEZE
    }

    pub fn seen(&self, player: Option<[i32; 4]>) -> Seen {
        let m = self;
        let villager = m.species() == Species::Villager;
        Seen {
            in_water: self.fluids.in_water,
            playing_dead: axolotl::is_playing_dead(self),
            last_damage: self.last_damage.map(|(d, _)| d),
            id: m.id,
            species: m.species(),
            position: m.position,
            bounds: m.bounds(),
            eye_y: m.eye_y(),
            alive: m.alive(),
            baby: m.is_baby(),
            in_love: if villager {
                villager::can_breed(m)
            } else {
                m.extra.in_love > 0
            },
            tame: m.extra.tame,
            owner: m.extra.owner,
            hostile: m.hostile(),
            target: m.ai.target,
            health: m.health,
            variant: m.extra.variant,
            size: m.extra.size,
            color: m.extra.color,
            job: villager.then_some(m.extra.home).flatten(),
            bed: villager.then_some(m.extra.bed).flatten(),
            yaw: m.yaw,
            velocity: m.velocity,
            known_speed: m.position.subtract(m.previous),
            leash: m.extra.leash,
            caravan: caravan::head_of(m),
            school: (m.extra.leader, m.extra.followers),
            delay: m.extra.ints[5],
            hurt_by: m.ai.last_hurt_by.map(|w| (w, m.ai.last_hurt_by_time)),
            active_raid: raider::has_active_raid(m),
            opened_chest: m.extra.opened_chest,
            passenger: m.extra.mount.is_some(),
            vehicle: m.extra.mount,
            player_rep: match (villager, player) {
                (true, Some(uuid)) => villager::ext(m).map_or(0, |e| e.gossips.reputation(uuid)),
                _ => 0,
            },
            pushable: m.pushable(),
            cold_strider: strider::suffocating(m),
            category: m.category(),
            potential_job: villager_brain::potential_job_site(m),
            meeting: villager.then_some(m.extra.meeting).flatten(),
            profession: m.extra.variant2,
            xp: villager::xp(m),
            villager: villager.then(|| Arc::new(villager::seen(m))),
            stats: [
                f64::from(m.attrs.max_health),
                m.extra.vector[0],
                m.attrs.speed,
            ],
            sitting: m.extra.sitting,
            silent: m.silent,
            lying: m.species() == Species::Cat && (m.extra.flag || m.extra.relax_one),
            ridden: m.rider.is_some() || m.extra.rider.is_some() || m.extra.mount.is_some(),
            knockback_resistance: m.attrs.knockback_resistance,
            effects: m
                .extra
                .effects
                .iter()
                .map(|e| e.0.trim_start_matches("minecraft:"))
                .collect(),
            hunted_recently: m.species() == Species::Piglin && m.extra.ints[0] > 0,
            hurt_recently: m.species() == Species::Piglin && m.extra.piglin.hurt,
        }
    }
    pub fn category(&self) -> &'static str {
        match self.kind.category.as_str() {
            "monster" => "monster",
            "creature" => "creature",
            "ambient" => "ambient",
            "axolotls" => "axolotls",
            "underground_water_creature" => "underground_water_creature",
            "water_creature" => "water_creature",
            "water_ambient" => "water_ambient",
            _ => "misc",
        }
    }
}
pub fn take_mob_pushes(events: &mut Vec<MobEvent>, from: usize, pending: &mut PendingPushes) {
    let mut kept = from;
    for i in from..events.len() {
        if let MobEvent::Push {
            target: Who::Mob(id),
            velocity,
        } = events[i]
        {
            pending.push(id, Vec3::new(velocity[0], velocity[1], velocity[2]));
        } else {
            events.swap(kept, i);
            kept += 1;
        }
    }
    events.truncate(kept);
}
#[derive(Default)]
pub struct PendingPushes {
    list: Vec<(u32, Vec3)>,
    by_mob: crate::fast_hash::FxHashMap<u32, Vec<Vec3>>,
}
impl PendingPushes {
    const LIST_MAX: usize = 64;
    pub fn push(&mut self, id: u32, v: Vec3) {
        if self.by_mob.is_empty() && self.list.len() < Self::LIST_MAX {
            self.list.push((id, v));
            return;
        }
        for (id, v) in self.list.drain(..) {
            self.by_mob.entry(id).or_default().push(v);
        }
        self.by_mob.entry(id).or_default().push(v);
    }
}
pub fn apply_mob_pushes(m: &mut Mob, pending: &mut PendingPushes) {
    let mut add = |v: Vec3| {
        m.velocity = Vec3::new(m.velocity.x + v.x, m.velocity.y + v.y, m.velocity.z + v.z);
        m.needs_sync = true;
    };
    if !pending.list.is_empty() {
        pending.list.retain(|(id, v)| {
            if *id != m.id {
                return true;
            }
            add(*v);
            false
        });
    }
    if !pending.by_mob.is_empty()
        && let Some(pushes) = pending.by_mob.remove(&m.id)
    {
        pushes.into_iter().for_each(add);
    }
}
pub fn apply_brain_events(mobs: &mut [Mob], events: &mut Vec<MobEvent>, from: usize) {
    let mut i = from;
    while i < events.len() {
        match events[i] {
            MobEvent::BreedTarget { mob, partner } => {
                if let Some(m) = mobs.iter_mut().find(|m| m.id == mob) {
                    animal_brain::set_breed_partner(m, partner);
                }
                events.remove(i);
            }
            MobEvent::LockGaze { mob, partner } => {
                if let Some(m) = mobs.iter_mut().find(|m| m.id == mob) {
                    animal_brain::lock_gaze(m, partner);
                    villager_brain::lock_gaze_half(m, Who::Mob(partner));
                }
                events.remove(i);
            }
            MobEvent::Retreat { mob, from, at } => {
                if let Some(m) = mobs.iter_mut().find(|m| m.id == mob) {
                    piglin::receive_retreat(m, from, at);
                }
                events.remove(i);
            }
            MobEvent::StartRiding { rider, vehicle } => {
                riders::start_riding(mobs, rider, vehicle);
                events.remove(i);
            }
            MobEvent::StartFollowing { fish, leader } => {
                aquatic::start_following(mobs, fish, leader);
                events.remove(i);
            }
            MobEvent::SchoolSize { leader, by } => {
                if let Some(m) = mobs.iter_mut().find(|m| m.id == leader) {
                    m.extra.followers += by;
                }
                events.remove(i);
            }
            MobEvent::Breed {
                parent, partner, ..
            } => {
                if let Some(m) = mobs.iter_mut().find(|m| m.id == partner) {
                    m.extra.age = 6000;
                    m.extra.in_love = 0;
                    animal_brain::forget_breed_target(m, parent);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}
#[derive(Clone, Debug)]
pub struct PlayerView {
    pub position: Vec3,
    pub bounds: Aabb,
    pub eye: [f64; 3],
    pub alive: bool,
    pub creative: bool,
    pub sneaking: bool,
    pub swimming: bool,
    pub look: [f64; 3],
    pub main_hand: Option<Arc<str>>,
    pub off_hand: Option<Arc<str>>,
    pub head: Option<Arc<str>>,
    pub armor_cover: f32,
    pub invisible: bool,
    pub spectator: bool,
    pub gold_armor: bool,
    pub hurt_by: Option<(Who, i64)>,
    pub last_hurt: Option<(Who, i64)>,
    pub velocity: Vec3,
    pub yaw: f32,
    pub sprinting: bool,
    pub riding: bool,
    pub hero: bool,
    pub effects: Vec<&'static str>,
    pub uuid: [i32; 4],
    pub allies: std::sync::Arc<std::collections::HashSet<u32>>,
    pub remote: bool,
    pub sleeping: bool,
    pub sleep_timer: u32,
    pub passenger: bool,
}
impl PlayerView {
    pub fn visibility_percent(&self) -> f64 {
        let mut visibility = 1.0;
        if self.sneaking {
            visibility *= 0.8;
        }
        if self.invisible {
            visibility *= 0.7 * f64::from(self.armor_cover).max(0.1);
        }
        visibility
    }
    pub fn has_effect(&self, effect: &str) -> bool {
        self.effects.contains(&effect)
    }
}
#[derive(Clone)]
pub struct MobWorld<'a> {
    pub world: &'a World,
    pub registry: &'a vise_world::registry::BlockRegistry,
    pub facts: &'a BlockFacts,
    pub fluid_tables: &'a crate::motion::FluidResources,
    pub movement: &'a vise_world::movement_properties::BoundMovementProperties,
    pub collision: &'a BlockCollisionRegistry<'a>,
    pub data: &'a MobData,
    pub player: Option<PlayerView>,
    pub nearest_player: Option<Vec3>,
    pub others: &'a [Seen],
    pub others_grid: Option<&'a seen_grid::SeenGrid>,
    pub others_index: Option<&'a crate::fast_hash::FxHashMap<u32, u32>>,
    pub daylight: bool,
    pub monsters_burn: bool,
    pub game_time: i64,
    pub day_time: i64,
    pub raining: bool,
    pub thundering: bool,
    pub nether: bool,
    pub block_tags: &'a HashMap<String, std::collections::HashSet<String>>,
    pub biomes: &'a crate::biome::Biomes,
    pub mob_griefing: bool,
    pub mob_drops: bool,
    pub max_entity_cramming: i32,
    pub difficulty: spawning::LocalDifficulty,
    pub end: bool,
    pub world_seed: i64,
    pub sea_level: i32,
    pub teams: std::sync::Arc<Teams<'a>>,
    pub items: Option<&'a crate::items::Items>,
    pub lid_openers: &'a HashMap<(i32, i32, i32), i32>,
}
#[derive(Clone, Debug, Default)]
pub struct Teams<'a> {
    pub player: Option<&'a crate::scoreboard::Team>,
    pub mobs: HashMap<u32, &'a crate::scoreboard::Team>,
}
impl<'a> Teams<'a> {
    pub fn of(&self, who: Who) -> Option<&'a crate::scoreboard::Team> {
        match who {
            Who::Player => self.player,
            Who::Mob(id) => self.mobs.get(&id).copied(),
        }
    }
}
impl Mob {
    pub fn new(id: u32, kind: Arc<MobType>, position: Vec3, yaw: f32) -> Self {
        let mut random = LegacyRandom::new(
            i64::from(id).wrapping_mul(0x5DEE_CE66D)
                ^ (position.x.to_bits() as i64)
                ^ (position.z.to_bits() as i64).rotate_left(17),
        );
        let _ = random.next_int();
        let mut mob = Self {
            id,
            health: kind.max_health,
            position,
            previous: position,
            velocity: Vec3::ZERO,
            yaw,
            previous_yaw: yaw,
            body_yaw: yaw,
            previous_body_yaw: yaw,
            head_yaw: yaw,
            previous_head_yaw: yaw,
            head_pitch: 0.0,
            previous_head_pitch: 0.0,
            on_ground: false,
            horizontal_collision: false,
            vertical_collision_below: false,
            needs_sync: false,
            hurt_time: 0,
            invulnerable: 0,
            death_time: 0,
            age: 0,
            limb_position: 0.0,
            limb_speed: 0.0,
            previous_limb_speed: 0.0,
            swell: 0,
            previous_swell: 0,
            ambient_timer: 0,
            fall_distance: 0.0,
            stuck_multiplier: Vec3::ZERO,
            piston: Default::default(),
            persistent: false,
            random,
            ai: Box::default(),
            extra: Extra::default(),
            fluids: living::Fluids::default(),
            was_eye_in_water: false,
            first_tick: true,
            no_gravity: false,
            supporting: None,
            no_ai: false,
            silent: false,
            invulnerable_flag: false,
            climbing: false,
            fire_ticks: 0,
            ticks_frozen: 0,
            glowing: false,
            air: 300,
            last_hurt: 0.0,
            last_damage: None,
            combat: combat::Tracker::default(),
            removed: false,
            custom_name: None,
            custom_name_text: None,
            name_visible: false,
            invisible: false,
            anim: anim::AnimState::default(),
            rider: None,
            outbox: Vec::new(),
            skin: None,
            slim: false,
            cape: None,
            profile: None,
            attrs: Attrs {
                speed: kind.speed,
                attack_damage: kind.attack_damage,
                max_health: kind.max_health,
                follow_range: kind.follow_range,
                knockback_resistance: kind.knockback_resistance,
                spawn_reinforcements: kind
                    .attributes
                    .get("minecraft:spawn_reinforcements")
                    .copied()
                    .unwrap_or(0.0),
                jump_strength: kind.jump_strength,
            },
            kind,
        };
        mob.extra.pick_up_loot = matches!(
            mob.species(),
            Species::Villager
                | Species::Piglin
                | Species::PiglinBrute
                | Species::Dolphin
                | Species::Fox
                | Species::Panda
        );
        mob.ai.nav.max_visited_nodes = (mob.kind.follow_range * 16.0).floor() as i32;
        ai::register(&mut mob);
        mob
    }
    pub fn can_pick_up_loot(&self) -> bool {
        match self.species() {
            Species::Allay => self.extra.main_hand.is_some() && self.extra.ints[3] <= 0,
            Species::SulfurCube => !sulfur_cube::has_body(self),
            _ => self.extra.pick_up_loot,
        }
    }
    pub fn bounds(&self) -> Aabb {
        self.bounds_at(self.position)
    }
    pub fn bounds_at(&self, position: Vec3) -> Aabb {
        let w = f64::from(self.width()) / 2.0;
        Aabb::new(
            Vec3::new(position.x - w, position.y, position.z - w),
            Vec3::new(
                position.x + w,
                position.y + f64::from(self.height()),
                position.z + w,
            ),
        )
    }
    pub fn record_damage(&mut self, source: Damage, damage: f32) {
        let entry = combat::Entry {
            source,
            damage,
            in_water: self.fluids.in_water,
            fall_distance: self.fall_distance as f32,
        };
        self.combat.record(entry, self.age);
    }
    pub fn alive(&self) -> bool {
        self.health > 0.0
    }
    pub fn hostile(&self) -> bool {
        self.kind.category == "monster"
    }
    pub fn species(&self) -> Species {
        self.kind.species
    }
    pub fn main_arm_left(&self) -> bool {
        self.extra.main_hand_left || self.extra.left_handed
    }
    pub fn hurt(
        &mut self,
        amount: f32,
        from: Option<[f64; 3]>,
        _random: &mut LegacyRandom,
    ) -> bool {
        let source = match from {
            Some(f) => Damage::player(f),
            None => Damage::of(DamageKind::Generic),
        };
        self.damage(amount, source)
    }
    pub fn worn_armor(&self) -> (f32, f32) {
        if equine::is_equine(self.species()) {
            return self
                .extra
                .head
                .as_deref()
                .map_or((0.0, 0.0), spawning::horse_armor_stats);
        }
        [
            &self.extra.head,
            &self.extra.chest,
            &self.extra.legs,
            &self.extra.feet,
        ]
        .into_iter()
        .flatten()
        .map(|i| spawning::armor_stats(i))
        .fold((0.0, 0.0), |(a, t), (x, y)| (a + x, t + y))
    }
    pub fn damage(&mut self, amount: f32, source: Damage) -> bool {
        self.damage_with(amount, source, None)
    }
    pub fn damage_with(
        &mut self,
        amount: f32,
        source: Damage,
        armor_effectiveness: Option<&mut dyn FnMut(f32) -> f32>,
    ) -> bool {
        axolotl::before_damage(self, amount, &source);
        raider::note_hurt(self);
        if !self.alive()
            || self.removed
            || (self.invulnerable_flag && !source.kind.bypasses_invulnerability())
        {
            return false;
        }
        let reflected = &*self.kind.id == "minecraft:ghast"
            && source.kind == DamageKind::Fireball
            && source.attacker == Some(Who::Player);
        if source.is_fire() && (self.kind.flags.fire_immune) && !reflected {
            return false;
        }
        if !ai::accepts_damage(self, &source, amount) {
            return false;
        }
        let crackiness = (self.species() == Species::IronGolem).then(|| golem::crackiness(self));
        let mut amount = if reflected { 1000.0 } else { amount };
        if self.species() == Species::Armadillo {
            amount = armadillo::reduce_damage(self, amount);
        }
        if matches!(
            source.kind,
            DamageKind::FallingBlock | DamageKind::FallingAnvil | DamageKind::FallingStalactite
        ) && !species::body_slot(self.species())
            && self.extra.head.is_some()
        {
            amount *= 0.75;
        }
        if source.kind == DamageKind::Freeze
            && matches!(
                self.species(),
                Species::Strider | Species::Blaze | Species::MagmaCube
            )
        {
            amount *= 5.0;
        }
        let (worn, worn_toughness) = self.worn_armor();
        let covered = if self.species() == Species::Shulker && self.extra.counter == 0 {
            20.0
        } else {
            0.0
        };
        if !source.bypasses_armor() && self.kind.armor + worn + covered > 0.0 {
            let armor = self.kind.armor + worn + covered;
            let toughness = 2.0 + (self.kind.armor_toughness + worn_toughness) / 4.0;
            let effective = (armor - amount / toughness).clamp(armor * 0.2, 20.0);
            let mut fraction = effective / 25.0;
            if let Some(modify) = armor_effectiveness {
                fraction = modify(fraction).clamp(0.0, 1.0);
            }
            amount *= 1.0 - fraction;
        }
        let epf = gear::protection(self, source.kind);
        if epf > 0.0 {
            amount *= 1.0 - epf / 25.0;
        }
        if self.species() == Species::Witch {
            if source.attacker == Some(Who::Mob(self.id)) {
                amount = 0.0;
            } else if matches!(
                source.kind,
                DamageKind::Magic
                    | DamageKind::IndirectMagic
                    | DamageKind::SonicBoom
                    | DamageKind::Thorns
            ) {
                amount *= 0.15;
            }
        }
        let absorbed = self.wolf_armor_absorbs(&source);
        let mut knock = true;
        let dealt = if self.invulnerable as f32 > 10.0 {
            if amount <= self.last_hurt {
                return false;
            }
            let dealt = amount - self.last_hurt;
            if absorbed {
                self.hurt_wolf_armor(dealt);
            } else {
                self.health -= dealt;
            }
            self.last_hurt = amount;
            knock = false;
            dealt
        } else {
            self.last_hurt = amount;
            self.invulnerable = 20;
            if absorbed {
                self.hurt_wolf_armor(amount);
            } else {
                self.health -= amount;
            }
            self.hurt_time = 10;
            amount
        };
        if !absorbed && dealt != 0.0 {
            self.record_damage(source, dealt);
        }
        if drops::is_animal(self.species()) {
            self.extra.in_love = 0;
        }
        self.last_damage = Some((source, i64::from(self.age)));
        if let Some(who) = source.attacker {
            self.ai.last_hurt_by = Some(who);
            self.ai.last_hurt_by_time = self.age as i32;
            if who == Who::Player {
                self.ai.last_hurt_by_player = 100;
            }
        }
        if knock && !source.no_knockback() {
            let (dx, dz) = source.from.map_or((0.0, 0.0), |f| {
                (f[0] - self.position.x, f[2] - self.position.z)
            });
            self.knockback(f64::from(0.4f32), dx, dz);
        }
        ai::after_hurt(self, &source);
        if self.health <= 0.0 {
            self.health = 0.0;
        }
        if knock {
            let dead = !self.alive();
            let sound = if absorbed {
                "item.wolf_armor.damage"
            } else {
                ai::voice_sounds(self)[if dead { 2 } else { 1 }]
            };
            if !dead {
                self.ambient_timer = -self.ambient_interval();
            }
            if !sound.is_empty() {
                let pitch = self.voice_pitch();
                if !self.silent {
                    let at = [self.position.x, self.position.y, self.position.z];
                    self.outbox
                        .push(MobEvent::Sound(sound, at, ai::sound_volume(self), pitch));
                }
            }
        }
        if let Some(before) = crackiness
            && before != golem::crackiness(self)
            && !self.silent
        {
            let at = [self.position.x, self.position.y, self.position.z];
            self.outbox
                .push(MobEvent::Sound("entity.iron_golem.damage", at, 1.0, 1.0));
        }
        if self.has_effect("minecraft:infested") {
            self.outbox.push(MobEvent::Infested { mob: self.id });
        }
        if matches!(self.species(), Species::Squid | Species::GlowSquid)
            && self.ai.last_hurt_by.is_some()
        {
            aquatic::spawn_ink(self);
        }
        if equine::is_equine(self.species()) {
            equine::after_hurt_server(self);
        }
        if self.species() == Species::GlowSquid {
            aquatic::hurt_server(self);
        }
        true
    }
    pub fn potion_harm(&self, amount: f32) -> f32 {
        if self.species() == Species::Witch {
            amount * 0.15
        } else {
            amount
        }
    }
    fn wolf_armor_absorbs(&self, source: &Damage) -> bool {
        self.species() == Species::Wolf
            && self.extra.head.as_deref() == Some("minecraft:wolf_armor")
            && !matches!(
                source.kind,
                DamageKind::Drown
                    | DamageKind::Magic
                    | DamageKind::IndirectMagic
                    | DamageKind::InWall
                    | DamageKind::Thorns
                    | DamageKind::OutOfWorld
                    | DamageKind::Cramming
                    | DamageKind::Generic
                    | DamageKind::GenericKill
                    | DamageKind::OutsideBorder
                    | DamageKind::Starve
                    | DamageKind::Wither
            )
    }
    fn hurt_wolf_armor(&mut self, damage: f32) {
        let crack = |lost: i32| {
            let left = (WOLF_ARMOR_DURABILITY - lost) as f32 / WOLF_ARMOR_DURABILITY as f32;
            [0.95_f32, 0.69, 0.32].iter().filter(|t| left < **t).count()
        };
        let before = self.extra.body_damage;
        let after = before + damage.ceil() as i32;
        let at = [self.position.x, self.position.y, self.position.z];
        if after >= WOLF_ARMOR_DURABILITY {
            self.extra.head = None;
            self.extra.body_damage = 0;
            self.extra.body_extra = None;
            self.outbox
                .push(MobEvent::Sound("item.wolf_armor.break", at, 1.0, 1.0));
        } else {
            self.extra.body_damage = after;
            if crack(before) != crack(after) {
                self.outbox
                    .push(MobEvent::Sound("item.wolf_armor.crack", at, 1.0, 1.0));
            }
        }
    }
    pub fn knockback(&mut self, power: f64, mut xd: f64, mut zd: f64) {
        if creaking::is_creaking(self) && !creaking::can_move(self) {
            return;
        }
        let power = power * (1.0 - self.attrs.knockback_resistance);
        if power <= 0.0 {
            return;
        }
        self.needs_sync = true;
        while xd * xd + zd * zd < f64::from(1.0e-5_f32) {
            xd = (self.random.next_double() - self.random.next_double()) * 0.01;
            zd = (self.random.next_double() - self.random.next_double()) * 0.01;
        }
        let len = (xd * xd + zd * zd).sqrt();
        let (kx, kz) = (xd / len * power, zd / len * power);
        self.velocity = Vec3::new(
            self.velocity.x / 2.0 - kx,
            if self.on_ground {
                (self.velocity.y / 2.0 + power).min(0.4)
            } else {
                self.velocity.y
            },
            self.velocity.z / 2.0 - kz,
        );
    }
    pub fn tick(
        &mut self,
        w: &MobWorld,
        level_random: &mut LegacyRandom,
        events: &mut Vec<MobEvent>,
    ) -> bool {
        let alive = self.alive();
        self.combat.recheck(alive, self.age);
        ai::tick(self, w, level_random, events)
    }
    pub fn distance_to(&self, p: [f64; 3]) -> f64 {
        let d = [
            p[0] - self.position.x,
            p[1] - self.position.y,
            p[2] - self.position.z,
        ];
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
    }
    pub fn distance_sqr(&self, p: Vec3) -> f64 {
        (p.x - self.position.x).powi(2)
            + (p.y - self.position.y).powi(2)
            + (p.z - self.position.z).powi(2)
    }
    pub fn movement_speed(&self) -> f64 {
        let mut v = strider::movement_speed(
            self,
            witch::speed(self, self.attrs.speed) + enderman::attacking_speed(self),
        ) * self.speed_bonus();
        if self.extra.sprinting {
            v *= 1.0 + f64::from(0.3f32);
        }
        for (e, _, amp) in &self.extra.effects {
            match *e {
                "minecraft:speed" => v *= 1.0 + 0.2 * f64::from(amp + 1),
                "minecraft:slowness" => v *= (1.0 - 0.15 * f64::from(amp + 1)).max(0.0),
                _ => {}
            }
        }
        v
    }
    pub fn has_effect(&self, effect: &str) -> bool {
        self.extra.effects.iter().any(|e| e.0 == effect)
    }
    pub fn is_currently_glowing(&self) -> bool {
        self.glowing || self.has_effect("minecraft:glowing")
    }
    pub fn is_invisible(&self) -> bool {
        self.invisible || self.has_effect("minecraft:invisibility")
    }
    pub fn add_effect(&mut self, effect: &'static str, ticks: i32, amplifier: i32) {
        let effect = vise_client_core::potions::namespaced(effect);
        if let Some(e) = self.extra.effects.iter_mut().find(|e| e.0 == effect) {
            if amplifier > e.2 {
                e.1 = ticks;
                e.2 = amplifier;
            } else if amplifier == e.2 && e.1 != -1 && (ticks == -1 || e.1 < ticks) {
                e.1 = ticks;
            }
        } else {
            self.extra.effects.push((effect, ticks, amplifier));
        }
    }
    pub fn speed_bonus(&self) -> f64 {
        ai::speed_bonus(self)
    }
    pub fn center(&self) -> [f64; 3] {
        [
            self.position.x,
            self.position.y + f64::from(self.height()) / 2.0,
            self.position.z,
        ]
    }
    pub fn sound(&self, event: &'static str, events: &mut Vec<MobEvent>) {
        let at = [self.position.x, self.eye_y(), self.position.z];
        events.push(MobEvent::Sound(event, at, 1.0, 1.0));
    }
    pub fn ambient_interval(&self) -> i32 {
        let ridden = self.rider.is_some() || self.extra.rider.is_some();
        if self.species() == Species::HappyGhast && ridden {
            self.kind.ambient_interval * 6
        } else {
            self.kind.ambient_interval
        }
    }
    pub fn voice_pitch(&mut self) -> f32 {
        if self.species() == Species::HappyGhast {
            return 1.0;
        }
        let baby = if self.species() == Species::ZombieVillager {
            2.0
        } else {
            1.5
        };
        let pitch = (self.random.next_float() - self.random.next_float()) * 0.2
            + if self.is_baby() { baby } else { 1.0 };
        if self.species() == Species::Bat {
            pitch * 0.95
        } else {
            pitch
        }
    }
    pub fn voice(&mut self, event: &'static str, events: &mut Vec<MobEvent>) {
        let pitch = self.voice_pitch();
        let at = [self.position.x, self.position.y, self.position.z];
        events.push(MobEvent::Sound(event, at, ai::sound_volume(self), pitch));
    }
}
pub fn render(
    mob: &Mob,
    alpha: f32,
    camera: glam::DVec3,
    uv: [f32; 4],
    light: u32,
    out: &mut Vec<Vertex>,
) {
    render::render_single(mob, alpha, camera, uv, light, out);
}
pub struct EntityModelDocument(Value);

impl EntityModelDocument {
    pub fn load(root: &Path) -> crate::Result<Self> {
        Ok(Self(serde_json::from_slice(&fs::read(
            root.join("entity-models.json"),
        )?)?))
    }

    pub fn model(&self, key: &str) -> Option<Part> {
        self.0.get(key).map(|m| Part::parse("root", m))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.0
            .as_object()
            .into_iter()
            .flat_map(|all| all.keys().map(String::as_str))
    }
}

pub fn load_model(root: &Path, key: &str) -> Option<Part> {
    EntityModelDocument::load(root).ok()?.model(key)
}
pub fn render_model(
    part: &Part,
    base: glam::Mat4,
    pose_for: &dyn Fn(&Part) -> [f32; 9],
    uv: [f32; 4],
    light: u32,
    overlay: u32,
    out: &mut Vec<Vertex>,
) {
    visit(
        part,
        base,
        ModelNormals::new(base),
        pose_for,
        uv,
        light,
        overlay,
        out,
    );
    fn visit(
        part: &Part,
        parent: glam::Mat4,
        normals: ModelNormals,
        pose_for: &dyn Fn(&Part) -> [f32; 9],
        uv: [f32; 4],
        light: u32,
        overlay: u32,
        out: &mut Vec<Vertex>,
    ) {
        let p = pose_for(part);
        let m = parent
            * glam::Mat4::from_translation(glam::Vec3::new(p[0], p[1], p[2]) / 16.0)
            * glam::Mat4::from_quat(glam::Quat::from_euler(
                glam::EulerRot::ZYX,
                p[5],
                p[4],
                p[3],
            ))
            * glam::Mat4::from_scale(glam::Vec3::new(p[6], p[7], p[8]));
        let normals = normals.part(p);
        for poly in &part.polygons {
            let normal = poly.vertex_normal(normals);
            for v in poly.vertices {
                let pos = m.transform_point3(glam::Vec3::new(v[0], v[1], v[2]) / 16.0);
                out.push(Vertex {
                    normal,
                    pos: pos.to_array(),
                    uv: [
                        uv[0] + (uv[2] - uv[0]) * v[3],
                        uv[1] + (uv[3] - uv[1]) * v[4],
                    ],
                    layer: overlay,
                    color: 0xFFFF_FFFF,
                    light,
                    glint_uv: if overlay == render::ARMOR_GLINT {
                        [v[3], v[4]]
                    } else {
                        [0.0, 0.0]
                    },
                });
            }
        }
        for c in &part.children {
            visit(c, m, normals, pose_for, uv, light, overlay, out);
        }
    }
}

impl Mob {
    pub fn saddle_item(&self) -> Option<crate::items::ItemStack> {
        self.extra.saddled.then(|| {
            self.extra
                .saddle_stack
                .clone()
                .unwrap_or_else(|| crate::items::ItemStack::new("minecraft:saddle", 1))
        })
    }
    pub fn set_saddle(&mut self, saddle: Option<crate::items::ItemStack>) {
        self.extra.saddled = saddle.is_some();
        self.extra.saddle_stack = saddle
            .map(|s| crate::items::ItemStack { count: 1, ..s })
            .filter(|s| s.damage != 0 || s.extra.is_some() || &*s.item != "minecraft:saddle");
    }
}

#[cfg(test)]
mod lighting_tests {
    use super::*;
    use glam::{Mat4, Vec3};
    use serde_json::json;

    fn plane(normal: [f32; 3]) -> Part {
        Part::parse(
            "root",
            &json!({
                "polygons": [[[0,0,0,0,0], [0,0,0,1,0], [0,0,0,1,1], [0,0,0,0,1]]],
                "normals": [normal]
            }),
        )
    }

    #[test]
    fn exported_normals_survive_collapsed_faces_and_mirroring() {
        let part = plane([1.0, 0.0, 0.0]);
        assert_eq!(
            part.polygons[0].vertex_normal(ModelNormals::new(Mat4::IDENTITY)),
            [127, 0, 0, -127]
        );
        assert_eq!(
            part.polygons[0].vertex_normal(ModelNormals::new(Mat4::from_scale(Vec3::new(
                -1.0, -1.0, 1.0
            )))),
            [-127, 0, 0, -127]
        );
    }

    #[test]
    fn normal_matrix_includes_parent_scale_and_child_rotation() {
        let mut child = plane([1.0, 0.0, 0.0]);
        child.pose[5] = std::f32::consts::FRAC_PI_4;
        let parent = Part {
            name: "parent".into(),
            pose: [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 1.0, 0.5],
            polygons: Vec::new(),
            children: vec![child],
        };
        let mut animated = Vec::new();
        render::draw(
            &parent,
            Mat4::IDENTITY,
            &anim::Poses::new(&parent),
            [0.0, 0.0, 1.0, 1.0],
            0x80402010,
            240,
            1,
            &mut animated,
        );
        let mut direct = Vec::new();
        render_model(
            &parent,
            Mat4::IDENTITY,
            &|p| p.pose,
            [0.0, 0.0, 1.0, 1.0],
            240,
            1,
            &mut direct,
        );
        assert_eq!(animated.len(), 4);
        for (a, b) in animated.iter().zip(&direct) {
            assert_eq!(a.normal, [56, 113, 0, -127]);
            assert_eq!(a.normal, b.normal);
            assert_eq!(a.pos, b.pos);
            assert_eq!(a.uv, b.uv);
            assert_eq!((a.color, a.light, a.layer), (0x80402010, 240, 1));
        }
    }

    #[test]
    fn trusted_rotation_retains_native_quaternion_rounding() {
        let mut part = plane([1.0, 0.0, 0.0]);
        part.pose[3] = std::f32::consts::FRAC_PI_2;
        let mut vertices = Vec::new();
        render::draw(
            &part,
            Mat4::IDENTITY,
            &anim::Poses::new(&part),
            [0.0, 0.0, 1.0, 1.0],
            0xffffffff,
            240,
            0,
            &mut vertices,
        );
        assert_eq!(vertices[0].normal, [126, 0, 0, -127]);
    }

    #[test]
    fn legacy_models_infer_non_degenerate_face_normals() {
        let part = Part::parse(
            "root",
            &json!({"polygons": [
                [[0,0,0,0,0], [1,0,0,1,0], [1,1,0,1,1], [0,1,0,0,1]]
            ]}),
        );
        assert_eq!(part.polygons[0].normal, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn packed_normal_uses_native_snorm_bytes_without_changing_vertex_offsets() {
        assert_eq!(std::mem::offset_of!(Vertex, normal), 32);
        assert_eq!(std::mem::size_of::<Vertex>(), 44);
        let part = plane([0.0, 1.0, 0.0]);
        assert_eq!(
            part.polygons[0].vertex_normal(ModelNormals::new(Mat4::from_scale(Vec3::ZERO))),
            [0, 0, 0, -127]
        );
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ElytraAnimation {
    pub old: [f32; 3],
    pub angles: [f32; 3],
}
impl ElytraAnimation {
    pub const RESTING: [f32; 3] = [
        std::f32::consts::PI / 12.0,
        0.0,
        -std::f32::consts::PI / 12.0,
    ];
    pub fn tick(&mut self, flying: bool, crouching: bool, velocity: vise_core::geometry::Vec3) {
        use std::f32::consts::PI;
        self.old = self.angles;
        let target = if flying {
            let length =
                (velocity.x * velocity.x + velocity.y * velocity.y + velocity.z * velocity.z)
                    .sqrt();
            let ratio = if velocity.y < 0.0 && length > 0.0 {
                1.0 - vise_core::jmath::pow(-velocity.y / length, 1.5) as f32
            } else {
                1.0
            };
            [
                PI / 12.0 + (PI / 9.0 - PI / 12.0) * ratio,
                0.0,
                -PI / 12.0 + (-PI / 2.0 + PI / 12.0) * ratio,
            ]
        } else if crouching {
            [PI * 2.0 / 9.0, 0.08726646, -PI / 4.0]
        } else {
            Self::RESTING
        };
        for (angle, target) in self.angles.iter_mut().zip(target) {
            *angle += (target - *angle) * 0.3;
        }
    }
    pub fn angles(&self, partial: f32) -> [f32; 3] {
        std::array::from_fn(|i| self.old[i] + (self.angles[i] - self.old[i]) * partial)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SwimAnimation {
    pub old: f32,
    pub amount: f32,
    pub visually: bool,
    pub in_water: bool,
}
impl SwimAnimation {
    pub fn tick(&mut self, visually: bool, in_water: bool) {
        self.old = self.amount;
        self.amount = if visually {
            (self.amount + 0.09).min(1.0)
        } else {
            (self.amount - 0.09).max(0.0)
        };
        self.visually = visually;
        self.in_water = in_water;
    }
    pub fn amount(&self, partial: f32) -> f32 {
        self.old + partial * (self.amount - self.old)
    }
}

impl Mob {
    pub fn tick_avatar_pose(&mut self, in_water: bool) {
        let (p, v) = (self.previous, self.velocity);
        let on_ground_alive = self.on_ground && self.alive();
        let e = &mut self.extra;
        e.fall_fly_ticks = if e.fall_flying {
            e.fall_fly_ticks + 1
        } else {
            0
        };
        let visually = e.synced_pose == 3 || (!e.fall_flying && e.synced_pose == 1);
        e.elytra.tick(e.fall_flying, e.crouching, self.velocity);
        e.swim.tick(visually, in_water);
        e.avatar.tick([p.x, p.y, p.z]);
        if e.passenger {
            e.avatar.reset_bob();
        } else {
            let swimming = e.synced_pose == 3;
            e.avatar
                .update_bob(on_ground_alive && !swimming, [v.x, v.y, v.z]);
        }
    }
}

#[cfg(test)]
mod elytra_tests {
    use super::*;
    #[test]
    fn wings_ease_between_ground_flight_and_crouch() {
        let mut wings = ElytraAnimation::default();
        wings.tick(false, false, Vec3::ZERO);
        assert!((wings.angles[0] - std::f32::consts::PI / 12.0 * 0.3).abs() < 0.000001);
        assert_eq!(wings.angles(0.0), wings.old);
        assert_eq!(wings.angles(1.0), wings.angles);
        for _ in 0..40 {
            wings.tick(true, false, Vec3::new(1.0, 0.0, 0.0));
        }
        assert!((wings.angles[2] + std::f32::consts::FRAC_PI_2).abs() < 0.00001);
        for _ in 0..40 {
            wings.tick(false, true, Vec3::ZERO);
        }
        assert!((wings.angles[1] - 0.08726646).abs() < 0.00001);
    }
    #[test]
    fn resting_wings_are_the_standing_target() {
        let mut wings = ElytraAnimation::default();
        for _ in 0..80 {
            wings.tick(false, false, Vec3::ZERO);
        }
        for (angle, resting) in wings.angles.iter().zip(ElytraAnimation::RESTING) {
            assert!((angle - resting).abs() < 0.00001);
        }
    }
}
#[cfg(test)]
mod teams_tests {
    use super::*;
    use crate::save::uuid_text;
    use crate::scoreboard::{Scoreboard, Team};

    fn board(mobs: u32) -> (Scoreboard, HashMap<u32, [i32; 4]>) {
        let mut board = Scoreboard::default();
        board.add_team("red");
        board.add_to_team("Steve", "red");
        let uuids: HashMap<u32, [i32; 4]> = (0..mobs)
            .map(|id| (id, [id as i32, 7, -(id as i32), 11]))
            .collect();
        for (id, uuid) in &uuids {
            if id % 2 == 0 {
                board.add_to_team(&uuid_text(*uuid), "red");
            }
        }
        (board, uuids)
    }
    fn owned(board: &Scoreboard, uuids: &HashMap<u32, [i32; 4]>) -> HashMap<u32, Team> {
        uuids
            .iter()
            .filter_map(|(id, u)| Some((*id, board.players_team(&uuid_text(*u))?.clone())))
            .collect()
    }
    fn borrowed<'a>(board: &'a Scoreboard, uuids: &HashMap<u32, [i32; 4]>) -> Teams<'a> {
        Teams {
            player: board.players_team("Steve"),
            mobs: uuids
                .iter()
                .filter_map(|(id, u)| Some((*id, board.players_team(&uuid_text(*u))?)))
                .collect(),
        }
    }

    #[test]
    fn borrowed_teams_answer_as_the_clones() {
        let (board, uuids) = board(40);
        let teams = borrowed(&board, &uuids);
        let old = owned(&board, &uuids);
        for id in 0..41 {
            assert_eq!(teams.of(Who::Mob(id)), old.get(&id), "mob {id}");
        }
        assert_eq!(teams.of(Who::Player).map(|t| t.name.as_str()), Some("red"));
    }

    #[test]
    #[ignore]
    fn teams_cost() {
        for mobs in [100u32, 250, 500] {
            let (board, uuids) = board(mobs);
            let t = std::time::Instant::now();
            for _ in 0..mobs {
                std::hint::black_box(owned(&board, &uuids));
            }
            let before = t.elapsed();
            let t = std::time::Instant::now();
            let reps = 20;
            for _ in 0..reps {
                std::hint::black_box(borrowed(&board, &uuids));
            }
            let after = t.elapsed() / reps;
            println!(
                "{mobs} mobs, half on a team: per tick {:.3} ms -> {:.3} ms",
                before.as_secs_f64() * 1e3,
                after.as_secs_f64() * 1e3
            );
        }
    }
}
#[cfg(test)]
pub(crate) mod sensor_scan_tests {
    use super::*;
    use vise_core::geometry::Vec3;
    use serde_json::json;

    pub(crate) fn kind() -> Arc<MobType> {
        Arc::new(MobType {
            id: "minecraft:cow".into(),
            species: Species::Cow,
            width: 0.9,
            height: 1.4,
            eye_height: 1.3,
            category: "creature".into(),
            max_health: 10.0,
            speed: 0.2,
            flying_speed: 0.4,
            attack_damage: 0.0,
            attack_knockback: 0.0,
            follow_range: 16.0,
            step_height: 0.6,
            armor: 0.0,
            armor_toughness: 0.0,
            knockback_resistance: 0.0,
            jump_strength: 0.42,
            attributes: Arc::default(),
            gravity: 0.08,
            safe_fall_distance: 3.0,
            fall_damage_multiplier: 1.0,
            water_efficiency: 0.0,
            water_slowdown: 0.8,
            air_speed: 0.02,
            baby_scale: 0.5,
            head_rot_speed: 10.0,
            max_head_x_rot: 40.0,
            max_head_y_rot: 75.0,
            ambient_interval: 120,
            flags: Flags {
                animal: true,
                ..Default::default()
            },
            model: Arc::new(Part::parse("root", &json!({}))),
            layers: HashMap::new(),
            texture: String::new(),
            brain: Brain::Passive,
            anim: Anim::Rabbit,
            sounds: [""; 3],
            step_sound: None,
            food: Arc::default(),
            tempt: Arc::default(),
        })
    }
    fn herd(n: u32, side: f64) -> (Vec<Seen>, seen_grid::SeenGrid) {
        let kind = kind();
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = |range: f64| {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
            (seed >> 11) as f64 / (1u64 << 53) as f64 * range
        };
        let mut pen = Vec3::ZERO;
        let seen: Vec<Seen> = (0..n)
            .map(|id| {
                if id % 20 == 0 {
                    pen = Vec3::new(next(side), 64.0, next(side));
                }
                let at = Vec3::new(pen.x + next(8.0), 64.0, pen.z + next(8.0));
                Mob::new(id, kind.clone(), at, 0.0).seen(None)
            })
            .collect();
        let grid = seen_grid::SeenGrid::new(seen.iter().map(|s| s.bounds));
        (seen, grid)
    }
    fn scan(seen: &[Seen], me: &Seen, area: Aabb) -> Vec<u32> {
        seen.iter()
            .filter(|o| o.id != me.id && o.alive && o.bounds.intersects(area))
            .map(|o| o.id)
            .collect()
    }
    fn indexed(seen: &[Seen], grid: &seen_grid::SeenGrid, me: &Seen, area: Aabb) -> Vec<u32> {
        let mut near = Vec::new();
        assert!(grid.candidates(area, &mut near));
        near.iter()
            .map(|&i| &seen[i as usize])
            .filter(|o| o.id != me.id && o.alive && o.bounds.intersects(area))
            .map(|o| o.id)
            .collect()
    }

    #[test]
    fn the_grid_finds_what_the_scan_finds() {
        let (seen, grid) = herd(600, 160.0);
        for me in &seen {
            let area = me.bounds.inflate(Vec3::new(16.0, 16.0, 16.0));
            assert_eq!(indexed(&seen, &grid, me, area), scan(&seen, me, area));
        }
    }

    #[test]
    #[ignore]
    fn lookup_cost() {
        for n in [500u32, 1000, 2000] {
            let (seen, _) = herd(n, 256.0);
            let index: crate::fast_hash::FxHashMap<u32, u32> = seen
                .iter()
                .enumerate()
                .map(|(i, s)| (s.id, i as u32))
                .collect();
            let wanted: Vec<u32> = (0..n).map(|i| (i * 7919) % n).collect();
            let t = std::time::Instant::now();
            let a: usize = wanted
                .iter()
                .filter_map(|&id| seen.iter().find(|s| s.id == id))
                .map(|s| s.id as usize)
                .sum();
            let before = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            let b: usize = wanted
                .iter()
                .filter_map(|&id| index.get(&id).map(|&i| &seen[i as usize]))
                .map(|s| s.id as usize)
                .sum();
            let after = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(a, b);
            println!(
                "{n} lookups over {n} mobs ({} B each): {before:.3} ms -> {after:.3} ms",
                std::mem::size_of::<Seen>()
            );
        }
    }

    #[test]
    #[ignore]
    fn spawn_box_cost() {
        let kind = kind();
        for n in [500u32, 1000, 2000] {
            let (seen, _) = herd(n, 384.0);
            let mobs: Vec<Mob> = seen
                .iter()
                .map(|s| Mob::new(s.id, kind.clone(), s.position, 0.0))
                .collect();
            let mut seed = 7u64;
            let mut next = |range: f64| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                (seed >> 11) as f64 / (1u64 << 53) as f64 * range
            };
            let queries: Vec<Aabb> = (0..100)
                .map(|_| {
                    let (x, z) = (next(384.0), next(384.0));
                    Aabb::new(Vec3::new(x, 64.0, z), Vec3::new(x + 0.9, 65.4, z + 0.9))
                })
                .collect();
            let t = std::time::Instant::now();
            let a: Vec<bool> = queries
                .iter()
                .map(|&q| {
                    mobs.iter()
                        .any(|m| m.alive() && !m.removed && m.bounds().intersects(q))
                })
                .collect();
            let before = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            let boxes: Vec<Aabb> = mobs.iter().map(|m| m.bounds()).collect();
            let grid = seen_grid::SeenGrid::new(boxes.iter().copied());
            let build = t.elapsed().as_secs_f64() * 1e3;
            let mut near = Vec::new();
            let b: Vec<bool> = queries
                .iter()
                .map(|&q| {
                    assert!(grid.candidates(q, &mut near));
                    near.iter().any(|&i| boxes[i as usize].intersects(q))
                })
                .collect();
            let after = t.elapsed().as_secs_f64() * 1e3;
            assert_eq!(a, b);
            println!(
                "{n} mobs, 100 spawn box tests: {before:.3} ms -> {after:.3} ms (grid build {build:.3} ms included)"
            );
        }
    }

    #[test]
    #[ignore]
    fn sensor_cost() {
        for (n, side) in [(500u32, 256.0), (1000, 384.0), (2000, 512.0)] {
            let (seen, grid) = herd(n, side);
            let time = |f: &dyn Fn(&Seen, Aabb) -> usize| {
                let t = std::time::Instant::now();
                let mut found = 0;
                for me in &seen {
                    found += f(me, me.bounds.inflate(Vec3::new(16.0, 16.0, 16.0)));
                }
                (t.elapsed().as_secs_f64() * 1e3 / 20.0, found)
            };
            let (before, a) = time(&|me, area| scan(&seen, me, area).len());
            let (after, b) = time(&|me, area| indexed(&seen, &grid, me, area).len());
            assert_eq!(a, b);
            println!("{n} mobs: per tick {before:.3} ms -> {after:.3} ms");
        }
    }
}
#[cfg(test)]
mod push_tests {
    use super::sensor_scan_tests::kind;
    use super::*;
    use vise_core::geometry::Vec3;

    fn take_old(events: &mut Vec<MobEvent>, from: usize, pending: &mut Vec<(u32, Vec3)>) {
        let mut i = from;
        while i < events.len() {
            if let MobEvent::Push {
                target: Who::Mob(id),
                velocity,
            } = events[i]
            {
                pending.push((id, Vec3::new(velocity[0], velocity[1], velocity[2])));
                events.remove(i);
            } else {
                i += 1;
            }
        }
    }
    fn apply_old(velocity: &mut Vec3, id: u32, pending: &mut Vec<(u32, Vec3)>) {
        pending.retain(|(to, v)| {
            if *to != id {
                return true;
            }
            *velocity = Vec3::new(velocity.x + v.x, velocity.y + v.y, velocity.z + v.z);
            false
        });
    }
    fn mobs(n: u32) -> Vec<Mob> {
        let kind = kind();
        (0..n)
            .map(|id| Mob::new(id, kind.clone(), Vec3::ZERO, 0.0))
            .collect()
    }
    fn crowd_tick(
        n: u32,
        tick: u32,
        mut on_push: impl FnMut(&mut Vec<MobEvent>, usize),
        mut on_apply: impl FnMut(u32),
    ) -> Vec<MobEvent> {
        let mut events = Vec::new();
        for i in 0..n {
            on_apply(i);
            let from = events.len();
            for j in 0..n {
                if j != i && !(i * 31 + j * 17 + tick).is_multiple_of(5) {
                    let f = f64::from(i * 7 + j + tick) * 0.001;
                    events.push(MobEvent::Push {
                        target: Who::Mob(j),
                        velocity: [f, -f * 0.5, f * 0.25],
                    });
                }
                if j.is_multiple_of(9) {
                    events.push(MobEvent::Hiss { mob: j });
                }
            }
            events.push(MobEvent::Push {
                target: Who::Player,
                velocity: [0.1, 0.0, 0.1],
            });
            on_push(&mut events, from);
        }
        for i in 0..n {
            on_apply(i);
        }
        events
    }

    #[test]
    fn pushes_land_as_with_the_list() {
        for n in [1u32, 3, 8, 20, 60] {
            let mut new_mobs = mobs(n);
            let mut old_v = vec![Vec3::ZERO; n as usize];
            let mut pending = PendingPushes::default();
            let mut old_pending = Vec::new();
            for tick in 0..4 {
                let new_events = {
                    let pending = std::cell::RefCell::new(&mut pending);
                    let mobs = std::cell::RefCell::new(&mut new_mobs);
                    crowd_tick(
                        n,
                        tick,
                        |e, from| take_mob_pushes(e, from, &mut pending.borrow_mut()),
                        |i| {
                            apply_mob_pushes(
                                &mut mobs.borrow_mut()[i as usize],
                                &mut pending.borrow_mut(),
                            )
                        },
                    )
                };
                let old_events = {
                    let pending = std::cell::RefCell::new(&mut old_pending);
                    let v = std::cell::RefCell::new(&mut old_v);
                    crowd_tick(
                        n,
                        tick,
                        |e, from| take_old(e, from, &mut pending.borrow_mut()),
                        |i| {
                            apply_old(
                                &mut v.borrow_mut()[i as usize],
                                i,
                                &mut pending.borrow_mut(),
                            )
                        },
                    )
                };
                assert_eq!(format!("{new_events:?}"), format!("{old_events:?}"));
                for i in 0..n as usize {
                    let (a, b) = (new_mobs[i].velocity, old_v[i]);
                    assert_eq!(
                        (a.x.to_bits(), a.y.to_bits(), a.z.to_bits()),
                        (b.x.to_bits(), b.y.to_bits(), b.z.to_bits()),
                        "{n} mobs, tick {tick}, mob {i}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn push_cost() {
        for n in [10u32, 30, 100, 200] {
            let mut ms = mobs(n);
            let mut v = vec![Vec3::ZERO; n as usize];
            let t = std::time::Instant::now();
            {
                let pending = std::cell::RefCell::new(Vec::new());
                let v = std::cell::RefCell::new(&mut v);
                crowd_tick(
                    n,
                    0,
                    |e, from| take_old(e, from, &mut pending.borrow_mut()),
                    |i| {
                        apply_old(
                            &mut v.borrow_mut()[i as usize],
                            i,
                            &mut pending.borrow_mut(),
                        )
                    },
                );
            }
            let before = t.elapsed().as_secs_f64() * 1e3;
            let t = std::time::Instant::now();
            {
                let pending = std::cell::RefCell::new(PendingPushes::default());
                let ms = std::cell::RefCell::new(&mut ms);
                crowd_tick(
                    n,
                    0,
                    |e, from| take_mob_pushes(e, from, &mut pending.borrow_mut()),
                    |i| {
                        apply_mob_pushes(
                            &mut ms.borrow_mut()[i as usize],
                            &mut pending.borrow_mut(),
                        )
                    },
                );
            }
            let after = t.elapsed().as_secs_f64() * 1e3;
            println!("{n} mobs pushing each other: {before:.3} ms -> {after:.3} ms");
        }
    }
}

#[cfg(test)]
mod held_stack_tests {
    use super::*;

    #[test]
    fn replacing_even_the_same_item_clears_its_old_components() {
        let mut extra = Extra::default();
        let mut named = crate::items::ItemStack::new("minecraft:diamond_sword", 1);
        named.data_mut().custom_name = Some(vise_world::nbt::Tag::String("Old Sword".into()));
        extra.set_main_hand_stack(Some(named.clone()));
        assert_eq!(
            extra.main_hand_stack().unwrap().custom_name().as_deref(),
            Some("Old Sword")
        );
        extra.set_main_hand(Some(named.item.clone()));
        assert!(extra.main_hand_stack().is_none());
        extra.set_main_hand_stack(Some(named));
        extra.take_main_hand();
        assert!(extra.main_hand.is_none());
        assert!(extra.main_hand_stack().is_none());
    }
}
