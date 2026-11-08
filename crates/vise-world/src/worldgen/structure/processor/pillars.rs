use super::{PlaceSettings, ProcessResult, ProcessorList, ProcessorRandom, process_list};
use crate::{
    block_properties::{BoundBlockProperty, BoundOcclusionFaces},
    fluid::{FluidKind, FluidStateTable},
    registry::BlockStateId,
    worldgen::{
        DecodeError,
        block_feature::FeatureBlockBindings,
        placement::{PlacementContext, PlacementQueryError},
        structure::{BlockInfo, StructureResources},
    },
};
use vise_core::geometry::Direction;
use serde_json::Value;
use std::{collections::BTreeSet, sync::Arc};

const UNBREAKABLE: [&str; 15] = [
    "minecraft:barrier",
    "minecraft:bedrock",
    "minecraft:chain_command_block",
    "minecraft:command_block",
    "minecraft:end_gateway",
    "minecraft:end_portal",
    "minecraft:end_portal_frame",
    "minecraft:jigsaw",
    "minecraft:light",
    "minecraft:moving_piston",
    "minecraft:nether_portal",
    "minecraft:repeating_command_block",
    "minecraft:structure_block",
    "minecraft:test_block",
    "minecraft:test_instance_block",
];

#[derive(Clone, Debug)]
pub struct Blocks {
    air: [BlockStateId; 3],
    fluids: Arc<FluidStateTable>,
    can_occlude: BoundBlockProperty,
    solid: BoundBlockProperty,
    occlusion: BoundOcclusionFaces,
    replaceable_by_trees: Arc<BTreeSet<BlockStateId>>,
    unbreakable: BTreeSet<BlockStateId>,
    no_motion: BTreeSet<BlockStateId>,
    structure_void: BTreeSet<BlockStateId>,
    liquid_blocks: BTreeSet<BlockStateId>,
}
impl Blocks {
    fn decode(bindings: &FeatureBlockBindings<'_>) -> Result<Self, DecodeError> {
        let block_set = |names: &[&str]| -> BTreeSet<BlockStateId> {
            names
                .iter()
                .flat_map(|n| bindings.blocks.get(*n).into_iter().flat_map(|s| s.iter()))
                .copied()
                .collect()
        };
        let missing = || DecodeError::Field("pillar processor block properties");
        Ok(Self {
            air: bindings.air,
            fluids: bindings.fluids.table().clone(),
            can_occlude: bindings.can_occlude.clone().ok_or_else(missing)?,
            solid: bindings.solid()?,
            occlusion: bindings.occlusion_faces.clone().ok_or_else(missing)?,
            replaceable_by_trees: bindings
                .tags
                .get("minecraft:replaceable_by_trees")
                .cloned()
                .unwrap_or_default(),
            unbreakable: block_set(&UNBREAKABLE),
            no_motion: block_set(&["minecraft:cobweb", "minecraft:bamboo_sapling"]),
            structure_void: block_set(&["minecraft:structure_void"]),
            liquid_blocks: block_set(&["minecraft:water", "minecraft:lava"]),
        })
    }
    fn air(&self, state: BlockStateId) -> bool {
        self.air.contains(&state)
    }
    fn fluid(&self, state: BlockStateId) -> bool {
        self.fluid_kind(state) != FluidKind::Empty
    }
    fn fluid_kind(&self, state: BlockStateId) -> FluidKind {
        self.fluids
            .get(state)
            .map_or(FluidKind::Empty, |f| f.kind())
    }
    fn passable(&self, state: BlockStateId, forced: bool) -> bool {
        (forced && !self.unbreakable.contains(&state)) || !self.can_occlude.contains(state)
    }
    fn replaceable(&self, state: BlockStateId) -> bool {
        self.air(state) || self.fluid(state) || self.replaceable_by_trees.contains(&state)
    }
    fn full_cube(&self, state: BlockStateId) -> bool {
        Direction::ALL
            .into_iter()
            .all(|d| self.occlusion.is_full(state, d).unwrap_or(false))
    }
    fn blocks_motion(&self, state: BlockStateId) -> bool {
        !self.no_motion.contains(&state) && self.solid.contains(state)
    }
}

fn read<W: PlacementContext>(
    world: &mut W,
    blocks: &Blocks,
    pos: [i32; 3],
) -> Result<BlockStateId, PlacementQueryError> {
    Ok(world.section_block(pos)?.unwrap_or(blocks.air[2]))
}

pub(super) fn outside_chunk(settings: &PlaceSettings<'_>, pos: [i32; 3]) -> bool {
    settings.bounding_box.is_some_and(|b| {
        pos[0] < b.min[0] || pos[0] > b.max[0] || pos[2] < b.min[2] || pos[2] > b.max[2]
    })
}

fn first_land_y<W: PlacementContext>(
    world: &mut W,
    blocks: &Blocks,
    pos: [i32; 3],
) -> Result<i32, PlacementQueryError> {
    let min = world.min_y()?;
    let mut y = pos[1];
    while y >= min && blocks.replaceable(read(world, blocks, [pos[0], y, pos[2]])?) {
        y -= 1;
    }
    Ok(y)
}

fn closer_than(a: [i32; 3], b: [i32; 3], length: i32) -> bool {
    let d: i64 = (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum();
    (d as f64) < f64::from(length) * f64::from(length)
}

const DIRECTION_NAMES: [&str; 6] = ["down", "up", "north", "south", "west", "east"];

fn direction(name: &str) -> Result<Direction, DecodeError> {
    let i = DIRECTION_NAMES
        .iter()
        .position(|n| *n == name)
        .ok_or(DecodeError::Field("direction"))?;
    Ok(Direction::ALL[i])
}

fn direction_name(d: Direction) -> &'static str {
    DIRECTION_NAMES[Direction::ALL.iter().position(|x| *x == d).unwrap_or(0)]
}

fn add(pos: [i32; 3], d: Direction) -> [i32; 3] {
    let n = d.normal();
    [pos[0] + n[0], pos[1] + n[1], pos[2] + n[2]]
}

fn nested(
    v: Option<&Value>,
    resources: &StructureResources,
    bindings: &FeatureBlockBindings<'_>,
) -> Result<Option<ProcessorList>, DecodeError> {
    match v {
        None => Ok(None),
        Some(Value::String(s)) if s == "minecraft:empty" => Ok(None),
        Some(v) => Ok(Some(ProcessorList::decode(
            &v.to_string(),
            resources,
            bindings,
        )?)),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_nested<W: PlacementContext>(
    list: Option<&ProcessorList>,
    world: &mut W,
    resources: &StructureResources,
    position: [i32; 3],
    origin: [i32; 3],
    at: [i32; 3],
    state: BlockStateId,
    settings: &mut PlaceSettings<'_>,
) -> ProcessResult {
    let info = BlockInfo {
        pos: at,
        state,
        nbt: None,
    };
    let Some(list) = list else {
        return Ok(Some(info));
    };
    let local = std::array::from_fn(|i| at[i] - origin[i] + position[i]);
    process_list(list, world, resources, local, at, local, info, settings)
}

#[derive(Clone, Debug)]
struct HeightRandomizer {
    entries: Vec<(BlockStateId, f32, Option<i32>, Option<i32>)>,
    default: BlockStateId,
}
impl HeightRandomizer {
    fn decode(
        v: &Value,
        state: &impl Fn(&Value) -> Result<BlockStateId, DecodeError>,
    ) -> Result<Self, DecodeError> {
        let entries = v["entries"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default()
            .iter()
            .map(|e| {
                let y = |k: &str| e.get(k).and_then(Value::as_i64).map(|y| y as i32);
                let p = e["probability"]
                    .as_f64()
                    .ok_or(DecodeError::Field("randomizer probability"))?;
                Ok((state(&e["block"])?, p as f32, y("min_y"), y("max_y")))
            })
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self {
            entries,
            default: state(&v["default"])?,
        })
    }
    fn get(&self, random: &mut dyn ProcessorRandom, y: i32) -> BlockStateId {
        let target = random.float();
        let mut bottom = 0f32;
        for &(state, p, min, max) in &self.entries {
            let in_range = min.is_none_or(|m| y >= m) && max.is_none_or(|m| y <= m);
            if bottom <= target && target < bottom + p && in_range {
                return state;
            }
            bottom += p;
        }
        self.default
    }
}

#[derive(Clone, Debug)]
pub struct Pillar {
    triggers: Vec<(BlockStateId, BlockStateId)>,
    list: Option<ProcessorList>,
    direction: Direction,
    original: Option<BlockStateId>,
    length: i32,
    forced: bool,
    randomizer: Option<HeightRandomizer>,
    blocks: Blocks,
}
impl Pillar {
    pub(super) fn decode(
        v: &Value,
        resources: &StructureResources,
        bindings: &FeatureBlockBindings<'_>,
        state: impl Fn(&Value) -> Result<BlockStateId, DecodeError>,
    ) -> Result<Self, DecodeError> {
        let triggers = v["pillar_trigger_and_replacements"]
            .as_array()
            .ok_or(DecodeError::Field("pillar_trigger_and_replacements"))?
            .iter()
            .map(|p| Ok((state(&p["trigger"])?, state(&p["replacement"])?)))
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self {
            triggers,
            list: nested(v.get("pillar_processor_list"), resources, bindings)?,
            direction: direction(v["direction"].as_str().unwrap_or("down"))?,
            original: v.get("original_replaced_block").map(&state).transpose()?,
            length: v["pillar_length"].as_i64().unwrap_or(1000) as i32,
            forced: v["forced_placement"].as_bool().unwrap_or(false),
            randomizer: v
                .get("pillar_state_randomizer")
                .map(|r| HeightRandomizer::decode(r, &state))
                .transpose()?,
            blocks: Blocks::decode(bindings)?,
        })
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        resources: &StructureResources,
        position: [i32; 3],
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        let Some(&(_, replacement)) = self.triggers.iter().find(|t| t.0 == info.state) else {
            return Ok(Some(info));
        };
        let origin = info.pos;
        let kept = self.original.unwrap_or(replacement);
        let result = (!self.blocks.structure_void.contains(&kept)).then_some(BlockInfo {
            pos: origin,
            state: kept,
            nbt: None,
        });
        if outside_chunk(settings, origin) {
            return Ok(result);
        }
        let mut terrain_y = i32::MIN;
        if self.direction == Direction::Down && !self.forced {
            terrain_y = first_land_y(world, &self.blocks, origin)?;
            let min = world.min_y()?;
            if terrain_y <= min && self.length + 2 >= origin[1] - min {
                return Ok(result);
            }
        }
        let mut at = origin;
        let mut block = read(world, &self.blocks, add(origin, self.direction))?;
        while self.blocks.passable(block, self.forced) {
            if (!self.forced && at[1] < terrain_y)
                || world.outside_build_height(at[1])?
                || !closer_than(at, origin, self.length)
            {
                break;
            }
            let fill = match &self.randomizer {
                Some(r) => settings.with_random(at, |random| r.get(random, at[1])),
                None => replacement,
            };
            let placed = run_nested(
                self.list.as_ref(),
                world,
                resources,
                position,
                origin,
                at,
                fill,
                settings,
            )?;
            if let Some(placed) = placed {
                world.set_chunk_block(at, placed.state)?;
            }
            at = add(at, self.direction);
            block = read(world, &self.blocks, at)?;
        }
        Ok(result)
    }
}

#[derive(Clone, Debug)]
pub struct BottomPillar {
    list: Option<ProcessorList>,
    length: i32,
    forced: bool,
    blocks: Blocks,
}
impl BottomPillar {
    pub(super) fn decode(
        v: &Value,
        resources: &StructureResources,
        bindings: &FeatureBlockBindings<'_>,
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            list: nested(v.get("pillar_processor_list"), resources, bindings)?,
            length: v["pillar_length"].as_i64().unwrap_or(1000) as i32,
            forced: v["forced_placement"].as_bool().unwrap_or(false),
            blocks: Blocks::decode(bindings)?,
        })
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        resources: &StructureResources,
        position: [i32; 3],
        relative: [i32; 3],
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        if relative[1] != 0 || outside_chunk(settings, info.pos) {
            return Ok(Some(info));
        }
        let origin = info.pos;
        if !self.forced {
            let terrain_y = first_land_y(world, &self.blocks, origin)?;
            let min = world.min_y()?;
            if terrain_y <= min && self.length + 2 >= origin[1] - min {
                return Ok(Some(info));
            }
        }
        let below = |pos: [i32; 3]| [pos[0], pos[1] - 1, pos[2]];
        let valid = |world: &mut W, at: [i32; 3]| -> Result<bool, PlacementQueryError> {
            let block = read(world, &self.blocks, at)?;
            Ok(self.blocks.passable(block, self.forced)
                && !world.outside_build_height(at[1])?
                && closer_than(at, origin, self.length))
        };
        let mut at = below(origin);
        let mut layer = valid(world, at)?;
        let mut skip_first = valid(world, below(at))?;
        while skip_first || layer {
            skip_first = false;
            let placed = run_nested(
                self.list.as_ref(),
                world,
                resources,
                position,
                origin,
                at,
                info.state,
                settings,
            )?;
            if let Some(placed) = placed {
                world.set_chunk_block(at, placed.state)?;
            }
            at = below(at);
            layer = valid(world, at)?;
        }
        Ok(Some(info))
    }
}

#[derive(Clone, Debug)]
pub struct CloseOffFluidSources {
    replacements: Vec<(BlockStateId, i32)>,
    ignore_down: bool,
    if_air: bool,
    blocks: Blocks,
}
impl CloseOffFluidSources {
    pub(super) fn decode(
        v: &Value,
        resources: &StructureResources,
        bindings: &FeatureBlockBindings<'_>,
    ) -> Result<Self, DecodeError> {
        let replacements = weighted_blocks(v, resources)?;
        Ok(Self {
            replacements,
            ignore_down: v["ignore_down"].as_bool().unwrap_or(false),
            if_air: v["if_air_in_world"].as_bool().unwrap_or(false),
            blocks: Blocks::decode(bindings)?,
        })
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        let state = info.state;
        if self.blocks.structure_void.contains(&state)
            || self.blocks.fluid(state)
            || outside_chunk(settings, info.pos)
            || (self.blocks.full_cube(state) && self.blocks.blocks_motion(state))
        {
            return Ok(Some(info));
        }
        if self.if_air && !self.blocks.air(read(world, &self.blocks, info.pos)?) {
            return Ok(Some(info));
        }
        for d in Direction::ALL {
            if self.ignore_down && d == Direction::Down {
                continue;
            }
            let at = add(info.pos, d);
            if world.outside_build_height(at[1])? || !world.fluid_state(at)?.is_source() {
                continue;
            }
            let block = settings.with_random(info.pos, |random| pick(&self.replacements, random));
            world.set_section_block(at, block)?;
        }
        Ok(Some(info))
    }
}

#[derive(Clone, Debug)]
pub struct CloseOffAirSources {
    replacements: Vec<(BlockStateId, i32)>,
    blocks: Blocks,
}
impl CloseOffAirSources {
    pub(super) fn decode(
        v: &Value,
        resources: &StructureResources,
        bindings: &FeatureBlockBindings<'_>,
    ) -> Result<Self, DecodeError> {
        Ok(Self {
            replacements: weighted_blocks(v, resources)?,
            blocks: Blocks::decode(bindings)?,
        })
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        let fluid = self.blocks.fluid_kind(info.state);
        if outside_chunk(settings, info.pos) || fluid == FluidKind::Empty {
            return Ok(Some(info));
        }
        for d in Direction::ALL {
            let at = add(info.pos, d);
            if world.outside_build_height(at[1])? {
                continue;
            }
            let Some(beside) = world.section_block(at)? else {
                continue;
            };
            let other_liquid = self.blocks.liquid_blocks.contains(&beside)
                && self.blocks.fluid_kind(beside) != fluid;
            if self.blocks.air(beside) || other_liquid {
                let block = match self.replacements.as_slice() {
                    [(only, _)] => *only,
                    _ => settings.with_random(info.pos, |random| pick(&self.replacements, random)),
                };
                world.set_section_block(at, block)?;
            }
        }
        Ok(Some(info))
    }
}

fn weighted_blocks(
    v: &Value,
    resources: &StructureResources,
) -> Result<Vec<(BlockStateId, i32)>, DecodeError> {
    let replacements = v["weighted_list_of_replacement_blocks"]
        .as_array()
        .ok_or(DecodeError::Field("weighted_list_of_replacement_blocks"))?
        .iter()
        .map(|e| {
            let name = e["block"].as_str().ok_or(DecodeError::Field("block"))?;
            let name = crate::worldgen::tags::identifier(name)?;
            let state = resources
                .registry
                .default_state(&name)
                .ok_or_else(|| DecodeError::Registry(format!("unknown block {name}")))?;
            let weight = e["weight"].as_i64().ok_or(DecodeError::Field("weight"))?;
            Ok((state, weight as i32))
        })
        .collect::<Result<Vec<_>, DecodeError>>()?;
    if replacements.is_empty() {
        return Err(DecodeError::Field("weighted_list_of_replacement_blocks"));
    }
    Ok(replacements)
}

fn pick(entries: &[(BlockStateId, i32)], random: &mut dyn ProcessorRandom) -> BlockStateId {
    let total: f64 = entries.iter().map(|r| f64::from(r.1)).sum();
    let mut picked = f64::from(random.float()) * total;
    let mut index = 0;
    while index < entries.len() - 1 {
        picked -= f64::from(entries[index].1);
        if picked <= 0.0 {
            break;
        }
        index += 1;
    }
    entries[index].0
}

#[derive(Clone, Debug)]
struct YungRandomizer {
    entries: Vec<(BlockStateId, f32, Option<f64>, Option<f64>)>,
    default: BlockStateId,
}
impl YungRandomizer {
    fn decode(
        v: &Value,
        state: &impl Fn(&Value) -> Result<BlockStateId, DecodeError>,
    ) -> Result<Self, DecodeError> {
        let entries = v["entries"]
            .as_array()
            .ok_or(DecodeError::Field("randomizer entries"))?
            .iter()
            .map(|e| {
                let p = e["probability"]
                    .as_f64()
                    .ok_or(DecodeError::Field("randomizer probability"))?;
                let (bottom, top) = match e.get("condition") {
                    None => (None, None),
                    Some(c) if c["type"] == "yungsapi:altitude" => {
                        (c["bottom_cutoff_y"].as_f64(), c["top_cutoff_y"].as_f64())
                    }
                    Some(_) => return Err(DecodeError::Field("randomizer condition")),
                };
                Ok((state(&e["blockState"])?, p as f32, bottom, top))
            })
            .collect::<Result<_, DecodeError>>()?;
        Ok(Self {
            entries,
            default: state(&v["defaultBlockState"])?,
        })
    }
    fn get(&self, random: &mut dyn ProcessorRandom, y: Option<i32>) -> BlockStateId {
        let target = random.float();
        let mut bottom = 0f32;
        for &(state, p, low, high) in &self.entries {
            let passes = y.is_none_or(|y| {
                let y = f64::from(y);
                !low.is_some_and(|l| y < l) && !high.is_some_and(|h| y > h)
            });
            if bottom <= target && target < bottom + p && passes {
                return state;
            }
            bottom += p;
        }
        self.default
    }
}

#[derive(Clone, Debug)]
pub struct YungPillar {
    target: String,
    output: YungRandomizer,
    pillar: YungRandomizer,
    keep_properties: bool,
    blocks: Blocks,
}
impl YungPillar {
    pub(super) fn decode(
        v: &Value,
        bindings: &FeatureBlockBindings<'_>,
        state: impl Fn(&Value) -> Result<BlockStateId, DecodeError>,
    ) -> Result<Self, DecodeError> {
        let target = state(&v["target_block"])?;
        Ok(Self {
            target: bindings.registry().state(target).unwrap().name.clone(),
            output: YungRandomizer::decode(&v["target_block_output"], &state)?,
            pillar: YungRandomizer::decode(&v["pillar_states"], &state)?,
            keep_properties: v["keep_properties"].as_bool().unwrap_or(false),
            blocks: Blocks::decode(bindings)?,
        })
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        resources: &StructureResources,
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        let name = resources
            .registry
            .state(info.state)
            .map(|s| s.name.as_str());
        if name != Some(self.target.as_str()) || outside_chunk(settings, info.pos) {
            return Ok(Some(info));
        }
        let (min, max) = (world.min_y()?, world.max_y()?);
        let origin = info.pos;
        let registry = &resources.registry;
        let kept = |state: BlockStateId| match self.keep_properties {
            true => registry
                .state(state)
                .and_then(|s| super::with_properties_of(registry, &s.name, info.state))
                .unwrap_or(state),
            false => state,
        };
        settings.with_random(origin, |random| {
            let state = kept(self.output.get(random, None));
            let mut at = [origin[0], origin[1] - 1, origin[2]];
            while at[1] > min && at[1] <= max {
                let block = read(world, &self.blocks, at)?;
                if !(self.blocks.air(block) || self.blocks.fluid(block)) {
                    break;
                }
                world.set_chunk_block(at, kept(self.pillar.get(random, Some(at[1]))))?;
                at[1] -= 1;
            }
            Ok(Some(BlockInfo { state, ..info }))
        })
    }
}

#[derive(Clone, Debug)]
pub struct StairPillar {
    blocks_to: Vec<(String, String)>,
    front: String,
    pillar: BlockStateId,
    blocks: Blocks,
}
impl StairPillar {
    pub(super) fn decode(
        v: &Value,
        resources: &StructureResources,
        bindings: &FeatureBlockBindings<'_>,
    ) -> Result<Self, DecodeError> {
        let id = |v: &Value| -> Result<String, DecodeError> {
            let name = v.as_str().ok_or(DecodeError::Field("stair_pillar block"))?;
            Ok(crate::worldgen::tags::identifier(name)?.to_string())
        };
        let blocks_to = v["blocks"]
            .as_object()
            .ok_or(DecodeError::Field("stair_pillar blocks"))?
            .iter()
            .map(|(from, to)| Ok((id(&Value::String(from.clone()))?, id(to)?)))
            .collect::<Result<_, DecodeError>>()?;
        let pillar = id(&v["pillar"])?;
        Ok(Self {
            blocks_to,
            front: id(&v["front"])?,
            pillar: resources
                .registry
                .default_state(&pillar)
                .ok_or_else(|| DecodeError::Registry(format!("unknown block {pillar}")))?,
            blocks: Blocks::decode(bindings)?,
        })
    }
    fn stair(
        resources: &StructureResources,
        block: &str,
        from: &std::collections::BTreeMap<String, String>,
        facing: Option<&str>,
    ) -> Option<BlockStateId> {
        let registry = resources.registry;
        let mut state = registry.state(registry.default_state(block)?)?.clone();
        for key in ["facing", "half", "shape"] {
            let value = match (key, facing) {
                ("facing", Some(f)) => f,
                _ => from.get(key)?.as_str(),
            };
            if state.properties.contains_key(key) {
                state.properties.insert(key.to_owned(), value.to_owned());
            }
        }
        registry.id(&state)
    }
    pub(super) fn process<W: PlacementContext>(
        &self,
        world: &mut W,
        resources: &StructureResources,
        info: BlockInfo,
        settings: &mut PlaceSettings<'_>,
    ) -> ProcessResult {
        let Some(old) = resources.registry.state(info.state) else {
            return Ok(Some(info));
        };
        let Some((_, to)) = self.blocks_to.iter().find(|(from, _)| *from == old.name) else {
            return Ok(Some(info));
        };
        let props = &old.properties;
        let Some(facing) = props.get("facing").and_then(|f| direction(f).ok()) else {
            return Ok(Some(info));
        };
        let Some(output) = Self::stair(resources, to, props, None) else {
            return Ok(Some(info));
        };
        let info = BlockInfo {
            state: output,
            ..info
        };
        let mut at = add(info.pos, facing);
        if outside_chunk(settings, at) {
            return Ok(Some(info));
        }
        let back = direction_name(facing.opposite());
        if let Some(front) = Self::stair(resources, &self.front, props, Some(back)) {
            world.set_chunk_block(at, front)?;
        }
        let (min, max) = (world.min_y()?, world.max_y()?);
        at[1] -= 1;
        while at[1] > min && at[1] <= max {
            let block = read(world, &self.blocks, at)?;
            if !(self.blocks.air(block) || self.blocks.fluid(block)) {
                break;
            }
            world.set_chunk_block(at, self.pillar)?;
            at[1] -= 1;
        }
        Ok(Some(info))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Floats(Vec<f32>);
    impl ProcessorRandom for Floats {
        fn float(&mut self) -> f32 {
            self.0.remove(0)
        }
        fn int(&mut self, _: i32) -> i32 {
            unreachable!()
        }
        fn long(&mut self) -> i64 {
            unreachable!()
        }
    }

    fn id(v: &Value) -> Result<BlockStateId, DecodeError> {
        let v = v.get("Name").unwrap_or(v);
        Ok(BlockStateId(v.as_str().unwrap().len() as u32))
    }

    #[test]
    fn height_randomizer_takes_the_share_in_range() {
        let r = HeightRandomizer::decode(
            &json!({"default": "d", "entries": [
                {"block": "aa", "probability": 0.25, "max_y": 10},
                {"block": "bbb", "probability": 0.5}]}),
            &id,
        )
        .unwrap();
        let (a, b, d) = (BlockStateId(2), BlockStateId(3), BlockStateId(1));
        assert_eq!(r.get(&mut Floats(vec![0.1]), 5), a);
        assert_eq!(r.get(&mut Floats(vec![0.1]), 11), d);
        assert_eq!(r.get(&mut Floats(vec![0.25]), 11), b);
        assert_eq!(r.get(&mut Floats(vec![0.75]), 0), d);
    }

    #[test]
    fn yung_randomizer_checks_altitude_only_with_a_height() {
        let r = YungRandomizer::decode(
            &json!({"defaultBlockState": "d", "entries": [
                {"blockState": "aa", "probability": 0.5,
                 "condition": {"type": "yungsapi:altitude", "bottom_cutoff_y": 40}}]}),
            &id,
        )
        .unwrap();
        let (a, d) = (BlockStateId(2), BlockStateId(1));
        assert_eq!(r.get(&mut Floats(vec![0.2]), None), a);
        assert_eq!(r.get(&mut Floats(vec![0.2]), Some(40)), a);
        assert_eq!(r.get(&mut Floats(vec![0.2]), Some(39)), d);
        assert_eq!(r.get(&mut Floats(vec![0.5]), None), d);
        let bad = json!({"defaultBlockState": "d", "entries": [
            {"blockState": "aa", "probability": 1.0, "condition": {"type": "x:y"}}]});
        assert!(YungRandomizer::decode(&bad, &id).is_err());
    }

    #[test]
    fn fluid_fix_picks_like_get_random_entry() {
        let fix = |weights: &[i32]| -> Vec<(BlockStateId, i32)> {
            weights
                .iter()
                .enumerate()
                .map(|(i, w)| (BlockStateId(i as u32), *w))
                .collect()
        };
        let three = fix(&[1, 2, 1]);
        for (f, want) in [(0.1, 0), (0.6, 1), (0.75, 1), (0.8, 2)] {
            assert_eq!(pick(&three, &mut Floats(vec![f])), BlockStateId(want));
        }
        assert_eq!(pick(&fix(&[5]), &mut Floats(vec![0.9])), BlockStateId(0));
    }

    #[test]
    fn directions_and_distances() {
        for name in DIRECTION_NAMES {
            assert_eq!(direction_name(direction(name).unwrap()), name);
        }
        assert!(direction("sideways").is_err());
        assert_eq!(add([0, 5, 0], direction("down").unwrap()), [0, 4, 0]);
        assert_eq!(add([0, 5, 0], direction("east").unwrap()), [1, 5, 0]);
        assert!(closer_than([0, 0, 0], [3, 0, 0], 4));
        assert!(!closer_than([0, 0, 0], [3, 0, 4], 5));
        assert!(closer_than([0, 0, 0], [3, 0, 4], 6));
    }
}
