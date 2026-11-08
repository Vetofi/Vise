use super::{Behaviours, LevelAccess, LevelEvent, Sim, feature_world::LevelFeatureWorld, flags};
use crate::{registry::BlockStateId, worldgen::live::LiveFeatures};
use vise_core::{position::BlockPos, random::RandomSource};
use std::sync::{Arc, OnceLock};

#[derive(Default)]
pub struct Growth {
    features: OnceLock<Arc<LiveFeatures>>,
}

impl Behaviours {
    pub fn set_live_features(&self, features: Arc<LiveFeatures>) -> bool {
        self.growth.features.set(features).is_ok()
    }
    pub fn live_features(&self) -> Option<&Arc<LiveFeatures>> {
        self.growth.features.get()
    }
    pub fn place_jigsaw<L: LevelAccess + ?Sized>(
        &self,
        level: &mut L,
        ticks: &mut super::Ticks,
        reference: [i32; 3],
        assemble: impl FnOnce() -> Option<crate::worldgen::generator::JigsawPieces>,
    ) -> bool {
        let Some(features) = self.live_features() else {
            return false;
        };
        let Some(jigsaw) = assemble() else {
            return false;
        };
        let mut sim = self.sim(level, ticks);
        let mut random = sim.random().clone();
        {
            let mut world = LevelFeatureWorld {
                sim: &mut sim,
                features,
            };
            let unbounded =
                crate::worldgen::structure::BoundingBox::new([i32::MIN; 3], [i32::MAX; 3]);
            for piece in &jigsaw.pieces {
                let mut place_feature =
                    |w: &mut LevelFeatureWorld<'_, '_, L>,
                     r: &mut vise_core::random::LegacyRandom,
                     name: &str,
                     p: [i32; 3]| { Ok(features.place_placed(name, w, r, p)) };
                if let Err(e) = piece.place(
                    &mut world,
                    &jigsaw.resources,
                    &mut random,
                    unbounded,
                    reference,
                    &mut place_feature,
                ) {
                    eprintln!("Vise: placing a jigsaw piece failed: {e}");
                }
            }
        }
        *sim.random() = random;
        true
    }
}

type Saplings = [(BlockStateId, BlockPos); 4];

pub(crate) struct TreeGrower {
    pub name: &'static str,
    trees: &'static [(&'static str, i32)],
    mega_trees: &'static [(&'static str, i32)],
    flower_trees: &'static [(&'static str, i32)],
    shortest: Option<&'static str>,
}

pub(crate) const GROWERS: &[TreeGrower] = &[
    TreeGrower {
        name: "oak",
        trees: &[("minecraft:oak", 9), ("minecraft:fancy_oak", 1)],
        mega_trees: &[],
        flower_trees: &[
            ("minecraft:oak_bees_005", 9),
            ("minecraft:fancy_oak_bees_005", 1),
        ],
        shortest: Some("minecraft:oak"),
    },
    TreeGrower {
        name: "spruce",
        trees: &[("minecraft:spruce", 1)],
        mega_trees: &[("minecraft:mega_spruce", 1), ("minecraft:mega_pine", 1)],
        flower_trees: &[],
        shortest: Some("minecraft:spruce"),
    },
    TreeGrower {
        name: "mangrove",
        trees: &[("minecraft:mangrove", 15), ("minecraft:tall_mangrove", 85)],
        mega_trees: &[],
        flower_trees: &[],
        shortest: Some("minecraft:mangrove"),
    },
    TreeGrower {
        name: "azalea",
        trees: &[("minecraft:azalea_tree", 1)],
        mega_trees: &[],
        flower_trees: &[],
        shortest: Some("minecraft:azalea_tree"),
    },
    TreeGrower {
        name: "birch",
        trees: &[("minecraft:birch", 1)],
        mega_trees: &[],
        flower_trees: &[("minecraft:birch_bees_005", 1)],
        shortest: Some("minecraft:birch"),
    },
    TreeGrower {
        name: "jungle",
        trees: &[("minecraft:jungle_tree_no_vine", 1)],
        mega_trees: &[("minecraft:mega_jungle_tree", 1)],
        flower_trees: &[],
        shortest: Some("minecraft:jungle_tree_no_vine"),
    },
    TreeGrower {
        name: "acacia",
        trees: &[("minecraft:acacia", 1)],
        mega_trees: &[],
        flower_trees: &[],
        shortest: Some("minecraft:acacia"),
    },
    TreeGrower {
        name: "cherry",
        trees: &[("minecraft:cherry", 1)],
        mega_trees: &[],
        flower_trees: &[("minecraft:cherry_bees_005", 1)],
        shortest: Some("minecraft:cherry"),
    },
    TreeGrower {
        name: "dark_oak",
        trees: &[],
        mega_trees: &[("minecraft:dark_oak", 1)],
        flower_trees: &[],
        shortest: None,
    },
    TreeGrower {
        name: "pale_oak",
        trees: &[],
        mega_trees: &[("minecraft:pale_oak_bonemeal", 1)],
        flower_trees: &[],
        shortest: None,
    },
    TreeGrower {
        name: "poplar",
        trees: &[
            ("minecraft:red_poplar", 1),
            ("minecraft:orange_poplar", 1),
            ("minecraft:yellow_poplar", 1),
        ],
        mega_trees: &[],
        flower_trees: &[],
        shortest: Some("minecraft:red_poplar"),
    },
];

pub(crate) fn grower(name: &str) -> Option<&'static TreeGrower> {
    GROWERS.iter().find(|g| g.name == name)
}

fn weighted<R: RandomSource>(
    list: &'static [(&'static str, i32)],
    random: &mut R,
) -> Option<&'static str> {
    let total: i32 = list.iter().map(|e| e.1).sum();
    if total == 0 {
        return None;
    }
    let mut selection = random.next_int_bound(total).unwrap_or(0);
    for &(name, weight) in list {
        if selection < weight {
            return Some(name);
        }
        selection -= weight;
    }
    None
}

impl<L: LevelAccess + ?Sized> Sim<'_, L> {
    pub(crate) fn place_configured_feature(&mut self, name: &str, pos: BlockPos) -> bool {
        let b = self.b;
        let Some(features) = b.live_features() else {
            return false;
        };
        let mut random = self.random().clone();
        let placed = {
            let mut world = LevelFeatureWorld {
                sim: self,
                features,
            };
            features.place_configured(name, &mut world, &mut random, [pos.x, pos.y, pos.z])
        };
        *self.random() = random;
        placed
    }
    pub(crate) fn place_placed_feature(&mut self, name: &str, pos: BlockPos) -> bool {
        let b = self.b;
        let Some(features) = b.live_features() else {
            return false;
        };
        let mut random = self.random().clone();
        let placed = {
            let mut world = LevelFeatureWorld {
                sim: self,
                features,
            };
            features.place_placed(name, &mut world, &mut random, [pos.x, pos.y, pos.z])
        };
        *self.random() = random;
        placed
    }
    fn feature_exists(&self, name: &str) -> bool {
        self.b
            .live_features()
            .is_some_and(|f| f.configured(name).is_some())
    }

    fn tree_has_flowers(&self, pos: BlockPos) -> bool {
        let b = self.b;
        let Some(flowers) = b.tag("minecraft:flowers") else {
            return false;
        };
        for y in -1..=1 {
            for z in -2..=2 {
                for x in -2..=2 {
                    if b.in_set(self.state(pos.offset(x, y, z)), flowers) {
                        return true;
                    }
                }
            }
        }
        false
    }
    fn two_by_two(&self, state: BlockStateId, pos: BlockPos) -> Option<(i32, i32, Saplings)> {
        let block = self.b.block(state);
        for dx in [0, -1] {
            for dz in [0, -1] {
                let at = |x: i32, z: i32| {
                    let p = pos.offset(dx + x, 0, dz + z);
                    (self.state(p), p)
                };
                let around = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
                if around.iter().all(|(s, _)| self.b.is_block(*s, block)) {
                    return Some((dx, dz, around));
                }
            }
        }
        None
    }
    fn remove_sapling(&mut self, pos: BlockPos) {
        let empty = self.b.legacy_block(self.fluid_at(pos));
        self.set_block(
            pos,
            empty,
            flags::UPDATE_SKIP_ALL_SIDEEFFECTS | flags::UPDATE_CLIENTS,
        );
    }
    pub(crate) fn grow_tree(
        &mut self,
        grower: &TreeGrower,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        if let Some(mega) = weighted(grower.mega_trees, self.random())
            && self.feature_exists(mega)
            && let Some((dx, dz, around)) = self.two_by_two(state, pos)
        {
            for &(_, p) in &around {
                self.remove_sapling(p);
            }
            if self.place_configured_feature(mega, pos.offset(dx, 0, dz)) {
                return true;
            }
            for &(s, p) in &around {
                self.set_block(p, s, flags::UPDATE_NONE);
            }
            return false;
        }
        let flowers = self.tree_has_flowers(pos);
        let list = if flowers && !grower.flower_trees.is_empty() {
            grower.flower_trees
        } else {
            grower.trees
        };
        let Some(feature) = weighted(list, self.random()) else {
            return false;
        };
        if !self.feature_exists(feature) {
            return false;
        }
        self.remove_sapling(pos);
        if self.place_configured_feature(feature, pos) {
            return true;
        }
        self.set_block(pos, state, flags::UPDATE_NONE);
        false
    }
    pub(crate) fn tree_can_grow(
        &mut self,
        grower: &TreeGrower,
        pos: BlockPos,
        state: BlockStateId,
    ) -> bool {
        let flowers = self.tree_has_flowers(pos);
        let list = if flowers && !grower.flower_trees.is_empty() {
            grower.flower_trees
        } else {
            grower.trees
        };
        let feature = weighted(list, self.random());
        let mega = weighted(grower.mega_trees, self.random());
        if feature.is_none() && mega.is_some() {
            self.two_by_two(state, pos).is_some()
        } else {
            true
        }
    }
    pub(crate) fn tree_minimum_height(&self, grower: &TreeGrower) -> i32 {
        grower
            .shortest
            .and_then(|name| self.b.live_features()?.configured_json(name))
            .filter(|json| json["type"] == "minecraft:tree")
            .and_then(|json| json["trunk_placer"]["base_height"].as_i64())
            .map_or(0, |h| h as i32)
    }
    pub(crate) fn advance_tree(&mut self, pos: BlockPos, state: BlockStateId) {
        let b = self.b;
        if b.table.int(state, b.props.stage) == 0 {
            self.set_block(pos, b.table.cycle(state, b.props.stage), flags::UPDATE_NONE);
            return;
        }
        let Some(name) = b.data(state).feature.clone() else {
            return;
        };
        match grower(&name) {
            Some(g) if b.live_features().is_some() => {
                self.grow_tree(g, pos, state);
            }
            _ => self.emit(LevelEvent::GrowTree {
                pos,
                state,
                grower: name,
            }),
        }
    }
    pub(crate) fn grow_mushroom(&mut self, pos: BlockPos, state: BlockStateId) -> bool {
        let Some(feature) = self.b.data(state).feature.clone() else {
            return false;
        };
        if !self.feature_exists(&feature) {
            return false;
        }
        self.remove_block(pos, false);
        if self.place_configured_feature(&feature, pos) {
            return true;
        }
        self.set_block_and_update(pos, state);
        false
    }
}
