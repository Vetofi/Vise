use super::{Mob, MobEvent, MobWorld, Species, goals::Cx, villager};
use vise_core::random::RandomSource;
use vise_world::nbt::{Compound, Tag};

pub const WAIT_MIN: i32 = 3600;
pub const WAIT_SPREAD: i32 = 2401;
const MAX_SPECIAL_BLOCKS: i32 = 14;
const RADIUS: i32 = 4;
const SPEEDS_UP_CURING: &str = "minecraft:speeds_up_zombie_villager_curing";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cure {
    pub time: i32,
    pub starter: Option<[i32; 4]>,
}

pub fn cure(m: &Mob) -> Option<&Cure> {
    villager::ext(m).and_then(|e| e.cure.as_ref())
}
pub fn set_converting(m: &mut Mob, on: bool) {
    villager::ext_mut(m).cure = on.then_some(Cure {
        time: 0,
        starter: None,
    });
}
pub fn is_converting(m: &Mob) -> bool {
    m.species() == Species::ZombieVillager && cure(m).is_some()
}
pub fn keeps_alive(m: &Mob) -> bool {
    m.species() == Species::ZombieVillager
        && (is_converting(m) || m.extra.trade.as_ref().is_some_and(|t| t.xp != 0))
}

pub fn start_converting(
    m: &mut Mob,
    starter: Option<[i32; 4]>,
    time: i32,
    events: &mut Vec<MobEvent>,
) {
    villager::ext_mut(m).cure = Some(Cure { time, starter });
    m.extra.effects.retain(|e| e.0 != "minecraft:weakness");
    m.add_effect("minecraft:strength", time, 0);
    let pitch = m.random.next_float() * 0.7 + 0.3;
    let volume = 1.0 + m.random.next_float();
    events.push(MobEvent::Sound(
        "entity.zombie_villager.cure",
        [m.position.x, m.eye_y(), m.position.z],
        volume,
        pitch,
    ));
}

fn progress(m: &mut Mob, w: &MobWorld) -> i32 {
    let mut amount = 1;
    if m.random.next_float() < 0.01 {
        let mut special = 0;
        let (bx, by, bz) = (
            m.position.x as i32,
            m.position.y as i32,
            m.position.z as i32,
        );
        'scan: for x in bx - RADIUS..bx + RADIUS {
            for y in by - RADIUS..by + RADIUS {
                for z in bz - RADIUS..bz + RADIUS {
                    if special >= MAX_SPECIAL_BLOCKS {
                        break 'scan;
                    }
                    if w.has_tag(SPEEDS_UP_CURING, w.block_name(x, y, z)) {
                        if m.random.next_float() < 0.3 {
                            amount += 1;
                        }
                        special += 1;
                    }
                }
            }
        }
    }
    amount
}

pub fn step(m: &mut Mob, cx: &mut Cx) {
    if !m.alive() || !is_converting(m) {
        return;
    }
    let amount = progress(m, cx.w);
    let Some(c) = villager::ext_mut(m).cure.as_mut() else {
        return;
    };
    c.time -= amount;
    if c.time <= 0 {
        cx.events.push(MobEvent::CureZombieVillager { mob: m.id });
    }
}

fn put_cure(map: &mut Compound, cure: Option<Cure>) {
    map.insert(
        "ConversionTime".into(),
        Tag::Int(cure.map_or(-1, |c| c.time)),
    );
    if let Some(starter) = cure.and_then(|c| c.starter) {
        map.insert("ConversionPlayer".into(), Tag::IntArray(starter.to_vec()));
    }
}
fn parse_cure(tag: &Tag) -> Option<Cure> {
    let time = match tag.get("ConversionTime") {
        Some(Tag::Int(v)) => *v,
        _ => -1,
    };
    (time != -1).then(|| Cure {
        time,
        starter: crate::save::uuid_from_tag(tag.get("ConversionPlayer")),
    })
}
pub fn write_nbt(m: &Mob, map: &mut Compound) {
    put_cure(map, cure(m).copied());
    if let Some(e) = villager::ext(m)
        && !e.gossips.is_empty()
    {
        map.insert("Gossips".into(), e.gossips.to_nbt());
    }
}
pub fn read_nbt(m: &mut Mob, tag: &Tag) {
    let cure = parse_cure(tag);
    let e = villager::ext_mut(m);
    e.cure = cure;
    e.gossips = tag
        .get("Gossips")
        .map(crate::gossip::Gossips::from_nbt)
        .unwrap_or_default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cure_round_trips_through_nbt() {
        let cure = Cure {
            time: 4200,
            starter: Some([1, 2, 3, 4]),
        };
        let mut map = Compound::new();
        put_cure(&mut map, Some(cure));
        assert_eq!(parse_cure(&Tag::Compound(map)), Some(cure));
    }
    #[test]
    fn a_no_ai_zombie_villager_still_cures() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !root.join("entity-types.json").exists()
            || !root.join("movement-properties.nbt").exists()
        {
            return;
        }
        let assets = crate::assets::Assets::load(&root).unwrap();
        let registry = assets.registry;
        let facts = crate::blocks::BlockFacts::load(&root, registry).unwrap();
        let motion = crate::motion::MotionResources::load(&root, &assets).unwrap();
        let data = super::super::MobData::load(&root, &facts);
        let types = super::super::MobTypes::load(&root).unwrap();
        let biomes = crate::biome::Biomes::load(&root).unwrap();
        let items = crate::items::Items::load(
            &root,
            registry,
            &facts,
            &vise_world::loot::PackOverlay::NONE,
        )
        .unwrap();
        let collision = crate::collisions::load(&root, registry).unwrap();
        let air = registry.default_state("minecraft:air").unwrap();
        let world = crate::world::World::new(air, 0);
        let lid_openers = std::collections::HashMap::new();
        let w = MobWorld {
            teams: Default::default(),
            world: &world,
            registry,
            facts: &facts,
            fluid_tables: &motion.fluids,
            movement: &motion.blocks,
            collision: &collision,
            data: &data,
            player: None,
            nearest_player: None,
            others: &[],
            others_grid: None,
            others_index: None,
            daylight: true,
            monsters_burn: false,
            game_time: 1000,
            day_time: 18000,
            raining: false,
            thundering: false,
            nether: false,
            block_tags: &items.block_tags,
            biomes: &biomes,
            mob_griefing: true,
            mob_drops: true,
            max_entity_cramming: 24,
            difficulty: super::super::spawning::LocalDifficulty::new(2, 18000, 0, 0.0),
            end: false,
            world_seed: 0,
            sea_level: 63,
            items: Some(&items),
            lid_openers: &lid_openers,
        };
        for no_ai in [false, true] {
            let kind = std::sync::Arc::new(types.types["minecraft:zombie_villager"].clone());
            let mut m = Mob::new(
                1,
                kind,
                vise_core::geometry::Vec3::new(0.5, 0.0, 0.5),
                0.0,
            );
            m.persistent = true;
            m.no_ai = no_ai;
            start_converting(&mut m, None, 3, &mut Vec::new());
            let mut level_random = vise_core::random::LegacyRandom::new(0);
            let mut ticks = 0;
            while ticks < 10 {
                let mut events = Vec::new();
                m.tick(&w, &mut level_random, &mut events);
                ticks += 1;
                if events
                    .iter()
                    .any(|e| matches!(e, MobEvent::CureZombieVillager { mob: 1 }))
                {
                    break;
                }
            }
            assert!(
                ticks <= 3,
                "no_ai={no_ai}: the cure must finish within its 3 ticks, took {ticks}"
            );
        }
    }
    #[test]
    fn no_cure_saves_minus_one() {
        let mut map = Compound::new();
        put_cure(&mut map, None);
        assert_eq!(map.get(&"ConversionTime".into()), Some(&Tag::Int(-1)));
        assert_eq!(parse_cure(&Tag::Compound(map)), None);
    }
}
