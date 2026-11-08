use vise_core::{position::BlockPos, random::RandomSource};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreRemove {
    Nothing,
    Contents { slots: usize, furnace: bool },
    Campfire,
    ShulkerBox,
    Jukebox,
    Lectern,
    SculkShrieker,
    CreakingHeart,
    Beacon,
}

pub fn pre_remove(name: &str) -> PreRemove {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    let contents = |slots| PreRemove::Contents {
        slots,
        furnace: false,
    };
    match n {
        "chest" | "trapped_chest" | "barrel" => contents(27),
        _ if n.ends_with("copper_chest") => contents(27),
        _ if n.ends_with("shulker_box") => PreRemove::ShulkerBox,
        "hopper" => contents(5),
        "dispenser" | "dropper" | "crafter" => contents(9),
        "brewing_stand" => contents(5),
        "chiseled_bookshelf" => contents(6),
        _ if n.ends_with("_shelf") => contents(3),
        "decorated_pot" => contents(1),
        "furnace" | "blast_furnace" | "smoker" => PreRemove::Contents {
            slots: 3,
            furnace: true,
        },
        "campfire" | "soul_campfire" => PreRemove::Campfire,
        "jukebox" => PreRemove::Jukebox,
        "lectern" => PreRemove::Lectern,
        "sculk_shrieker" => PreRemove::SculkShrieker,
        "creaking_heart" => PreRemove::CreakingHeart,
        "beacon" => PreRemove::Beacon,
        _ => PreRemove::Nothing,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spilled {
    pub pos: [f64; 3],
    pub count: u32,
    pub motion: [f64; 3],
}

pub fn drop_item_stack<R: RandomSource>(random: &mut R, pos: BlockPos, count: u32) -> Vec<Spilled> {
    let size = f64::from(0.25f32);
    let center_range = 1.0 - size;
    let half = size / 2.0;
    let x = f64::from(pos.x) + random.next_double() * center_range + half;
    let y = f64::from(pos.y) + random.next_double() * center_range;
    let z = f64::from(pos.z) + random.next_double() * center_range + half;
    let mut left = count;
    let mut out = Vec::new();
    while left > 0 {
        let piece = (random.next_int_bound(21).unwrap_or(0) + 10) as u32;
        let piece = piece.min(left);
        left -= piece;
        let deviation = 0.114_850_001_711_398_36;
        let motion = [
            random.triangle(0.0, deviation),
            random.triangle(0.2, deviation),
            random.triangle(0.0, deviation),
        ];
        out.push(Spilled {
            pos: [x, y, z],
            count: piece,
            motion,
        });
    }
    out
}

pub fn jukebox_pop_position<R: RandomSource>(random: &mut R, pos: BlockPos) -> [f64; 3] {
    let dx = f64::from((random.next_float() - 0.5) * 0.7);
    let dz = f64::from((random.next_float() - 0.5) * 0.7);
    [
        f64::from(pos.x) + 0.5 + dx,
        f64::from(pos.y) + 1.01,
        f64::from(pos.z) + 0.5 + dz,
    ]
}

pub fn lectern_book_position(pos: BlockPos, step: [i32; 2]) -> [f64; 3] {
    let xo = 0.25f32 * step[0] as f32;
    let zo = 0.25f32 * step[1] as f32;
    [
        f64::from(pos.x) + 0.5 + f64::from(xo),
        f64::from(pos.y + 1),
        f64::from(pos.z) + 0.5 + f64::from(zo),
    ]
}

pub fn break_experience<R: RandomSource>(random: &mut R, name: &str) -> Option<u32> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    let mut uniform = |min: i32, max: i32| random.next_int_bound(max - min + 1).unwrap_or(0) + min;
    let xp = match n {
        "coal_ore" | "deepslate_coal_ore" => uniform(0, 2),
        "nether_gold_ore" => uniform(0, 1),
        "lapis_ore" | "deepslate_lapis_ore" | "nether_quartz_ore" => uniform(2, 5),
        "diamond_ore" | "deepslate_diamond_ore" | "emerald_ore" | "deepslate_emerald_ore" => {
            uniform(3, 7)
        }
        "redstone_ore" | "deepslate_redstone_ore" => uniform(1, 5),
        "sculk" => 1,
        "sculk_catalyst" | "sculk_sensor" | "calibrated_sculk_sensor" | "sculk_shrieker" => 5,
        "spawner" => {
            let a = random.next_int_bound(15).unwrap_or(0);
            15 + a + random.next_int_bound(15).unwrap_or(0)
        }
        _ => return None,
    };
    (xp > 0).then_some(xp as u32)
}

pub fn orb_value(value: u32) -> u32 {
    [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3, 1]
        .into_iter()
        .find(|&v| value >= v)
        .unwrap_or(1)
}
