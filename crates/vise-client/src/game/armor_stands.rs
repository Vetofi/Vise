use super::Game;
use vise_client::{
    entities::ArmorStand,
    items::ItemStack,
    mesher::Vertex,
    mobs::{Mob, Part, anim::Poses},
    trims::{TrimOverride, TrimRegistry, TrimTexture},
};
use vise_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

fn layer_color(undyed: Option<u32>, stack: Option<&ItemStack>) -> u32 {
    let dyed = stack
        .and_then(|s| s.data())
        .and_then(|d| d.dyed_color)
        .map(|c| c as u32 & 0xFF_FFFF);
    undyed.map_or(0xFFFF_FFFF, |undyed| {
        let c = dyed.unwrap_or(undyed);
        0xFF00_0000 | (c & 0xFF) << 16 | (c & 0xFF00) | (c >> 16) & 0xFF
    })
}

pub(super) struct ArmorStandAssets {
    pub main: Option<Part>,
    pub small: Option<Part>,
    pub layers: HashMap<(&'static str, bool), Part>,
    pub player: HashMap<&'static str, Part>,
    pub equipment: HashMap<String, HashMap<String, Vec<(String, Option<u32>)>>>,
    pub trim_overrides: HashMap<String, Vec<TrimOverride>>,
    pub trims: TrimRegistry,
    pub root: PathBuf,
}
impl ArmorStandAssets {
    pub fn load(
        root: &std::path::Path,
        document: Option<&vise_client::mobs::EntityModelDocument>,
    ) -> Self {
        let load = |key: &str| document.and_then(|document| document.model(key));
        let mut layers = HashMap::new();
        let mut player = HashMap::new();
        for slot in ["helmet", "chestplate", "leggings", "boots"] {
            if let Some(part) = load(&format!("minecraft:player#{slot}")) {
                player.insert(slot, part);
            }
            for (small, model) in [(false, "armor_stand"), (true, "armor_stand_small")] {
                if let Some(part) = load(&format!("minecraft:{model}#{slot}")) {
                    layers.insert((slot, small), part);
                }
            }
        }
        for (key, name) in [
            ("minecraft:elytra#main", "elytra"),
            ("minecraft:elytra_baby#main", "elytra_baby"),
        ] {
            if let Some(part) = load(key) {
                player.insert(name, part);
            }
        }
        let mut equipment = HashMap::new();
        let mut trim_overrides = HashMap::new();
        let namespaces = std::iter::once("minecraft".to_owned())
            .chain(vise_client::mod_content::namespaces(root));
        for namespace in namespaces {
            let Ok(entries) = std::fs::read_dir(root.join(format!("assets/{namespace}/equipment")))
            else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                let Ok(v) =
                    std::fs::read(&path).map(|b| serde_json::from_slice::<serde_json::Value>(&b))
                else {
                    continue;
                };
                let Ok(v) = v else { continue };
                let id = format!("{namespace}:{name}");
                let mut kinds = HashMap::new();
                for kind in ["humanoid", "humanoid_leggings", "wings"] {
                    let layers: Vec<(String, Option<u32>)> = v["layers"][kind]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|layer| {
                            let tex = layer["texture"].as_str().unwrap_or(&id);
                            let (tex_namespace, tex) =
                                tex.split_once(':').unwrap_or(("minecraft", tex));
                            let sprite = match tex_namespace {
                                "minecraft" => format!("entity/equipment/{kind}/{tex}"),
                                other => format!("{other}:entity/equipment/{kind}/{tex}"),
                            };
                            let undyed = layer["dyeable"]["color_when_undyed"]
                                .as_i64()
                                .map(|c| c as u32 & 0xFF_FFFF);
                            (sprite, undyed)
                        })
                        .collect();
                    if !layers.is_empty() {
                        kinds.insert(kind.to_owned(), layers);
                    }
                }
                trim_overrides.insert(id.clone(), TrimOverride::list(&v));
                equipment.insert(id, kinds);
            }
        }
        Self {
            main: load("minecraft:armor_stand#main"),
            small: load("minecraft:armor_stand_small#main"),
            layers,
            player,
            equipment,
            trim_overrides,
            trims: TrimRegistry::load(root),
            root: root.to_owned(),
        }
    }
    pub fn reload_trims(&mut self, root: &std::path::Path) {
        let fresh = Self::load(root, None);
        self.trim_overrides = fresh.trim_overrides;
        self.trims = fresh.trims;
        self.root = fresh.root;
    }
    fn trim_texture(&self, asset: &str, kind: &str, stack: &ItemStack) -> Option<TrimTexture> {
        let (material, pattern) = stack.extra.as_ref()?.trim.as_ref()?;
        let overrides = self
            .trim_overrides
            .get(asset)
            .map_or(&[][..], Vec::as_slice);
        TrimTexture::resolve(&self.trims, overrides, material, pattern, kind)
    }
}
impl Game<'_> {
    pub(super) fn player_armor_layers(&self) -> Vec<(&Part, [f32; 4], u32, u32)> {
        self.stack_armor_layers(&self.armor)
    }
    pub(super) fn stack_armor_layers(
        &self,
        armor: &[Option<ItemStack>; 4],
    ) -> Vec<(&Part, [f32; 4], u32, u32)> {
        self.armor_layers(
            armor
                .each_ref()
                .map(|s| s.as_ref().map(|s| (&*s.item, Some(s)))),
        )
    }
    pub(super) fn worn_armor_layers(&self, m: &Mob) -> Vec<(&Part, [f32; 4], u32, u32)> {
        let e = &m.extra;
        self.armor_layers(
            [
                (e.head.as_deref(), e.armor[0].as_ref()),
                (e.chest.as_deref(), e.armor[1].as_ref()),
                (e.legs.as_deref(), e.armor[2].as_ref()),
                (e.feet.as_deref(), e.armor[3].as_ref()),
            ]
            .map(|(item, stack)| item.map(|i| (i, stack))),
        )
    }
    fn armor_layers(
        &self,
        worn: [Option<(&str, Option<&ItemStack>)>; 4],
    ) -> Vec<(&Part, [f32; 4], u32, u32)> {
        let assets = &self.armor_stand_assets;
        let mut out = Vec::new();
        for (item, layer) in worn
            .into_iter()
            .zip(["helmet", "chestplate", "leggings", "boots"])
        {
            let Some((item, stack)) = item else {
                continue;
            };
            let Some(asset) = self
                .res
                .items()
                .get(item)
                .and_then(|i| i.equip_asset.clone())
            else {
                continue;
            };
            let kind = if layer == "leggings" {
                "humanoid_leggings"
            } else {
                "humanoid"
            };
            let Some(layers) = assets.equipment.get(&asset).and_then(|k| k.get(kind)) else {
                continue;
            };
            let Some(part) = assets.player.get(layer) else {
                continue;
            };
            let item_glint = self
                .res
                .items()
                .enchantments
                .props
                .get(item)
                .is_some_and(|p| p.glint);
            let overlay = if stack.is_some_and(|s| s.has_foil(item_glint)) {
                vise_client::mobs::render::ARMOR_GLINT
            } else {
                0
            };
            for (i, (texture, undyed)) in layers.iter().enumerate() {
                let Some(r) = self.gui.atlas.sprites.get(texture.as_str()) else {
                    continue;
                };
                let color = layer_color(*undyed, stack);
                out.push((
                    part,
                    [r.u0, r.v0, r.u1, r.v1],
                    color,
                    if i == 0 { overlay } else { 0 },
                ));
            }
            if let Some(r) = stack
                .and_then(|s| assets.trim_texture(&asset, kind, s))
                .and_then(|t| self.gui.atlas.sprites.get(&t.sprite()))
            {
                out.push((part, [r.u0, r.v0, r.u1, r.v1], 0xFFFF_FFFF, overlay));
            }
        }
        out
    }
    pub(super) fn prepare_trims(&mut self) {
        use vise_client::hud::TRIM_SLOTS;
        let assets = &self.armor_stand_assets;
        let items = self.items();
        let mut wanted: Vec<TrimTexture> = Vec::new();
        let guests = self.lan.iter().flat_map(|host| host.hub.guests.iter());
        let stacks = self
            .armor
            .iter()
            .chain(guests.flat_map(|g| g.armor.iter()))
            .chain(self.mobs.iter().flat_map(|m| m.extra.armor.iter()))
            .zip(
                ["helmet", "chestplate", "leggings", "boots"]
                    .into_iter()
                    .cycle(),
            )
            .chain(self.armor_stands.iter().flat_map(|a| {
                [5usize, 4, 3, 2]
                    .map(|slot| &a.equipment[slot])
                    .into_iter()
                    .zip(["helmet", "chestplate", "leggings", "boots"])
            }));
        for (stack, layer) in stacks {
            let Some(stack) = stack.as_ref().filter(|s| s.extra.is_some()) else {
                continue;
            };
            let Some(asset) = items
                .get(&stack.item)
                .and_then(|i| i.equip_asset.as_deref())
            else {
                continue;
            };
            let kind = if layer == "leggings" {
                "humanoid_leggings"
            } else {
                "humanoid"
            };
            if let Some(t) = assets.trim_texture(asset, kind, stack)
                && !wanted.contains(&t)
            {
                wanted.push(t);
            }
        }
        for t in wanted {
            let name = t.sprite();
            let sprites = &self.gui.atlas.sprites;
            self.trim_slots.retain(|(n, _)| sprites.contains_key(n));
            if let Some(i) = self.trim_slots.iter().position(|e| e.0 == name) {
                let e = self.trim_slots.remove(i);
                self.trim_slots.push(e);
                continue;
            }
            let Some((w, h, rgba)) = t.generate(&self.armor_stand_assets.root) else {
                continue;
            };
            let used: Vec<usize> = self.trim_slots.iter().map(|e| e.1).collect();
            let slot = match (0..TRIM_SLOTS).find(|i| !used.contains(i)) {
                Some(free) => free,
                None => {
                    let (old, slot) = self.trim_slots.remove(0);
                    self.gui.atlas.sprites.remove(&old);
                    slot
                }
            };
            let Some(area) = self
                .gui
                .atlas
                .sprites
                .get(&format!("vise/trim{slot}"))
                .copied()
            else {
                continue;
            };
            if w > area.w || h > area.h {
                continue;
            }
            let (aw, ah) = (self.gui.atlas.width as f32, self.gui.atlas.height as f32);
            let (x, y) = ((area.u0 * aw).round(), (area.v0 * ah).round());
            self.map_patches.push(vise_client::font::Patch {
                x: x as u32,
                y: y as u32,
                width: w,
                height: h,
                rgba,
            });
            let rect = vise_client::hud::Rect {
                u0: area.u0,
                v0: area.v0,
                u1: (x + w as f32) / aw,
                v1: (y + h as f32) / ah,
                w,
                h,
            };
            self.gui.atlas.sprites.insert(name.clone(), rect);
            self.trim_slots.push((name, slot));
        }
    }
    pub(super) fn armor_stand_spawn(
        &self,
        hit: &super::Hit,
        stack: &ItemStack,
    ) -> Option<([i32; 3], ArmorStand)> {
        if hit.face == [0, -1, 0] {
            return None;
        }
        let clicked = self
            .res
            .facts()
            .get(self.world.block(hit.pos[0], hit.pos[1], hit.pos[2]));
        let pos = if clicked.replaceable {
            hit.pos
        } else {
            std::array::from_fn(|i| hit.pos[i] + hit.face[i])
        };
        let foot = Vec3::new(
            f64::from(pos[0]) + 0.5,
            f64::from(pos[1]),
            f64::from(pos[2]) + 0.5,
        );
        let yaw =
            (((self.yaw - 180.0 + 180.0).rem_euclid(360.0) - 180.0 + 22.5) / 45.0).floor() * 45.0;
        let mut stand = ArmorStand::new(self.next_entity + 1, foot, yaw);
        if let Some(name) = stack.custom_name() {
            stand.custom_name = Some(vise_client::text::Text::literal(name));
        }
        let b = stand.bounds();
        let (lo, hi) = (b.min_position(), b.max_position());
        let e = 1.0e-7;
        let bounds = Aabb::new(
            Vec3::new(lo.x + e, lo.y + e, lo.z + e),
            Vec3::new(hi.x - e, hi.y - e, hi.z - e),
        );
        let mut scene = vise_client::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        if !scene.unobstructed(bounds).unwrap_or(false)
            || self
                .armor_stands
                .iter()
                .any(|o| overlaps(o.bounds(), bounds))
        {
            return None;
        }
        Some((pos, stand))
    }
    pub(super) fn place_armor_stand(&mut self, hit: &super::Hit, stack: &ItemStack) -> bool {
        if &*stack.item != "minecraft:armor_stand" {
            return false;
        }
        let Some((pos, stand)) = self.armor_stand_spawn(hit, stack) else {
            return false;
        };
        let foot = stand.position;
        self.next_entity += 1;
        self.audio.play(
            "minecraft:entity.armor_stand.place",
            Some([foot.x, foot.y, foot.z]),
            0.75,
            0.8,
        );
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
        self.armor_stands.push(stand);
        if !self.creative {
            self.consume_held();
        }
        self.swing_arm();
        true
    }
    pub(super) fn armor_stand_hit(&self) -> Option<(usize, f64, [f64; 3])> {
        let (eye, dir) = self.aim()?;
        let reach: f64 = if self.creative { 5.0 } else { 3.0 };
        let mut best: Option<(usize, f64, [f64; 3])> = None;
        for (i, a) in self.armor_stands.iter().enumerate() {
            if a.marker {
                continue;
            }
            let b = a.bounds();
            let (lo, hi) = (b.min_position(), b.max_position());
            let (lo, hi) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
            let (mut t0, mut t1) = (0.0f64, reach);
            let mut hit = true;
            for k in 0..3 {
                if dir[k].abs() < 1e-9 {
                    hit &= eye[k] >= lo[k] && eye[k] <= hi[k];
                    continue;
                }
                let (mut ta, mut tb) = ((lo[k] - eye[k]) / dir[k], (hi[k] - eye[k]) / dir[k]);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
            }
            if hit && t0 <= t1 && best.is_none_or(|b| t0 < b.1) {
                best = Some((i, t0, std::array::from_fn(|k| eye[k] + dir[k] * t0)));
            }
        }
        best
    }
    fn equipment_slot(&self, stack: &ItemStack) -> usize {
        match self
            .res
            .items()
            .get(&stack.item)
            .and_then(|i| i.equip_slot.as_deref())
        {
            Some("feet") => 2,
            Some("legs") => 3,
            Some("chest") => 4,
            Some("head") => 5,
            Some("offhand") => 1,
            _ => 0,
        }
    }
    pub(super) fn interact_armor_stand(&mut self) -> bool {
        let Some((i, d, point)) = self.armor_stand_hit() else {
            return false;
        };
        if !self.before_block(d) {
            return false;
        }
        let held = self.held().cloned();
        if held
            .as_ref()
            .is_some_and(|h| &*h.item == "minecraft:name_tag")
        {
            return false;
        }
        let a = &self.armor_stands[i];
        let slot = match &held {
            Some(h) => {
                let slot = self.equipment_slot(h);
                if slot < 2 && !a.show_arms {
                    return true;
                }
                slot
            }
            None => {
                let y = (point[1] - a.position.y) / if a.small { 0.5 } else { 1.0 };
                let has = |s: usize| a.equipment[s].is_some();
                let small = a.small;
                if y >= 0.1 && y < 0.1 + if small { 0.8 } else { 0.45 } && has(2) {
                    2
                } else if y >= 0.9 + if small { 0.3 } else { 0.0 }
                    && y < 0.9 + if small { 1.0 } else { 0.7 }
                    && has(4)
                {
                    4
                } else if y >= 0.4 && y < 0.4 + if small { 1.0 } else { 0.8 } && has(3) {
                    3
                } else if y >= 1.6 && has(5) {
                    5
                } else if !has(0) && has(1) {
                    1
                } else {
                    0
                }
            }
        };
        let current = self.armor_stands[i].equipment[slot].clone();
        if held.is_none() && current.is_none() {
            return false;
        }
        match held {
            Some(h) if self.creative && current.is_none() => {
                self.armor_stands[i].equipment[slot] = Some(ItemStack { count: 1, ..h });
            }
            Some(h) if h.count > 1 => {
                if current.is_some() {
                    return true;
                }
                self.armor_stands[i].equipment[slot] = Some(ItemStack {
                    count: 1,
                    ..h.clone()
                });
                *self.held_mut() = Some(ItemStack {
                    count: h.count - 1,
                    ..h
                });
            }
            h => {
                self.armor_stands[i].equipment[slot] = h;
                *self.held_mut() = current;
            }
        }
        let p = self.armor_stands[i].position;
        self.world
            .modified
            .insert(((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4));
        self.swing_arm();
        true
    }
    pub(super) fn attack_armor_stand(&mut self) -> bool {
        let Some((i, d, _)) = self.armor_stand_hit() else {
            return false;
        };
        if !self.before_block(d) || self.mob_hit().is_some_and(|m| m.1 < d) {
            return false;
        }
        let (area, id) = (self.armor_stands[i].bounds(), self.armor_stands[i].id);
        let (_, _, _, magic) = self.melee_damage("minecraft:armor_stand", true);
        let hurt = self.punch_armor_stand(i);
        if hurt {
            self.attack_effects(
                area,
                Some(id as i32),
                super::mace::Spared::Stand(id),
                magic.then_some(id as i32),
            );
            self.swing_arm();
        }
        hurt
    }
    fn show_breaking_particles(&mut self, at: [f64; 3], width: f64, height: f64) {
        let Some(state) = self
            .pack
            .assets
            .registry
            .default_state("minecraft:oak_planks")
        else {
            return;
        };
        let block = [
            at[0].floor() as i32,
            at[1].floor() as i32,
            at[2].floor() as i32,
        ];
        let Some((sprite, tint)) = self.particle_sprite(state, block) else {
            return;
        };
        let options = vise_client::particles::ParticleOptions::block(
            vise_client::particles::ParticleType::Block,
            sprite,
            tint,
        );
        let centre = [at[0], at[1] + height * (2.0 / 3.0), at[2]];
        let spread = [width / 4.0, height / 4.0, width / 4.0];
        self.particles
            .send_particles(&options, centre, spread, 0.05, 10);
        self.lan_level_particles(&options, centre, spread, 0.05, 10, Some(state));
    }
    pub(super) fn punch_armor_stand(&mut self, i: usize) -> bool {
        let time = self.world.game_time;
        let at = self.armor_stands[i].position;
        let at = [at.x, at.y, at.z];
        if self.creative {
            let (width, height) = stand_box(&self.armor_stands[i]);
            self.armor_stands.remove(i);
            self.audio
                .play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
            self.show_breaking_particles(at, f64::from(width), f64::from(height));
            return true;
        }
        if time - self.armor_stands[i].last_hit > 5 {
            self.armor_stands[i].last_hit = time;
            self.audio
                .play("minecraft:entity.armor_stand.hit", Some(at), 0.3, 1.0);
            return true;
        }
        let a = self.armor_stands.remove(i);
        let (width, height) = stand_box(&a);
        self.audio
            .play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        self.show_breaking_particles(at, f64::from(width), f64::from(height));
        let center = [at[0], at[1] + if a.small { 0.4875 } else { 0.9875 }, at[2]];
        let stand = ItemStack {
            item: Arc::from("minecraft:armor_stand"),
            count: 1,
            damage: 0,
            extra: None,
        };
        self.spawn_item(stand, center, [0.0; 3], 10);
        for stack in a.equipment.into_iter().flatten() {
            let r = &mut self.world.random;
            let v = [
                f64::from(r.next_float()) * 0.2 - 0.1,
                0.2,
                f64::from(r.next_float()) * 0.2 - 0.1,
            ];
            self.spawn_item(stack, center, v, 10);
        }
        self.world
            .modified
            .insert(((at[0].floor() as i32) >> 4, (at[2].floor() as i32) >> 4));
        true
    }
    pub(super) fn tick_armor_stands(&mut self) {
        let mut scene = vise_client::collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: None,
        };
        let mut dead = Vec::new();
        let ticking = self.entity_ticking_area();
        for (i, a) in self.armor_stands.iter_mut().enumerate() {
            if !ticking(a.position.x.floor() as i32, a.position.z.floor() as i32) {
                continue;
            }
            let in_water = a.in_water(&self.world, &self.res.motion().fluids);
            let (world, registry, facts, collision) = (
                &self.world,
                self.pack.assets.registry,
                self.res.facts(),
                self.collision,
            );
            a.tick(&mut scene, in_water, &mut |bounds, velocity| {
                vise_client::motion::bubble_column_push(
                    world, registry, facts, collision, bounds, velocity,
                );
            });
            if !a.environment_tick(&self.world, self.res.facts(), self.pack.assets.registry) {
                dead.push(i);
            }
        }
        for i in dead.into_iter().rev() {
            self.break_armor_stand_by_anything(i);
        }
    }
    pub(super) fn break_armor_stand_by_anything(&mut self, i: usize) {
        let a = self.armor_stands.remove(i);
        let at = [a.position.x, a.position.y, a.position.z];
        let (pos, drops) = broken_armor_stand_equipment(a);
        for stack in drops {
            self.pop_resource(pos, stack);
        }
        self.audio
            .play("minecraft:entity.armor_stand.break", Some(at), 1.0, 1.0);
        self.world.modified.insert((pos[0] >> 4, pos[2] >> 4));
    }
    pub(super) fn armor_stand_geometry(
        &self,
        camera: glam::DVec3,
        alpha: f64,
        out: &mut Vec<Vertex>,
    ) {
        let assets = &self.armor_stand_assets;
        let sprites = &self.gui.atlas.sprites;
        let uv_of = |name: &str| sprites.get(name).map(|r| [r.u0, r.v0, r.u1, r.v1]);
        let Some(wood) = uv_of("entity/armorstand/armorstand") else {
            return;
        };
        for a in &self.armor_stands {
            let entity_start = out.len();
            let (width, height) = match (a.marker, a.small) {
                (true, _) => (0.0, 0.0),
                (false, true) => (0.25, 0.9875),
                (false, false) => (0.5, 1.975),
            };
            let edge = (width + height + width) / 3.0 * 4.0;
            let range = if edge == 0.0 { 4.0 } else { edge } * 64.0;
            let dragon = a.equipment[5]
                .as_ref()
                .is_some_and(|i| &*i.item == "minecraft:dragon_head");
            let position = super::entity_culling::at(a.position);
            if !self.standing_entity_in_view(
                camera,
                position,
                position,
                alpha,
                width,
                height,
                range,
                if dragon { 0.5 } else { 0.0 },
            ) {
                continue;
            }
            let Some(model) = (if a.small { &assets.small } else { &assets.main }) else {
                continue;
            };
            let p = glam::DVec3::new(a.position.x, a.position.y, a.position.z) - camera;
            let wiggle = (self.world.game_time - a.last_hit) as f32 + alpha as f32;
            let wobble = if wiggle < 5.0 {
                (wiggle / 1.5 * std::f32::consts::PI).sin() * 3.0
            } else {
                0.0
            };
            let base = glam::Mat4::from_translation(p.as_vec3())
                * glam::Mat4::from_rotation_y((180.0 - a.yaw + wobble).to_radians())
                * glam::Mat4::from_scale(glam::Vec3::new(-1.0, -1.0, 1.0))
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, -1.501, 0.0));
            let mut poses = Poses::new(model);
            apply_pose(&mut poses, &a.pose);
            let posed = poses.clone();
            let mut hide = |name: &str| {
                if poses.has(name) {
                    let pose = poses.get(name);
                    pose[6] = 0.0;
                    pose[7] = 0.0;
                    pose[8] = 0.0;
                }
            };
            if !a.show_arms {
                hide("left_arm");
                hide("right_arm");
            }
            if a.no_base_plate {
                hide("base_plate");
            }
            let light = self.world.light(
                a.position.x.floor() as i32,
                (a.position.y + 0.5).floor() as i32,
                a.position.z.floor() as i32,
            );
            let packed = (u32::from(light >> 4) * 16) | ((u32::from(light & 15) * 16) << 8);
            if !a.invisible {
                vise_client::mobs::render::draw(
                    model,
                    base,
                    &poses,
                    wood,
                    0xFFFF_FFFF,
                    packed,
                    0,
                    out,
                );
            }
            self.armor_stand_items(a, model, base, &posed, packed, out);
            for (slot, layer) in [
                (5usize, "helmet"),
                (4, "chestplate"),
                (3, "leggings"),
                (2, "boots"),
            ] {
                let Some(stack) = &a.equipment[slot] else {
                    continue;
                };
                let Some(asset) = self
                    .res
                    .items()
                    .get(&stack.item)
                    .and_then(|i| i.equip_asset.clone())
                else {
                    continue;
                };
                let kind = if layer == "leggings" {
                    "humanoid_leggings"
                } else {
                    "humanoid"
                };
                let Some(layers) = assets.equipment.get(&asset).and_then(|k| k.get(kind)) else {
                    continue;
                };
                let Some(part) = assets.layers.get(&(layer, a.small)) else {
                    continue;
                };
                let item_glint = self
                    .res
                    .items()
                    .enchantments
                    .props
                    .get(&stack.item)
                    .is_some_and(|p| p.glint);
                let overlay = if stack.has_foil(item_glint) {
                    vise_client::mobs::render::ARMOR_GLINT
                } else {
                    0
                };
                let mut layer_poses = Poses::new(part);
                apply_pose(&mut layer_poses, &a.pose);
                for (i, (texture, undyed)) in layers.iter().enumerate() {
                    let Some(uv) = uv_of(texture) else {
                        continue;
                    };
                    let color = layer_color(*undyed, Some(stack));
                    vise_client::mobs::render::draw(
                        part,
                        base,
                        &layer_poses,
                        uv,
                        color,
                        packed,
                        if i == 0 { overlay } else { 0 },
                        out,
                    );
                }
                if let Some(trim) = assets
                    .trim_texture(&asset, kind, stack)
                    .and_then(|t| uv_of(&t.sprite()))
                {
                    vise_client::mobs::render::draw(
                        part,
                        base,
                        &layer_poses,
                        trim,
                        0xFFFF_FFFF,
                        packed,
                        overlay,
                        out,
                    );
                }
            }
            self.wings_geometry(
                a.equipment[4].as_ref().map(|s| &*s.item),
                None,
                a.small,
                base,
                vise_client::mobs::ElytraAnimation::RESTING,
                false,
                packed,
                self.wings_overlay(a.equipment[4].as_ref()),
                out,
            );
            self.cull_entity_vertices(out, entity_start);
        }
    }
}
fn apply_pose(poses: &mut Poses, pose: &[[f32; 3]; 6]) {
    let parts: [&[&str]; 6] = [
        &["head"],
        &[
            "body",
            "right_body_stick",
            "left_body_stick",
            "shoulder_stick",
        ],
        &["left_arm"],
        &["right_arm"],
        &["left_leg"],
        &["right_leg"],
    ];
    for (names, rotation) in parts.into_iter().zip(pose) {
        for name in names {
            if poses.has(name) {
                let part = poses.get(name);
                part[3] = rotation[0].to_radians();
                part[4] = rotation[1].to_radians();
                part[5] = rotation[2].to_radians();
            }
        }
    }
}
fn overlaps(a: Aabb, b: Aabb) -> bool {
    let (a0, a1, b0, b1) = (
        a.min_position(),
        a.max_position(),
        b.min_position(),
        b.max_position(),
    );
    a0.x < b1.x && b0.x < a1.x && a0.y < b1.y && b0.y < a1.y && a0.z < b1.z && b0.z < a1.z
}
impl Game<'_> {
    pub(super) fn equip_from_hand(&mut self, stack: &ItemStack) -> bool {
        let Some(info) = self.items().get(&stack.item) else {
            return false;
        };
        let (index, slot) = match info.equip_slot.as_deref() {
            Some("head") => (0, "head"),
            Some("chest") => (1, "chest"),
            Some("legs") => (2, "legs"),
            Some("feet") => (3, "feet"),
            _ => return false,
        };
        if !info.swappable {
            return false;
        }
        let current = self.armor[index].clone();
        if current.as_ref().is_some_and(|c| {
            (c.enchantment_level("minecraft:binding_curse") > 0 && !self.creative)
                || c.same_item_same_components(stack)
        }) {
            return true;
        }
        let equipped = ItemStack {
            count: 1,
            ..stack.clone()
        };
        self.armor[index] = Some(equipped.clone());
        self.player_equipped(slot, Some(&equipped));
        if !self.creative {
            let mut held = stack.clone();
            held.count -= 1;
            if held.count == 0 {
                *self.held_mut() = current;
            } else {
                *self.held_mut() = Some(held);
                if let Some(old) = current
                    && let Some(rest) = self.give(old)
                    && let Some(eye) = self.eye_position()
                {
                    self.spawn_item(rest, eye, [0.0; 3], 40);
                }
            }
        }
        self.apply_equipment();
        self.swing_arm();
        true
    }
}
impl Game<'_> {
    fn armor_stand_items(
        &self,
        a: &ArmorStand,
        model: &Part,
        base: glam::Mat4,
        poses: &Poses,
        light: u32,
        out: &mut Vec<Vertex>,
    ) {
        use vise_assets::item_model::DisplayContext;
        use vise_client::mobs::render::part_matrix;
        let owner = super::item_draw::owner_at(
            [a.position.x, a.position.y, a.position.z],
            a.yaw,
            "minecraft:armor_stand",
        );
        let (ox, oy, oz) = if a.small {
            (0.0, 1.0, -4.5)
        } else {
            (1.0, 2.0, -10.0)
        };
        for (slot, left, context) in [
            (0usize, false, DisplayContext::ThirdPersonRightHand),
            (1, true, DisplayContext::ThirdPersonLeftHand),
        ] {
            let Some(stack) = &a.equipment[slot] else {
                continue;
            };
            let arm = if left { "left_arm" } else { "right_arm" };
            let Some(hand) = part_matrix(model, base, poses, arm, 0.0) else {
                continue;
            };
            let pose = hand
                * glam::Mat4::from_rotation_x((-90.0f32).to_radians())
                * glam::Mat4::from_rotation_y(std::f32::consts::PI)
                * glam::Mat4::from_translation(glam::Vec3::new(
                    if left { -ox } else { ox } / 16.0,
                    oy / 16.0,
                    oz / 16.0,
                ));
            let seed = a.id as i32 + if left { 1 } else { 2 };
            let state = self.resolve_item(stack, context, Some(&owner), seed);
            self.submit_item_world(&state, pose, light, out);
        }
        if let Some(stack) = &a.equipment[5]
            && let Some(kind) = skull_kind(&stack.item)
            && let Some(head) = part_matrix(model, base, poses, "head", 0.0)
        {
            let pose = head * glam::Mat4::from_scale(glam::Vec3::splat(1.1875));
            let special = vise_assets::item_model::SpecialModel {
                kind: if kind == "player" {
                    vise_assets::item_model::SpecialKind::PlayerHead
                } else {
                    vise_assets::item_model::SpecialKind::Head {
                        kind: kind.to_owned(),
                        texture: None,
                        animation: 0.0,
                    }
                },
            };
            let mut sink = vise_client::item_render::WorldSink {
                out,
                light,
                lights: Some(vise_client::block_entity_render::diffuse_lights(
                    self.dim == super::Dim::Nether,
                )),
                glint: self.glint(),
            };
            let profile = stack
                .extra
                .as_deref()
                .and_then(|d| d.other.iter().find(|(k, _)| k == "minecraft:profile"))
                .map(|(_, t)| vise_client::scoreboard::tag_to_json(t))
                .unwrap_or(serde_json::Value::Null);
            self.pack.item_renderer.submit_special(
                &special,
                &profile,
                false,
                pose,
                &self.gui.atlas.sprites,
                &self.item_host(),
                &mut sink,
            );
            return;
        }
        if let Some(stack) = &a.equipment[5]
            && self
                .res
                .items()
                .get(&stack.item)
                .is_none_or(|i| i.equip_asset.is_none())
            && let Some(head) = part_matrix(model, base, poses, "head", 0.0)
        {
            let pose = head
                * glam::Mat4::from_translation(glam::Vec3::new(0.0, -0.25, 0.0))
                * glam::Mat4::from_rotation_y(std::f32::consts::PI)
                * glam::Mat4::from_scale(glam::Vec3::new(0.625, -0.625, -0.625));
            let state =
                self.resolve_item(stack, DisplayContext::Head, Some(&owner), a.id as i32 + 5);
            self.submit_item_world(&state, pose, light, out);
        }
    }
}
fn stand_box(a: &ArmorStand) -> (f32, f32) {
    match (a.marker, a.small) {
        (true, _) => (0.0, 0.0),
        (false, true) => (0.25, 0.9875),
        (false, false) => (0.5, 1.975),
    }
}
pub(super) fn skull_kind(item: &str) -> Option<&'static str> {
    Some(match item {
        "minecraft:skeleton_skull" => "skeleton",
        "minecraft:wither_skeleton_skull" => "wither_skeleton",
        "minecraft:zombie_head" => "zombie",
        "minecraft:creeper_head" => "creeper",
        "minecraft:dragon_head" => "dragon",
        "minecraft:piglin_head" => "piglin",
        "minecraft:player_head" => "player",
        _ => return None,
    })
}

pub(super) fn spared_by_blast(a: &ArmorStand) -> bool {
    a.invisible || a.marker
}

fn broken_armor_stand_equipment(a: ArmorStand) -> ([i32; 3], Vec<ItemStack>) {
    let pos = [
        a.position.x.floor() as i32,
        a.position.y.floor() as i32 + 1,
        a.position.z.floor() as i32,
    ];
    let drops = a
        .equipment
        .into_iter()
        .flatten()
        .filter(|stack| stack.enchantment_level("minecraft:vanishing_curse") == 0)
        .collect();
    (pos, drops)
}
#[cfg(test)]
mod burn_drop_tests {
    use super::*;
    #[test]
    fn only_visible_non_marker_stands_are_broken_by_blasts() {
        let mut stand = ArmorStand::new(1, Vec3::new(0.5, 64.0, 0.5), 0.0);
        assert!(!spared_by_blast(&stand));
        stand.invisible = true;
        assert!(spared_by_blast(&stand));
        stand.invisible = false;
        stand.marker = true;
        assert!(spared_by_blast(&stand));
    }
    #[test]
    fn burnt_equipment_drops_above_the_block_and_vanishing_items_do_not_drop() {
        let mut stand = ArmorStand::new(1, Vec3::new(-0.1, 4.8, 0.0), 0.0);
        let kept = ItemStack::new("minecraft:diamond_boots", 1);
        let mut vanished = ItemStack::new("minecraft:diamond_helmet", 1);
        vanished
            .data_mut()
            .enchantments
            .push(("minecraft:vanishing_curse".into(), 1));
        stand.equipment[0] = Some(kept.clone());
        stand.equipment[1] = Some(vanished);
        let (pos, drops) = broken_armor_stand_equipment(stand);
        assert_eq!(pos, [-1, 5, 0]);
        assert_eq!(drops, vec![kept]);
    }
}
#[cfg(test)]
mod layer_color_tests {
    use super::*;
    #[test]
    fn a_dyed_stack_overrides_the_undyed_leather_colour() {
        let mut dyed = ItemStack::new("minecraft:leather_helmet", 1);
        dyed.data_mut().dyed_color = Some(0x1E_90FF);
        assert_eq!(layer_color(Some(0xA06540), Some(&dyed)), 0xFFFF_901E);
        let plain = ItemStack::new("minecraft:leather_helmet", 1);
        assert_eq!(layer_color(Some(0xA06540), Some(&plain)), 0xFF40_65A0);
        assert_eq!(layer_color(None, Some(&plain)), 0xFFFF_FFFF);
        assert_eq!(layer_color(None, Some(&dyed)), 0xFFFF_FFFF);
    }
}
