use super::{Game, chat::PLAYER_NAME, commands::Target};
use vise_client::{
    health::HealthEvent,
    mobs::{Mob, Species},
    motion::MotionEvent,
    particles::{ParticleOptions, ParticleType},
    text::Text,
};
use vise_core::random::RandomSource;

pub(super) const DROWN_EVENT: i8 = 67;

struct Entry {
    damage_type: String,
    damage: f32,
    fall_location: Option<&'static str>,
    fall_distance: f32,
    attacker: Option<(u32, Text)>,
    direct: Option<Text>,
}
#[derive(Default)]
pub(super) struct CombatTracker {
    entries: Vec<Entry>,
    last_damage: i64,
    in_combat: bool,
    pending: Vec<(String, (u32, Text))>,
    pending_direct: Vec<(String, Text)>,
    last_hurt_by: Option<((u32, Text), i64)>,
    pub(super) last_hurt_mob: Option<(u32, i64)>,
    pub(super) projectile_owner: Option<u32>,
    pub(super) projectile_direct: Option<Text>,
    pub(super) refuse_player_hit: bool,
    pub(super) player_attacker: Option<([f64; 3], f32)>,
    pub(super) sweeps: Option<Vec<super::lan_pvp::Sweep>>,
    pub(super) explosion_source: Option<u32>,
    pub(super) explosion_source_named: bool,
    pub(super) hive_emptying_blast: bool,
    pub(super) explosion_owner: Option<u32>,
    pub(super) charged_exploders: Vec<u32>,
    pub(super) exploded: Vec<(u32, Text)>,
    fall_before: f64,
}
impl Game<'_> {
    pub(super) fn drown_particles(&mut self, at: [f64; 3], velocity: [f64; 3]) {
        let bubble = ParticleOptions::simple(ParticleType::Bubble);
        for _ in 0..8 {
            let mut triangle = || {
                let r = self.particles.random();
                r.next_double() - r.next_double()
            };
            let p = [at[0] + triangle(), at[1] + triangle(), at[2] + triangle()];
            self.particles.add(&bubble, p, velocity);
        }
    }
    pub(super) fn remote_drown(&mut self, id: i32) {
        let own = self.remote.as_ref().is_some_and(|r| r.entity_id == id);
        let body = if own {
            self.player.as_ref().map(|p| {
                let (at, v) = (p.position, p.velocity);
                ([at.x, at.y, at.z], [v.x, v.y, v.z])
            })
        } else {
            self.mobs.iter().find(|m| m.id == id as u32).map(|m| {
                let (at, v) = (m.position, m.velocity);
                ([at.x, at.y, at.z], [v.x, v.y, v.z])
            })
        };
        if let Some((at, v)) = body {
            self.drown_particles(at, v);
        }
    }
    pub(super) fn mob_display_name(&self, id: u32) -> Option<Text> {
        let m = self.mobs.iter().find(|m| m.id == id)?;
        Some(match &m.custom_name {
            Some(name) => Text::literal(name.clone()),
            None => Text::translate(&format!("entity.{}", m.kind.id.replace(':', ".")), &[]),
        })
    }
    pub(super) fn player_hurt_by(&self) -> Option<(vise_client::mobs::Who, i64)> {
        let ((id, _), at) = self.combat.last_hurt_by.as_ref()?;
        (self.world.game_time - at <= 100).then_some((vise_client::mobs::Who::Mob(*id), *at))
    }
    pub(super) fn note_attacker(&mut self, damage_type: &str, mob: u32) {
        if let Some(name) = self.mob_display_name(mob) {
            self.note_named_attacker(damage_type, mob, name);
        }
    }
    pub(super) fn note_named_attacker(&mut self, damage_type: &str, id: u32, name: Text) {
        self.combat
            .pending
            .push((damage_type.to_owned(), (id, name)));
    }
    pub(super) fn note_direct_entity(&mut self, damage_type: &str, name: Text) {
        self.combat
            .pending_direct
            .push((damage_type.to_owned(), name));
    }
    pub(super) fn player_hurt_sound(&mut self, sound: &str, at: [f64; 3]) {
        self.audio.play(sound, Some(at), 1.0, 1.0);
    }
    pub(super) fn record_combat(&mut self, events: &[MotionEvent]) {
        self.recheck_combat();
        for e in events {
            let (name, amount) = match e {
                MotionEvent::Health(HealthEvent::Statistic { name, amount }) => {
                    match *name {
                        "resisted" => self.stats.custom("damage_resisted", *amount),
                        "absorbed" => self.stats.custom("damage_absorbed", *amount),
                        "blocked" => self.stats.custom("damage_blocked_by_shield", *amount),
                        _ => {}
                    }
                    continue;
                }
                MotionEvent::Health(HealthEvent::RecordedDamage { source, amount }) => {
                    let Some(d) = self.motion.health.damage.definition(*source) else {
                        continue;
                    };
                    (d.name.clone(), *amount)
                }
                MotionEvent::Health(HealthEvent::StarvationDamage(amount)) => {
                    ("minecraft:starve".to_owned(), *amount)
                }
                _ => continue,
            };
            let attacker = self
                .combat
                .pending
                .iter()
                .position(|(t, _)| *t == name)
                .map(|i| self.combat.pending.remove(i).1);
            let direct = match attacker {
                Some(_) => None,
                None => self
                    .combat
                    .pending_direct
                    .iter()
                    .position(|(t, _)| *t == name)
                    .map(|i| self.combat.pending_direct.remove(i).1),
            };
            let (fall_location, fall_distance) = self.fall_location();
            let now = self.world.game_time;
            if let Some(a) = &attacker {
                self.combat.last_hurt_by = Some((a.clone(), now));
                if !self.combat.in_combat {
                    self.combat.in_combat = true;
                }
            }
            self.stats
                .custom("damage_taken", (amount * 10.0).round() as i32);
            self.combat.entries.push(Entry {
                damage_type: name,
                damage: amount,
                fall_location,
                fall_distance,
                attacker,
                direct,
            });
            self.combat.last_damage = now;
            if self.potions.effects.get("infested").is_some() {
                self.player_infested();
            }
        }
        self.combat.pending.clear();
        self.combat.pending_direct.clear();
        self.combat.projectile_direct = None;
        self.combat.fall_before = self.player.as_ref().map_or(0.0, |p| p.fall_distance);
    }
    fn fall_location(&self) -> (Option<&'static str>, f32) {
        let Some(p) = self.player.as_ref() else {
            return (None, 0.0);
        };
        let distance = p.fall_distance.max(self.combat.fall_before) as f32;
        let location = if let Some(c) = p.last_climbable {
            let name = self
                .res
                .facts()
                .get(self.world.block(c.x, c.y, c.z))
                .name
                .clone();
            Some(match &*name {
                "minecraft:ladder" => "ladder",
                "minecraft:vine" => "vines",
                "minecraft:weeping_vines" | "minecraft:weeping_vines_plant" => "weeping_vines",
                "minecraft:twisting_vines" | "minecraft:twisting_vines_plant" => "twisting_vines",
                "minecraft:scaffolding" => "scaffolding",
                _ => "other_climbable",
            })
        } else {
            let f = self.res.facts().get(self.world.block(
                p.position.x.floor() as i32,
                (p.position.y + 0.1).floor() as i32,
                p.position.z.floor() as i32,
            ));
            (f.fluid == vise_client::blocks::Fluid::Water || f.waterlogged).then_some("water")
        };
        (location, distance)
    }
    fn recheck_combat(&mut self) {
        let reset = if self.combat.in_combat { 300 } else { 100 };
        if !self.combat.entries.is_empty() && self.world.game_time - self.combat.last_damage > reset
        {
            self.combat.entries.clear();
            self.combat.in_combat = false;
        }
    }
    fn damage_in_tag(&self, name: &str, tag: &str) -> bool {
        let damage = &self.motion.health.damage;
        damage.id(name).is_some_and(|id| {
            damage.in_tag(id, tag) || damage.in_tag(id, &format!("minecraft:{tag}"))
        })
    }
    pub(super) fn combat_killer_kind(&self) -> Option<String> {
        let (id, _) = self.combat.entries.last()?.attacker.clone()?;
        self.mobs
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.kind.id.clone())
    }
    fn most_significant_fall(&self) -> Option<usize> {
        let (mut result, mut alternative) = (None, None);
        let (mut best_fall, mut alt_damage) = (0.0f32, 0.0f32);
        for (i, e) in self.combat.entries.iter().enumerate() {
            let fake = self.damage_in_tag(&e.damage_type, "always_most_significant_fall");
            let distance = if fake { f32::MAX } else { e.fall_distance };
            if (self.damage_in_tag(&e.damage_type, "is_fall") || fake)
                && distance > 0.0
                && (result.is_none() || distance > best_fall)
            {
                result = Some(i.saturating_sub(1));
                best_fall = distance;
            }
            if e.fall_location.is_some() && (alternative.is_none() || e.damage > alt_damage) {
                alternative = Some(i);
                alt_damage = e.damage;
            }
        }
        if best_fall > 5.0 && result.is_some() {
            result
        } else if alt_damage > 5.0 {
            alternative
        } else {
            None
        }
    }
    fn mob_main_hand_name(&self, id: u32) -> Option<Text> {
        let m = self.mobs.iter().find(|m| m.id == id)?;
        let mut stack = m.extra.main_hand_stack()?.clone();
        stack.custom_name()?;
        if let Some(g) = vise_client::mobs::gear::gear(m) {
            stack.data_mut().enchantments = g.slots[vise_client::mobs::gear::MAIN].clone();
        }
        Some(self.stack_display_name(&stack))
    }
    pub(super) fn combat_death_message(&mut self) -> Text {
        self.combat_death_message_for(Text::literal(PLAYER_NAME))
    }
    pub(super) fn combat_death_message_for(&mut self, me: Text) -> Text {
        self.recheck_combat();
        let entries = std::mem::take(&mut self.combat.entries);
        self.combat.in_combat = false;
        let Some(killing) = entries.last() else {
            return Text::translate("death.attack.generic", &[me]);
        };
        let def = self
            .motion
            .health
            .damage
            .id(&killing.damage_type)
            .and_then(|id| self.motion.health.damage.definition(id))
            .map(|d| (d.message_id.clone(), d.death_message_type.clone()))
            .unwrap_or_else(|| ("generic".into(), "default".into()));
        self.combat.entries = entries;
        let knock = self.most_significant_fall();
        let entries = std::mem::take(&mut self.combat.entries);
        let killing = entries.last().expect("checked above");
        let killer = killing.attacker.clone();
        let direct = killing.direct.clone();
        if def.1 == "fall_variants"
            && let Some(knock) = knock.and_then(|i| entries.get(i))
        {
            let fall = self.damage_in_tag(&knock.damage_type, "is_fall")
                || self.damage_in_tag(&knock.damage_type, "always_most_significant_fall");
            if fall {
                let at = knock.fall_location.unwrap_or("generic");
                return Text::translate(&format!("death.fell.accident.{at}"), &[me]);
            }
            return match (&knock.attacker, &killer) {
                (Some(a), k) if k.as_ref().is_none_or(|k| k.0 != a.0) => {
                    Text::translate("death.fell.assist", &[me, a.1.clone()])
                }
                (_, Some(k)) => Text::translate("death.fell.finish", &[me, k.1.clone()]),
                _ => Text::translate("death.fell.killer", &[me]),
            };
        }
        let key = format!("death.attack.{}", def.0);
        if def.1 == "intentional_game_design" {
            let link = Text::literal("[")
                .append(Text::translate(&format!("{key}.link"), &[]))
                .append(Text::literal("]"));
            return Text::translate(&format!("{key}.message"), &[me, link]);
        }
        if let Some((id, name)) = killer {
            let (key, args) = vise_client::scoreboard::death_message_args(
                &key,
                &me,
                &name,
                self.mob_main_hand_name(id).as_ref(),
            );
            return Text::translate(&key, &args);
        }
        if let Some(name) = direct {
            return Text::translate(&key, &[me, name]);
        }
        if let Some(((_, name), at)) = &self.combat.last_hurt_by
            && self.world.game_time - at <= 100
        {
            return Text::translate(&format!("{key}.player"), &[me, name.clone()]);
        }
        Text::translate(&key, &[me])
    }
    pub(super) fn tell_owner_pet_died(&mut self, m: &Mob, killer: Option<Option<u32>>) {
        let tamable = matches!(
            m.species(),
            Species::Wolf
                | Species::Cat
                | Species::Parrot
                | Species::Nautilus
                | Species::ZombieNautilus
        );
        let Some(owner) = m.extra.owner.filter(|_| tamable && m.extra.tame) else {
            return;
        };
        if !self.ui.rules.bool("show_death_messages") {
            return;
        }
        let host = self.player.is_some()
            && self.options.dedicated.is_none()
            && vise_client::mobs::tame::owned_by(
                Some(owner),
                self.level.player_uuid.unwrap_or_default(),
            );
        let uuid = vise_client::lan::playerdata::ints_uuid(owner);
        let guest = !host
            && self
                .lan
                .as_ref()
                .is_some_and(|l| l.hub.guests.iter().any(|g| g.placed && g.uuid == uuid));
        if !host && !guest {
            return;
        }
        let message = self.mob_death_message(m, killer);
        if host {
            self.system_message(message);
        } else if let Some(lan) = &self.lan {
            let content = vise_client::lan::data::component_nbt(&message.component());
            lan.hub.system_message_to_player(uuid, content);
        }
    }
    pub(super) fn mob_death_message(&mut self, m: &Mob, killer: Option<Option<u32>>) -> Text {
        use vise_client::mobs::{DamageKind, Who};
        let me = mob_name(m);
        let player = match killer {
            Some(Some(conn)) => Some((u32::MAX - 1 - conn, self.target_name(Target::Guest(conn)))),
            Some(None) => Some((u32::MAX, self.target_name(Target::Player))),
            None if self.player.is_some() || self.lan_acting.is_some() => {
                Some((u32::MAX, self.target_name(Target::Player)))
            }
            None => None,
        };
        let name = |g: &Self, who: Who| match who {
            Who::Player => player.clone(),
            Who::Mob(id) => g
                .mob_display_name(id)
                .or_else(|| {
                    g.combat
                        .exploded
                        .iter()
                        .find(|(e, _)| *e == id)
                        .map(|(_, n)| n.clone())
                })
                .map(|n| (id, n)),
        };
        let entries = m
            .combat
            .entries
            .iter()
            .map(|e| {
                let attacker = e.source.attacker.and_then(|w| name(self, w));
                let direct = (attacker.is_none() && e.source.kind == DamageKind::Arrow)
                    .then(|| Text::translate("entity.minecraft.arrow", &[]));
                Entry {
                    damage_type: super::creatures::damage_type(
                        e.source.kind,
                        e.source.attacker.is_some(),
                    )
                    .to_owned(),
                    damage: e.damage,
                    fall_location: e.in_water.then_some("water"),
                    fall_distance: e.fall_distance,
                    attacker,
                    direct,
                }
            })
            .collect();
        let credit = if m.ai.last_hurt_by_player > 0 && player.is_some() {
            player.clone()
        } else {
            m.ai.last_hurt_by
                .filter(|_| m.age as i32 - m.ai.last_hurt_by_time <= 100)
                .and_then(|w| name(self, w))
        };
        let now = self.world.game_time;
        let mut tracker = CombatTracker {
            entries,
            last_damage: now,
            in_combat: m.combat.in_combat,
            last_hurt_by: credit.map(|c| (c, now)),
            ..CombatTracker::default()
        };
        std::mem::swap(&mut self.combat, &mut tracker);
        let text = self.combat_death_message_for(me);
        std::mem::swap(&mut self.combat, &mut tracker);
        text
    }
}

fn mob_name(m: &Mob) -> Text {
    match (&m.custom_name_text, &m.custom_name) {
        (Some(text), _) => text.clone(),
        (None, Some(name)) => Text::literal(name.clone()),
        (None, None) => Text::translate(&format!("entity.{}", m.kind.id.replace(':', ".")), &[]),
    }
}
