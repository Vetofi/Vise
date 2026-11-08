use super::Game;
use vise_client::blocks::RenderShape;
use vise_client::mobs::{MobEvent, ambient};
use vise_client::motion::{LAVA, WATER};
use vise_client::particles::{ParticleEngine, ParticleOptions, ParticleType};
use vise_client::pose::Pose;
use vise_core::{geometry::Vec3, random::RandomSource};

impl Game<'_> {
    pub(super) fn tick_client_ambient(&mut self) {
        if self.serving.is_some() || self.options.dedicated.is_some() {
            return;
        }
        let mut events = Vec::new();
        let random = self.particles.level_random();
        for m in self.mobs.iter_mut().filter(|m| !m.removed) {
            ambient::client_ai_step(m, &mut events);
            ambient::client_particles(m, random, &mut events);
        }
        for event in events {
            match event {
                MobEvent::ParticleAt { kind, at, velocity } => {
                    if let Some(ty) = ParticleType::from_name(kind) {
                        self.particles
                            .add(&ParticleOptions::simple(ty), at, velocity);
                    }
                }
                MobEvent::LocalSound(name, at, volume, pitch) => {
                    self.audio
                        .play_unshared(&format!("minecraft:{name}"), Some(at), volume, pitch)
                }
                _ => {}
            }
        }
        self.eye_particles();
        self.panda_effects();
        self.sprint_dust();
        self.effect_swirls();
    }

    fn sprint_dust(&mut self) {
        let mut runners: Vec<(Vec3, Vec3, Option<[i32; 3]>)> = Vec::new();
        if let Some(p) = self.player.as_ref()
            && p.sprinting
            && !p.spectator
            && p.pose != Pose::Crouching
            && !p.health.dead_or_dying()
            && !p.fluids.was_touching_water
            && !p.is_in_lava()
            && p.fluid_height(WATER) <= 0.0
            && p.fluid_height(LAVA) <= 0.0
        {
            let support = p.ground.supporting_block.map(|b| [b.x, b.y, b.z]);
            runners.push((p.position, p.velocity, support));
        }
        let guests = self.lan.iter().flat_map(|host| host.models.values());
        for m in self.mobs.iter().chain(guests) {
            let feet = [
                m.position.x.floor() as i32,
                m.position.y.floor() as i32,
                m.position.z.floor() as i32,
            ];
            if m.extra.sprinting
                && !m.extra.crouching
                && m.alive()
                && &*m.kind.id == "minecraft:player"
                && self
                    .res
                    .facts()
                    .get(self.world.block(feet[0], feet[1], feet[2]))
                    .fluid_level
                    == 0
            {
                runners.push((m.position, m.position.subtract(m.previous), None));
            }
        }
        for (position, movement, support) in runners {
            let below = (position.y - f64::from(0.2f32)).floor() as i32;
            let tall = |game: &Self, s: [i32; 3]| {
                let name = game.block_name(s);
                [
                    "minecraft:fences",
                    "minecraft:walls",
                    "minecraft:fence_gates",
                ]
                .iter()
                .any(|tag| game.block_in_tag(tag, &name))
            };
            let at = match support {
                Some(s) if tall(self, s) => s,
                Some(s) => [s[0], below, s[2]],
                None => [position.x.floor() as i32, below, position.z.floor() as i32],
            };
            let state = self.world.block(at[0], at[1], at[2]);
            if self.res.facts().get(state).render_shape == RenderShape::Invisible {
                continue;
            }
            let Some((sprite, tint)) = self.particle_sprite(state, at) else {
                continue;
            };
            let width = f64::from(0.6f32);
            let r = self.particles.random();
            let mut x = position.x + (r.next_double() - 0.5) * width;
            let mut z = position.z + (r.next_double() - 0.5) * width;
            if position.x.floor() as i32 != at[0] {
                x = x.clamp(f64::from(at[0]), f64::from(at[0]) + 1.0);
            }
            if position.z.floor() as i32 != at[2] {
                z = z.clamp(f64::from(at[2]), f64::from(at[2]) + 1.0);
            }
            self.particles.add(
                &ParticleOptions::block(ParticleType::Block, sprite, tint),
                [x, position.y + 0.1, z],
                [movement.x * -4.0, 1.5, movement.z * -4.0],
            );
        }
    }

    fn effect_swirls(&mut self) {
        if let Some(p) = self.player.as_ref() {
            let effects = &self.potions.effects.list;
            let list: Vec<_> = effects
                .iter()
                .filter(|e| e.visible)
                .filter_map(|e| effect_particle(e.id, e.ambient))
                .collect();
            let ambient = effects.iter().filter(|e| e.visible).all(|e| e.ambient);
            let invisible = self.own_invisible();
            let size = (
                f64::from(p.dimensions.width),
                f64::from(p.dimensions.height),
            );
            swirl(
                &mut self.particles,
                p.position,
                size,
                &list,
                ambient,
                invisible,
            );
        }
        let guests = self.lan.iter().flat_map(|host| host.models.values());
        for m in self.mobs.iter().chain(guests) {
            if !m.alive() {
                continue;
            }
            let size = (f64::from(m.width()), f64::from(m.height()));
            let invisible = m.invisible || m.has_effect("minecraft:invisibility");
            if m.extra.effect_particles.is_empty() {
                if m.extra.effects.is_empty() {
                    continue;
                }
                let list: Vec<_> = m
                    .extra
                    .effects
                    .iter()
                    .filter(|e| !m.extra.hidden_effects.contains(&e.0))
                    .filter_map(|e| effect_particle(e.0, false))
                    .collect();
                swirl(
                    &mut self.particles,
                    m.position,
                    size,
                    &list,
                    false,
                    invisible,
                );
            } else {
                let list = &m.extra.effect_particles;
                let ambient = m.extra.effect_ambient;
                swirl(
                    &mut self.particles,
                    m.position,
                    size,
                    list,
                    ambient,
                    invisible,
                );
            }
        }
    }
}

pub(super) fn effect_particle(effect: &str, ambient: bool) -> Option<ParticleOptions> {
    Some(match effect.trim_start_matches("minecraft:") {
        "trial_omen" => ParticleOptions::simple(ParticleType::TrialOmen),
        "raid_omen" => ParticleOptions::simple(ParticleType::RaidOmen),
        other => {
            let color = vise_client::brewing::effect(other)?.color;
            let channel = |shift: u32| ((color >> shift) & 255) as f32 / 255.0;
            let alpha = if ambient { 38.0 / 255.0 } else { 1.0 };
            ParticleOptions::colored(
                ParticleType::EntityEffect,
                [channel(16), channel(8), channel(0), alpha],
            )
        }
    })
}

fn swirl(
    particles: &mut ParticleEngine,
    feet: Vec3,
    size: (f64, f64),
    list: &[ParticleOptions],
    ambient: bool,
    invisible: bool,
) {
    if list.is_empty() {
        return;
    }
    let bound = if invisible { 15 } else { 4 } * if ambient { 5 } else { 1 };
    let r = particles.random();
    let Ok(0) = r.next_int_bound(bound) else {
        return;
    };
    let pick = r.next_int_bound(list.len() as i32).unwrap_or_default() as usize;
    let x = feet.x + size.0 * (2.0 * r.next_double() - 1.0) * 0.5;
    let y = feet.y + size.1 * r.next_double();
    let z = feet.z + size.0 * (2.0 * r.next_double() - 1.0) * 0.5;
    particles.add(&list[pick], [x, y, z], [1.0; 3]);
}
