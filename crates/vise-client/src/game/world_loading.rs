use super::*;

pub(super) struct WorldRequest {
    pub save_root: PathBuf,
    pub folder: String,
    pub name: Option<String>,
    pub seed: Option<i64>,
    pub creative: bool,
    pub create: Option<vise_client::title::WorldOptions>,
}

pub(super) struct PreparedWorld {
    pub save_root: PathBuf,
    pub shapes: vise_client::chunk::LevelShapes,
    pub level: Level,
    pub world: World,
    pub streaming: Streaming,
    pub save: Arc<SaveDir>,
    pub session_lock: vise_client::save::SessionLock,
    pub rand_sequences: vise_client::random_sequences::RandomSequences,
    pub locator: Option<Arc<VanillaTerrain>>,
}

impl WorldRequest {
    pub fn new(
        folder: &str,
        name: Option<&str>,
        seed: Option<i64>,
        creative: bool,
        create: Option<&vise_client::title::WorldOptions>,
    ) -> Result<Self> {
        Ok(Self {
            save_root: std::env::current_dir()?.join("saves").join(folder),
            folder: folder.to_owned(),
            name: name.map(str::to_owned),
            seed,
            creative,
            create: create.cloned(),
        })
    }

    pub fn prepare(
        self,
        res: &Resources,
        pack: &PackResources,
        options: &Options,
        settings: &Settings,
    ) -> Result<PreparedWorld> {
        vise_client::structure_blocks::loot_data(
            &options.resources,
            &vise_world::loot::PackOverlay::NONE,
        );
        let Self {
            save_root,
            folder,
            name,
            seed,
            creative,
            create,
        } = self;
        let name = name.as_deref();
        let create = create.as_ref();
        vise_client::save::upgrade_older_world(&save_root)?;
        vise_client::save::migrate_legacy(&save_root)?;
        let session_lock = vise_client::save::SessionLock::acquire(&save_root)?;
        let rand_sequences =
            vise_client::save::read_dat(&save_root.join("data/minecraft/random_sequences.dat"))
                .and_then(|t| t.get("data").cloned())
                .map(|d| vise_client::random_sequences::RandomSequences::from_nbt(&d))
                .unwrap_or_default();
        let existing = Level::read(&save_root);
        let created = existing.is_none();
        let level = existing.unwrap_or_else(|| Level {
            name: name.unwrap_or(&folder).to_owned(),
            seed: seed.unwrap_or_else(|| {
                let t = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                (t ^ (t >> 29)).wrapping_mul(0x9E37_79B9_7F4A_7C15) as i64
            }),
            creative,
            day_time: 0,
            health: 20.0,
            food: 20,
            saturation: 5.0,
            ..Level::default()
        });
        let mut level = level;
        if created && let Some(o) = create {
            level.hardcore = o.hardcore;
            level.difficulty = vise_client::title::DIFFICULTIES[o.difficulty()].into();
            level.allow_commands = Some(o.commands(creative));
            vise_client::save::write_world_gen_settings_with(
                &save_root,
                level.seed,
                o.world_gen(),
                o.flat_settings(),
            )?;
            level.game_rules = o.rules.by_name();
        }
        if created {
            level.allow_commands.get_or_insert(creative);
        }
        if let Some(d) = &options.dimension {
            level.dimension = d.clone();
            level.position = None;
        }
        let dim = Dim::from_id(&level.dimension);
        let shapes = level_shapes(&save_root, res.biomes());
        let shape = shapes.get(dim.id());
        let (mut streaming, save, locator) = make_streaming(
            res, pack, options, settings, &save_root, dim, shape, level.seed,
        )?;
        streaming.radius = settings.render_distance;
        let mut world = World::new(res.facts().air, level.seed);
        world.shape = shape;
        world.fast_lava = dim == Dim::Nether;
        world.game_time = level.time;
        world.day_time = options.time.unwrap_or(level.day_time);
        world.weather = level.weather;
        if let Some(w) = options.weather.as_deref() {
            let (rain, thunder) = (w != "clear", w == "thunder");
            world.weather = vise_client::world::Weather {
                clear_time: if w == "clear" { 6000 } else { 0 },
                rain_time: 6000,
                thunder_time: 6000,
                raining: rain,
                thundering: thunder,
                rain_level: if rain { (1.0, 1.0) } else { (0.0, 0.0) },
                thunder_level: if thunder { (1.0, 1.0) } else { (0.0, 0.0) },
            };
        }

        growth::install_live_features(res, pack, options);

        if !save_root.join("level.dat").exists() {
            level.write(&save_root)?;
        }
        Ok(PreparedWorld {
            save_root,
            shapes,
            level,
            world,
            streaming,
            save,
            session_lock,
            rand_sequences,
            locator,
        })
    }
}

pub(super) struct WorldLoader {
    pub cancelled: bool,
    handle: JoinHandle<std::result::Result<PreparedWorld, String>>,
}

impl WorldLoader {
    pub fn start(
        request: WorldRequest,
        res: Arc<Resources>,
        pack: PackResources,
        options: Options,
        settings: Settings,
    ) -> Result<Self> {
        let handle = std::thread::Builder::new()
            .name("World initialization".into())
            .spawn(move || {
                request
                    .prepare(&res, &pack, &options, &settings)
                    .map_err(|e| e.to_string())
            })?;
        Ok(Self {
            cancelled: false,
            handle,
        })
    }

    pub fn cancel(self) -> Self {
        Self {
            cancelled: true,
            handle: std::thread::spawn(move || {
                drop(self.finish());
                Err("world initialization cancelled".into())
            }),
        }
    }

    pub fn ready(&self) -> bool {
        self.handle.is_finished()
    }

    pub fn finish(self) -> Result<PreparedWorld> {
        self.handle
            .join()
            .map_err(|_| "world initialization worker panicked")?
            .map_err(Into::into)
    }
}

const PRELOAD_KEPT: std::time::Duration = std::time::Duration::from_secs(60);

enum Preload {
    Idle,
    Started {
        thread: std::thread::JoinHandle<()>,
        left: Option<std::time::Instant>,
    },
    Used,
}

static PRELOAD: std::sync::Mutex<Preload> = std::sync::Mutex::new(Preload::Idle);

pub(super) fn preload_world_data(
    on_world_screens: bool,
    resources: &std::path::Path,
    registry: &'static vise_world::registry::BlockRegistry,
) {
    let mut preload = PRELOAD.lock().unwrap_or_else(|e| e.into_inner());
    match &mut *preload {
        Preload::Used => {}
        Preload::Idle if on_world_screens => {
            let resources = resources.to_owned();
            let spawned = std::thread::Builder::new()
                .name("World data preload".into())
                .spawn(move || {
                    vise_threads::behind_server();
                    if let Ok(data) = vise_client::vanillagen::shared_data(
                        &resources.join("worldgen"),
                        registry,
                    ) {
                        let _ = data.prepare_decoration("minecraft:normal", "minecraft:overworld");
                    }
                    let _ = vise_client::structure_blocks::loot_data(
                        &resources,
                        &vise_world::loot::PackOverlay::NONE,
                    );
                    let _ =
                        super::lan::sync_data(&resources, &vise_world::loot::PackOverlay::NONE);
                });
            match spawned {
                Ok(thread) => *preload = Preload::Started { thread, left: None },
                Err(e) => log::warn!("Vise: world data preload: {e}"),
            }
        }
        Preload::Idle => {}
        Preload::Started { left, .. } if on_world_screens => *left = None,
        Preload::Started { thread, left } => {
            let left = *left.get_or_insert_with(std::time::Instant::now);
            if left.elapsed() >= PRELOAD_KEPT && thread.is_finished() {
                vise_client::vanillagen::forget_shared_data();
                vise_client::structure_blocks::forget_loot_data();
                super::lan::forget_sync_data();
                vise_threads::release_free_memory();
                *preload = Preload::Idle;
                log::info!("Vise: preloaded world data dropped, no world opened");
            }
        }
    }
}

pub(super) fn world_data_used() {
    *PRELOAD.lock().unwrap_or_else(|e| e.into_inner()) = Preload::Used;
}

pub(super) fn preload_server_loot(resources: &std::path::Path) {
    world_data_used();
    if vise_client::structure_blocks::loaded_loot_data().is_some() {
        return;
    }
    let resources = resources.to_owned();
    let spawned = std::thread::Builder::new()
        .name("Loot preload".into())
        .spawn(move || {
            vise_threads::behind_server();
            let _ = vise_client::structure_blocks::loot_data(
                &resources,
                &vise_world::loot::PackOverlay::NONE,
            );
        });
    if let Err(e) = spawned {
        log::warn!("Vise: loot preload: {e}");
    }
}

fn world_worldgen_overrides(
    world_root: &std::path::Path,
    biomes: &vise_client::biome::Biomes,
) -> vise_world::worldgen::overrides::WorldgenOverrides {
    let packs = vise_client::datapack::discover(world_root);
    let enabled = vise_client::datapack::enabled_ids(world_root, &packs);
    let enabled: Vec<_> = enabled
        .iter()
        .filter_map(|id| packs.iter().find(|p| &p.id == id))
        .collect();
    let (overrides, warnings) =
        vise_client::datapack::worldgen_overrides(&enabled, |b| biomes.contains(b));
    for warning in warnings {
        log::warn!("Vise: data pack worldgen: {warning}");
    }
    overrides
}

pub(super) fn level_shapes(
    world_root: &std::path::Path,
    biomes: &vise_client::biome::Biomes,
) -> vise_client::chunk::LevelShapes {
    use vise_client::chunk::{LevelShape, LevelShapes};
    let packs = vise_client::datapack::discover(world_root);
    let enabled = vise_client::datapack::enabled_ids(world_root, &packs);
    let enabled: Vec<_> = enabled
        .iter()
        .filter_map(|id| packs.iter().find(|p| &p.id == id))
        .collect();
    let mut shapes = LevelShapes::VANILLA;
    if enabled.is_empty() {
        return shapes;
    }
    let (overrides, _) =
        vise_client::datapack::worldgen_overrides(&enabled, |b| biomes.contains(b));
    for level in [
        "minecraft:overworld",
        "minecraft:the_nether",
        "minecraft:the_end",
    ] {
        let shape = overrides.level_height(level).and_then(|h| {
            LevelShape::new(h.min_y, h.height, h.logical_height, h.has_skylight).ok()
        });
        match shape {
            Some(shape) => *shapes.get_mut(level) = shape,
            None => log::warn!(
                "Vise: data pack worldgen: the dimension type of {level} is not one the \
                 game accepts; the level keeps the vanilla build height"
            ),
        }
    }
    shapes
}

pub(super) fn make_streaming(
    res: &Resources,
    pack: &PackResources,
    options: &Options,
    settings: &Settings,
    world_root: &std::path::Path,
    dim: Dim,
    shape: vise_client::chunk::LevelShape,
    seed: i64,
) -> Result<(Streaming, Arc<SaveDir>, Option<Arc<VanillaTerrain>>)> {
    let save = Arc::new(SaveDir::new(
        vise_client::save::dimension_dir(world_root, dim.id()),
        shape,
    ));
    let mut locator = None;
    let flat = (dim == Dim::Overworld)
        .then(|| vise_client::save::world_gen_settings(world_root))
        .flatten()
        .and_then(|s| {
            vise_client::flatgen::FlatTerrain::from_settings(
                &s,
                pack.assets.registry,
                res.biomes(),
                shape,
            )
        });
    let mut streaming = if let Some(inner) = flat {
        Streaming::new(
            Arc::new(Persistent {
                inner: Arc::new(inner),
                save: save.clone(),
                registry: pack.assets.clone(),
                biomes: res.biomes().clone(),
            }),
            pack.context.clone(),
            res.threads(),
        )
    } else if options.test_terrain && dim == Dim::Overworld {
        let inner = Arc::new(TestTerrain::new(pack.assets.registry, res.biomes())?);
        Streaming::new(
            Arc::new(Persistent {
                inner,
                save: save.clone(),
                registry: pack.assets.clone(),
                biomes: res.biomes().clone(),
            }),
            pack.context.clone(),
            res.threads(),
        )
    } else {
        let structures =
            vise_client::save::WorldGen::read(world_root).is_none_or(|w| w.structures);
        let preset = vise_client::save::WorldGen::preset(world_root);
        let overrides = world_worldgen_overrides(world_root, res.biomes());
        let inner = Arc::new(VanillaTerrain::for_world_with(&options.resources.join("worldgen"), &overrides, pack.assets.registry, res.biomes(), seed, preset, dim.id(), structures).map_err(|e| {
                if overrides.is_empty() {
                    format!("{e}\nPrepare the worldgen bundle with the launcher, or: vise-prepare --prepare")
                } else {
                    format!("{e}\nThe world's data packs change its world generation: one of them may not be made for this version")
                }
            })?);
        if dim == Dim::Overworld && structures {
            inner.search_rings_behind();
        }
        locator = Some(inner.clone());
        Streaming::new(
            Arc::new(Persistent {
                inner,
                save: save.clone(),
                registry: pack.assets.clone(),
                biomes: res.biomes().clone(),
            }),
            pack.context.clone(),
            res.threads(),
        )
    };
    streaming.forced = save.load_forced_chunks();
    streaming.radius = settings.render_distance;
    streaming.biome_blend = settings.biome_blend;
    streaming.smooth_lighting = settings.smooth_lighting;
    streaming.cutout_leaves = settings.cutout_leaves;
    streaming.biome_zoom = BiomeZoom::new(obfuscate_seed(seed));
    streaming.sky_light = dim != Dim::Nether;
    streaming.nether_shading = dim == Dim::Nether;
    streaming.mesh = options.execution_role.is_client();
    Ok((streaming, save, locator))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polling_does_not_wait_for_world_initialization() {
        let (release, wait) = std::sync::mpsc::channel();
        let loader = WorldLoader {
            cancelled: false,
            handle: std::thread::spawn(move || {
                wait.recv().unwrap();
                Err("test world failure".into())
            }),
        };
        assert!(!loader.ready());
        release.send(()).unwrap();
        assert_eq!(
            loader.finish().err().unwrap().to_string(),
            "test world failure"
        );
    }

    #[test]
    fn cancellation_keeps_initialization_tracked_until_it_has_stopped() {
        let (release, wait) = std::sync::mpsc::channel();
        let loader = WorldLoader {
            cancelled: false,
            handle: std::thread::spawn(move || {
                wait.recv().unwrap();
                Err("cancelled test world".into())
            }),
        }
        .cancel();
        assert!(loader.cancelled);
        assert!(!loader.ready());
        release.send(()).unwrap();
        assert_eq!(
            loader.finish().err().unwrap().to_string(),
            "world initialization cancelled"
        );
    }

    #[test]
    fn worker_panics_reach_the_client() {
        let loader = WorldLoader {
            cancelled: false,
            handle: std::thread::spawn(|| panic!("broken world initialization")),
        };
        assert_eq!(
            loader.finish().err().unwrap().to_string(),
            "world initialization worker panicked"
        );
    }

    #[test]
    fn prepared_world_can_cross_threads() {
        fn send<T: Send>() {}
        send::<PreparedWorld>();
    }
}
