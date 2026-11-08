use super::{
    Dim, Game, GuestUse, Hit, Mining, Screen, lan::LanHost, respawn_anchor::RespawnConfig,
};
use vise_client::{
    command::vanilla,
    containers::BlockEntity,
    entities::Experience,
    first_person::{Hand, SwingAnimation},
    items::ItemStack,
    lan::{
        guest::SPECTATOR,
        hub::TickCtx,
        menu::{Rules, Slots},
    },
    motion::{MotionEvent, MotionWorld, PlayerMotion},
    pose::Dimensions,
};
use vise_core::{
    geometry::{Aabb, Vec3},
    random::RandomSource,
};
use vise_protocol::{
    packets::play::{self, Clientbound as C, Serverbound as S},
    text::literal,
};
use vise_world::entity_move::EntityMoveWorld;
use winit::keyboard::KeyCode;

#[derive(Default)]
pub(super) struct GuestPlay {
    pub(super) sign_editing: Option<[i32; 3]>,
    mining: Option<Mining>,
    destroy_delay: u32,
    using: Option<(usize, u32)>,
    use_delay: u32,
    pub(super) shield_cooldown: u32,
    creative_drops: u32,
    pub(super) chat_spam: u32,
    pub(super) command_spam: u32,
    suggestion: Option<Vec<u8>>,
    suggestion_budget: bool,
    pub(super) cooldowns: vise_client::cooldowns::Cooldowns,
    attack_ticks: u32,
    attack_item: Option<std::sync::Arc<str>>,
    swing: vise_client::first_person::SwingState,
    digging: Option<([i32; 3], [i32; 3])>,
    dig_start: u64,
    delayed: Option<([i32; 3], u64)>,
    last_progress: i32,
    swings: u32,
    mount: super::mounts::MountState,
    pub(super) merchant: Option<super::lan_trading::GuestMerchant>,
    pub(super) crafting: super::lan_crafting::GuestCrafting,
    pub(super) station: Option<super::lan_stations::GuestStation>,
    pub(super) mount_window: Option<super::lan_mount_menu::GuestMount>,
    pub(super) lectern: Option<super::lan_stations::GuestLectern>,
    potions: super::potions::BodyPotions,
    pub(super) stats: super::stats::Stats,
    pub(super) left_game: bool,
    last_inventory: u64,
    levitation: Option<([f64; 3], u64)>,
    marks: super::advancements::TravelMarks,
    pub(super) portal: vise_client::nether_portal::PortalState,
    pub(super) won_game: bool,
    pub(super) respawn_requested: bool,
    xp_delay: u32,
    death_cause: Option<String>,
    modifiers: Vec<(String, String)>,
    equipped: Option<Vec<Option<ItemStack>>>,
    pub(super) hook: Option<super::fishing::Hook>,
    sculk: super::sculk::PlayerEvents,
}

pub(super) struct GuestCommand {
    pub(super) conn: u32,
    pub(super) command: String,
    pub(super) permission: u8,
    pub(super) integrated: bool,
    pub(super) quiet: bool,
}

pub(super) struct GuestSwap {
    conn: u32,
    i: usize,
    play: GuestPlay,
    player: Option<PlayerMotion>,
    eye: (f32, f32),
    yaw: f32,
    pitch: f32,
    selected: usize,
    mode: vise_client::command::args::GameMode,
    creative: bool,
    permission: u8,
    xp: Experience,
    host_keys: std::collections::HashSet<KeyCode>,
    screen: Screen,
    host_sign: Option<super::sign_edit::SignEditor>,
    host_book: Option<super::books::BookScreen>,
    host_command_block: Option<super::command_blocks::CommandBlockScreen>,
    host_mount_menu: super::mount_menu::MountMenu,
    craft_size: usize,
    craft_table: Option<[i32; 3]>,
    craft_grid: Vec<Option<ItemStack>>,
    container: Option<(i32, i32, i32)>,
    mining: Option<Mining>,
    destroy_delay: u32,
    using: Option<(usize, u32)>,
    use_delay: u32,
    pub(super) shield_cooldown: u32,
    cooldowns: vise_client::cooldowns::Cooldowns,
    attack_ticks: u32,
    attack_item: Option<std::sync::Arc<str>>,
    swing: vise_client::first_person::SwingState,
    left: bool,
    right: bool,
    riding: Option<u32>,
    host_station: Option<super::stations::Station>,
    sleeping: Option<[i32; 3]>,
    sleep_timer: u32,
    spawn: Option<Vec3>,
    respawn: (
        Option<super::respawn_anchor::RespawnConfig>,
        Option<[f64; 3]>,
    ),
    message: Option<(String, std::time::Instant)>,
    host_ender: Option<(i32, i32, i32)>,
    adv_loaded: bool,
    last_inventory: u64,
    levitation: Option<([f64; 3], u64)>,
    marks: super::advancements::TravelMarks,
    death_cause: Option<String>,
    combat: super::combat::CombatTracker,
    acting: (Option<u32>, Option<String>, Option<u128>, Option<u8>),
    known_movement: Vec3,
    packets: Vec<play::Clientbound>,
    entity_events: Vec<i8>,
    swings: u32,
    projectiles: usize,
    modifiers: Vec<(String, String)>,
}

impl GuestPlay {
    pub(super) fn using(&self) -> Option<(usize, u32)> {
        self.using
    }
}

struct ItemRules<'a>(&'a vise_client::items::Items);
impl Rules for ItemRules<'_> {
    fn items(&self) -> &vise_client::items::Items {
        self.0
    }
    fn max_stack(&self, item: &str) -> u32 {
        self.0.get(item).map_or(64, |i| i.max_stack)
    }
    fn armor_index(&self, item: &str) -> Option<usize> {
        match self.0.get(item)?.equip_slot.as_deref()? {
            "head" => Some(0),
            "chest" => Some(1),
            "legs" => Some(2),
            "feet" => Some(3),
            _ => None,
        }
    }
}

pub(super) fn guest_within_entity_range(
    g: &vise_client::lan::guest::Guest,
    bounds: &vise_core::geometry::Aabb,
    buffer: f64,
) -> bool {
    let eye = [g.position[0], g.position[1] + g.eye_height(), g.position[2]];
    let (lo, hi) = (bounds.min_position(), bounds.max_position());
    let d = [
        eye[0] - eye[0].clamp(lo.x, hi.x),
        eye[1] - eye[1].clamp(lo.y, hi.y),
        eye[2] - eye[2].clamp(lo.z, hi.z),
    ];
    let range = if g.creative() { 5.0 } else { 3.0 } + buffer;
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < range * range
}
fn direction(face: i32) -> [i32; 3] {
    match face {
        0 => [0, -1, 0],
        1 => [0, 1, 0],
        2 => [0, 0, -1],
        3 => [0, 0, 1],
        4 => [-1, 0, 0],
        _ => [1, 0, 0],
    }
}

impl Game<'_> {
    pub(super) fn as_guest<R>(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let swap = self.enter_guest(host, conn)?;
        let result = f(self);
        self.leave_guest(host, swap);
        Some(result)
    }

    pub(super) fn enter_guest(&mut self, host: &mut LanHost, conn: u32) -> Option<GuestSwap> {
        if self.lan_acting == Some(conn) {
            return None;
        }
        let i = host.hub.guests.iter().position(|g| g.conn == conn)?;
        let permission = std::mem::replace(
            &mut self.ui.registries.permission,
            host.hub.guests[i].permission,
        );
        let fresh = host.hub.guests[i].motion.is_none();
        if fresh {
            let p = Vec3::new(
                host.hub.guests[i].position[0],
                host.hub.guests[i].position[1],
                host.hub.guests[i].position[2],
            );
            host.hub.guests[i].motion = PlayerMotion::new(self.motion, p).ok().map(Box::new);
            if let Some(saved) = host.hub.guests[i].loaded_body.take()
                && let Some(m) = host.hub.guests[i].motion.as_mut()
            {
                saved.apply(m);
            }
            let saved = host.hub.guests[i].saved_absorption.take();
            if let (Some(m), Some(amount)) = (host.hub.guests[i].motion.as_mut(), saved) {
                let _ = m.internal_set_absorption(amount);
            }
            let kept = host.hub.guests[i].logs_kept.take();
            if let (Some(m), Some((hurts, attacks))) = (host.hub.guests[i].motion.as_mut(), kept) {
                m.hurt_log = hurts;
                m.attack_log = attacks;
            }
        }
        let mut play = host.play.remove(&conn).unwrap_or_default();
        if fresh {
            play.equipped = None;
            play.potions = Default::default();
            play.sculk = Default::default();
        }
        let g = &mut host.hub.guests[i];
        let Some(mut motion) = g.motion.take() else {
            self.ui.registries.permission = permission;
            host.play.insert(conn, play);
            return None;
        };
        self.sculk.swap_player(&mut play.sculk);
        let guest_combat = if fresh {
            Default::default()
        } else {
            self.guest_combat.remove(&conn).unwrap_or_default()
        };
        let host_combat = std::mem::replace(&mut self.combat, guest_combat);
        self.potions.swap_player(&mut g.effects, &mut play.potions);
        motion.position = Vec3::new(g.position[0], g.position[1], g.position[2]);
        motion.ground.on_ground = g.on_ground;
        motion.yaw = g.yaw;
        motion.pitch = g.pitch;
        motion.sprinting = g.sprinting;
        motion.may_fly = g.creative() || g.game_mode == SPECTATOR;
        motion.flying = g.flying;
        motion.fall_flying = g.fall_flying;
        motion.invulnerable = motion.may_fly;
        let _ = motion.set_health(g.health, false);
        motion.health.food.food = g.food;
        motion.health.food.saturation = g.saturation;
        let mut player = Some(*motion);
        std::mem::swap(&mut self.player, &mut player);
        let eye = (g.eye_height() as f32, self.eye);
        self.eye = eye.0;
        let (yaw, pitch) = (self.yaw, self.pitch);
        self.yaw = g.yaw;
        self.pitch = g.pitch;
        std::mem::swap(&mut self.inventory, &mut g.inventory);
        std::mem::swap(&mut self.armor, &mut g.armor);
        std::mem::swap(&mut self.offhand, &mut g.offhand);
        std::mem::swap(&mut self.carried, &mut g.carried);
        let selected = std::mem::replace(&mut self.selected, g.selected);
        let (mode, creative) = (self.mode, self.creative);
        self.set_mode_raw(g.game_mode);
        let xp = std::mem::replace(
            &mut self.experience,
            Experience {
                level: g.xp_level,
                progress: g.xp_progress,
                total: g.xp_total,
            },
        );
        let host_keys = std::mem::take(&mut self.keys);
        if g.sneaking {
            self.keys.insert(KeyCode::ShiftLeft);
        }
        let screen = std::mem::replace(&mut self.screen, Screen::Playing);
        let host_sign = self.sign_edit.take();
        let host_book = self.book.take();
        let host_command_block = self.command_block_screen.take();
        let host_mount_menu = std::mem::take(&mut self.mount_menu);
        let craft_size = std::mem::replace(&mut self.craft_size, 2);
        let craft_table = self.craft_table.take();
        let craft_grid = std::mem::replace(&mut self.craft_grid, vec![None; 9]);
        let container = self.container.take();
        let mining = std::mem::replace(&mut self.mining, play.mining.take());
        let destroy_delay = std::mem::replace(&mut self.destroy_delay, play.destroy_delay);
        let using = std::mem::replace(&mut self.using, play.using.take());
        let use_delay = std::mem::replace(&mut self.use_delay, play.use_delay);
        let shield_cooldown = std::mem::replace(&mut self.shield_cooldown, play.shield_cooldown);
        let cooldowns = std::mem::replace(&mut self.cooldowns, std::mem::take(&mut play.cooldowns));
        let attack_ticks = std::mem::replace(&mut self.attack_ticks, play.attack_ticks);
        let attack_item = std::mem::replace(&mut self.attack_item, play.attack_item.take());
        let swing = std::mem::replace(&mut self.swing, play.swing);
        std::mem::swap(&mut self.mount, &mut play.mount);
        let (left, right) = (self.left, self.right);
        self.left = play.digging.is_some();
        self.right = play.using.is_some() || self.using.is_some();
        let riding = std::mem::replace(&mut self.riding, g.vehicle);
        let host_station = self.station.take();
        let sleeping = std::mem::replace(&mut self.sleeping, g.bed);
        let sleep_timer = std::mem::replace(&mut self.sleep_timer, g.sleep_timer);
        let spawn = std::mem::replace(
            &mut self.spawn,
            g.spawn.map(|s| Vec3::new(s[0], s[1], s[2])),
        );
        let respawn = (self.respawn_config.take(), self.level.spawn);
        self.respawn_config = g.respawn_elsewhere.as_ref().map(RespawnConfig::from_saved);
        let message = self.message.take();
        let host_ender = self.ender_open;
        self.close_ender_chest();
        std::mem::swap(&mut self.ender_items, &mut g.ender_items);
        self.advancement_tree();
        std::mem::swap(&mut self.adv.progress, &mut g.advancements);
        std::mem::swap(&mut self.adv.changed, &mut g.advancements_changed);
        std::mem::swap(&mut self.adv.unlocked, &mut g.recipes_unlocked);
        std::mem::swap(&mut self.level.recipe_book, &mut g.recipe_book);
        std::mem::swap(&mut self.level.player_extra, &mut g.saved);
        std::mem::swap(&mut self.stats, &mut play.stats);
        std::mem::swap(&mut self.hook, &mut play.hook);
        let adv_loaded = std::mem::replace(&mut self.adv.loaded, true);
        let last_inventory = std::mem::replace(&mut self.adv.last_inventory, play.last_inventory);
        let levitation = std::mem::replace(&mut self.adv.levitation, play.levitation);
        let marks = std::mem::replace(&mut self.adv.marks, std::mem::take(&mut play.marks));
        let death_cause = std::mem::replace(&mut self.ui.death_cause, play.death_cause.take());
        let [x, y, z] = g.known_movement;
        let known_movement = std::mem::replace(&mut self.known_movement, Vec3::new(x, y, z));
        let acting = (
            self.lan_acting.replace(conn),
            self.lan_acting_name.replace(g.name.clone()),
            self.lan_acting_uuid.replace(g.uuid),
            self.lan_acting_permission.replace(g.permission),
        );
        let packets = std::mem::take(&mut self.guest_packets);
        let entity_events = std::mem::take(&mut self.guest_entity_events);
        let swings = self.swings_started;
        let projectiles = self.creatures.projectiles.len();
        let modifiers = std::mem::replace(
            &mut self.applied_modifiers,
            std::mem::take(&mut play.modifiers),
        );
        let mut equipped: Vec<Option<ItemStack>> = self.armor.to_vec();
        equipped.push(self.inventory.get(self.selected).cloned().flatten());
        if play.equipped.as_ref() != Some(&equipped) {
            self.apply_equipment();
            play.equipped = Some(equipped);
        }
        if fresh {
            self.apply_effect_status();
        }
        self.update_totem_ready();
        Some(GuestSwap {
            conn,
            i,
            play,
            player,
            eye,
            yaw,
            pitch,
            selected,
            mode,
            creative,
            permission,
            xp,
            host_keys,
            screen,
            host_sign,
            host_book,
            host_command_block,
            host_mount_menu,
            craft_size,
            craft_table,
            craft_grid,
            container,
            mining,
            destroy_delay,
            using,
            use_delay,
            shield_cooldown,
            cooldowns,
            attack_ticks,
            attack_item,
            swing,
            left,
            right,
            riding,
            host_station,
            sleeping,
            sleep_timer,
            spawn,
            respawn,
            message,
            host_ender,
            adv_loaded,
            last_inventory,
            levitation,
            marks,
            death_cause,
            acting,
            known_movement,
            packets,
            entity_events,
            swings,
            projectiles,
            modifiers,
            combat: host_combat,
        })
    }

    pub(super) fn leave_guest(&mut self, host: &mut LanHost, swap: GuestSwap) {
        let totem = self.apply_totem();
        let GuestSwap {
            conn,
            i,
            mut play,
            mut player,
            eye,
            yaw,
            pitch,
            selected,
            mode,
            creative,
            permission,
            xp,
            host_keys,
            screen,
            host_sign,
            host_book,
            host_command_block,
            host_mount_menu,
            craft_size,
            craft_table,
            craft_grid,
            container,
            mining,
            destroy_delay,
            using,
            use_delay,
            shield_cooldown,
            cooldowns,
            attack_ticks,
            attack_item,
            swing,
            left,
            right,
            riding,
            host_station,
            sleeping,
            sleep_timer,
            spawn,
            respawn,
            message,
            host_ender,
            adv_loaded,
            last_inventory,
            levitation,
            marks,
            death_cause,
            combat,
            acting,
            known_movement,
            packets,
            entity_events,
            swings,
            projectiles,
            modifiers,
        } = swap;
        self.ui.registries.permission = permission;
        self.sculk.swap_player(&mut play.sculk);
        let guest_combat = std::mem::replace(&mut self.combat, combat);
        self.guest_combat.insert(conn, guest_combat);
        play.modifiers = std::mem::replace(&mut self.applied_modifiers, modifiers);
        for p in self.creatures.projectiles.iter_mut().skip(projectiles) {
            if p.owner == Some(vise_client::mobs::Who::Player) && p.guest.is_none() {
                p.guest = Some(conn);
            }
        }
        play.swings += self.swings_started.wrapping_sub(swings);
        self.swings_started = swings;
        for packet in std::mem::replace(&mut self.guest_packets, packets) {
            host.hub.send_to(conn, &packet);
        }
        let entity_id = host.hub.guests[i].entity_id;
        for event in std::mem::replace(&mut self.guest_entity_events, entity_events) {
            host.hub.send_to(
                conn,
                &C::EntityEvent(play::EntityEvent {
                    id: entity_id,
                    event,
                }),
            );
        }
        let players = vise_client::settings::SOUND_SOURCES
            .iter()
            .position(|s| *s == "player")
            .unwrap_or(0) as i32;
        for (name, at, volume, pitch) in std::mem::take(&mut self.guest_player_sounds) {
            host.hub
                .sound(&name, at, volume, pitch, players, Some(conn));
        }
        for (group, duration) in std::mem::take(&mut self.guest_cooldowns) {
            host.hub.send_to(
                conn,
                &C::Cooldown(play::Cooldown {
                    group: group.to_string(),
                    duration,
                }),
            );
        }
        (
            self.lan_acting,
            self.lan_acting_name,
            self.lan_acting_uuid,
            self.lan_acting_permission,
        ) = acting;
        self.known_movement = known_movement;
        play.death_cause = std::mem::replace(&mut self.ui.death_cause, death_cause);
        self.close_ender_chest();

        let opened = self.container.take();
        let guest_table = std::mem::replace(&mut self.craft_table, craft_table);
        let opened_table = (std::mem::replace(&mut self.craft_size, craft_size) == 3)
            .then_some(guest_table)
            .flatten();
        self.craft_grid = craft_grid;
        let opened_station = std::mem::replace(&mut self.station, host_station);
        let bed = std::mem::replace(&mut self.sleeping, sleeping);
        let guest_sleep_timer = std::mem::replace(&mut self.sleep_timer, sleep_timer);
        let guest_spawn = std::mem::replace(&mut self.spawn, spawn).map(|s| [s.x, s.y, s.z]);
        let guest_respawn = std::mem::replace(&mut self.respawn_config, respawn.0);
        self.level.spawn = respawn.1;
        let guest_message = std::mem::replace(&mut self.message, message);
        let guest_vehicle = std::mem::replace(&mut self.riding, riding);
        self.left = left;
        self.right = right;
        play.mining = std::mem::replace(&mut self.mining, mining);
        play.destroy_delay = std::mem::replace(&mut self.destroy_delay, destroy_delay);
        play.using = std::mem::replace(&mut self.using, using);
        play.use_delay = std::mem::replace(&mut self.use_delay, use_delay);
        play.shield_cooldown = std::mem::replace(&mut self.shield_cooldown, shield_cooldown);
        play.cooldowns = std::mem::replace(&mut self.cooldowns, cooldowns);
        play.attack_ticks = std::mem::replace(&mut self.attack_ticks, attack_ticks);
        play.attack_item = std::mem::replace(&mut self.attack_item, attack_item);
        play.swing = std::mem::replace(&mut self.swing, swing);
        std::mem::swap(&mut self.mount, &mut play.mount);
        self.container = container;
        self.screen = screen;
        let guest_sign = std::mem::replace(&mut self.sign_edit, host_sign);
        let guest_lectern = match std::mem::replace(&mut self.book, host_book) {
            Some(super::books::BookScreen::View {
                lectern: Some(key), ..
            }) => Some(key),
            _ => None,
        };
        let guest_command_block =
            std::mem::replace(&mut self.command_block_screen, host_command_block).map(|s| s.pos());
        let guest_mount = std::mem::replace(&mut self.mount_menu, host_mount_menu).opened_mob();
        self.keys = host_keys;
        let g = &mut host.hub.guests[i];
        let left_vehicle = g.vehicle.filter(|_| guest_vehicle.is_none());
        if guest_vehicle.is_none() {
            g.vehicle_good = None;
        }
        g.vehicle = guest_vehicle;
        let fell_asleep = bed.is_some() && g.bed.is_none();
        (g.bed, g.sleep_timer, g.spawn) = (bed, guest_sleep_timer, guest_spawn);
        g.respawn_elsewhere = guest_respawn.map(|c| c.saved());
        g.sleeping = g.bed.is_some();
        if let Some((text, _)) = guest_message {
            g.send(&C::SystemChat(play::SystemChat {
                content: literal(&text),
                overlay: true,
            }));
        }
        std::mem::swap(&mut self.ender_items, &mut g.ender_items);
        std::mem::swap(&mut self.adv.progress, &mut g.advancements);
        std::mem::swap(&mut self.adv.changed, &mut g.advancements_changed);
        std::mem::swap(&mut self.adv.unlocked, &mut g.recipes_unlocked);
        std::mem::swap(&mut self.level.recipe_book, &mut g.recipe_book);
        std::mem::swap(&mut self.level.player_extra, &mut g.saved);
        std::mem::swap(&mut self.stats, &mut play.stats);
        std::mem::swap(&mut self.hook, &mut play.hook);
        self.adv.loaded = adv_loaded;
        play.last_inventory = std::mem::replace(&mut self.adv.last_inventory, last_inventory);
        play.levitation = std::mem::replace(&mut self.adv.levitation, levitation);
        play.marks = std::mem::replace(&mut self.adv.marks, marks);
        let stat_events = std::mem::take(&mut play.stats.events);
        let name = g.name.clone();
        self.potions.swap_player(&mut g.effects, &mut play.potions);
        if let Some(key) = host_ender {
            self.world
                .block_entities
                .insert(key, BlockEntity::Chest(self.ender_items.clone()));
            self.ender_open = Some(key);
        }
        let new_xp = std::mem::replace(&mut self.experience, xp);
        g.xp_level = new_xp.level;
        g.xp_progress = new_xp.progress;
        g.xp_total = new_xp.total;
        let new_mode = self.mode_id();
        self.mode = mode;
        self.creative = creative;
        g.selected = std::mem::replace(&mut self.selected, selected);
        std::mem::swap(&mut self.inventory, &mut g.inventory);
        std::mem::swap(&mut self.armor, &mut g.armor);
        std::mem::swap(&mut self.offhand, &mut g.offhand);
        std::mem::swap(&mut self.carried, &mut g.carried);
        let turned = [self.yaw, self.pitch] != [g.yaw, g.pitch];
        [g.yaw, g.pitch] = [self.yaw, self.pitch];
        self.yaw = yaw;
        self.pitch = pitch;
        self.eye = eye.1;
        std::mem::swap(&mut self.player, &mut player);
        if let Some(mut m) = player {
            g.health = m.health.health();
            g.food = m.health.food.food;
            g.saturation = m.health.food.saturation;
            let at = [m.position.x, m.position.y, m.position.z];
            if !m.sprinting {
                g.sprinting = false;
            }
            m.sprinting = false;
            if let Some(yaw) = m.health.indicated_dir.take() {
                g.send(&C::HurtAnimation(play::HurtAnimation {
                    id: g.entity_id,
                    yaw,
                }));
            }
            let teleported = std::mem::take(&mut m.teleported);
            let moved: f64 = (0..3).map(|k| (at[k] - g.position[k]).powi(2)).sum();
            g.motion = Some(Box::new(m));
            let left_vehicle = match g.vehicle.filter(|_| teleported) {
                Some(v) => {
                    (g.vehicle, g.vehicle_good) = (None, None);
                    Some(v)
                }
                None => left_vehicle,
            };
            if let Some(vehicle) = left_vehicle {
                g.send(&C::SetPassengers(play::SetPassengers {
                    vehicle: vehicle as i32,
                    passengers: Vec::new(),
                }));
            }
            if g.vehicle.is_none() && (moved > 0.0001 || turned || teleported) {
                host.hub.teleport(i, Some(at));
            }
        }
        let g = &mut host.hub.guests[i];
        if let Some(head) = g.bed.filter(|_| fell_asleep) {
            let at = [
                f64::from(head[0]) + 0.5,
                f64::from(head[1]) + 0.5625 + 0.125,
                f64::from(head[2]) + 0.5,
            ];
            g.position = at;
            if let Some(m) = g.motion.as_mut() {
                m.position = Vec3::new(at[0], at[1], at[2]);
                m.velocity = Vec3::ZERO;
            }
        }
        if new_mode != g.game_mode {
            let conn = g.conn;
            host.hub.set_game_mode(conn, new_mode);
        }
        if let Some(body) = totem {
            let id = host.hub.guests[i].entity_id;
            host.hub
                .entity_event(id, super::totem::TOTEM_EVENT, body.at);
            if self.player.is_some() {
                self.totem_event(body, false);
            }
        }
        host.play.insert(conn, play);
        self.stat_criteria_changed(&name, stat_events);
        if let Some(pos) = opened
            && !self.lan_open_container(host, conn, pos)
            && !host.hub.guests[i].spectator()
        {
            self.container_openers_changed(pos, false);
        }
        if let Some(station) = opened_station {
            self.lan_station_opened(host, conn, station);
        }
        if let Some(edit) = guest_sign {
            self.lan_open_sign_editor(host, conn, edit);
        }
        if let Some(key) = guest_lectern {
            self.lan_open_lectern(host, conn, key);
        }
        if let Some(pos) = guest_command_block {
            self.lan_open_command_block(host, conn, pos);
        }
        if let Some(pos) = opened_table {
            self.lan_open_crafting(host, conn, pos);
        }
        if let Some(mob) = guest_mount {
            self.lan_open_mount(host, conn, mob);
        }
    }

    pub(super) fn for_each_player(&mut self, mut f: impl FnMut(&mut Self)) {
        if self.player.is_some() {
            f(self);
        }
        let Some(mut host) = self.lan.take() else {
            return;
        };
        let conns: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
            .map(|g| g.conn)
            .collect();
        for conn in conns {
            self.as_guest(&mut host, conn, &mut f);
        }
        self.lan = Some(host);
    }

    fn set_mode_raw(&mut self, id: i32) {
        use vise_client::command::args::GameMode;
        self.mode = match id {
            1 => GameMode::Creative,
            2 => GameMode::Adventure,
            3 => GameMode::Spectator,
            _ => GameMode::Survival,
        };
        self.creative = self.mode == GameMode::Creative;
    }

    fn lan_open_container(&mut self, host: &mut LanHost, conn: u32, pos: (i32, i32, i32)) -> bool {
        let name = &self
            .res
            .facts()
            .get(self.world.block(pos.0, pos.1, pos.2))
            .name;
        let (menu, slots) = if &**name == "minecraft:ender_chest" {
            ("minecraft:generic_9x3", Slots::Ender)
        } else if let Some(BlockEntity::Furnace(f)) = self.world.block_entities.get(&pos) {
            let menu = match f.kind.recipe_type() {
                "minecraft:blasting" => "minecraft:blast_furnace",
                "minecraft:smoking" => "minecraft:smoker",
                _ => "minecraft:furnace",
            };
            (menu, Slots::Block(pos))
        } else if let Some(BlockEntity::Crafter(_)) = self.world.block_entities.get(&pos) {
            ("minecraft:crafter_3x3", Slots::Block(pos))
        } else if let Some(BlockEntity::Brewing(_)) = self.world.block_entities.get(&pos) {
            ("minecraft:brewing_stand", Slots::Block(pos))
        } else if let Some((other, right)) = self.chest_partner(pos) {
            let halves = if right {
                Slots::Double(pos, other)
            } else {
                Slots::Double(other, pos)
            };
            ("minecraft:generic_9x6", halves)
        } else if let Some(menu) = self.mod_menu_at(pos) {
            (menu, Slots::Block(pos))
        } else {
            let Some(BlockEntity::Chest(items)) = self.world.block_entities.get(&pos) else {
                return false;
            };
            let menu = match items.len() {
                27 => "minecraft:generic_9x3",
                9 => "minecraft:generic_3x3",
                5 => "minecraft:hopper",
                _ => return false,
            };
            (menu, Slots::Block(pos))
        };
        let custom = |k: (i32, i32, i32)| self.world.block_names.get(&k).map(String::as_str);
        let title = match slots {
            Slots::Ender => vise_protocol::text::translatable("container.enderchest", &[]),
            Slots::Double(first, second) => {
                vise_client::containers::double_chest_name(custom(first), custom(second))
            }
            Slots::Block(k) => {
                let key = match vise_client::containers::default_name_key(name) {
                    Some(key) => key.to_owned(),
                    None if self.mod_menu_at(k).is_some() => {
                        format!("block.{}", name.replace(':', "."))
                    }
                    None => return false,
                };
                vise_client::containers::display_name(custom(k), &key)
            }
            Slots::Native => return false,
        };
        let Some(items) = host
            .hub
            .guest(conn)
            .and_then(|g| slots.read(&self.world, g))
        else {
            return false;
        };
        let menu = match slots {
            Slots::Block(_) => vise_client::containers::block_menu_type(name).unwrap_or(menu),
            _ => menu,
        };
        self.guest_container_closed(host, conn);
        self.lan_station_closed(host, conn, None);
        host.hub
            .open_container(conn, menu, title, pos, slots, items);
        true
    }

    pub(super) fn guest_container_closed(&mut self, host: &mut LanHost, conn: u32) {
        if let Some(g) = host.hub.guest(conn)
            && let Some(pos) = g.menu.as_ref().and_then(|m| m.pos)
            && !g.spectator()
            && self.container_openers_changed(pos, false)
        {
            self.container_edge_sound(pos, false);
        }
    }

    fn lan_containers_tick(&mut self, host: &mut LanHost) {
        let far: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g))
            .filter(|g| {
                g.menu.as_ref().is_some_and(|m| {
                    (m.slots.valid(&self.world) && !m.slots.still_valid(&self.world, g, m.pos))
                        || m.pos.is_some_and(|k| {
                            k.1 == super::minecarts::CART_KEY_Y
                                && !self.chest_vehicle_still_valid(k.2, g)
                        })
                })
            })
            .map(|g| g.conn)
            .collect();
        for conn in far {
            self.guest_container_closed(host, conn);
            host.hub.close_container(conn);
            if let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) {
                self.guest_carried_back(host, gi);
            }
        }
    }

    fn chest_vehicle_still_valid(&self, id: i32, g: &vise_client::lan::guest::Guest) -> bool {
        let bounds = self
            .carts
            .iter()
            .find(|c| c.id as i32 == id)
            .map(|c| c.bounds())
            .or_else(|| {
                self.boats
                    .iter()
                    .find(|b| b.id as i32 == id)
                    .map(|b| b.bounds())
            });
        bounds.is_some_and(|b| guest_within_entity_range(g, &b, 4.0))
    }

    fn under_spawn_protection(&self, host: &LanHost, gi: usize, pos: [i32; 3]) -> bool {
        let g = &host.hub.guests[gi];
        let radius = host.hub.spawn_protection;
        if self.options.dedicated.is_none() || radius <= 0 || g.dimension != "minecraft:overworld" {
            return false;
        }
        let Some(spawn) = self.level.world_spawn else {
            return false;
        };
        if host.hub.ops.is_empty() || host.hub.ops.contains_key(&g.uuid) {
            return false;
        }
        let dx = pos[0].abs_diff(spawn[0]);
        let dz = pos[2].abs_diff(spawn[2]);
        dx.max(dz) <= radius as u32
    }

    fn spawn_protection_message(&self, host: &LanHost, conn: u32, pos: [i32; 3]) {
        let message = vise_client::text::Text::translate(
            "build.spawn_protection",
            &[vise_client::text::Text::literal(format!(
                "{}, {}, {}",
                pos[0], pos[1], pos[2]
            ))],
        )
        .with_color(vise_client::text::RED);
        host.hub.send_to(
            conn,
            &C::SystemChat(play::SystemChat {
                content: vise_client::lan::data::component_nbt(&message.component()),
                overlay: true,
            }),
        );
    }

    fn was_block_placement_attempt(&mut self, hand: Hand) -> bool {
        let Some(stack) = self.with_hand(hand, |g| g.held().cloned()) else {
            return false;
        };
        let Some(info) = self.items().get(&stack.item).cloned() else {
            return false;
        };
        (info.block.is_some() || vise_client::items::bucket_contents(&stack.item).is_some())
            && !self
                .cooldowns
                .is_on_cooldown(&vise_client::cooldowns::group(
                    &stack,
                    info.use_cooldown.as_ref(),
                ))
    }

    fn build_limit_message(&self, host: &LanHost, conn: u32, too_high: bool, limit: i32) {
        let message = vise_client::text::Text::translate(
            if too_high {
                "build.tooHigh"
            } else {
                "build.tooLow"
            },
            &[vise_client::text::Text::literal(limit.to_string())],
        )
        .with_color(vise_client::text::RED);
        host.hub.send_to(
            conn,
            &C::SystemChat(play::SystemChat {
                content: vise_client::lan::data::component_nbt(&message.component()),
                overlay: true,
            }),
        );
    }

    pub(super) fn lan_guest_packet(&mut self, host: &mut LanHost, conn: u32, packet: S) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        if host.hub.guests[gi].dead
            && !matches!(
                packet,
                S::ClientCommand(_) | S::ChatCommand(_) | S::ChatCommandSigned(_)
            )
        {
            return;
        }
        let spectator = host.hub.guests[gi].spectator();
        match packet {
            S::PlayerAction(a) => match a.action {
                0 => {
                    host.hub.ack(conn, a.sequence);
                    if spectator {
                        return;
                    }
                    if !host.hub.guests[gi].may_reach_block(a.pos) {
                        return;
                    }
                    if self.under_spawn_protection(host, gi, a.pos) {
                        self.spawn_protection_message(host, conn, a.pos);
                        return;
                    }
                    if !self
                        .border
                        .contains(f64::from(a.pos[0]), f64::from(a.pos[2]))
                    {
                        return;
                    }
                    let face = direction(i32::from(a.direction));
                    let play = host.play.entry(conn).or_default();
                    play.digging = Some((a.pos, face));
                    play.dig_start = host.hub.ticks;
                    play.last_progress = -1;
                    play.destroy_delay = 0;
                    let pos = a.pos;
                    let destroyed = self.as_guest(host, conn, |g| {
                        if g.creative {
                            let _ = g.break_block(pos);
                            return true;
                        }
                        if !g.block_action_restricted(pos) {
                            let _ = g.attack_block(pos);
                            g.mining = Some(Mining {
                                pos,
                                progress: 0.0,
                                ticks: 0,
                            });
                        }
                        false
                    });
                    if destroyed == Some(true)
                        && let Some(play) = host.play.get_mut(&conn)
                    {
                        play.digging = None;
                    }
                }
                1 => {}
                3 => {
                    host.hub.ack(conn, a.sequence);
                    if !host.hub.guests[gi].may_reach_block(a.pos) {
                        let play = host.play.entry(conn).or_default();
                        play.digging = None;
                        play.mining = None;
                        return;
                    }
                    self.guest_stop_digging(host, conn, a.pos);
                }
                2 => {
                    host.hub.ack(conn, a.sequence);
                    let play = host.play.entry(conn).or_default();
                    play.digging = None;
                    play.mining = None;
                    let state = self.world.block(a.pos[0], a.pos[1], a.pos[2]).0 as i32;
                    host.hub.send_to(
                        conn,
                        &C::BlockUpdate(play::BlockUpdate { pos: a.pos, state }),
                    );
                    self.destroy_block_progress(host.hub.guests[gi].entity_id, a.pos, -1);
                    host.hub.broadcast_near(
                        [
                            f64::from(a.pos[0]),
                            f64::from(a.pos[1]),
                            f64::from(a.pos[2]),
                        ],
                        64.0,
                        &C::BlockDestruction(play::BlockDestruction {
                            id: host.hub.guests[gi].entity_id,
                            pos: a.pos,
                            progress: 255,
                        }),
                        Some(conn),
                    );
                }
                4 | 5 => {
                    if !spectator {
                        self.as_guest(host, conn, |g| g.drop_held(a.action == 4));
                    }
                }
                6 => {
                    self.as_guest(host, conn, |g| {
                        let Some(hand) = g.used_hand() else {
                            return;
                        };
                        g.with_hand(hand, |g| {
                            if let Some((slot, ticks)) = g.using.take()
                                && let Some(stack) = g.stack_in(slot).cloned()
                            {
                                g.release_used_item(&stack, ticks, false);
                            }
                        });
                    });
                }
                7 if !spectator => {
                    let g = &mut host.hub.guests[gi];
                    let slot = g.selected;
                    std::mem::swap(&mut g.inventory[slot], &mut g.offhand);
                    let (offhand, main) = (g.offhand.clone(), g.inventory[slot].clone());
                    if let Some(play) = host.play.get_mut(&conn) {
                        play.using = None;
                    }
                    self.as_guest(host, conn, |g| {
                        g.player_on_equip("offhand", main.as_ref(), offhand.as_ref());
                        g.player_on_equip("mainhand", offhand.as_ref(), main.as_ref());
                    });
                }
                8 if !spectator => self.guest_jab(host, conn),
                _ => {}
            },
            S::SetCarriedItem(p) => {
                if (0..9).contains(&p.slot) {
                    host.hub.guests[gi].selected = p.slot as usize;
                    if let Some(play) = host.play.get_mut(&conn) {
                        play.using = None;
                    }
                }
            }
            S::Punch(_) => {
                self.as_guest(host, conn, |g| {
                    g.swing_hand(Hand::Main, SwingAnimation::DEFAULT);
                    g.reset_attack_strength();
                });
            }
            S::UseItemOn(p) => {
                host.hub.ack(conn, p.sequence);
                if !host.hub.guests[gi].may_reach_block(p.pos) {
                    return;
                }
                let inside = p
                    .cursor
                    .iter()
                    .all(|&c| (f64::from(c) - 0.5).abs() < 1.0000001);
                if !inside {
                    let d = vise_client::java_text::double_to_string;
                    let at = |k: usize| d(f64::from(p.pos[k]) + f64::from(p.cursor[k]));
                    let line = format!(
                        "Rejecting UseItemOnPacket from {}: Location ({}, {}, {}) too far away from hit block BlockPos{{x={}, y={}, z={}}}.",
                        host.hub.guests[gi].name,
                        at(0),
                        at(1),
                        at(2),
                        p.pos[0],
                        p.pos[1],
                        p.pos[2]
                    );
                    self.lan_log_event(vise_client::lan::hub::HostEvent::Warn(line));
                    return;
                }
                let face = direction(p.face);
                let hit = Hit {
                    pos: p.pos,
                    face,
                    point: [
                        f64::from(p.pos[0]) + f64::from(p.cursor[0]),
                        f64::from(p.pos[1]) + f64::from(p.cursor[1]),
                        f64::from(p.pos[2]) + f64::from(p.cursor[2]),
                    ],
                };
                let hand = if p.hand == 1 { Hand::Off } else { Hand::Main };
                let min_y = self.world.shape.min_y;
                let max_y = self.world.shape.max_y() - 1;
                if p.pos[1] > max_y {
                    self.build_limit_message(host, conn, true, max_y);
                    return;
                }
                if p.pos[1] < min_y {
                    self.build_limit_message(host, conn, false, min_y);
                    return;
                }
                if self.under_spawn_protection(host, gi, p.pos) {
                    self.spawn_protection_message(host, conn, p.pos);
                } else if self
                    .border
                    .contains(f64::from(p.pos[0]), f64::from(p.pos[2]))
                {
                    let logged = self.world.block_log.as_ref().map_or(0, Vec::len);
                    let heard = self.audio.capture.as_ref().map_or(0, Vec::len);
                    let used = self.as_guest(host, conn, |g| {
                        let attempt = g.was_block_placement_attempt(hand);
                        g.forced_hit = Some(hit);
                        g.guest_use = Some(GuestUse::OnBlock(hand));
                        let swings = g.swings_started;
                        let consumed = matches!(g.use_item(), Ok(Some(_)));
                        g.forced_hit = None;
                        g.guest_use = None;
                        (g.swings_started != swings, consumed, attempt)
                    });
                    let swung = used.map(|u| u.0);
                    self.lan_swing_back(host, conn, swung);
                    self.guest_use_sounds(host, conn, logged, heard);
                    if let Some((_, false, true)) = used {
                        if p.face == 1 && p.pos[1] >= max_y {
                            self.build_limit_message(host, conn, true, max_y);
                            self.build_limit_message(host, conn, true, max_y);
                        } else if p.face == 0 && p.pos[1] <= min_y {
                            self.build_limit_message(host, conn, false, min_y);
                        }
                    }
                } else {
                    self.build_limit_message(host, conn, true, max_y);
                }
                for pos in [
                    p.pos,
                    [p.pos[0] + face[0], p.pos[1] + face[1], p.pos[2] + face[2]],
                ] {
                    let state = self.world.block(pos[0], pos[1], pos[2]).0 as i32;
                    host.hub
                        .send_to(conn, &C::BlockUpdate(play::BlockUpdate { pos, state }));
                }
            }
            S::UseItem(p) => {
                host.hub.ack(conn, p.sequence);
                if spectator {
                    return;
                }
                let hand = if p.hand == 1 { Hand::Off } else { Hand::Main };
                let g = &mut host.hub.guests[gi];
                (g.yaw, g.pitch) = (p.yaw, p.pitch);
                let heard = self.audio.capture.as_ref().map_or(0, Vec::len);
                let used = self.as_guest(host, conn, |g| {
                    let horn = g.with_hand(hand, |g| {
                        let held = g.held()?;
                        g.res.items().instrument(held).map(|i| i.sound)
                    });
                    g.guest_use = Some(GuestUse::Item(hand));
                    let swings = g.swings_started;
                    let _ = g.use_item();
                    g.guest_use = None;
                    (g.swings_started != swings, horn)
                });
                let (swung, horn) = match used {
                    Some((swung, horn)) => (Some(swung), horn),
                    None => (None, None),
                };
                self.lan_swing_back(host, conn, swung);
                self.guest_instrument_sound(host, conn, heard, horn);
            }
            S::Attack(p) => self.guest_attack(host, conn, p.id),
            S::Interact(p) => {
                if let Some(target) = self.guest_target_box(host, gi, p.id)
                    && self.as_guest(host, conn, |g| {
                        g.within_entity_interaction_range(&target, 3.0)
                    }) != Some(true)
                {
                    return;
                }
                let before = host.hub.guests[gi].vehicle;
                if self
                    .look_at_entity(host, gi, p.id, Some(p.location))
                    .is_some()
                {
                    let hand = if p.hand == 1 { Hand::Off } else { Hand::Main };
                    let swung = self.as_guest(host, conn, |g| {
                        g.guest_use = Some(GuestUse::Entity(hand));
                        let swings = g.swings_started;
                        let _ = g.use_item();
                        g.guest_use = None;
                        g.swings_started != swings
                    });
                    self.lan_swing_back(host, conn, swung);
                    self.guest_boarded(host, conn, before);
                }
            }
            S::MoveVehicle(p) => self.guest_move_vehicle(host, conn, &p),
            S::PaddleBoat(p) => self.guest_paddle_boat(host, conn, [p.left, p.right]),
            S::SetCreativeModeSlot(p) if p.slot < 0 => {
                if !host.hub.guests[gi].creative() {
                    return;
                }
                let Some(stack) = self.item_from_network(host, &p.item) else {
                    return;
                };
                if stack.count
                    > self
                        .res
                        .items()
                        .get(&stack.item)
                        .map_or(64, |i| i.max_stack)
                {
                    return;
                }
                let play = host.play.entry(conn).or_default();
                if play.creative_drops >= 1480 {
                    eprintln!(
                        "Player {} was dropping items too fast in creative mode, ignoring.",
                        host.hub.guests[gi].name
                    );
                    return;
                }
                play.creative_drops += 20;
                self.as_guest(host, conn, |g| g.player_drop(stack));
            }
            S::SetCreativeModeSlot(p) => {
                let g = &mut host.hub.guests[gi];
                if !g.creative() || !(0..46).contains(&p.slot) {
                    return;
                }
                let item = self.item_from_network(host, &p.item);
                let slot = p.slot as usize;
                let g = &mut host.hub.guests[gi];
                let old = g.menu_slot(slot);
                g.set_menu_slot(slot, item.clone());
                g.menu_sent[slot] = item.clone();
                let equipment = match slot {
                    5..=8 => Some(["head", "chest", "legs", "feet"][slot - 5]),
                    45 => Some("offhand"),
                    _ => None,
                };
                if let Some(equipment) = equipment {
                    self.as_guest(host, conn, |g| {
                        g.player_on_equip(equipment, old.as_ref(), item.as_ref());
                    });
                }
            }
            S::ContainerClick(p) if self.lan_merchant_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_inventory_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_crafting_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_machine_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_station_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_lectern_click(host, conn, &p) => {}
            S::ContainerClick(p) if self.lan_mount_click(host, conn, &p) => {}
            S::PlayerCommand(p) if p.action == 5 => {
                self.as_guest(host, conn, |g| {
                    g.open_ridden_mount_menu();
                });
            }
            S::PlayerCommand(p) if p.action == 6 => self.lan_start_gliding(host, conn),
            S::ContainerButtonClick(p) => {
                self.lan_station_button(host, conn, p.container, p.button);
            }
            S::RenameItem(p) => self.lan_station_rename(host, conn, &p.name),
            S::SignUpdate(p) => self.lan_sign_update(host, conn, p),
            S::SetCommandBlock(p) => self.lan_set_command_block(host, conn, p),
            S::EditBook(p) => self.lan_edit_book(host, conn, p),
            S::ContainerSlotStateChanged(p) => {
                self.lan_crafter_slot_state(host, conn, p.container, p.slot, p.enabled);
            }
            S::SetBeacon(p) => self.lan_station_beacon(host, conn, p.primary, p.secondary),
            S::SelectTrade(p) => self.lan_select_trade(host, conn, p.item),
            S::ContainerClose(p) if self.lan_merchant_closed(host, conn, Some(p.container)) => {
                self.guest_carried_back(host, gi);
            }
            S::ContainerClose(p)
                if self.lan_crafting_closed(host, conn, Some(p.container))
                    || self.lan_station_closed(host, conn, Some(p.container))
                    || self.lan_mount_closed(host, conn, Some(p.container)) =>
            {
                self.guest_carried_back(host, gi);
            }
            S::SelectBundleItem(p) => {
                if !spectator && let Ok(slot) = usize::try_from(p.slot) {
                    host.hub
                        .select_bundle_item(conn, slot, p.index, &mut self.world);
                }
            }
            S::ContainerClick(p) => {
                let rules = ItemRules(world_items!(self));
                let slots = host.hub.guests[gi]
                    .menu
                    .as_ref()
                    .filter(|m| m.window == p.container)
                    .map(|m| m.slots);
                let before = slots.and_then(|s| s.read(&self.world, &host.hub.guests[gi]));
                let drops = host.hub.container_click(conn, &p, &mut self.world, &rules);
                if let Some(Slots::Block(key) | Slots::Double(key, _)) = slots
                    && slots.and_then(|s| s.read(&self.world, &host.hub.guests[gi])) != before
                {
                    self.container_changed(key);
                }
                if let Some(Slots::Block(key)) = slots {
                    self.store_cart_items(key);
                }
                self.drop_guest_items(host, gi, drops);
            }
            S::ContainerClose(p) => {
                if host.hub.guests[gi]
                    .menu
                    .as_ref()
                    .is_some_and(|m| m.window == p.container)
                {
                    self.guest_container_closed(host, conn);
                }
                host.hub.container_closed(conn, p.container);
                self.guest_carried_back(host, gi);
            }
            S::ChatCommand(c) => self.guest_chat_command(host, conn, &c.command),
            S::CustomPayload(p) if p.channel == vise_protocol::quiet::CHANNEL => {
                self.guest_quiet_commands(host, conn, &p.data)
            }
            S::ChatCommandSigned(p) => self.guest_signed_command(host, conn, p),
            S::ChangeDifficulty(p) if host.hub.guests[gi].permission >= 2 => {
                if !self.level.difficulty_locked && (0..=3).contains(&p.difficulty) {
                    self.ui.difficulty = if self.level.hardcore {
                        3
                    } else {
                        p.difficulty as u8
                    };
                    self.level.set_difficulty(self.ui.difficulty);
                    self.sync_rules();
                    if self.ui.difficulty == 0 {
                        self.mobs.retain(|m| !m.hostile());
                    }
                    host.hub
                        .broadcast(&C::ChangeDifficulty(play::ChangeDifficulty {
                            difficulty: i32::from(self.ui.difficulty),
                            locked: self.level.difficulty_locked,
                        }));
                }
            }
            S::LockDifficulty(p) if host.hub.guests[gi].permission >= 2 => {
                self.level.difficulty_locked = p.locked;
                host.hub
                    .broadcast(&C::ChangeDifficulty(play::ChangeDifficulty {
                        difficulty: i32::from(self.ui.difficulty),
                        locked: self.level.difficulty_locked,
                    }));
            }
            S::ClientCommand(p) if p.action == 2 && host.hub.guests[gi].permission >= 2 => {
                host.hub.send_to(
                    conn,
                    &C::GameRuleValues(play::GameRuleValues {
                        entries: vise_client::gamerules::RULES
                            .iter()
                            .map(|r| play::GameRuleEntry {
                                name: format!("minecraft:{}", r.name),
                                value: self.ui.rules.get(r.name).unwrap_or(r.default).to_string(),
                            })
                            .collect(),
                    }),
                );
            }
            S::SetGameRule(p) if host.hub.guests[gi].permission >= 2 => {
                for entry in p.entries {
                    if self.ui.rules.set_string(&entry.name, &entry.value) {
                        let message = vise_client::text::Text::tr(
                            "commands.gamerule.set",
                            &[&entry.name, &entry.value],
                        );
                        let content =
                            vise_client::lan::data::component_nbt(&message.component());
                        for g in &host.hub.guests {
                            if g.permission >= 2 {
                                g.send(&C::SystemChat(play::SystemChat {
                                    content: content.clone(),
                                    overlay: false,
                                }));
                            }
                        }
                        if self.options.dedicated.is_none() && self.allow_commands() {
                            self.system_message(message);
                        }
                    }
                }
                self.sync_rules();
            }
            S::TeleportToEntity(p) if spectator => self.guest_spectate_teleport(host, gi, p.uuid),
            S::ChangeGameMode(p) if host.hub.guests[gi].permission >= 2 => {
                if let Some(mode) = usize::try_from(p.mode)
                    .ok()
                    .and_then(|index| vise_client::command::args::GameMode::ALL.get(index))
                {
                    let lines = self.as_guest(host, conn, |g| {
                        g.chat_capture = Some(Vec::new());
                        g.change_game_mode(*mode);
                        g.chat_capture
                            .take()
                            .unwrap_or_default()
                            .iter()
                            .map(|t| vise_client::lan::data::component_nbt(&t.component()))
                            .collect::<Vec<_>>()
                    });
                    for content in lines.unwrap_or_default() {
                        host.hub.send_to(
                            conn,
                            &C::SystemChat(play::SystemChat {
                                content,
                                overlay: false,
                            }),
                        );
                    }
                    self.send_admin_feedback(host, Some(conn));
                }
            }
            S::EntityTagQuery(p) if host.hub.guests[gi].permission >= 2 => {
                if let Some(tag) = self.level_entity_tag(host, p.entity) {
                    host.hub.send_to(
                        conn,
                        &C::TagQuery(play::TagQuery {
                            transaction: p.transaction,
                            tag,
                        }),
                    );
                }
            }
            S::BlockEntityTagQuery(p) if host.hub.guests[gi].permission >= 2 => {
                let tag = self
                    .block_entity_tag(p.pos)
                    .unwrap_or(vise_world::nbt::Tag::End);
                host.hub.send_to(
                    conn,
                    &C::TagQuery(play::TagQuery {
                        transaction: p.transaction,
                        tag,
                    }),
                );
            }
            S::Unknown(id, payload)
                if id == vise_protocol::ids::play::serverbound::COMMAND_SUGGESTION =>
            {
                host.play.entry(conn).or_default().suggestion = Some(payload);
                self.guest_try_suggestions(host, conn);
            }
            S::PlaceRecipe(p) => self.lan_place_recipe(host, conn, &p),
            S::ClientCommand(c)
                if c.action == 0 && host.play.get(&conn).is_some_and(|p| p.won_game) =>
            {
                host.play.entry(conn).or_default().respawn_requested = true;
            }
            S::ClientCommand(c) if c.action == 0 => self.guest_respawn(host, conn),
            S::ClientCommand(c) if c.action == 1 => {
                let stats = host
                    .play
                    .get_mut(&conn)
                    .map(|p| p.stats.take_dirty())
                    .unwrap_or_default();
                host.hub.send_stats(conn, &stats);
            }
            S::PickItemFromBlock(p) => {
                self.guest_pick_item(host, conn, |g| {
                    let eye = g.eye_position()?;
                    let d2: f64 = (0..3)
                        .map(|a| {
                            let lo = f64::from(p.pos[a]);
                            (eye[a] - eye[a].clamp(lo, lo + 1.0)).powi(2)
                        })
                        .sum();
                    let range = g.reach() + 1.0;
                    if d2 >= range * range {
                        return None;
                    }
                    g.picked_stack(p.pos, g.creative && p.include_data)
                });
            }
            S::PickItemFromEntity(p) => {
                self.guest_pick_item(host, conn, |g| {
                    g.within_entity_range(p.id)
                        .then(|| g.entity_pick_result(p.id))
                        .flatten()
                });
            }
            _ => {}
        }
    }

    fn level_entity_tag(&mut self, host: &mut LanHost, id: i32) -> Option<vise_world::nbt::Tag> {
        use super::commands::Target;
        let player = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id == id && host.hub.here(g))
            .map(|g| g.conn);
        let mut tag = match player {
            Some(player) => self
                .as_guest(host, player, |g| g.entity_nbt(Target::Player))
                .flatten()?,
            None => {
                let target = self.level_entity(id)?;
                self.entity_nbt(target)?
            }
        };
        if let vise_world::nbt::Tag::Compound(c) = &mut tag {
            c.remove(&"id".into());
        }
        Some(tag)
    }

    fn guest_pick_item(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        stack: impl FnOnce(&mut Self) -> Option<ItemStack>,
    ) {
        let picked = self.as_guest(host, conn, |g| {
            let stack = stack(g)?;
            let infinite = g.creative;
            vise_client::pick_item::pick(
                &mut g.inventory[..36],
                &mut g.selected,
                &stack,
                infinite,
            );
            Some(())
        });
        if picked.flatten().is_some()
            && let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn)
        {
            host.hub.send_to(
                conn,
                &C::SetHeldSlot(play::SetHeldSlot {
                    slot: g.selected as i32,
                }),
            );
        }
    }

    fn item_from_network(
        &self,
        host: &LanHost,
        stack: &Option<vise_protocol::item::Stack>,
    ) -> Option<ItemStack> {
        let s = stack.as_ref()?;
        let mut out = host.hub.stack_from_network(s);
        out.count = out.count.max(1);
        Some(out)
    }

    fn guest_spectate_teleport(&mut self, host: &mut LanHost, gi: usize, uuid: u128) {
        let own = host.hub.guests[gi].conn;
        let host_player = self
            .player
            .as_ref()
            .filter(|_| self.options.dedicated.is_none() && uuid == host.host_uuid)
            .map(|p| {
                (
                    [p.position.x, p.position.y, p.position.z],
                    self.yaw,
                    self.pitch,
                    self.dim,
                )
            });
        let target = host_player.or_else(|| {
            host.hub
                .guests
                .iter()
                .find(|g| g.placed && g.conn != own && g.uuid == uuid)
                .map(|g| (g.position, g.yaw, g.pitch, Dim::from_id(&g.dimension)))
        });
        let Some((at, yaw, pitch, dim)) = target else {
            return;
        };
        if Dim::from_id(&host.hub.guests[gi].dimension) != dim {
            if self.options.dedicated.is_some() {
                self.level_teleports
                    .push(super::server_travel::LevelTeleport {
                        conn: own,
                        dim,
                        at,
                        rotation: Some([yaw, pitch]),
                    });
            }
            return;
        }
        let g = &mut host.hub.guests[gi];
        (g.yaw, g.pitch) = (yaw, pitch);
        g.motion = None;
        host.hub.teleport(gi, Some(at));
    }
    fn guest_tick_death(&mut self, host: &mut LanHost, gi: usize) {
        if host.hub.guests[gi].tick_death() {
            self.guest_drop_carried(host, gi);
        }
    }

    fn guest_drop_carried(&mut self, host: &mut LanHost, gi: usize) {
        if let Some(c) = host.hub.guests[gi].carried.take() {
            self.drop_guest_items(host, gi, vec![c]);
        }
    }

    pub(super) fn guest_carried_back(&mut self, host: &mut LanHost, gi: usize) {
        if let Some(c) = host.hub.guests[gi].carried.take() {
            self.lan_place_back(host, gi, vec![c]);
            host.hub.send_inventory(gi);
        }
    }

    pub(super) fn drop_guest_items(&mut self, host: &LanHost, gi: usize, stacks: Vec<ItemStack>) {
        let g = &host.hub.guests[gi];
        let eye = [
            g.position[0],
            g.position[1] + g.eye_height() - 0.3,
            g.position[2],
        ];
        let (yaw, pitch) = (
            f64::from(g.yaw).to_radians(),
            f64::from(g.pitch).to_radians(),
        );
        for stack in stacks {
            let v = [
                -yaw.sin() * pitch.cos() * 0.3,
                -pitch.sin() * 0.3 + 0.1,
                yaw.cos() * pitch.cos() * 0.3,
            ];
            self.spawn_item(stack, eye, v, 40);
        }
    }

    fn look_at_entity(
        &mut self,
        host: &mut LanHost,
        gi: usize,
        id: i32,
        location: Option<[f64; 3]>,
    ) -> Option<([f64; 3], [f64; 3])> {
        let eye = {
            let g = &host.hub.guests[gi];
            [g.position[0], g.position[1] + g.eye_height(), g.position[2]]
        };
        let object = || {
            let (lo, hi, origin) = self.object_target(id)?;
            let middle = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2.0);
            let Some(l) = location else {
                let carries = u32::try_from(id).is_ok_and(|v| {
                    self.boats.iter().any(|b| b.id == v) && !self.boat_mobs(v).is_empty()
                });
                if carries {
                    let near = |k: usize| eye[k].clamp(lo[k] + 0.01, hi[k] - 0.01);
                    return Some([near(0), lo[1] + 0.05, near(2)]);
                }
                return Some(middle);
            };
            let point = [0, 1, 2].map(|k| origin[k] + l[k]);
            let inside = [0, 1, 2].map(|k| {
                let margin = ((hi[k] - lo[k]) / 2.0).min(0.01);
                point[k].clamp(lo[k] + margin, hi[k] - margin)
            });
            let on = (0..3).all(|k| (point[k] - inside[k]).abs() < 0.1);
            Some(if on { inside } else { middle })
        };
        let centre = match self.mobs.iter().find(|m| m.id as i32 == id) {
            Some(m) => [
                m.position.x,
                m.position.y + m.kind.height * 0.5,
                m.position.z,
            ],
            None if self.object_target(id).is_some() => object()?,
            None => {
                let b = self.tracked_special_box(id)?;
                let (lo, hi) = (b.min_position(), b.max_position());
                [
                    (lo.x + hi.x) / 2.0,
                    (lo.y + hi.y) / 2.0,
                    (lo.z + hi.z) / 2.0,
                ]
            }
        };
        let g = &mut host.hub.guests[gi];
        let d = [centre[0] - eye[0], centre[1] - eye[1], centre[2] - eye[2]];
        let horizontal = (d[0] * d[0] + d[2] * d[2]).sqrt();
        g.yaw = (-d[0].atan2(d[2])).to_degrees() as f32;
        g.pitch = (-d[1].atan2(horizontal)).to_degrees() as f32;
        Some((centre, eye))
    }

    fn guest_jab(&mut self, host: &mut LanHost, conn: u32) {
        let Some(g) = host.hub.guest(conn) else {
            return;
        };
        let at = g.position;
        let sound = self
            .guest_hit_by(host, conn, |g| {
                if g.cannot_attack_with_item(g.attack_ticks as i32, 5) {
                    None
                } else {
                    g.jab()
                }
            })
            .flatten();
        if let Some(name) = sound {
            let source = vise_client::settings::sound_source(name) as i32;
            host.hub.sound(name, at, 1.0, 1.0, source, Some(conn));
        }
    }

    fn guest_target_box(&self, host: &LanHost, gi: usize, id: i32) -> Option<Aabb> {
        let conn = host.hub.guests[gi].conn;
        if id == host.host_entity {
            return self.player.as_ref().map(|p| p.bounds());
        }
        if let Some(g) = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id == id && g.conn != conn && host.hub.here(g))
        {
            return Some(g.bounds());
        }
        if let Some(m) = self.mobs.iter().find(|m| m.id as i32 == id) {
            return Some(m.bounds());
        }
        if let Some((lo, hi, _)) = self.object_target(id) {
            return Some(Aabb::new(
                Vec3::new(lo[0], lo[1], lo[2]),
                Vec3::new(hi[0], hi[1], hi[2]),
            ));
        }
        if let Some(b) = self.tracked_special_box(id) {
            return Some(b);
        }
        self.creatures
            .projectiles
            .iter()
            .find(|p| p.id != 0 && p.id as i32 == id)
            .map(|p| p.bounds())
    }

    fn guest_attack(&mut self, host: &mut LanHost, conn: u32, id: i32) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        if host.hub.guests[gi].spectator() {
            return;
        }
        if let Some(target) = self.guest_target_box(host, gi, id)
            && self.as_guest(host, conn, |g| g.within_attack_range(&target)) != Some(true)
        {
            return;
        }
        if let Some(victim) = Self::pvp_victim(host, conn, id) {
            if let Some(Some(strike)) = self.as_guest(host, conn, |g| g.pvp_strike())
                && self.pvp_land(host, victim, &strike)
            {
                let host_on_ground = self.player.as_ref().map(|p| p.ground.on_ground);
                let on_ground = Self::pvp_victim_on_ground(host, victim, host_on_ground);
                self.as_guest(host, conn, |g| {
                    g.weapon_hit_player(on_ground);
                    g.pvp_exhaustion();
                    if strike.stops_sprint() {
                        g.stop_sprint_after_extra_knockback();
                    }
                });
            }
            return;
        }
        if let Some(i) = self
            .creatures
            .projectiles
            .iter()
            .position(|p| p.id != 0 && p.id as i32 == id && super::deflect::redirectable(p))
        {
            self.as_guest(host, conn, |g| {
                if g.within_interaction_range(i) {
                    g.deflect(i);
                }
            });
            return;
        }
        let (old_yaw, old_pitch) = (host.hub.guests[gi].yaw, host.hub.guests[gi].pitch);
        if self.look_at_entity(host, gi, id, None).is_none() {
            return;
        }
        let sent = self
            .as_guest(host, conn, |g| {
                let marked = g
                    .player
                    .as_mut()
                    .is_some_and(|p| std::mem::take(&mut p.health.sync_velocity));
                g.attack();
                let p = g.player.as_mut()?;
                let sent = std::mem::replace(&mut p.health.sync_velocity, marked);
                sent.then_some(p.velocity)
            })
            .flatten();
        self.send_guest_motion(host, conn, sent);
        if let Ok(id) = u32::try_from(id) {
            host.mob_players.insert(id, conn);
        }
        host.hub.guests[gi].yaw = old_yaw;
        host.hub.guests[gi].pitch = old_pitch;
    }

    pub(super) fn guest_command(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        command: &str,
        signed: Option<Vec<(String, vise_client::lan::hub::chat::ChatMessage)>>,
    ) {
        if self.guest_message_command(host, conn, command, signed.as_ref()) {
            return;
        }
        let words: Vec<&str> = command.trim_start_matches('/').split(' ').collect();
        if words[0] == "list" && (words.len() == 1 || words[1..] == ["uuids"]) {
            let me = (self.options.dedicated.is_none()
                && self.player.is_some()
                && host.hub.singleplayer_owner.is_none())
            .then_some((host.host_uuid, host.host_name.as_str()));
            let content = host.hub.list_reply(me, words.len() == 2);
            host.hub.send_to(
                conn,
                &C::SystemChat(play::SystemChat {
                    content,
                    overlay: false,
                }),
            );
            return;
        }
        let integrated = host.hub.config.integrated;
        if let Some(level) = Self::native_command_level(host, command.trim_start_matches('/'))
            && let Some(permission) = host.hub.guest(conn).map(|g| g.permission)
            && permission >= level
        {
            host.commands.push(GuestCommand {
                conn,
                command: command.trim_start_matches('/').to_owned(),
                permission,
                integrated,
                quiet: false,
            });
            return;
        }
        let allowed = self.options.dedicated.is_some() || self.allow_commands();
        if !integrated && (!host.settings.guest_commands || !allowed) {
            host.hub.send_to(
                conn,
                &C::SystemChat(play::SystemChat {
                    content: vise_protocol::text::translatable("command.unknown.command", &[]),
                    overlay: false,
                }),
            );
            return;
        }
        let Some(permission) = host.hub.guest(conn).map(|g| g.permission) else {
            return;
        };
        host.commands.push(GuestCommand {
            conn,
            command: command.trim_start_matches('/').to_owned(),
            permission,
            integrated,
            quiet: false,
        });
    }

    pub(super) fn run_guest_commands(&mut self) {
        loop {
            let Some(host) = self.lan.as_mut() else {
                return;
            };
            if host.commands.is_empty() {
                return;
            }
            let c = host.commands.remove(0);
            self.run_guest_command(c);
        }
    }

    pub(super) fn run_guest_command(&mut self, c: GuestCommand) -> bool {
        let Some(mut host) = self.lan.take() else {
            return false;
        };
        let Some(swap) = self.enter_guest(&mut host, c.conn) else {
            self.lan = Some(host);
            return false;
        };
        self.lan = Some(host);
        self.chat_capture = Some(Vec::new());
        let level = std::mem::replace(&mut self.ui.registries.permission, c.permission);
        let ok = if self.perform_native_command(c.conn, c.permission, &c.command) {
            true
        } else if c.quiet {
            self.run_command_quietly(&c.command)
        } else {
            self.run_command(&c.command)
        };
        self.ui.registries.permission = level;
        let lines = self.chat_capture.take().unwrap_or_default();
        let mut host = self.lan.take().expect("a command puts the hub back");
        self.leave_guest(&mut host, swap);
        host.hub.sync_all_effects();
        if !c.integrated {
            self.run_level_teleports(&mut host);
        }
        self.announce_guest_advancements(&mut host);
        for line in lines {
            host.hub.send_to(
                c.conn,
                &C::SystemChat(play::SystemChat {
                    content: vise_client::lan::data::component_nbt(&line.component()),
                    overlay: false,
                }),
            );
        }
        self.send_admin_feedback(&mut host, Some(c.conn));
        self.lan = Some(host);
        ok
    }

    pub(super) fn send_admin_feedback(&mut self, host: &mut LanHost, source: Option<u32>) {
        let host_op = source.is_some()
            && !host.hub.config.dedicated
            && (self.allow_commands() || host.hub.config.guest_commands);
        for (line, ops, console) in std::mem::take(&mut self.admin_feedback) {
            if ops {
                let content = vise_client::lan::data::component_nbt(&line.component());
                for g in host
                    .hub
                    .guests
                    .iter()
                    .filter(|g| Some(g.conn) != source && g.permission > 0)
                {
                    g.send(&C::SystemChat(play::SystemChat {
                        content: content.clone(),
                        overlay: false,
                    }));
                }
                if host_op {
                    self.system_message(line.clone());
                }
            }
            if console {
                super::dedicated::log("INFO", &format!("System chat: {}", line.plain()));
            }
        }
    }

    fn guest_try_suggestions(&mut self, host: &mut LanHost, conn: u32) {
        let play = host.play.entry(conn).or_default();
        if !play.suggestion_budget {
            return;
        }
        let Some(payload) = play.suggestion.take() else {
            return;
        };
        play.suggestion_budget = false;
        self.guest_suggestions(host, conn, &payload);
    }

    fn guest_suggestions(&mut self, host: &mut LanHost, conn: u32, payload: &[u8]) {
        let mut r = vise_protocol::buf::Reader::new(payload);
        let (Ok(id), Ok(text)) = (r.varint(), r.string(32500)) else {
            return;
        };
        let Some(permission) = host.hub.guest(conn).map(|g| g.permission) else {
            return;
        };
        let skip = usize::from(text.starts_with('/'));
        let command = &text[skip..];
        let d = vanilla::game_dispatcher();
        let level = std::mem::replace(&mut self.ui.registries.permission, permission);
        let parse = d.parse(command, &self.ui.registries);
        let ((start, end), mut suggestions) =
            d.suggestions(&parse, command.len(), &self.ui.registries);
        self.ui.registries.permission = level;
        suggestions.truncate(1000);
        let utf16 = |byte: usize| command[..byte.min(command.len())].encode_utf16().count() as i32;
        let mut w = vise_protocol::buf::Writer::new();
        w.varint(id);
        w.varint(utf16(start) + skip as i32);
        w.varint(utf16(end) - utf16(start));
        w.varint(suggestions.len() as i32);
        for s in &suggestions {
            w.string(&s.text);
            w.bool(s.tooltip.is_some());
            if let Some(tip) = &s.tooltip {
                w.nbt(&literal(tip));
            }
        }
        host.hub.send_to(
            conn,
            &C::Unknown(
                vise_protocol::ids::play::clientbound::COMMAND_SUGGESTIONS,
                w.data,
            ),
        );
    }

    fn guest_respawn(&mut self, host: &mut LanHost, conn: u32) {
        let Some(i) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let world = self
            .level
            .world_spawn
            .map(|[x, y, z]| [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]);
        let spawn = host.hub.guests[i]
            .spawn
            .or(world)
            .or(self.spawn.map(|s| [s.x, s.y, s.z]))
            .unwrap_or([0.5, 80.0, 0.5]);
        if !host.hub.guests[i].dead {
            return;
        }
        self.guest_drop_carried(host, i);
        let server = self.options.dedicated.is_some();
        let own = host.hub.guests[i]
            .respawn_elsewhere
            .as_ref()
            .map(RespawnConfig::from_saved);
        let elsewhere = own.filter(|_| server);
        let in_frame_bed = own.filter(|c| !server && c.dim == super::Dim::Overworld);
        let spawn = in_frame_bed.map_or(spawn, |c| {
            [
                f64::from(c.pos[0]) + 0.5,
                f64::from(c.pos[1]) + 0.6,
                f64::from(c.pos[2]) + 0.5,
            ]
        });
        let level = elsewhere.map_or(super::Dim::Overworld, |c| c.dim);
        let spawn = elsewhere.map_or(spawn, |c| {
            [
                f64::from(c.pos[0]) + 0.5,
                f64::from(c.pos[1]),
                f64::from(c.pos[2]) + 0.5,
            ]
        });
        let back = self.dim;
        let away = server && host.hub.guests[i].dimension != level.id();
        if server {
            self.server_trips.retain(|t| !t.is(conn));
        }
        if away && let Err(e) = self.enter_level(Some(host), level) {
            log::error!("could not open {}: {e}", level.id());
            return;
        }
        let g = &mut host.hub.guests[i];
        g.dead = false;
        g.death_time = 0;
        g.health = 20.0;
        g.food = 20;
        g.saturation = 5.0;
        g.sent_health = None;
        if let Some(m) = g.motion.take() {
            let m = *m;
            g.logs_kept = Some((m.hurt_log, m.attack_log));
        }
        let mut xp = Experience {
            level: g.xp_level,
            progress: g.xp_progress,
            total: g.xp_total,
        };
        xp.respawn(self.ui.rules.bool("keep_inventory") || g.game_mode == SPECTATOR);
        (g.xp_level, g.xp_progress, g.xp_total) = (xp.level, xp.progress, xp.total);
        let clock_rate = self.network_clock_rate();
        let ctx = TickCtx {
            world: &mut self.world,
            host: None,
            entities: &[],
            next_entity: &mut self.next_entity,
            spawn,
            difficulty: i32::from(self.ui.difficulty),
            difficulty_locked: self.level.difficulty_locked,
            clock_rate,
            tick_rate: self.clock.rate(),
            tick_frozen: self.clock.frozen(),
            tick_steps: self.clock.step_state(),
            collision: Some(self.collision),
            border: &self.border,
            streaming: Some(&self.streaming),
        };
        host.hub.respawn(i, spawn, &ctx);
        if self.level.hardcore {
            host.hub.set_game_mode(conn, SPECTATOR);
        }
        if let Some(config) = elsewhere {
            self.respawn_trip(conn, config);
        } else if in_frame_bed.is_none() && host.hub.guests[i].spawn.is_none() {
            self.world_spawn_trip(conn);
        }
        if let Some(c) = in_frame_bed
            && self.dim == super::Dim::Overworld
            && self.world.loaded(c.pos[0], c.pos[2])
        {
            let found = self
                .as_guest(host, conn, |g| g.resolve_server_respawn(c))
                .flatten();
            let world = self
                .level
                .world_spawn
                .map_or([0.5, 80.0, 0.5], |[x, y, z]| {
                    [f64::from(x) + 0.5, f64::from(y), f64::from(z) + 0.5]
                });
            let at = found.map_or(world, |(at, yaw)| {
                host.hub.guests[i].yaw = yaw;
                [at.x, at.y, at.z]
            });
            if found.is_none() {
                self.world_spawn_trip(conn);
            }
            host.hub.teleport(i, Some(at));
        }
        if away && let Err(e) = self.enter_level(Some(host), back) {
            log::error!("could not reopen {}: {e}", back.id());
        }
    }

    pub(super) fn tick_guests(&mut self, host: &mut LanHost) {
        self.lan_merchant_tick(host);
        self.lan_stations_tick(host);
        self.lan_tables_tick(host);
        self.lan_containers_tick(host);
        self.tick_lan_mount_menus(host);
        self.announce_guest_advancements(host);
        let conns: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded)
            .map(|g| g.conn)
            .collect();
        if host.play.values().any(|p| p.hook.is_some()) {
            self.refresh_hook_targets(host);
        }
        for conn in conns {
            let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
                continue;
            };
            let play = host.play.entry(conn).or_default();
            play.chat_spam = play.chat_spam.saturating_sub(1);
            play.command_spam = play.command_spam.saturating_sub(1);
            play.suggestion_budget = true;
            self.guest_try_suggestions(host, conn);
            self.guest_sleep_tick(host, gi);
            if host.hub.guests[gi].dead {
                self.guest_tick_death(host, gi);
                continue;
            }
            if self.on_server_trip(conn) {
                self.guest_die(host, gi, conn);
                continue;
            }
            let play = host.play.entry(conn).or_default();
            play.creative_drops = play.creative_drops.saturating_sub(1);
            let digging = play.digging;
            let using = play.using.is_some();
            let delayed = play.destroy_delay > 0 || play.use_delay > 0 || play.attack_ticks < 100;
            if digging.is_some() || using || delayed {
                self.as_guest(host, conn, |g| {
                    g.attack_ticks = g.attack_ticks.saturating_add(1);
                    if let Some((pos, face)) = digging {
                        g.forced_hit = Some(Hit {
                            pos,
                            face,
                            point: [
                                f64::from(pos[0]) + 0.5,
                                f64::from(pos[1]) + 0.5,
                                f64::from(pos[2]) + 0.5,
                            ],
                        });
                    }
                    let _ = g.tick_mining();
                    g.forced_hit = None;
                    g.right = g.using.is_some();
                    g.tick_using();
                    if g.use_delay > 0 {
                        g.use_delay -= 1;
                    }
                });
                if digging.is_some()
                    && let Some(play) = host.play.get_mut(&conn)
                    && (host.hub.guests[gi].creative() || play.mining.is_none())
                {
                    play.digging = None;
                }
            }
            let play = host.play.entry(conn).or_default();
            play.shield_cooldown = play.shield_cooldown.saturating_sub(1);
            for group in play.cooldowns.tick() {
                host.hub.send_to(
                    conn,
                    &C::Cooldown(play::Cooldown {
                        group: group.to_string(),
                        duration: 0,
                    }),
                );
            }
            let g = &host.hub.guests[gi];
            let held = g
                .inventory
                .get(g.selected)
                .cloned()
                .flatten()
                .map(|s| s.item);
            let play = host.play.entry(conn).or_default();
            if play.attack_item != held {
                play.attack_ticks = 0;
                play.attack_item = held;
            }
            self.guest_delayed_destroy(host, conn);
            self.guest_server_tick(host, conn);
            let sleeping = host.hub.guests[gi].sleeping;
            self.as_guest(host, conn, |g| {
                g.tick_fishing();
                g.tick_maps();
                g.tick_lodestone_compasses();
                g.count_tick_stats(sleeping);
                g.tick_advancements();
                g.update_player_attributes();
            });
            self.lan_map_updates(host, conn);
            host.hub.flush_progress(conn);
            self.guest_dig_feedback(host, conn);
            self.guest_pickups(host, gi);
            self.guest_arrows(host, gi);
            self.guest_orbs(host, gi);
            self.guest_die(host, gi, conn);
        }
    }

    fn guest_die(&mut self, host: &mut LanHost, gi: usize, conn: u32) {
        if host.hub.guests[gi].health <= 0.0 && !host.hub.guests[gi].dead {
            self.guest_forget_digging(host, conn);
            if let Some(play) = host.play.get_mut(&conn) {
                play.hook = None;
            }
        }
        let here = host.hub.here(&host.hub.guests[gi]);
        let g = &mut host.hub.guests[gi];
        if g.health <= 0.0 && !g.dead {
            g.dead = true;
            g.last_death = Some((g.dimension.clone(), g.position.map(|v| v.floor() as i32)));
            let (id, at) = (g.entity_id, g.position);
            let reward = Experience {
                level: g.xp_level,
                progress: g.xp_progress,
                total: g.xp_total,
            }
            .death_reward(self.ui.rules.bool("keep_inventory") || g.game_mode == SPECTATOR);
            let (name, uuid) = (g.name.clone(), g.uuid);
            let drops =
                (g.game_mode != SPECTATOR).then(|| [at[0], at[1] + g.eye_height() - 0.3, at[2]]);
            let cause = host.play.get_mut(&conn).and_then(|p| p.death_cause.take());
            if here {
                self.tell_neutral_mobs_player_died(at, Some(conn), &host.mob_players);
            }
            let message = if self.ui.rules.bool("show_death_messages") {
                let me = self.player_text(&name, uuid);
                let text = match cause {
                    Some(key) => vise_client::text::Text::translate(&key, &[me]),
                    None if at[1] < f64::from(self.world.bottom_y() - 64) => {
                        vise_client::text::Text::translate("death.attack.outOfWorld", &[me])
                    }
                    None => self.guest_death_message(conn, me),
                };
                let message = vise_client::lan::data::component_nbt(&text.component());
                let board = &self.score.board;
                host.hub.system_message_to(message.clone(), |other| {
                    vise_client::scoreboard::death_message_reaches(board, &name, other)
                });
                if self.options.dedicated.is_some() {
                    if board.players_team(&name).is_none_or(|t| {
                        t.death_message_visibility == vise_client::scoreboard::Visibility::Always
                    }) {
                        super::dedicated::log("INFO", &format!("System chat: {}", text.plain()));
                    }
                } else if self.player.is_some()
                    && vise_client::scoreboard::death_message_reaches(
                        board,
                        &name,
                        super::chat::PLAYER_NAME,
                    )
                {
                    self.system_message(text);
                }
                message
            } else {
                literal("")
            };
            host.hub.send_to(
                conn,
                &C::PlayerCombatKill(play::PlayerCombatKill { id, message }),
            );
            host.hub.entity_event(id, 3, at);
            if let Some(eyes) = drops {
                self.as_guest(host, conn, |game| game.drop_inventory(eyes));
            }
            self.spawn_orbs(at, reward);
        }
    }

    fn guest_death_message(
        &mut self,
        conn: u32,
        me: vise_client::text::Text,
    ) -> vise_client::text::Text {
        let mut tracker = self.guest_combat.remove(&conn).unwrap_or_default();
        std::mem::swap(&mut self.combat, &mut tracker);
        let text = self.combat_death_message_for(me);
        std::mem::swap(&mut self.combat, &mut tracker);
        self.guest_combat.insert(conn, tracker);
        text
    }

    fn guest_sleep_tick(&mut self, host: &mut LanHost, gi: usize) {
        let g = &mut host.hub.guests[gi];
        g.sleep_timer = super::tick_sleep_timer(g.sleep_timer, g.bed.is_some());
        if let Some(head) = g.bed {
            let block = self.world.block(head[0], head[1], head[2]);
            let bed = self.res.facts().get(block).name.ends_with("_bed");
            if g.dead || g.health <= 0.0 || !bed {
                host.hub.wake(gi);
            }
        }
    }

    fn sleep_counts(&self) -> (i64, i64, i64) {
        let (mut active, mut sleeping, mut deep) = (0i64, 0i64, 0i64);
        let mut count = |in_bed: bool, timer: u32| {
            active += 1;
            if in_bed {
                sleeping += 1;
                if timer >= 100 {
                    deep += 1;
                }
            }
        };
        if self.player.is_some() && !self.spectator() {
            count(self.sleeping.is_some(), self.sleep_timer);
        }
        if let Some(host) = self.lan.as_ref() {
            for g in host
                .hub
                .guests
                .iter()
                .filter(|g| g.placed && !g.spectator())
            {
                count(g.bed.is_some(), g.sleep_timer);
            }
        }
        (active, sleeping, deep)
    }

    fn sleepers_needed(&self, active: i64) -> i64 {
        let percent = i64::from(self.ui.rules.int("players_sleeping_percentage").max(0));
        ((active * percent + 99) / 100).max(1)
    }

    pub(super) fn enough_slept(&self) -> bool {
        let (active, sleeping, deep) = self.sleep_counts();
        let needed = self.sleepers_needed(active);
        sleeping >= needed && deep >= needed
    }

    pub(super) fn announce_sleep_status(&mut self) {
        let (active, sleeping, _) = self.sleep_counts();
        let (old_active, old_sleeping) = self.sleep_seen;
        self.sleep_seen = (active, sleeping);
        let changed = (old_sleeping > 0 || sleeping > 0)
            && (old_active != active || old_sleeping != sleeping);
        let published = self.lan.is_some() || self.options.dedicated.is_some();
        if !changed || !published {
            return;
        }
        if self.ui.rules.int("players_sleeping_percentage") > 100 {
            return;
        }
        let needed = self.sleepers_needed(active);
        let text = if sleeping >= needed {
            vise_client::text::Text::translate("sleep.skipping_night", &[])
        } else {
            vise_client::text::Text::translate(
                "sleep.players_sleeping",
                &[
                    vise_client::text::Text::literal(sleeping.to_string()),
                    vise_client::text::Text::literal(needed.to_string()),
                ],
            )
        };
        if let Some(host) = self.lan.as_ref() {
            host.hub
                .overlay_message(vise_client::lan::data::component_nbt(&text.component()));
        }
        if self.options.dedicated.is_none() {
            self.titles.set_overlay(text, false);
        }
    }

    pub(super) fn wake_guests(&mut self) {
        if let Some(host) = self.lan.as_mut() {
            for i in 0..host.hub.guests.len() {
                host.hub.wake(i);
            }
        }
    }

    pub(super) fn announce_guest_advancements(&mut self, host: &mut LanHost) {
        for text in std::mem::take(&mut self.adv.announced) {
            host.hub
                .system_message(vise_client::lan::data::component_nbt(&text.component()));
            if self.options.dedicated.is_none() {
                self.system_message(text);
            }
        }
    }

    fn guest_server_tick(&mut self, host: &mut LanHost, conn: u32) {
        self.guest_vehicle_tick(host, conn);
        let fell = host
            .hub
            .guest_mut(conn)
            .and_then(|g| g.moves.last().map(|m| m.0[1]))
            .unwrap_or(0.0);
        self.guest_glide_tick(host, conn, fell);
        let Some(g) = host.hub.guest_mut(conn) else {
            return;
        };
        let moves = std::mem::take(&mut g.moves);
        let from_ground = if moves.is_empty() {
            g.on_ground
        } else {
            g.moves_from_ground
        };
        if g.spectator() {
            if !g.effects.list.is_empty() {
                self.as_guest(host, conn, |game| game.tick_mob_effects());
            }
            self.as_guest(host, conn, |game| game.handle_shoulder_entities());
            return;
        }
        let in_wall = !g.sleeping && self.in_wall(g.position, g.dimensions());
        let at = g.position;
        let (sneaking, sleeping) = (g.sneaking, g.sleeping);
        let riding = g.vehicle.is_some();
        let mut soul = None;
        let events = self.as_guest(host, conn, |game| {
            let (rules, motion) = (game.world.health_rules, game.motion);
            let health = &motion.health;
            let freeze_immune = game.wears_freeze_immune();
            let burn_immune = game.wears_burn_from_stepping_immunity();
            let Some(player) = game.player.as_mut() else {
                return (Vec::new(), None);
            };
            let mut w = MotionWorld {
                entities: &[],
                player,
                scene: &mut game.world,
                assets: &game.pack.assets,
                collision_binding: game.collision,
                resources: game.motion,
            };
            w.player.health.advance_clock();
            let _ = w.fluid_base_tick();
            w.player.ground.on_ground = from_ground;
            if w.player.flying && !riding {
                w.player.fall_distance = 0.0;
            }
            let travelled: Vec<[f64; 3]> = moves.iter().map(|m| m.0).collect();
            let end = w.player.position;
            let start = travelled
                .iter()
                .fold(end, |p, d| p.subtract(Vec3::new(d[0], d[1], d[2])));
            w.player.history.clear();
            let mut from = start;
            for d in &travelled {
                let delta = Vec3::new(d[0], d[1], d[2]);
                let to = from.add_vector(delta);
                w.player
                    .history
                    .add(vise_world::entity_move::MovementRecord {
                        from,
                        to,
                        original: Some(delta),
                    });
                from = to;
            }
            for (moved, on_ground) in moves {
                let movement = Vec3::new(moved[0], moved[1], moved[2]);
                if w.player.ground.on_ground && !on_ground && moved[1] > 0.0 {
                    let _ = w.jump_from_ground();
                }
                let landed = (|| -> vise_client::Result<()> {
                    w.set_on_ground_with_movement(on_ground, false, movement)?;
                    let pos = w.legacy_on_position()?;
                    let state = w.block_state(pos)?;
                    w.check_fall_damage(moved[1], on_ground, state, pos)?;
                    w.check_movement_statistics(movement)
                })();
                if landed.is_err() {
                    break;
                }
            }
            if w.climbable().unwrap_or(false) {
                w.player.fall_distance = 0.0;
            }
            w.player.shift_key_down = sneaking;
            let _ = w.player.set_sleeping(sleeping);
            w.player.freeze_immune = freeze_immune;
            w.player.burn_from_stepping_immune = burn_immune;
            let _ = w.server_block_damage(start);
            let _ = w.update_soul_speed_block();
            let _ = w.tick_environment_damage();
            let _ = w.apply_fire_contact();
            let _ = w.server_hazards_tick(in_wall);
            let _ = w.server_player_pose_tick();
            let mut events = std::mem::take(&mut w.player.events);
            for event in &events {
                if let MotionEvent::CauldronExtinguish { pos } = event {
                    game.cauldron_extinguish(*pos);
                }
            }
            game.rain_extinguish_player();
            for event in &events {
                if matches!(event, MotionEvent::InsideWitherRose) {
                    game.wither_rose_contact();
                }
            }
            for d in travelled {
                game.movement_stats(Vec3::new(d[0], d[1], d[2]));
            }
            game.location_changed_effects([start.x, start.y, start.z], from_ground);
            game.border_damage();
            game.tick_mob_effects();
            soul = game.tick_soul_speed();
            game.handle_shoulder_entities();
            let Some(player) = game.player.as_mut() else {
                return (Vec::new(), None);
            };
            let _ = player.peaceful_regeneration(rules);
            let _ = player.food_tick(health, rules);
            events.append(&mut player.events);
            let _ = MotionWorld {
                entities: &[],
                player,
                scene: &mut game.world,
                assets: &game.pack.assets,
                collision_binding: game.collision,
                resources: game.motion,
            }
            .server_travel();
            events.append(&mut player.events);
            let marked = std::mem::take(&mut player.health.sync_velocity);
            let velocity = marked.then_some(player.velocity);
            for event in &events {
                if let MotionEvent::Fall(vise_client::landing::FallEvent::Statistic(cm)) = event
                {
                    game.stats.custom("fall_one_cm", *cm);
                }
            }
            game.record_combat(&events);
            game.wear_armor(&events);
            (events, velocity)
        });
        let Some((events, marked)) = events else {
            return;
        };
        if let Some((at, velocity)) = soul {
            super::lan_particles::send_soul_particle(host, at, velocity);
        }
        self.as_guest(host, conn, |game| {
            game.sculk_step_on_player();
            game.tick_player_sculk_events();
            for event in &events {
                if matches!(event, MotionEvent::Bounce { .. }) {
                    game.post_entity_event("minecraft:bounce", vise_client::mobs::Who::Player);
                }
            }
        });
        for event in events {
            match event {
                MotionEvent::HurtSound(sound) => self.player_hurt_sound(sound, at),
                MotionEvent::Fall(vise_client::landing::FallEvent::Particles {
                    state,
                    position,
                    count,
                    speed,
                }) => self.landing_dust(Some(host), state, position, count, speed),
                MotionEvent::Fall(vise_client::landing::FallEvent::Sound {
                    sound,
                    volume,
                    pitch,
                    ..
                }) => {
                    if let Some(name) = sound.player_fall_sound() {
                        let source = vise_client::settings::sound_source(name) as i32;
                        self.audio.play_unshared(name, Some(at), volume, pitch);
                        host.hub.sound(name, at, volume, pitch, source, Some(conn));
                    }
                }
                MotionEvent::Fall(event) => self.landing_block_updates(&event),
                MotionEvent::Drown => {
                    if let Some(g) = host.hub.guest(conn) {
                        host.hub
                            .entity_event(g.entity_id, super::combat::DROWN_EVENT, at);
                    }
                    self.drown_particles(at, [0.0; 3]);
                }
                MotionEvent::HoneySlide {
                    pos,
                    sound,
                    particles,
                } => {
                    if self.world.game_time % 20 == 0 {
                        self.as_guest(host, conn, |game| {
                            game.trigger_honey_slide([pos.x, pos.y, pos.z])
                        });
                    }
                    let Some((id, at)) = host.hub.guest(conn).map(|g| (g.entity_id, g.position))
                    else {
                        continue;
                    };
                    if sound {
                        let name = "minecraft:block.honey_block.slide";
                        let source = vise_client::settings::sound_source(name) as i32;
                        self.audio.play_unshared(name, Some(at), 1.0, 1.0);
                        host.hub.sound(name, at, 1.0, 1.0, source, Some(conn));
                    }
                    if particles {
                        self.honey_slide_particles(at);
                        host.hub.entity_event(id, 53, at);
                    }
                }
                _ => {}
            }
        }
        if let (Some(v), Some(g)) = (marked, host.hub.guest(conn)) {
            g.send(&C::SetEntityMotion(play::SetEntityMotion {
                id: g.entity_id,
                velocity: [v.x, v.y, v.z],
            }));
        }
    }

    fn lan_start_gliding(&mut self, host: &mut LanHost, conn: u32) {
        let Some(g) = host.hub.guest_mut(conn) else {
            return;
        };
        let start = !g.fall_flying && self.guest_can_glide(host, conn) == Some(true);
        let in_liquid = self
            .as_guest(host, conn, |game| {
                game.player
                    .as_ref()
                    .is_some_and(|p| p.fluids.was_touching_water || p.is_in_lava())
            })
            .unwrap_or(true);
        if let Some(g) = host.hub.guest_mut(conn) {
            g.fall_flying = start && !in_liquid;
        }
    }

    fn guest_can_glide(&mut self, host: &mut LanHost, conn: u32) -> Option<bool> {
        let g = host.hub.guest_mut(conn)?;
        if g.flying || g.on_ground || g.vehicle.is_some() {
            return Some(false);
        }
        self.as_guest(host, conn, |game| {
            game.glider_worn() && game.potions.effects.get("levitation").is_none()
        })
    }

    fn guest_glide_tick(&mut self, host: &mut LanHost, conn: u32, fell: f64) {
        let Some(g) = host.hub.guest_mut(conn) else {
            return;
        };
        if !g.fall_flying {
            if let Some(m) = g.motion.as_mut() {
                m.fall_fly_ticks = 0;
            }
            return;
        }
        let creative = g.creative();
        if let Some(m) = g.motion.as_mut()
            && fell > -0.5
            && m.fall_distance > 1.0
        {
            m.fall_distance = 1.0;
        }
        if self.guest_can_glide(host, conn) != Some(true) {
            if let Some(g) = host.hub.guest_mut(conn) {
                g.fall_flying = false;
            }
            return;
        }
        self.as_guest(host, conn, |game| {
            let Some(p) = game.player.as_mut() else {
                return;
            };
            p.fall_fly_ticks += 1;
            let next = p.fall_fly_ticks + 1;
            let at = [p.position.x, p.position.y, p.position.z];
            if next % 10 != 0 {
                return;
            }
            if (next / 10) % 2 == 0
                && !creative
                && let Some(stack) = game.armor[1].clone()
            {
                let loss = game.wear(&stack, 1);
                if let Some(elytra) = game.armor[1].as_mut() {
                    elytra.damage += loss;
                }
            }
            let ctx = game.player_context(None);
            game.post_game_event("minecraft:elytra_glide", at, ctx);
        });
    }

    fn in_wall(&self, at: [f64; 3], body: Dimensions) -> bool {
        let y = (at[1] + f64::from(body.eye_height)).floor() as i32;
        let [xs, zs] = eye_columns(at, body);
        xs.into_iter().any(|x| {
            zs.clone().any(|z| {
                let f = self.res.facts().get(self.world.block(x, y, z));
                f.full_collision && f.solid_render
            })
        })
    }

    fn guest_stop_digging(&mut self, host: &mut LanHost, conn: u32, pos: [i32; 3]) {
        let ticks = host.hub.ticks;
        let play = host.play.entry(conn).or_default();
        let dug = play.digging.take().is_some_and(|(p, _)| p == pos);
        play.mining = None;
        let start = play.dig_start;
        let state = self.world.block(pos[0], pos[1], pos[2]);
        if !dug {
            return;
        }
        if self.res.facts().is_air(state) {
            self.guest_crack_gone(host, conn, pos);
            return;
        }
        let progress = self
            .as_guest(host, conn, |g| g.destroy_progress(pos))
            .map_or(0.0, |p| p * (ticks - start + 1) as f32);
        if progress >= 0.7 {
            self.guest_destroy(host, conn, pos);
        } else {
            let play = host.play.entry(conn).or_default();
            if play.delayed.is_none() {
                play.delayed = Some((pos, start));
            }
        }
    }

    fn guest_delayed_destroy(&mut self, host: &mut LanHost, conn: u32) {
        let Some((pos, start)) = host.play.get(&conn).and_then(|p| p.delayed) else {
            return;
        };
        let ticks = host.hub.ticks;
        let state = self.world.block(pos[0], pos[1], pos[2]);
        let progress = if self.res.facts().is_air(state) {
            None
        } else {
            self.as_guest(host, conn, |g| g.destroy_progress(pos))
                .map(|p| p * (ticks - start + 1) as f32)
        };
        match progress {
            Some(p) if p < 1.0 => {}
            done => {
                if let Some(play) = host.play.get_mut(&conn) {
                    play.delayed = None;
                }
                if done.is_some() {
                    self.guest_destroy(host, conn, pos);
                }
            }
        }
    }

    fn guest_destroy(&mut self, host: &mut LanHost, conn: u32, pos: [i32; 3]) {
        self.as_guest(host, conn, |g| {
            let _ = g.break_block(pos);
        });
        self.guest_crack_gone(host, conn, pos);
    }

    fn guest_dig_feedback(&mut self, host: &mut LanHost, conn: u32) {
        let Some(gi) = host.hub.guests.iter().position(|g| g.conn == conn) else {
            return;
        };
        let play = host.play.entry(conn).or_default();
        let Some((pos, _)) = play.digging else {
            return;
        };
        if play.last_progress >= 0
            && self
                .res
                .facts()
                .is_air(self.world.block(pos[0], pos[1], pos[2]))
        {
            play.last_progress = -1;
            self.guest_crack_gone(host, conn, pos);
            return;
        }
        let progress = play
            .mining
            .as_ref()
            .map_or(-1, |m| (m.progress.clamp(0.0, 1.0) * 10.0) as i32 - 1);
        let swinging = play.swing.swinging();
        if progress != play.last_progress && progress >= 0 {
            play.last_progress = progress;
            self.destroy_block_progress(host.hub.guests[gi].entity_id, pos, progress);
            host.hub.broadcast_near(
                [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])],
                64.0,
                &C::BlockDestruction(play::BlockDestruction {
                    id: host.hub.guests[gi].entity_id,
                    pos,
                    progress: progress as u8,
                }),
                Some(conn),
            );
        }
        let _ = swinging;
    }

    fn guest_forget_digging(&mut self, host: &mut LanHost, conn: u32) {
        let Some(play) = host.play.get_mut(&conn) else {
            return;
        };
        let cracked = play.digging.take().filter(|_| play.last_progress >= 0);
        play.mining = None;
        play.delayed = None;
        play.destroy_delay = 0;
        play.using = None;
        play.last_progress = -1;
        if let Some((pos, _)) = cracked {
            self.guest_crack_gone(host, conn, pos);
        }
    }
    fn guest_crack_gone(&mut self, host: &mut LanHost, conn: u32, pos: [i32; 3]) {
        let Some(id) = host.hub.guest(conn).map(|g| g.entity_id) else {
            return;
        };
        self.destroy_block_progress(id, pos, -1);
        host.hub.broadcast_near(
            [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])],
            64.0,
            &C::BlockDestruction(play::BlockDestruction {
                id,
                pos,
                progress: 255,
            }),
            Some(conn),
        );
    }

    fn guest_pickups(&mut self, host: &mut LanHost, gi: usize) {
        let (conn, mode, position, id, uuid) = {
            let g = &host.hub.guests[gi];
            (
                g.conn,
                g.game_mode,
                g.position,
                g.entity_id,
                vise_client::lan::playerdata::uuid_ints(g.uuid),
            )
        };
        if mode == SPECTATOR || self.dropped.is_empty() {
            return;
        }
        let near: Vec<usize> = self
            .dropped
            .iter()
            .enumerate()
            .filter(|(_, d)| {
                d.pickup_delay == 0
                    && d.target.is_none_or(|t| t == uuid)
                    && (d.position.x - position[0]).abs() < 1.5
                    && (d.position.z - position[2]).abs() < 1.5
                    && d.position.y > position[1] - 0.6
                    && d.position.y < position[1] + 2.4
            })
            .map(|(i, _)| i)
            .collect();
        let mut taken = Vec::new();
        for i in near.into_iter().rev() {
            let (stack, item_id) = (self.dropped[i].stack.clone(), self.dropped[i].id);
            let before = stack.count;
            let thrower = self.dropped[i].thrower;
            let rest = self.as_guest(host, conn, |g| g.give(stack)).flatten();
            match rest {
                None => {
                    let item = self.dropped.remove(i);
                    if let Some(thrower) = thrower {
                        self.as_guest(host, conn, |g| {
                            g.trigger_thrown_picked_up_by_player(&item.stack, thrower);
                        });
                    }
                    self.guest_pickup(host, id, item);
                    taken.push((item_id, before));
                }
                Some(rest) if rest.count < before => {
                    self.guest_pickup(host, id, self.dropped[i].clone());
                    self.dropped[i].stack = rest;
                    if let Some(thrower) = thrower {
                        let left = self.dropped[i].stack.clone();
                        self.as_guest(host, conn, |g| {
                            g.trigger_thrown_picked_up_by_player(&left, thrower);
                        });
                    }
                    taken.push((item_id, before - self.dropped[i].stack.count));
                }
                Some(_) => {}
            }
        }
        for (item, count) in taken {
            host.hub.take_item(item as i32, id, count as i32, position);
        }
    }

    fn guest_arrows(&mut self, host: &mut LanHost, gi: usize) {
        use vise_client::arrows::Touch;
        let (conn, id, at, creative, unable, dims) = {
            let g = &host.hub.guests[gi];
            (
                g.conn,
                g.entity_id,
                g.position,
                g.creative(),
                g.spectator() || g.dead || g.health <= 0.0,
                g.dimensions(),
            )
        };
        if unable || self.creatures.projectiles.is_empty() {
            return;
        }
        let w = f64::from(dims.width) / 2.0;
        let reach = Aabb::new(
            Vec3::new(at[0] - w, at[1], at[2] - w),
            Vec3::new(at[0] + w, at[1] + f64::from(dims.height), at[2] + w),
        )
        .inflate(Vec3::new(1.0, 0.5, 1.0));
        let mut projectiles = std::mem::take(&mut self.creatures.projectiles);
        let mut picked = Vec::new();
        projectiles.retain(|shot| {
            let Some(a) = shot.arrow.as_ref().filter(|a| !a.removed) else {
                return true;
            };
            let p = shot.position;
            let bounds = Aabb::new(
                Vec3::new(p.x - 0.25, p.y, p.z - 0.25),
                Vec3::new(p.x + 0.25, p.y + 0.5, p.z + 0.25),
            );
            if !bounds.intersects(reach) {
                return true;
            }
            let taken = match a.player_touch(creative) {
                Touch::Ignore => false,
                Touch::Take => true,
                Touch::Give(stack) => {
                    self.as_guest(host, conn, |g| g.give(stack))
                        .flatten()
                        .is_none()
                        || creative
                }
            };
            if taken {
                picked.push(shot.clone());
            }
            !taken
        });
        self.creatures.projectiles = projectiles;
        for shot in picked {
            host.hub.take_item(shot.id as i32, id, 1, at);
            if let Some(target) = super::pickups::guest_target(host, id as u32) {
                self.add_pickup_toward(
                    super::pickups::Taken::Arrow(Box::new(shot)),
                    super::pickups::Collector::Guest(id as u32),
                    target,
                );
            }
        }
    }

    fn guest_orbs(&mut self, host: &mut LanHost, gi: usize) {
        let g = &host.hub.guests[gi];
        let height = f64::from(g.dimensions().height);
        let (conn, id, at) = (g.conn, g.entity_id, g.position);
        let unable = g.spectator() || g.dead || g.health <= 0.0;
        let play = host.play.entry(conn).or_default();
        if play.xp_delay > 0 {
            play.xp_delay -= 1;
        }
        if unable || play.xp_delay > 0 {
            return;
        }
        let (min, max) = (
            [at[0] - 1.3, at[1] - 0.5, at[2] - 1.3],
            [at[0] + 1.3, at[1] + height + 0.5, at[2] + 1.3],
        );
        let touching: Vec<usize> = self
            .orbs
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                o.position.x + 0.25 > min[0]
                    && o.position.x - 0.25 < max[0]
                    && o.position.y + 0.5 > min[1]
                    && o.position.y < max[1]
                    && o.position.z + 0.25 > min[2]
                    && o.position.z - 0.25 < max[2]
            })
            .map(|(i, _)| i)
            .collect();
        if touching.is_empty() {
            return;
        }
        let pick = self
            .world
            .random
            .next_int_bound(touching.len() as i32)
            .unwrap_or(0) as usize;
        let orb = self.orbs[touching[pick]].clone();
        if orb.count <= 1 {
            self.orbs.remove(touching[pick]);
        } else {
            self.orbs[touching[pick]].count -= 1;
        }
        play.xp_delay = 2;
        host.hub.take_item(orb.id as i32, id, 1, at);
        let pitch = (self.world.random.next_float() - self.world.random.next_float()) * 0.35 + 0.9;
        let p = orb.position;
        self.audio.play(
            "minecraft:entity.experience_orb.pickup",
            Some([p.x, p.y, p.z]),
            0.1,
            pitch,
        );
        let value = orb.value;
        if let Some(target) = super::pickups::guest_target(host, id as u32) {
            use super::pickups::{Collector, Taken};
            self.add_pickup_toward(
                Taken::Orb(Box::new(orb)),
                Collector::Guest(id as u32),
                target,
            );
        }
        self.as_guest(host, conn, |game| {
            let rest = game.mending(value);
            if rest > 0 {
                game.give_orb_points(rest);
            }
        });
    }

    fn guest_pickup(
        &mut self,
        host: &LanHost,
        id: i32,
        item: vise_client::entities::ItemEntity,
    ) {
        use super::pickups::{Collector, Taken, guest_target};
        if let Some(target) = guest_target(host, id as u32) {
            self.add_pickup_toward(
                Taken::Item(Box::new(item)),
                Collector::Guest(id as u32),
                target,
            );
        }
    }

    fn guest_use_sounds(&mut self, host: &mut LanHost, conn: u32, logged: usize, heard: usize) {
        let changed: Vec<(i32, i32, i32)> = self
            .world
            .block_log
            .as_ref()
            .map_or(&[][..], |log| log.get(logged..).unwrap_or(&[]))
            .to_vec();
        let facts = self.res.facts();
        let placed: Vec<([f64; 3], String)> = changed
            .iter()
            .map(|&(x, y, z)| {
                let at = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
                let name = &facts.get(self.world.block(x, y, z)).sounds.place;
                (at, name.trim_start_matches("minecraft:").to_owned())
            })
            .collect();
        if let Some(capture) = self.audio.capture.as_mut() {
            let mut i = heard.min(capture.len());
            while i < capture.len() {
                let (name, at, ..) = &capture[i];
                let place = placed
                    .iter()
                    .any(|(p, n)| p == at && name.trim_start_matches("minecraft:") == n.as_str());
                if place {
                    let (name, at, volume, pitch) = capture.remove(i);
                    let source = vise_client::settings::sound_source(&name) as i32;
                    host.hub.sound(&name, at, volume, pitch, source, Some(conn));
                } else {
                    i += 1;
                }
            }
        }
        if changed.is_empty()
            || !changed.iter().all(|&(x, y, z)| {
                super::remote::hand_operated(&facts.get(self.world.block(x, y, z)).name)
            })
        {
            return;
        }
        let Some(capture) = self.audio.capture.as_mut() else {
            return;
        };
        let sounds: Vec<_> = capture.drain(heard.min(capture.len())..).collect();
        for (name, at, volume, pitch) in sounds {
            let source = vise_client::settings::sound_source(&name) as i32;
            host.hub.sound(&name, at, volume, pitch, source, Some(conn));
        }
    }

    fn guest_instrument_sound(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        heard: usize,
        horn: Option<String>,
    ) {
        let (Some(horn), Some(capture)) = (horn, self.audio.capture.as_mut()) else {
            return;
        };
        let mut i = heard.min(capture.len());
        while i < capture.len() {
            if capture[i].0 == horn {
                let (name, at, volume, pitch) = capture.remove(i);
                let source = vise_client::settings::sound_source(&name) as i32;
                host.hub.sound(&name, at, volume, pitch, source, Some(conn));
            } else {
                i += 1;
            }
        }
    }

    pub(super) fn lan_broadcast_effects(&mut self, host: &mut LanHost) {
        let sounds = self.audio.capture.take().unwrap_or_default();
        self.audio.capture = Some(Vec::new());
        let blocks: Vec<[f64; 3]> = std::mem::take(&mut host.broken);
        for (name, at, volume, pitch) in sounds {
            if blocks
                .iter()
                .any(|b| (0..3).all(|k| (b[k] + 0.5 - at[k]).abs() < 1e-6))
            {
                continue;
            }
            let source = vise_client::settings::sound_source(&name) as i32;
            host.hub.sound(&name, at, volume, pitch, source, None);
        }
    }

    pub(super) fn lan_living_flags(&mut self, host: &mut LanHost) {
        for g in &mut host.hub.guests {
            let using = host.play.get(&g.conn).and_then(GuestPlay::using);
            g.living_flags = super::lan::living_flags(using);
        }
    }

    fn lan_swing_back(&mut self, host: &mut LanHost, conn: u32, swung: Option<bool>) {
        if swung != Some(true) {
            return;
        }
        let (Some(g), Some(play)) = (host.hub.guest(conn), host.play.get(&conn)) else {
            return;
        };
        if let Some(c) = play.swing.current {
            host.hub
                .send_to(conn, &swing_packet(g.entity_id, c.hand, c.animation));
        }
    }

    pub(super) fn lan_swings(&mut self, host: &mut LanHost) {
        let conns: Vec<u32> = host.play.keys().copied().collect();
        for conn in conns {
            let Some(g) = host.hub.guests.iter().find(|g| g.conn == conn) else {
                continue;
            };
            let (id, at, here) = (g.entity_id, g.position, host.hub.here(g));
            let play = host.play.get_mut(&conn).expect("play");
            let swung = std::mem::take(&mut play.swings) > 0;
            if let (true, true, Some(c)) = (swung, here, play.swing.current) {
                let packet = swing_packet(id, c.hand, c.animation);
                host.hub.broadcast_near(at, 128.0, &packet, Some(conn));
                if let Some(m) = host.models.get_mut(&conn) {
                    m.swing_hand(c.hand == Hand::Off, c.duration_ticks);
                }
            }
            play.swing.tick();
        }
        self.lan_host_crack(host);
        if self.swings_started != host.host_swings {
            host.host_swings = self.swings_started;
            if let (Some(p), Some(c)) = (self.player.as_ref(), self.swing.current) {
                let at = [p.position.x, p.position.y, p.position.z];
                let packet = swing_packet(host.host_entity, c.hand, c.animation);
                host.hub.broadcast_near(at, 128.0, &packet, None);
            }
        }
    }
}

impl Game<'_> {
    fn lan_host_crack(&mut self, host: &mut LanHost) {
        let now = self
            .mining
            .as_ref()
            .and_then(|m| (m.progress > 0.0).then(|| (m.pos, ((m.progress * 10.0) as i32).min(9))));
        if now == host.host_crack {
            return;
        }
        let packet = |pos: [i32; 3], progress: u8| {
            C::BlockDestruction(play::BlockDestruction {
                id: host.host_entity,
                pos,
                progress,
            })
        };
        let near = |pos: [i32; 3]| [f64::from(pos[0]), f64::from(pos[1]), f64::from(pos[2])];
        if let Some((pos, _)) = host.host_crack
            && now.is_none_or(|(p, _)| p != pos)
        {
            host.hub
                .broadcast_near(near(pos), 64.0, &packet(pos, 255), None);
        }
        if let Some((pos, stage)) = now {
            host.hub
                .broadcast_near(near(pos), 64.0, &packet(pos, stage as u8), None);
        }
        host.host_crack = now;
    }
}

fn swing_packet(id: i32, hand: Hand, animation: SwingAnimation) -> C {
    C::SwingAnimation(play::SwingAnimation {
        id,
        hand: i32::from(hand == Hand::Off),
        kind: i32::from(animation.kind),
        duration: animation.duration,
    })
}

fn eye_columns(at: [f64; 3], body: Dimensions) -> [std::ops::RangeInclusive<i32>; 2] {
    let half = f64::from(body.width * 0.8) / 2.0;
    let range = |v: f64| (v - half).floor() as i32..=(v + half).floor() as i32;
    [range(at[0]), range(at[2])]
}

#[cfg(test)]
mod tests {
    use super::eye_columns;
    use vise_client::pose::Pose;

    #[test]
    fn a_small_player_checks_only_the_blocks_at_their_small_eyes() {
        let at = [205.95, 100.0, 530.5];
        assert_eq!(
            eye_columns(at, Pose::Standing.dimensions(0.1)),
            [205..=205, 530..=530]
        );
        assert_eq!(
            eye_columns(at, Pose::Standing.dimensions(1.0)),
            [205..=206, 530..=530]
        );
        assert_eq!(
            eye_columns([205.7, 100.0, 530.5], Pose::Standing.dimensions(1.0)),
            [205..=205, 530..=530]
        );
    }
}
