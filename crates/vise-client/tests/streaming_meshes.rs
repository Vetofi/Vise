use vise_client::{
    assets::Assets,
    biome::Biomes,
    blocks::BlockFacts,
    chunk::{Chunk, HEIGHT},
    light::LightTables,
    mesher::{MeshTables, Mesher, Region, SectionMesh, borrow},
    models,
    streaming::{ChunkGenerator, Proto, Streaming, WorkerContext},
    world::World,
};
use vise_core::biome::BiomeZoom;
use vise_world::registry::BlockStateId;
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

fn resources() -> Option<std::path::PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/vise-client")
        .join(vise_core::VANILLA_VERSION);
    root.join("items.json").exists().then_some(root)
}

struct Saved;
impl ChunkGenerator for Saved {
    type Worker = ();
    fn worker(&self) {}
    fn load(&self, x: i32, z: i32) -> Option<Chunk> {
        let mut chunk = Chunk::empty(x, z);
        chunk.lit = true;
        chunk.light_loaded = true;
        Some(chunk)
    }
    fn proto(&self, _: &mut (), _: i32, _: i32) -> Proto {
        unreachable!("every column is saved")
    }
    fn finish(&self, _: Proto) -> Chunk {
        unreachable!("every column is saved")
    }
}

#[test]
fn columns_loaded_past_the_render_distance_mesh_when_they_come_in_range() {
    let Some(root) = resources() else {
        return;
    };
    let assets = Arc::new(Assets::load(&root).unwrap());
    let render = Arc::new(assets.reload(&root).unwrap());
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = models::load_visibility(&root, assets.registry).unwrap();
    let context = Arc::new(WorkerContext {
        tables: Arc::new(MeshTables::new(&assets, &render, &facts, visibility).unwrap()),
        light: Arc::new(LightTables::load(&root, assets.registry, &facts).unwrap()),
        biomes: Arc::new(Biomes::load(&root).unwrap()),
        assets,
        render: render.clone(),
    });
    let mut streaming = Streaming::new(Arc::new(Saved), context, 3);
    streaming.radius = 4;
    let mut world = World::new(BlockStateId(0), 0);
    let mut meshed = HashSet::new();
    let mut settle_at = |streaming: &mut Streaming, center: (i32, i32)| {
        streaming.center = center;
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut quiet = 0;
        while quiet < 40 {
            assert!(Instant::now() < deadline, "streaming never settled");
            let before = (world.chunks.len(), meshed.len());
            let (uploads, _) = streaming.update(&mut world);
            meshed.extend(uploads.iter().map(|(key, _)| *key));
            streaming.free(uploads);
            thread::sleep(Duration::from_millis(5));
            let idle = streaming.pending() == 0 && before == (world.chunks.len(), meshed.len());
            quiet = if idle { quiet + 1 } else { 0 };
        }
    };
    settle_at(&mut streaming, (0, 0));
    settle_at(&mut streaming, (3, 0));
    let (cx, cz): (i32, i32) = (3, 0);
    let mut checked = 0;
    for x in cx - 4..=cx + 4 {
        for z in cz - 4..=cz + 4 {
            if (x - cx).pow(2) + (z - cz).pow(2) > 16 {
                continue;
            }
            for sy in 0..HEIGHT / 16 {
                assert!(
                    meshed.contains(&(x, sy, z)),
                    "section {x} {sy} {z} is in range but was never meshed"
                );
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 49);
}

fn roofed(x: i32, z: i32, stone: u32, glowstone: u32) -> Chunk {
    let mut chunk = Chunk::empty(x, z);
    for lz in 0..16 {
        for lx in 0..16 {
            chunk.set(lx, 62, lz, stone);
            if (lx + 3 * lz + x * 5 + z * 7).rem_euclid(11) != 0 {
                chunk.set(lx, 64, lz, stone);
            }
        }
    }
    chunk.set(
        (x * 3).rem_euclid(16),
        62,
        (z * 5).rem_euclid(16),
        glowstone,
    );
    chunk
}

#[test]
fn relit_columns_keep_every_mesh_current() {
    let Some(root) = resources() else {
        return;
    };
    let assets = Arc::new(Assets::load(&root).unwrap());
    let render = Arc::new(assets.reload(&root).unwrap());
    let facts = BlockFacts::load(&root, assets.registry).unwrap();
    let visibility = models::load_visibility(&root, assets.registry).unwrap();
    let context = Arc::new(WorkerContext {
        tables: Arc::new(MeshTables::new(&assets, &render, &facts, visibility).unwrap()),
        light: Arc::new(LightTables::load(&root, assets.registry, &facts).unwrap()),
        biomes: Arc::new(Biomes::load(&root).unwrap()),
        assets: assets.clone(),
        render: render.clone(),
    });
    let state = |name: &str| assets.registry.default_state(name).unwrap().0;
    let (stone, glowstone, torch) = (
        state("minecraft:stone"),
        state("minecraft:glowstone"),
        state("minecraft:torch"),
    );
    let mut streaming = Streaming::new(Arc::new(Saved), context.clone(), 3);
    streaming.radius = 4;
    streaming.remote = true;
    let mut world = World::new(BlockStateId(0), 0);
    for x in -5..=5 {
        for z in -5..=5 {
            if (x, z) != (2, 0) {
                streaming.insert_remote(&mut world, roofed(x, z, stone, glowstone));
            }
        }
    }
    let mut meshes: HashMap<(i32, i32, i32), SectionMesh> = HashMap::new();
    let mut settle = |streaming: &mut Streaming, world: &mut World| {
        let deadline = Instant::now() + Duration::from_secs(60);
        let mut quiet = 0;
        while quiet < 40 {
            assert!(Instant::now() < deadline, "streaming never settled");
            let (uploads, _) = streaming.update(world);
            let idle = uploads.is_empty() && streaming.pending() == 0;
            meshes.extend(uploads);
            thread::sleep(Duration::from_millis(5));
            quiet = if idle { quiet + 1 } else { 0 };
        }
    };
    settle(&mut streaming, &mut world);
    assert!(streaming.is_lit(1, 0) && streaming.is_lit(0, 0));
    for i in 2..14 {
        for (lx, lz) in [(i, 2), (i, 13), (2, i), (13, i)] {
            let (bx, bz) = (16 + lx, lz);
            world.set(bx, 64, bz, BlockStateId(0)).unwrap();
            if i % 3 == 0 {
                world.set(bx, 63, bz, BlockStateId(torch)).unwrap();
            }
        }
    }
    settle(&mut streaming, &mut world);
    streaming.insert_remote(&mut world, roofed(2, 0, stone, glowstone));
    settle(&mut streaming, &mut world);
    let mut checked = 0;
    for (&(x, sy, z), mesh) in &meshes {
        let region: Region = std::array::from_fn(|dz| {
            std::array::from_fn(|dx| {
                world
                    .chunks
                    .get(&(x + dx as i32 - 1, z + dz as i32 - 1))
                    .cloned()
            })
        });
        let mut mesher = Mesher {
            nether_shading: false,
            zoom: BiomeZoom::new(0),
            blend_radius: 2,
            smooth_lighting: true,
            cutout_leaves: true,
            assets: &context.assets,
            render: &context.render,
            tables: &context.tables,
            biomes: &context.biomes,
            visibility: context.tables.visibility.clone(),
        };
        let mut fresh = mesher.mesh(&borrow(&region), sy);
        fresh.sort_solid();
        let bytes = |m: &SectionMesh| {
            [
                bytemuck::cast_slice::<_, u8>(&m.solid).to_vec(),
                bytemuck::cast_slice::<_, u8>(&m.translucent).to_vec(),
            ]
        };
        assert!(
            bytes(mesh) == bytes(&fresh) && mesh.visibility == fresh.visibility,
            "section {x} {sy} {z} kept a stale mesh"
        );
        checked += 1;
    }
    assert!(checked >= 45 * 24, "only {checked} sections checked");
    for x in 0..=3 {
        for z in -1..=1 {
            assert!((0..24).all(|sy| meshes.contains_key(&(x, sy, z))));
        }
    }
}
