use crate::{
    blocks::{BlockFacts, Fluid},
    collisions::SceneCollisions,
    items::ItemStack,
    world::World,
};
use vise_core::geometry::{Aabb, Vec3};
use vise_world::block_collision::BlockCollisionRegistry;

mod fluids;

#[derive(Clone, Debug)]
pub struct ItemEntity {
    pub health: i32,
    pub fire_ticks: i32,
    pub id: u32,
    pub stack: ItemStack,
    pub thrower: Option<[i32; 4]>,
    pub target: Option<[i32; 4]>,
    pub position: Vec3,
    pub previous: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub age: u32,
    pub pickup_delay: u32,
    pub bob_offset: f32,
    pub ticks: u32,
}
#[derive(Debug)]
pub struct ItemEnvironmentTick {
    pub alive: bool,
    pub lava_hurt: bool,
}
impl ItemEnvironmentTick {
    pub fn should_play_lava_hurt_sound(&self, tick: u32) -> bool {
        self.lava_hurt && (!self.alive || tick.is_multiple_of(10))
    }
}
impl ItemEntity {
    pub const WIDTH: f64 = 0.25;
    pub fn bounds(&self) -> Aabb {
        let h = Self::WIDTH / 2.0;
        Aabb::new(
            Vec3::new(self.position.x - h, self.position.y, self.position.z - h),
            Vec3::new(
                self.position.x + h,
                self.position.y + Self::WIDTH,
                self.position.z + h,
            ),
        )
    }
    pub fn environment_tick(
        &mut self,
        world: &World,
        facts: &BlockFacts,
        registry: &vise_world::registry::BlockRegistry,
        items: &crate::items::Items,
    ) -> ItemEnvironmentTick {
        let at = world.block(
            self.position.x.floor() as i32,
            (self.position.y + 0.1).floor() as i32,
            self.position.z.floor() as i32,
        );
        let fluid = facts.get(at).fluid;
        let fire_immune = items.resists_fire(&self.stack, "minecraft:in_fire");
        if fluid == Fluid::Water || fire_immune {
            self.fire_ticks = 0;
        }
        if self.fire_ticks > 0 {
            if self.fire_ticks % 20 == 0
                && fluid != Fluid::Lava
                && !items.resists_fire(&self.stack, "minecraft:on_fire")
            {
                self.health -= 1;
            }
            self.fire_ticks -= 1;
        }
        let mut lava_hurt = false;
        if fluid == Fluid::Lava && !fire_immune {
            self.fire_ticks = self.fire_ticks.max(300);
            if !items.resists_fire(&self.stack, "minecraft:lava") {
                self.health -= 4;
                lava_hurt = true;
            }
        }
        let fire_damage = crate::motion::environment::fire_contact(world, registry, self.bounds());
        if fire_damage > 0.0 && !fire_immune {
            self.fire_ticks = self.fire_ticks.max(160);
            if !items.resists_fire(&self.stack, "minecraft:in_fire") {
                self.health -= fire_damage as i32;
            }
        }
        if crate::motion::environment::cactus_contact(world, registry, self.bounds())
            && !items.resists_cactus(&self.stack)
        {
            self.health -= 1;
        }
        ItemEnvironmentTick {
            alive: self.health > 0,
            lava_hurt,
        }
    }
    pub fn inside_block(&self, world: &World, collision: &BlockCollisionRegistry<'_>) -> bool {
        let mut scene = SceneCollisions {
            world,
            binding: collision,
            subject: None,
        };
        let inner = self.bounds().deflate(Vec3::new(1.0e-7, 1.0e-7, 1.0e-7));
        !crate::collisions::no_collision_empty(&mut scene, inner)
    }
    pub fn move_towards_closest_space(&mut self, world: &World, facts: &BlockFacts, speed: f32) {
        let bounds = self.bounds();
        let at = Vec3::new(
            self.position.x,
            (bounds.min_position().y + bounds.max_position().y) / 2.0,
            self.position.z,
        );
        let pos = [
            at.x.floor() as i32,
            at.y.floor() as i32,
            at.z.floor() as i32,
        ];
        let delta = [
            at.x - f64::from(pos[0]),
            at.y - f64::from(pos[1]),
            at.z - f64::from(pos[2]),
        ];
        let mut closest = (1, 1);
        let mut nearest = f64::MAX;
        for (axis, step) in [(2, -1), (2, 1), (0, -1), (0, 1), (1, 1)] {
            let mut n = pos;
            n[axis] += step;
            if !facts.get(world.block(n[0], n[1], n[2])).full_collision {
                let d = if step > 0 {
                    1.0 - delta[axis]
                } else {
                    delta[axis]
                };
                if d < nearest {
                    nearest = d;
                    closest = (axis, step);
                }
            }
        }
        let push = f64::from(closest.1 as f32 * speed);
        let v = self.velocity.scale(0.75);
        self.velocity = match closest.0 {
            0 => Vec3::new(push, v.y, v.z),
            1 => Vec3::new(v.x, push, v.z),
            _ => Vec3::new(v.x, v.y, push),
        };
    }
    pub fn tick(
        &mut self,
        world: &World,
        facts: &BlockFacts,
        collision: &BlockCollisionRegistry<'_>,
        no_physics: bool,
        fluids: &crate::motion::FluidResources,
    ) -> bool {
        self.ticks += 1;
        self.previous = self.position;
        if self.pickup_delay > 0 && self.pickup_delay != u32::MAX {
            self.pickup_delay -= 1;
        }
        let (water_height, lava_height) = self.update_fluids(world, fluids);
        let in_water = water_height > f64::from(0.1f32);
        if in_water {
            self.velocity = Vec3::new(
                self.velocity.x * 0.99,
                self.velocity.y + if self.velocity.y < 0.06 { 5.0e-4 } else { 0.0 },
                self.velocity.z * 0.99,
            );
        } else if lava_height > f64::from(0.1f32) {
            self.velocity = Vec3::new(
                self.velocity.x * 0.95,
                self.velocity.y + if self.velocity.y < 0.06 { 5.0e-4 } else { 0.0 },
                self.velocity.z * 0.95,
            );
        } else {
            self.velocity.y -= 0.04;
        }
        let horizontal = self.velocity.x * self.velocity.x + self.velocity.z * self.velocity.z;
        if !self.on_ground || horizontal > 1.0e-5 || (self.age + self.id).is_multiple_of(4) {
            let mut scene = SceneCollisions {
                world,
                binding: collision,
                subject: None,
            };
            let wanted = self.velocity;
            let moved = if no_physics {
                wanted
            } else {
                scene
                    .resolve(self.bounds(), wanted, 0.0, self.on_ground)
                    .unwrap_or(Vec3::ZERO)
            };
            self.position = Vec3::new(
                self.position.x + moved.x,
                self.position.y + moved.y,
                self.position.z + moved.z,
            );
            if !no_physics {
                self.on_ground = wanted.y < 0.0 && moved.y != wanted.y;
            }
            if moved.x != wanted.x {
                self.velocity.x = 0.0;
            }
            if moved.z != wanted.z {
                self.velocity.z = 0.0;
            }
            if moved.y != wanted.y {
                self.velocity.y = 0.0;
            }
            self.apply_honey_slide(world, facts);
            let mut friction = 0.98;
            if self.on_ground {
                let below = world.block(
                    self.position.x.floor() as i32,
                    (self.position.y - 0.999_999).floor() as i32,
                    self.position.z.floor() as i32,
                );
                friction = f64::from(facts.get(below).friction) * 0.98;
            }
            self.velocity = Vec3::new(
                self.velocity.x * friction,
                self.velocity.y * 0.98,
                self.velocity.z * friction,
            );
            if self.on_ground && self.velocity.y < 0.0 {
                self.velocity.y *= -0.5;
            }
        }
        self.age += 1;
        self.update_fluids(world, fluids);
        self.age < 6000 && self.position.y > -128.0
    }
    fn apply_honey_slide(&mut self, world: &World, facts: &BlockFacts) {
        let b = self.bounds().deflate(Vec3::new(1e-5, 1e-5, 1e-5));
        let (lo, hi) = (b.min_position(), b.max_position());
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..=hi.y.floor() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    if &*facts.get(world.block(x, y, z)).name == "minecraft:honey_block" {
                        self.honey_block_inside([x, y, z]);
                    }
                }
            }
        }
    }
    pub(crate) fn honey_block_inside(&mut self, pos: [i32; 3]) {
        let old_delta_y = |dy: f64| dy / f64::from(0.98_f32) + 0.08;
        let new_delta_y = |dy: f64| (dy - 0.08) * f64::from(0.98_f32);
        let [x, y, z] = pos.map(f64::from);
        if self.on_ground
            || self.position.y > y + 0.9375 - 1.0e-7
            || old_delta_y(self.velocity.y) >= -0.08
        {
            return;
        }
        let dx = (x + 0.5 - self.position.x).abs();
        let dz = (z + 0.5 - self.position.z).abs();
        let overlap = 0.4375 + Self::WIDTH / 2.0;
        if dx + 1.0e-7 <= overlap && dz + 1.0e-7 <= overlap {
            return;
        }
        let v = self.velocity;
        let old = old_delta_y(v.y);
        self.velocity = if old < -0.13 {
            let factor = -0.05 / old;
            Vec3::new(v.x * factor, new_delta_y(-0.05), v.z * factor)
        } else {
            Vec3::new(v.x, new_delta_y(-0.05), v.z)
        };
    }
    fn mergeable(&self, age: u32, max_stack: u32) -> bool {
        self.pickup_delay != u32::MAX && age < 6000 && self.stack.count < max_stack
    }
}
#[derive(Default)]
pub struct ItemCells {
    cells: crate::fast_hash::FxHashMap<(i32, i32, i32), Vec<u32>>,
}
impl ItemCells {
    pub fn new(items: &[ItemEntity]) -> Self {
        let mut cells = Self::default();
        for (i, item) in items.iter().enumerate() {
            cells.add(i, item.position);
        }
        cells
    }
    fn cell(p: Vec3) -> (i32, i32, i32) {
        (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32)
    }
    fn add(&mut self, i: usize, at: Vec3) {
        self.cells.entry(Self::cell(at)).or_default().push(i as u32);
    }
    pub fn remove(&mut self, i: usize, at: Vec3) {
        if let Some(list) = self.cells.get_mut(&Self::cell(at)) {
            list.retain(|&j| j as usize != i);
        }
    }
    pub fn moved(&mut self, i: usize, from: Vec3, to: Vec3) {
        if Self::cell(from) != Self::cell(to) {
            self.remove(i, from);
            self.add(i, to);
        }
    }
    fn near(&self, area: Aabb, out: &mut Vec<u32>) {
        out.clear();
        let (min, max) = (area.min_position(), area.max_position());
        let (lo, hi) = (
            Self::cell(Vec3::new(min.x - 0.125, min.y - 0.25, min.z - 0.125)),
            Self::cell(Vec3::new(max.x + 0.125, max.y, max.z + 0.125)),
        );
        for x in lo.0..=hi.0 {
            for y in lo.1..=hi.1 {
                for z in lo.2..=hi.2 {
                    if let Some(list) = self.cells.get(&(x, y, z)) {
                        out.extend_from_slice(list);
                    }
                }
            }
        }
        out.sort_unstable();
    }
}
pub fn merge_with_neighbours(
    items: &mut [ItemEntity],
    removed: &mut [bool],
    cells: &mut ItemCells,
    i: usize,
    max_stack: &dyn Fn(&ItemStack) -> u32,
) {
    let age = |items: &[ItemEntity], j: usize| {
        if j == i {
            items[j].age - 1
        } else {
            items[j].age
        }
    };
    if removed[i] || !items[i].mergeable(age(items, i), max_stack(&items[i].stack)) {
        return;
    }
    let area = items[i].bounds().inflate(Vec3::new(0.5, 0.0, 0.5));
    let mut near = Vec::new();
    cells.near(area, &mut near);
    for j in near {
        let j = j as usize;
        if j == i
            || removed[j]
            || !items[j].bounds().intersects(area)
            || !items[j].mergeable(age(items, j), max_stack(&items[j].stack))
        {
            continue;
        }
        let (this, other) = (&items[i].stack, &items[j].stack);
        if items[i].target != items[j].target
            || this.count + other.count > max_stack(other)
            || !this.same_item_same_components(other)
        {
            continue;
        }
        let (to, from) = if other.count < this.count {
            (i, j)
        } else {
            (j, i)
        };
        let room = max_stack(&items[to].stack).min(64) - items[to].stack.count;
        let moved = room.min(items[from].stack.count);
        items[to].stack.count += moved;
        items[from].stack.count -= moved;
        let (delay, from_age, to_age) =
            (items[from].pickup_delay, age(items, from), age(items, to));
        let to_item = &mut items[to];
        to_item.pickup_delay = to_item.pickup_delay.max(delay);
        to_item.age = to_age.min(from_age) + u32::from(to == i);
        if items[from].stack.count == 0 {
            removed[from] = true;
            cells.remove(from, items[from].position);
        }
        if removed[i] {
            return;
        }
    }
}
#[derive(Clone, Debug)]
pub struct ExperienceOrb {
    pub id: u32,
    pub position: Vec3,
    pub previous: Vec3,
    pub velocity: Vec3,
    pub value: u32,
    pub age: u32,
    pub on_ground: bool,
    pub count: u32,
    pub ticks: u32,
}
impl ExperienceOrb {
    pub fn bounds(&self) -> Aabb {
        let h = 0.25;
        Aabb::new(
            Vec3::new(self.position.x - h, self.position.y, self.position.z - h),
            Vec3::new(
                self.position.x + h,
                self.position.y + 0.5,
                self.position.z + h,
            ),
        )
    }
    pub fn can_merge(&self, id: u32, value: u32) -> bool {
        self.value == value && (self.id as i32).wrapping_sub(id as i32) % 40 == 0
    }
    pub fn split(value: u32) -> u32 {
        [2477, 1237, 617, 307, 149, 73, 37, 17, 7, 3, 1]
            .into_iter()
            .find(|&v| value >= v)
            .unwrap_or(1)
    }
    pub fn icon(&self) -> u32 {
        match self.value {
            2477.. => 10,
            1237.. => 9,
            617.. => 8,
            307.. => 7,
            149.. => 6,
            73.. => 5,
            37.. => 4,
            17.. => 3,
            7.. => 2,
            3.. => 1,
            _ => 0,
        }
    }
    pub fn tick(
        &mut self,
        world: &World,
        facts: &BlockFacts,
        collision: &BlockCollisionRegistry<'_>,
        player: Option<[f64; 3]>,
        next_float: &mut dyn FnMut() -> f32,
    ) -> bool {
        self.ticks += 1;
        self.previous = self.position;
        let block = world.block(
            self.position.x.floor() as i32,
            self.position.y.floor() as i32,
            self.position.z.floor() as i32,
        );
        if facts.get(block).fluid == Fluid::Water {
            self.velocity = Vec3::new(
                self.velocity.x * 0.99,
                (self.velocity.y + 5.0e-4).min(0.06),
                self.velocity.z * 0.99,
            );
        } else {
            self.velocity.y -= 0.03;
        }
        if facts.get(block).fluid == Fluid::Lava {
            self.velocity = Vec3::new(
                f64::from((next_float() - next_float()) * 0.2),
                f64::from(0.2f32),
                f64::from((next_float() - next_float()) * 0.2),
            );
        }
        if let Some(p) = player {
            let d = [
                (p[0] - self.position.x) / 8.0,
                (p[1] - self.position.y) / 8.0,
                (p[2] - self.position.z) / 8.0,
            ];
            let length = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let power = 1.0 - length;
            if power > 0.0 && length > 1e-6 {
                let power = power * power;
                self.velocity = Vec3::new(
                    self.velocity.x + d[0] / length * power * 0.1,
                    self.velocity.y + d[1] / length * power * 0.1,
                    self.velocity.z + d[2] / length * power * 0.1,
                );
            }
        }
        let bounds = self.bounds();
        let mut scene = SceneCollisions {
            world,
            binding: collision,
            subject: None,
        };
        let wanted = self.velocity;
        let moved = scene
            .resolve(bounds, wanted, 0.0, self.on_ground)
            .unwrap_or(Vec3::ZERO);
        self.position = Vec3::new(
            self.position.x + moved.x,
            self.position.y + moved.y,
            self.position.z + moved.z,
        );
        self.on_ground = wanted.y < 0.0 && moved.y != wanted.y;
        let mut friction = 0.98;
        if self.on_ground {
            let below = world.block(
                self.position.x.floor() as i32,
                (self.position.y - 1.0).floor() as i32,
                self.position.z.floor() as i32,
            );
            friction = f64::from(facts.get(below).friction) * 0.98;
        }
        self.velocity = Vec3::new(
            self.velocity.x * friction,
            self.velocity.y * 0.98,
            self.velocity.z * friction,
        );
        if self.on_ground {
            self.velocity.y *= -0.9;
        }
        self.age += 1;
        self.age < 6000
    }
}
pub fn merge_orbs(orbs: &mut Vec<ExperienceOrb>) {
    let mut removed = vec![false; orbs.len()];
    for i in 0..orbs.len() {
        if removed[i] || orbs[i].ticks % 20 != 1 {
            continue;
        }
        let area = orbs[i].bounds().inflate(Vec3::new(0.5, 0.5, 0.5));
        for j in 0..orbs.len() {
            if j != i
                && !removed[j]
                && orbs[j].can_merge(orbs[i].id, orbs[i].value)
                && orbs[j].bounds().intersects(area)
            {
                orbs[i].count += orbs[j].count;
                orbs[i].age = orbs[i].age.min(orbs[j].age);
                removed[j] = true;
            }
        }
    }
    let mut removed = removed.into_iter();
    orbs.retain(|_| !removed.next().unwrap_or(false));
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Experience {
    pub level: i32,
    pub progress: f32,
    pub total: i32,
}
impl Experience {
    pub fn death_reward(&self, keep: bool) -> u32 {
        if keep {
            0
        } else {
            self.level.wrapping_mul(7).clamp(0, 100) as u32
        }
    }
    pub fn respawn(&mut self, keep: bool) {
        if !keep {
            *self = Self::default();
        }
    }
    pub fn needed(level: i32) -> i32 {
        if level >= 30 {
            112 + (level - 30) * 9
        } else if level >= 15 {
            37 + (level - 15) * 5
        } else {
            7 + level * 2
        }
    }
    pub fn give(&mut self, points: i32) -> Option<i32> {
        let mut sound_level = None;
        self.progress += points as f32 / Self::needed(self.level) as f32;
        self.total = self.total.saturating_add(points).max(0);
        while self.progress >= 1.0 {
            self.progress = (self.progress - 1.0) * Self::needed(self.level) as f32;
            self.level += 1;
            if self.level % 5 == 0 {
                sound_level.get_or_insert(self.level);
            }
            self.progress /= Self::needed(self.level) as f32;
        }
        sound_level
    }
}
#[derive(Clone, Debug)]
pub struct Painting {
    pub id: u32,
    pub pos: [i32; 3],
    pub facing: u8,
    pub variant: std::sync::Arc<str>,
    pub width: i32,
    pub height: i32,
    pub check_interval: u32,
}
impl Painting {
    pub fn step(&self) -> [i32; 3] {
        [[0, 0, 1], [-1, 0, 0], [0, 0, -1], [1, 0, 0]][usize::from(self.facing & 3)]
    }
    pub fn left(&self) -> [i32; 3] {
        [[1, 0, 0], [0, 0, 1], [-1, 0, 0], [0, 0, -1]][usize::from(self.facing & 3)]
    }
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let s = self.step().map(f64::from);
        let l = self.left().map(f64::from);
        let off = |n: i32| if n % 2 == 0 { 0.5 } else { 0.0 };
        let (h, v) = (off(self.width), off(self.height));
        let c = [
            f64::from(self.pos[0]) + 0.5 - s[0] * 0.46875 + l[0] * h,
            f64::from(self.pos[1]) + 0.5 + v,
            f64::from(self.pos[2]) + 0.5 - s[2] * 0.46875 + l[2] * h,
        ];
        let along_x = s[0] != 0.0;
        let size = [
            if along_x {
                0.0625
            } else {
                f64::from(self.width)
            },
            f64::from(self.height),
            if along_x {
                f64::from(self.width)
            } else {
                0.0625
            },
        ];
        (
            std::array::from_fn(|i| c[i] - size[i] / 2.0),
            std::array::from_fn(|i| c[i] + size[i] / 2.0),
        )
    }
}
pub fn dye_index(name: &str) -> u8 {
    crate::mobs::render::DYE_NAMES
        .iter()
        .position(|n| *n == name)
        .unwrap_or(0) as u8
}
#[derive(Clone, Debug)]
pub struct Cushion {
    pub id: u32,
    pub position: Vec3,
    pub pos: [i32; 3],
    pub yaw: f32,
    pub color: u8,
    pub check_interval: u32,
}
impl Cushion {
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let p = self.position;
        (
            [p.x - 0.5, p.y, p.z - 0.5],
            [p.x + 0.5, p.y + 0.25, p.z + 0.5],
        )
    }
    pub fn color_name(&self) -> &'static str {
        crate::mobs::render::DYE_NAMES[usize::from(self.color.min(15))]
    }
    pub fn item(&self) -> String {
        format!("minecraft:{}_cushion", self.color_name())
    }
    pub fn seat(&self) -> Vec3 {
        Vec3::new(self.position.x, self.position.y - 0.35, self.position.z)
    }
    pub fn dismount(&self) -> Vec3 {
        Vec3::new(self.position.x, self.position.y + 0.25, self.position.z)
    }
}
#[derive(Clone, Debug)]
pub struct ItemFrame {
    pub id: u32,
    pub pos: [i32; 3],
    pub facing: u8,
    pub glow: bool,
    pub item: Option<crate::items::ItemStack>,
    pub rotation: u8,
    pub drop_chance: f32,
    pub fixed: bool,
    pub invisible: bool,
    pub check_interval: u32,
}
impl ItemFrame {
    pub fn step(&self) -> [i32; 3] {
        [
            [0, -1, 0],
            [0, 1, 0],
            [0, 0, -1],
            [0, 0, 1],
            [-1, 0, 0],
            [1, 0, 0],
        ][usize::from(self.facing.min(5))]
    }
    pub fn kind(&self) -> &'static str {
        if self.glow {
            "minecraft:glow_item_frame"
        } else {
            "minecraft:item_frame"
        }
    }
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let s = self.step().map(f64::from);
        let c: [f64; 3] = std::array::from_fn(|i| f64::from(self.pos[i]) + 0.5 - s[i] * 0.46875);
        let size: [f64; 3] = std::array::from_fn(|i| if s[i] != 0.0 { 0.0625 } else { 0.75 });
        (
            std::array::from_fn(|i| c[i] - size[i] / 2.0),
            std::array::from_fn(|i| c[i] + size[i] / 2.0),
        )
    }
}
#[derive(Clone, Debug)]
pub struct ArmorStand {
    pub fire_ticks: i32,
    pub health: f32,
    pub id: u32,
    pub position: Vec3,
    pub velocity: Vec3,
    pub on_ground: bool,
    pub yaw: f32,
    pub equipment: [Option<crate::items::ItemStack>; 6],
    pub small: bool,
    pub show_arms: bool,
    pub no_base_plate: bool,
    pub invisible: bool,
    pub marker: bool,
    pub custom_name: Option<crate::text::Text>,
    pub name_visible: bool,
    pub last_hit: i64,
    pub pose: [[f32; 3]; 6],
}
impl ArmorStand {
    pub const SLOTS: [&'static str; 6] = ["mainhand", "offhand", "feet", "legs", "chest", "head"];
    pub const POSE_KEYS: [&'static str; 6] =
        ["Head", "Body", "LeftArm", "RightArm", "LeftLeg", "RightLeg"];
    pub const DEFAULT_POSE: [[f32; 3]; 6] = [
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [-10.0, 0.0, -10.0],
        [-15.0, 0.0, 10.0],
        [-1.0, 0.0, -1.0],
        [1.0, 0.0, 1.0],
    ];
    pub fn new(id: u32, position: Vec3, yaw: f32) -> Self {
        Self {
            fire_ticks: 0,
            health: 20.0,
            id,
            position,
            velocity: Vec3::ZERO,
            on_ground: true,
            yaw,
            equipment: Default::default(),
            small: false,
            show_arms: false,
            no_base_plate: false,
            invisible: false,
            marker: false,
            custom_name: None,
            name_visible: false,
            last_hit: i64::MIN / 2,
            pose: Self::DEFAULT_POSE,
        }
    }
    pub fn environment_tick(
        &mut self,
        world: &World,
        facts: &BlockFacts,
        registry: &vise_world::registry::BlockRegistry,
    ) -> bool {
        let (lo, hi) = (self.bounds().min_position(), self.bounds().max_position());
        let mut water = false;
        let mut lava = false;
        for x in lo.x.floor() as i32..=hi.x.floor() as i32 {
            for y in lo.y.floor() as i32..hi.y.ceil() as i32 {
                for z in lo.z.floor() as i32..=hi.z.floor() as i32 {
                    match facts.get(world.block(x, y, z)).fluid {
                        Fluid::Water => water = true,
                        Fluid::Lava => lava = true,
                        _ => {}
                    }
                }
            }
        }
        if water {
            self.fire_ticks = 0;
        }
        if self.fire_ticks > 0 {
            if self.fire_ticks % 20 == 0
                && !lava
                && !self.invisible
                && !self.marker
                && self.health > 0.5
            {
                self.health -= 4.0;
            }
            self.fire_ticks -= 1;
        }
        if lava {
            self.fire_ticks = self.fire_ticks.max(300);
        }
        let fire = crate::motion::environment::fire_contact(world, registry, self.bounds()) > 0.0;
        if fire && !self.invisible && !self.marker {
            if self.fire_ticks > 0 {
                self.health -= 0.15;
            } else {
                self.fire_ticks = 100;
            }
        }
        self.health > 0.5
    }
    pub fn bounds(&self) -> Aabb {
        let s = if self.small { 0.5 } else { 1.0 };
        let (w, h) = (0.25 * s, 1.975 * s);
        let p = self.position;
        Aabb::new(
            Vec3::new(p.x - w, p.y, p.z - w),
            Vec3::new(p.x + w, p.y + h, p.z + w),
        )
    }
    pub fn in_water(&self, world: &World, fluids: &crate::motion::FluidResources) -> bool {
        let (lo, hi) = (self.bounds().min_position(), self.bounds().max_position());
        let water = |x: i32, y: i32, z: i32| {
            let state = fluids.fluid(world.block(x, y, z));
            (state.kind() == vise_world::fluid::FluidKind::Water).then_some(state)
        };
        (lo.x.floor() as i32..=hi.x.floor() as i32).any(|x| {
            (lo.y.floor() as i32..hi.y.ceil() as i32).any(|y| {
                (lo.z.floor() as i32..=hi.z.floor() as i32).any(|z| {
                    water(x, y, z).is_some_and(|state| {
                        let height = if water(x, y + 1, z).is_some() {
                            1.0
                        } else {
                            state.own_height()
                        };
                        f64::from(y) + f64::from(height) >= lo.y
                    })
                })
            })
        })
    }
    pub fn tick(
        &mut self,
        scene: &mut crate::collisions::SceneCollisions,
        in_water: bool,
        push: &mut dyn FnMut(Aabb, &mut Vec3),
    ) {
        if self.marker {
            return;
        }
        let wanted = self.velocity;
        let falling = wanted.y <= 0.0;
        let mut y = wanted.y;
        if let Ok(moved) = scene.resolve(self.bounds(), wanted, 0.0, self.on_ground) {
            self.position = Vec3::new(
                self.position.x + moved.x,
                self.position.y + moved.y,
                self.position.z + moved.z,
            );
            self.on_ground = wanted.y < 0.0 && moved.y > wanted.y;
            if moved.y != wanted.y {
                y = 0.0;
            }
        }
        let mut velocity = Vec3::new(0.0, y, 0.0);
        push(self.bounds(), &mut velocity);
        self.velocity = if in_water {
            let y = velocity.y * f64::from(0.8f32);
            let adjusted =
                if falling && (y - 0.005).abs() >= 0.003 && (y - 0.08 / 16.0).abs() < 0.003 {
                    -0.003
                } else {
                    y - 0.08 / 16.0
                };
            Vec3::new(0.0, adjusted, 0.0)
        } else {
            Vec3::new(0.0, (velocity.y - 0.08) * f64::from(0.98f32), 0.0)
        };
    }
}

#[cfg(test)]
mod experience_tests {
    use super::*;

    #[test]
    fn death_reward_uses_levels_and_caps_the_recoverable_points() {
        for (level, reward) in [(0, 0), (1, 7), (14, 98), (15, 100), (30, 100)] {
            let xp = Experience {
                level,
                progress: 0.75,
                total: 10_000,
            };
            assert_eq!(xp.death_reward(false), reward);
            assert_eq!(xp.death_reward(true), 0);
        }
    }

    #[test]
    fn respawn_clears_all_xp_unless_it_is_kept() {
        let mut xp = Experience::default();
        xp.give(155);
        let before = (xp.level, xp.progress, xp.total);
        xp.respawn(true);
        assert_eq!((xp.level, xp.progress, xp.total), before);
        xp.respawn(false);
        assert_eq!((xp.level, xp.progress, xp.total), (0, 0.0, 0));
        xp.give(7);
        assert_eq!((xp.level, xp.progress, xp.total), (1, 0.0, 7));
    }
}

#[cfg(test)]
mod fire_environment_tests {
    use super::*;
    use vise_world::loot::PackOverlay;
    #[test]
    fn ordinary_items_burn_but_native_netherite_resists_lava() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/vise-client")
            .join(vise_core::VANILLA_VERSION);
        if !root.join("items.json").exists() {
            return;
        }
        let assets = crate::assets::Assets::load(&root).unwrap();
        let render = assets.reload(&root).unwrap();
        let facts = BlockFacts::load(&root, assets.registry).unwrap();
        let items =
            crate::items::Items::load(&root, assets.registry, &facts, &PackOverlay::NONE).unwrap();
        let mut world = World::test_scene(&assets, &render, 3).unwrap();
        let at = Vec3::new(20.5, 4.0, 20.5);
        world
            .set(
                20,
                4,
                20,
                assets.registry.default_state("minecraft:lava").unwrap(),
            )
            .unwrap();
        let make = |id: &str| ItemEntity {
            id: 1,
            thrower: None,
            target: None,
            stack: ItemStack {
                item: std::sync::Arc::from(id),
                count: 1,
                damage: 0,
                extra: None,
            },
            health: 5,
            fire_ticks: 0,
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            on_ground: false,
            age: 0,
            pickup_delay: 0,
            bob_offset: 0.0,
            ticks: 0,
        };
        let mut wood = make("minecraft:oak_planks");
        let mut metal = make("minecraft:netherite_ingot");
        let first = wood.environment_tick(&world, &facts, assets.registry, &items);
        assert!(first.alive && first.lava_hurt);
        assert!(
            !first.should_play_lava_hurt_sound(1),
            "ordinary entry damage must not repeat the burn sound"
        );
        assert!(first.should_play_lava_hurt_sound(10));
        assert!(
            wood.fire_ticks > 0,
            "lava contact must ignite dropped items"
        );
        let last = wood.environment_tick(&world, &facts, assets.registry, &items);
        assert!(
            !last.alive && last.lava_hurt,
            "lethal lava damage still emits burn sound"
        );
        assert!(
            last.should_play_lava_hurt_sound(2),
            "the lethal hit must always play the sound"
        );
        for _ in 0..40 {
            let result = metal.environment_tick(&world, &facts, assets.registry, &items);
            assert!(result.alive && !result.lava_hurt);
            assert!(!result.should_play_lava_hurt_sound(10));
        }
        assert_eq!(metal.health, 5);
        assert_eq!(metal.fire_ticks, 0, "fire-resistant items must not ignite");
        world
            .set(
                20,
                4,
                20,
                assets.registry.default_state("minecraft:fire").unwrap(),
            )
            .unwrap();
        let mut burning = make("minecraft:oak_planks");
        let result = burning.environment_tick(&world, &facts, assets.registry, &items);
        assert!(result.alive && !result.lava_hurt);
        assert!(burning.fire_ticks > 0);
        world
            .set(
                20,
                4,
                20,
                assets
                    .registry
                    .default_state("minecraft:soul_fire")
                    .unwrap(),
            )
            .unwrap();
        let mut soul = make("minecraft:oak_planks");
        soul.environment_tick(&world, &facts, assets.registry, &items);
        assert_eq!(soul.health, 3);
        let mut stand = ArmorStand::new(2, at, 0.0);
        world
            .set(
                20,
                4,
                20,
                assets.registry.default_state("minecraft:lava").unwrap(),
            )
            .unwrap();
        for _ in 0..150 {
            assert!(stand.environment_tick(&world, &facts, assets.registry));
        }
        assert_eq!(stand.health, 20.0, "lava does not hurt armor stands");
    }
}
#[cfg(test)]
mod merge_tests {
    use super::*;

    fn item(id: &str, count: u32, x: f64, age: u32) -> ItemEntity {
        let at = Vec3::new(x, 64.0, 0.5);
        ItemEntity {
            id: 0,
            thrower: None,
            target: None,
            stack: ItemStack {
                item: std::sync::Arc::from(id),
                count,
                damage: 0,
                extra: None,
            },
            health: 5,
            fire_ticks: 0,
            position: at,
            previous: at,
            velocity: Vec3::ZERO,
            on_ground: true,
            age,
            pickup_delay: 0,
            bob_offset: 0.0,
            ticks: age,
        }
    }
    fn max_stack(stack: &ItemStack) -> u32 {
        if &*stack.item == "minecraft:ender_pearl" {
            16
        } else {
            64
        }
    }
    fn merge(items: &mut [ItemEntity], i: usize) -> Vec<Option<u32>> {
        let mut removed = vec![false; items.len()];
        let mut cells = ItemCells::new(items);
        merge_with_neighbours(items, &mut removed, &mut cells, i, &max_stack);
        items
            .iter()
            .zip(removed)
            .map(|(item, gone)| (!gone).then_some(item.stack.count))
            .collect()
    }

    #[test]
    fn the_smaller_stack_goes_into_the_larger() {
        let mut items = [
            item("minecraft:sand", 3, 0.5, 41),
            item("minecraft:sand", 5, 1.2, 7),
        ];
        assert_eq!(merge(&mut items, 0), [None, Some(8)]);
        assert_eq!(items[1].age, 7);
        let mut items = [
            item("minecraft:sand", 5, 0.5, 41),
            item("minecraft:sand", 3, 1.2, 7),
        ];
        assert_eq!(merge(&mut items, 0), [Some(8), None]);
        assert_eq!(items[0].age, 8, "age 7, plus the rest of this tick");
    }

    #[test]
    fn items_merge_only_within_half_a_block_sideways() {
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 1.24, 1),
        ];
        assert_eq!(merge(&mut items, 0), [None, Some(2)]);
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 1.26, 1),
        ];
        assert_eq!(merge(&mut items, 0), [Some(1), Some(1)]);
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 0.5, 1),
        ];
        items[1].position.y += 0.3;
        assert_eq!(
            merge(&mut items, 0),
            [Some(1), Some(1)],
            "no vertical growth"
        );
    }

    #[test]
    fn different_or_full_stacks_stay_apart() {
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:gravel", 1, 0.5, 1),
        ];
        assert_eq!(merge(&mut items, 0), [Some(1), Some(1)]);
        let mut items = [
            item("minecraft:sand", 60, 0.5, 1),
            item("minecraft:sand", 10, 0.5, 1),
        ];
        assert_eq!(
            merge(&mut items, 0),
            [Some(60), Some(10)],
            "70 is over a stack"
        );
        let mut items = [
            item("minecraft:ender_pearl", 10, 0.5, 1),
            item("minecraft:ender_pearl", 6, 0.5, 1),
        ];
        assert_eq!(merge(&mut items, 0), [Some(16), None]);
        let mut items = [
            item("minecraft:ender_pearl", 16, 0.5, 1),
            item("minecraft:ender_pearl", 1, 0.5, 1),
        ];
        assert_eq!(
            merge(&mut items, 0),
            [Some(16), Some(1)],
            "a full stack doesn't merge"
        );
    }

    #[test]
    fn unmergeable_items_are_skipped() {
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 0.5, 1),
        ];
        items[1].pickup_delay = u32::MAX;
        assert_eq!(merge(&mut items, 0), [Some(1), Some(1)]);
        let mut items = [
            item("minecraft:sand", 1, 0.5, 6000),
            item("minecraft:sand", 1, 0.5, 6000),
        ];
        assert_eq!(merge(&mut items, 0), [Some(1), Some(1)]);
        items[1].age = 10;
        assert_eq!(merge(&mut items, 0), [None, Some(2)]);
        assert_eq!(items[1].age, 10);
    }

    #[test]
    fn a_stack_takes_its_neighbours_in_spawn_order_until_full() {
        let mut items = [
            item("minecraft:sand", 40, 0.5, 1),
            item("minecraft:sand", 20, 0.7, 1),
            item("minecraft:sand", 10, 0.3, 1),
            item("minecraft:sand", 3, 0.4, 1),
        ];
        assert_eq!(merge(&mut items, 0), [Some(63), None, Some(10), None]);
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 0.5, 1),
        ];
        items[1].pickup_delay = 30;
        assert_eq!(merge(&mut items, 0), [None, Some(2)]);
        assert_eq!(items[1].pickup_delay, 30, "the longer pickup delay stays");
    }

    #[test]
    fn the_cells_follow_moves() {
        let mut items = [
            item("minecraft:sand", 1, 0.5, 1),
            item("minecraft:sand", 1, 5.5, 1),
        ];
        let mut removed = [false; 2];
        let mut cells = ItemCells::new(&items);
        let from = items[1].position;
        items[1].position.x = 0.9;
        cells.moved(1, from, items[1].position);
        merge_with_neighbours(&mut items, &mut removed, &mut cells, 0, &max_stack);
        assert_eq!(removed, [true, false]);
        assert_eq!(items[1].stack.count, 2);
    }
}

#[cfg(test)]
mod honey_slide_tests {
    use super::*;

    fn falling_item(x: f64, y: f64, z: f64, vx: f64, vy: f64, vz: f64) -> ItemEntity {
        let at = Vec3::new(x, y, z);
        ItemEntity {
            id: 1,
            thrower: None,
            target: None,
            stack: ItemStack {
                item: std::sync::Arc::from("minecraft:stone"),
                count: 1,
                damage: 0,
                extra: None,
            },
            health: 5,
            fire_ticks: 0,
            position: at,
            previous: at,
            velocity: Vec3::new(vx, vy, vz),
            on_ground: false,
            age: 10,
            pickup_delay: 0,
            bob_offset: 0.0,
            ticks: 10,
        }
    }

    fn old_delta_y(dy: f64) -> f64 {
        dy / f64::from(0.98_f32) + 0.08
    }
    fn new_delta_y(dy: f64) -> f64 {
        (dy - 0.08) * f64::from(0.98_f32)
    }

    #[test]
    fn a_falling_item_beside_honey_slides_down() {
        let mut item = falling_item(-0.0625, 10.4, 0.5, 0.12, -0.20, 0.04);
        assert!(old_delta_y(-0.20) < -0.08);
        assert!(old_delta_y(-0.20) >= -0.13);
        item.honey_block_inside([0, 10, 0]);
        assert_eq!(item.velocity.x, 0.12);
        assert_eq!(item.velocity.z, 0.04);
        assert_eq!(item.velocity.y, new_delta_y(-0.05));
    }

    #[test]
    fn a_fast_fall_scales_the_horizontal_speed() {
        let mut item = falling_item(-0.0625, 10.4, 0.5, 0.2, -0.30, -0.1);
        let old = old_delta_y(-0.30);
        assert!(old < -0.13);
        item.honey_block_inside([0, 10, 0]);
        let factor = -0.05 / old;
        assert_eq!(item.velocity.x, 0.2 * factor);
        assert_eq!(item.velocity.z, -0.1 * factor);
        assert_eq!(item.velocity.y, new_delta_y(-0.05));
    }

    #[test]
    fn an_item_on_the_ground_or_on_top_does_not_slide() {
        let mut grounded = falling_item(-0.0625, 10.4, 0.5, 0.0, -0.20, 0.0);
        grounded.on_ground = true;
        grounded.honey_block_inside([0, 10, 0]);
        assert_eq!(grounded.velocity.y, -0.20);

        let mut on_top = falling_item(0.5, 10.9375, 0.5, 0.0, -0.20, 0.0);
        on_top.honey_block_inside([0, 10, 0]);
        assert_eq!(on_top.velocity.y, -0.20, "standing on the honey");
    }

    #[test]
    fn a_centred_or_slow_item_does_not_slide() {
        let mut centred = falling_item(0.5, 10.4, 0.5, 0.0, -0.20, 0.0);
        centred.honey_block_inside([0, 10, 0]);
        assert_eq!(centred.velocity.y, -0.20, "inside, not on a side");

        let mut slow = falling_item(-0.0625, 10.4, 0.5, 0.0, -0.04, 0.0);
        assert!(old_delta_y(-0.04) >= -0.08);
        slow.honey_block_inside([0, 10, 0]);
        assert_eq!(slow.velocity.y, -0.04);
    }
}
