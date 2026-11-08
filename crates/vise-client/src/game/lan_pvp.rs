use super::{Game, lan::LanHost, lan_mobs::guest_bounds, mace::Spared, ray_box};

use vise_client::{
    Result,
    mobs::Who,
    projectiles::{Hit, Projectile, ProjectileKind},
};
use vise_core::geometry::{Aabb, Vec3};

pub(super) fn meets_players(kind: &ProjectileKind) -> bool {
    matches!(
        kind,
        ProjectileKind::Arrow { .. } | ProjectileKind::Snowball | ProjectileKind::Egg
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Victim {
    Host,
    Guest(u32),
}

pub(super) struct Strike {
    attacker: Option<u32>,
    amount: f32,
    scale: f32,
    crit: bool,
    magic: bool,
    from: [f64; 3],
    disable_blocking: f32,
    sweep: Option<[f64; 3]>,
    knockback: (f32, f64, f64),
    knockback_attack: bool,
    team: Option<vise_client::scoreboard::Team>,
}

#[derive(Clone)]
pub(super) struct Sweep {
    attacker: Option<u32>,
    from: [f64; 3],
    damage: f32,
    knockback: (f64, f64),
    team: Option<vise_client::scoreboard::Team>,
    pub(super) hit_box: Aabb,
    pub(super) skip: Option<i32>,
}

impl Strike {
    pub(super) fn stops_sprint(&self) -> bool {
        self.knockback.0 > 0.0
    }
}

impl Game<'_> {
    pub(super) fn guest_hit(&self) -> Option<(u32, f64)> {
        let host = self.lan.as_ref()?;
        let (eye, dir) = self.aim()?;
        let reach = if self.creative { 5.0 } else { 3.0 };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .filter_map(|g| Some((g.conn, ray_box(eye, dir, reach, guest_bounds(g))?)))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    pub(super) fn attack_guest(&mut self) -> bool {
        let Some((conn, d)) = self.guest_hit() else {
            return false;
        };
        if self.mob_hit().is_some_and(|(_, m)| m < d) || !self.before_block(d) {
            return false;
        }
        let Some(mut host) = self.lan.take() else {
            return false;
        };
        if let Some(strike) = self.pvp_strike()
            && self.pvp_land(&mut host, Victim::Guest(conn), &strike)
        {
            self.weapon_hit_player(Self::pvp_victim_on_ground(&host, Victim::Guest(conn), None));
            self.pvp_exhaustion();
            if strike.stops_sprint() {
                self.stop_sprint_after_extra_knockback();
            }
        }
        self.lan = Some(host);
        true
    }

    pub(super) fn pvp_victim_on_ground(
        host: &LanHost,
        victim: Victim,
        host_on_ground: Option<bool>,
    ) -> bool {
        match victim {
            Victim::Host => host_on_ground.unwrap_or(true),
            Victim::Guest(conn) => host
                .hub
                .guest(conn)
                .and_then(|g| g.motion.as_deref())
                .is_some_and(|p| p.ground.on_ground),
        }
    }

    pub(super) fn pvp_victim(host: &LanHost, conn: u32, id: i32) -> Option<Victim> {
        if id == host.host_entity {
            return Some(Victim::Host);
        }
        host.hub
            .guests
            .iter()
            .find(|g| g.entity_id == id && g.conn != conn && host.hub.here(g) && !g.dead)
            .map(|g| Victim::Guest(g.conn))
    }

    pub(super) fn pvp_exhaustion(&mut self) {
        if let Some(p) = self.player.as_mut()
            && !self.creative
        {
            p.cause_food_exhaustion(0.1);
        }
    }

    pub(super) fn pvp_strike(&mut self) -> Option<Strike> {
        let p = self.player.as_ref()?;
        let from = [p.position.x, p.position.y, p.position.z];
        let (amount, scale, crit, magic) = self.melee_damage("minecraft:player", true);
        let sweep = self.sweep_attack_position(scale, crit);
        let (strength, knockback_attack) = self.extra_knockback("minecraft:player", scale);
        let (xd, zd) = self.extra_knockback_direction();
        self.attack_ticks = 0;
        self.swing_arm();
        let disable_blocking = if self.using.is_some() {
            0.0
        } else {
            self.attack_item_info()
                .map_or(0.0, |i| i.disable_blocking_seconds)
        };
        Some(Strike {
            attacker: self.lan_acting,
            amount,
            scale,
            crit,
            magic,
            from,
            disable_blocking,
            sweep,
            knockback: (strength, xd, zd),
            knockback_attack,
            team: self.player_team().cloned(),
        })
    }

    pub(super) fn lan_guest_entities(&self) -> Vec<(u32, u32)> {
        self.lan.as_ref().map_or_else(Vec::new, |host| {
            host.hub
                .guests
                .iter()
                .map(|g| (g.conn, g.entity_id as u32))
                .collect()
        })
    }

    pub(super) fn projectile_pvp_victim(&self, p: &Projectile, hit: Hit) -> Option<Victim> {
        if p.owner != Some(Who::Player) || !meets_players(&p.kind) {
            return None;
        }
        let Hit::Entity { who, .. } = hit else {
            return None;
        };
        match who {
            Who::Player => p.guest.map(|_| Victim::Host),
            Who::Mob(id) => self
                .lan
                .as_ref()?
                .hub
                .guests
                .iter()
                .find(|g| g.entity_id as u32 == id)
                .map(|g| Victim::Guest(g.conn)),
        }
    }

    pub(super) fn projectile_hits_player(
        &mut self,
        p: &Projectile,
        hit: Hit,
        victim: Victim,
    ) -> Result<()> {
        let Hit::Entity { at, .. } = hit else {
            return Ok(());
        };
        let Some(mut host) = self.lan.take() else {
            return self.projectile_hit(p, hit);
        };
        let team_of = |game: &Self, name: &str| game.score.board.players_team(name).cloned();
        let attacker = match p.guest {
            Some(conn) => host.hub.guest(conn).and_then(|g| team_of(self, &g.name)),
            None => self.player_team().cloned(),
        };
        let target = match victim {
            Victim::Host => self.player_team().cloned(),
            Victim::Guest(conn) => host.hub.guest(conn).and_then(|g| team_of(self, &g.name)),
        };
        let refused = !(self.ui.rules.bool("pvp")
            && vise_client::scoreboard::teams_allow_damage(target.as_ref(), attacker.as_ref()));
        let on_player = Hit::Entity {
            who: Who::Player,
            at,
        };
        let result = match victim {
            Victim::Host => {
                self.lan = Some(host);
                self.combat.refuse_player_hit = refused;
                let r = self.projectile_hit(p, on_player);
                self.combat.refuse_player_hit = false;
                return r;
            }
            Victim::Guest(conn) => self.guest_hit_by(&mut host, conn, |g| {
                g.combat.refuse_player_hit = refused;
                let r = g.projectile_hit(p, on_player);
                g.combat.refuse_player_hit = false;
                r
            }),
        };
        self.lan = Some(host);
        result.unwrap_or(Ok(()))
    }

    pub(super) fn pvp_land(&mut self, host: &mut LanHost, victim: Victim, s: &Strike) -> bool {
        let team = match victim {
            Victim::Host => self.player_team(),
            Victim::Guest(conn) => host
                .hub
                .guest(conn)
                .and_then(|g| self.score.board.players_team(&g.name)),
        };
        let harm = vise_client::scoreboard::teams_allow_damage(team, s.team.as_ref());
        if s.knockback_attack {
            self.audio.play(
                "minecraft:entity.player.attack.knockback",
                Some(s.from),
                1.0,
                1.0,
            );
        }
        let land = |g: &mut Self| {
            g.combat.player_attacker = Some((s.from, s.disable_blocking));
            let hurt = g.mob_hits_player(s.amount, s.from, "minecraft:player_attack");
            g.combat.player_attacker = None;
            let (strength, xd, zd) = s.knockback;
            if hurt && strength > 0.0 {
                g.knock_back_player(f64::from(strength), xd, zd);
            }
            let thorns = if hurt { g.thorns_damage() } else { Vec::new() };
            (hurt, thorns)
        };
        let (hurt, thorns) = if self.ui.rules.bool("pvp") && harm {
            match victim {
                Victim::Host => land(self),
                Victim::Guest(conn) => self.guest_struck_by(host, conn, land).unwrap_or_default(),
            }
        } else {
            Default::default()
        };
        if hurt {
            let died = match victim {
                Victim::Host => self
                    .player
                    .as_ref()
                    .is_some_and(|p| p.health.dead_or_dying() && !p.health.totem_used),
                Victim::Guest(conn) => host
                    .hub
                    .guest(conn)
                    .and_then(|g| g.motion.as_deref())
                    .is_some_and(|p| p.health.dead_or_dying() && !p.health.totem_used),
            };
            if died {
                match s.attacker {
                    Some(conn) => {
                        self.as_guest(host, conn, |g| g.stats.custom("player_kills", 1));
                    }
                    None => self.stats.custom("player_kills", 1),
                }
            }
        }
        if hurt && (s.crit || s.magic) {
            let target = match victim {
                Victim::Host => self.player.as_ref().map(|p| (host.host_entity, p)),
                Victim::Guest(conn) => host
                    .hub
                    .guest(conn)
                    .and_then(|g| g.motion.as_deref().map(|p| (g.entity_id, p))),
            };
            if let Some((id, p)) = target {
                let d = p.dimensions;
                let (feet, width, height) = (
                    [p.position.x, p.position.y, p.position.z],
                    f64::from(d.width),
                    f64::from(d.height),
                );
                if s.crit {
                    self.critical_hit(s.attacker, id, feet, width, height, false);
                }
                if s.magic {
                    self.critical_hit(s.attacker, id, feet, width, height, true);
                }
            }
        }
        for damage in thorns {
            match s.attacker {
                None => self.hurt_player_by("minecraft:thorns", damage),
                Some(conn) => {
                    self.guest_hit_by(host, conn, |g| g.hurt_player_by("minecraft:thorns", damage));
                }
            }
        }
        let sound = match (hurt, s.crit, s.scale > 0.9) {
            (false, ..) => "minecraft:entity.player.attack.nodamage",
            (true, true, _) => "minecraft:entity.player.attack.crit",
            (true, false, true) => "minecraft:entity.player.attack.strong",
            (true, false, false) => "minecraft:entity.player.attack.weak",
        };
        if let Some(at) = s.sweep.filter(|_| hurt) {
            let (area, id) = match victim {
                Victim::Host => (self.player.as_ref().map(|p| p.bounds()), host.host_entity),
                Victim::Guest(conn) => {
                    let g = host.hub.guest(conn);
                    (g.map(guest_bounds), g.map_or(-1, |g| g.entity_id))
                }
            };
            let target = area.map(|area| (area, Spared::Player(id)));
            match s.attacker {
                None => {
                    self.combat.sweeps = Some(Vec::new());
                    self.sweep_attack(at, Some(s.from), target, s.scale);
                    for sweep in self.combat.sweeps.take().unwrap_or_default() {
                        self.sweep_players(host, &sweep);
                    }
                }
                Some(conn) => {
                    self.as_guest(host, conn, |g| {
                        g.sweep_attack(at, Some(s.from), target, s.scale)
                    });
                }
            }
        } else {
            self.audio.play(sound, Some(s.from), 1.0, 1.0);
        }
        hurt
    }

    pub(super) fn player_sweep(&mut self, base: f32, scale: f32) -> Sweep {
        let from = self
            .player
            .as_ref()
            .map_or([0.0; 3], |p| [p.position.x, p.position.y, p.position.z]);
        let damage = self.enchanted_damage("minecraft:player", base) * scale;
        Sweep {
            attacker: self.lan_acting,
            from,
            damage,
            knockback: self.extra_knockback_direction(),
            team: self.player_team().cloned(),
            hit_box: Aabb::new(Vec3::ZERO, Vec3::ZERO),
            skip: None,
        }
    }

    pub(super) fn sweep_other_players(&mut self, sweep: Sweep) {
        if let Some(collect) = self.combat.sweeps.as_mut() {
            collect.push(sweep);
        } else if self.lan_acting.is_some() {
            self.creatures.sweeps.push(sweep);
        } else if let Some(mut host) = self.lan.take() {
            self.sweep_players(&mut host, &sweep);
            self.lan = Some(host);
        }
    }

    pub(super) fn flush_sweeps(&mut self) {
        for sweep in std::mem::take(&mut self.creatures.sweeps) {
            self.sweep_other_players(sweep);
        }
    }

    pub(super) fn sweep_players(&mut self, host: &mut LanHost, sweep: &Sweep) {
        let from = Vec3::new(sweep.from[0], sweep.from[1], sweep.from[2]);
        let mut victims = Vec::new();
        if sweep.attacker.is_some()
            && self.options.dedicated.is_none()
            && !self.spectator()
            && let Some(p) = self.player.as_ref()
            && !p.health.dead_or_dying()
            && p.bounds().intersects(sweep.hit_box)
            && p.position.distance_squared(from) < 9.0
        {
            victims.push((Victim::Host, host.host_entity, self.player_team().cloned()));
        }
        for g in &host.hub.guests {
            if host.hub.here(g)
                && g.loaded
                && !g.dead
                && !g.spectator()
                && sweep.attacker != Some(g.conn)
                && guest_bounds(g).intersects(sweep.hit_box)
                && Vec3::new(g.position[0], g.position[1], g.position[2]).distance_squared(from)
                    < 9.0
            {
                let team = self.score.board.players_team(&g.name).cloned();
                victims.push((Victim::Guest(g.conn), g.entity_id, team));
            }
        }
        let (xd, zd) = sweep.knockback;
        let land = |g: &mut Self| {
            let hurt = g.mob_hits_player(sweep.damage, sweep.from, "minecraft:player_attack");
            if hurt {
                g.knock_back_player(0.4, xd, zd);
            }
        };
        for (victim, id, team) in victims {
            if sweep.skip == Some(id)
                || vise_client::scoreboard::teams_allied(team.as_ref(), sweep.team.as_ref())
                || !self.ui.rules.bool("pvp")
            {
                continue;
            }
            match victim {
                Victim::Host => land(self),
                Victim::Guest(conn) => {
                    self.guest_struck_by(host, conn, land);
                }
            }
        }
    }
}
