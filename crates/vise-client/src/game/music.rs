use super::Game;
use vise_client::{
    blocks::Fluid,
    music::{BackgroundMusic, Music, MusicFrequency, Situation},
    sound_engine::{SoundEngine, SoundInstance, Sounds, TickContext},
};

impl Game<'_> {
    pub(super) fn tick_music(&mut self) {
        let (music, volume) = self.situational_music();
        let frequency = MusicFrequency::parse(&self.settings.file.name("musicFrequency"));
        if frequency != self.music.frequency {
            self.music.set_frequency(frequency, music.as_ref());
        }
        let loading = self.title.is_none() && self.player.is_none() && self.win.is_none();
        self.music
            .tick(music.as_ref(), volume, loading, &mut self.audio);
    }
    pub(super) fn tick_sound_engine(&mut self) {
        if self.pause != self.sound_paused {
            if self.pause {
                self.audio.pause();
            } else {
                self.audio.resume();
            }
            self.sound_paused = self.pause;
        }
        let (vehicle, carts) = self.start_cart_sounds();
        let mobs = self.start_mob_sounds();
        let (player_position, player_speed_sqr, player_fall_flying) =
            self.player.as_ref().map_or(([0.0; 3], 0.0, false), |p| {
                let at = p.position;
                (
                    [at.x, at.y, at.z],
                    p.velocity.length_squared(),
                    p.fall_flying,
                )
            });
        if player_fall_flying && !self.was_fall_flying {
            let instance = SoundInstance::elytra(player_position, SoundEngine::unseeded());
            self.audio.play_instance(instance, false);
        }
        self.was_fall_flying = player_fall_flying;
        let ctx = TickContext {
            player_underwater: self.ambience.was_underwater(),
            player_removed: self.player.is_none(),
            vehicle,
            carts,
            mobs,
            frozen: !self.clock.runs_normally(),
            player_position,
            player_speed_sqr,
            player_fall_flying,
        };
        self.audio.tick(self.pause, &ctx);
    }
    pub(super) fn stop_sounds_for_new_level(&mut self) {
        self.ambience.clear(&mut self.audio);
        self.audio.stop_all();
    }
    pub(super) fn situational_music(&self) -> (Option<Music>, f32) {
        let empty = BackgroundMusic::default();
        let eye = if self.title.is_some() {
            None
        } else {
            self.eye_position().or_else(|| {
                self.remote
                    .as_ref()
                    .is_some_and(|r| r.logged_in)
                    .then_some([0.0; 3])
            })
        };
        let (background, probe_volume) = match eye {
            Some(eye) => {
                let (dimension, biome) = (self.dim.id(), self.biome_at(eye));
                let tables = self.res.music();
                (
                    tables.background(dimension, biome),
                    tables.volume(dimension, biome),
                )
            }
            None => (empty, 1.0),
        };
        let situation = Situation {
            screen_music: self
                .win
                .is_some()
                .then(vise_client::music::Music::credits),
            in_world: eye.is_some(),
            in_end: self.dim == super::Dim::End,
            boss_music: self.boss_music(),
            background: &background,
            probe_volume,
            creative: self.creative,
            underwater: self.ambience.was_underwater(),
        };
        (situation.music(), situation.volume())
    }
    fn biome_at(&self, p: [f64; 3]) -> &str {
        let (x, y, z) = (
            p[0].floor() as i32,
            p[1].floor() as i32,
            p[2].floor() as i32,
        );
        self.world
            .chunks
            .get(&(x >> 4, z >> 4))
            .map_or("minecraft:plains", |c| {
                self.res.biomes().list[c.biome(x, y, z) as usize]
                    .name
                    .as_str()
            })
    }
    pub(super) fn eye_in_water(&self, x: f64, eye_y: f64, z: f64) -> bool {
        let (bx, by, bz) = (x.floor() as i32, eye_y.floor() as i32, z.floor() as i32);
        let f = self.res.facts().get(self.world.block(bx, by, bz));
        if f.fluid != Fluid::Water {
            return false;
        }
        let above = self.res.facts().get(self.world.block(bx, by + 1, bz));
        let height = if above.fluid == Fluid::Water {
            1.0
        } else {
            f64::from(f.fluid_level) / 9.0
        };
        eye_y <= f64::from(by) + height
    }
}

impl Game<'_> {
    pub(super) fn tick_ambience(&mut self) {
        let Some(p) = self.player.as_ref() else {
            return;
        };
        let feet = [p.position.x, p.position.y, p.position.z];
        let eye_y = feet[1] + f64::from(p.dimensions.eye_height);
        let sounds = self.res.ambient().get(self.dim.id(), self.biome_at(feet));
        let underwater = self.eye_in_water(feet[0], eye_y, feet[2]);
        let world = &self.world;
        let light = |b: [i32; 3]| {
            let l = world.light(b[0], b[1], b[2]);
            (l >> 4, l & 15)
        };
        let (half, height) = (
            f64::from(p.dimensions.width / 2.0),
            f64::from(p.dimensions.height),
        );
        let registry = &self.pack.assets.registry;
        let facts = self.res.facts();
        let bubble_column = vise_client::ambience::bubble_column_in(
            [
                feet[0] - half,
                feet[1],
                feet[2] - half,
                feet[0] + half,
                feet[1] + height,
                feet[2] + half,
            ],
            |b| {
                let state = world.block(b[0], b[1], b[2]);
                (&*facts.get(state).name == "minecraft:bubble_column").then(|| {
                    registry
                        .state(state)
                        .and_then(|s| s.properties.get("drag"))
                        .is_some_and(|d| d == "true")
                })
            },
        );
        let ctx = vise_client::ambience::AmbientContext {
            sounds: &sounds,
            position: feet,
            eye_y,
            underwater,
            spectator: self.spectator(),
            light: &light,
            bubble_column,
        };
        self.ambience.tick(&ctx, &mut self.audio);
    }
}
