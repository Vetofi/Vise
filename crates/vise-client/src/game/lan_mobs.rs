use super::{Game, lan::LanHost};
use vise_client::{
    mobs::{Mob, MobEvent, PlayerView, Species, Who},
    projectiles::{Hit, Projectile},
};
use vise_core::geometry::{Aabb, Vec3};
use vise_protocol::packets::play::{self, Clientbound as C};

pub(super) type GuestViews = Vec<(u32, PlayerView)>;

pub(super) fn player_for(
    m: &Mob,
    host: Option<&PlayerView>,
    guests: &GuestViews,
    bound: Option<u32>,
) -> Option<u32> {
    if m.species() == Species::Cat
        && m.extra.tame
        && let Some(owner) = m.extra.owner
    {
        let asleep = |v: &PlayerView| v.uuid == owner && (v.sleeping || v.sleep_timer > 0);
        if host.is_some_and(asleep) {
            return None;
        }
        if let Some((conn, _)) = guests.iter().find(|(_, v)| asleep(v)) {
            return Some(*conn);
        }
    }
    let targeting = m.ai.target == Some(Who::Player)
        || m.ai.last_hurt_by == Some(Who::Player) && m.age as i32 - m.ai.last_hurt_by_time <= 100;
    if !targeting
        && matches!(m.species(), Species::Wolf | Species::Cat | Species::Parrot)
        && m.extra.tame
        && let Some(owner) = m.extra.owner
    {
        if host.is_some_and(|v| v.alive && v.uuid == owner) {
            return None;
        }
        if let Some((conn, _)) = guests.iter().find(|(_, v)| v.alive && v.uuid == owner) {
            return Some(*conn);
        }
    }
    choose(m.position, targeting, host, guests, bound)
}

fn choose(
    at: Vec3,
    targeting: bool,
    host: Option<&PlayerView>,
    guests: &GuestViews,
    bound: Option<u32>,
) -> Option<u32> {
    let targetable = |v: &PlayerView| v.alive && !v.creative && !v.spectator;
    if targeting
        && let Some(conn) = bound
        && guests.iter().any(|(c, v)| *c == conn && targetable(v))
    {
        return Some(conn);
    }
    let d = |v: &PlayerView| at.distance_squared(v.position);
    let nearest = |ok: &dyn Fn(&PlayerView) -> bool| {
        let mut best = host.filter(|v| ok(v)).map(|v| (None, d(v)));
        for (conn, v) in guests.iter().filter(|(_, v)| ok(v)) {
            if best.is_none_or(|(_, b)| d(v) < b) {
                best = Some((Some(*conn), d(v)));
            }
        }
        best
    };
    nearest(&targetable)
        .or_else(|| nearest(&|v: &PlayerView| v.alive && !v.spectator))
        .and_then(|(conn, _)| conn)
}

pub(super) fn acts_on_player(e: &MobEvent) -> bool {
    matches!(
        e,
        MobEvent::AttackPlayer(..)
            | MobEvent::AttackPlayerWithEffect { .. }
            | MobEvent::DamagePlayer { .. }
            | MobEvent::SonicBoom {
                target: Who::Player,
                ..
            }
            | MobEvent::Push {
                target: Who::Player,
                ..
            }
            | MobEvent::Knockback {
                target: Who::Player,
                ..
            }
            | MobEvent::StabDismount {
                target: Who::Player,
            }
            | MobEvent::Ignite {
                target: Who::Player,
                ..
            }
            | MobEvent::Launch {
                target: Who::Player,
                ..
            }
            | MobEvent::EffectPlayer { .. }
            | MobEvent::AxolotlSupportPlayer { .. }
            | MobEvent::ParrotToShoulder { .. }
    )
}

impl Game<'_> {
    pub(super) fn lan_guest_views(&mut self) -> GuestViews {
        let Some(mut host) = self.lan.take() else {
            return Vec::new();
        };
        let conns: Vec<(u32, u128, [f64; 3])> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead)
            .map(|g| {
                let moved = [0, 1, 2].map(|k| g.position[k] - g.previous_position[k]);
                (g.conn, g.uuid, moved)
            })
            .collect();
        let mut views = Vec::new();
        for (conn, uuid, moved) in conns {
            if let Some(Some(mut v)) = self.as_guest(&mut host, conn, |g| g.player_view()) {
                v.hurt_by = None;
                v.last_hurt = None;
                v.hero = false;
                v.velocity = Vec3::new(moved[0], moved[1], moved[2]);
                v.uuid = vise_client::lan::playerdata::uuid_ints(uuid);
                v.allies = Default::default();
                v.remote = true;
                views.push((conn, v));
            }
        }
        self.lan = Some(host);
        views
    }

    pub(super) fn each_guest_hit(&mut self, f: impl FnMut(&mut Self)) {
        self.each_guest_hit_except(None, f);
    }

    pub(super) fn each_guest_hit_except(
        &mut self,
        except: Option<u32>,
        mut f: impl FnMut(&mut Self),
    ) {
        let Some(mut host) = self.lan.take() else {
            return;
        };
        let conns: Vec<u32> = host
            .hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && Some(g.conn) != except)
            .map(|g| g.conn)
            .collect();
        for conn in conns {
            self.guest_hit_by(&mut host, conn, &mut f);
        }
        self.lan = Some(host);
    }

    pub(super) fn lan_guest_mob_events(&mut self, events: Vec<(u32, MobEvent)>) {
        if events.is_empty() {
            return;
        }
        let Some(mut host) = self.lan.take() else {
            return;
        };
        for (conn, e) in events {
            self.guest_hit_by(&mut host, conn, |g| {
                let _ = g.handle_mob_events(vec![e]);
            });
        }
        self.lan = Some(host);
    }

    pub(super) fn lan_block_trigger_boxes(&self) -> Vec<([f64; 3], [f64; 3])> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| {
                let b = g.bounds();
                let (lo, hi) = (b.min_position(), b.max_position());
                ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z])
            })
            .collect()
    }
    pub(super) fn lan_guest_feet(&self) -> Vec<([f64; 3], bool)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (g.position, g.on_ground))
            .collect()
    }

    pub(super) fn lan_projectile_targets(&self) -> Vec<(Who, Aabb)> {
        let Some(host) = self.lan.as_ref() else {
            return Vec::new();
        };
        host.hub
            .guests
            .iter()
            .filter(|g| host.hub.here(g) && g.loaded && !g.dead && !g.spectator())
            .map(|g| (Who::Mob(g.entity_id as u32), guest_bounds(g)))
            .collect()
    }

    pub(super) fn lan_projectile_hit(&mut self, p: &Projectile, id: u32, at: Vec3) -> bool {
        let Some(mut host) = self.lan.take() else {
            return false;
        };
        let conn = host
            .hub
            .guests
            .iter()
            .find(|g| g.entity_id as u32 == id)
            .map(|g| g.conn);
        if let Some(conn) = conn {
            self.guest_hit_by(&mut host, conn, |g| {
                let hit = Hit::Entity {
                    who: Who::Player,
                    at,
                };
                let _ = g.projectile_hit(p, hit);
            });
        }
        self.lan = Some(host);
        conn.is_some()
    }
}

impl Game<'_> {
    pub(super) fn guest_hit_by<R>(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, pushed) = self.as_guest(host, conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let pushed = g
                .player
                .as_ref()
                .filter(|p| !p.health.sync_velocity && Some(p.velocity) != before)
                .map(|p| p.velocity);
            (result, pushed)
        })?;
        self.send_guest_motion(host, conn, pushed);
        Some(result)
    }

    pub(super) fn guest_struck_by<R>(
        &mut self,
        host: &mut LanHost,
        conn: u32,
        f: impl FnOnce(&mut Self) -> R,
    ) -> Option<R> {
        let (result, sent) = self.as_guest(host, conn, |g| {
            let before = g.player.as_ref().map(|p| p.velocity);
            let result = f(g);
            let sent = g.player.as_mut().zip(before).and_then(|(p, before)| {
                std::mem::take(&mut p.health.sync_velocity)
                    .then(|| std::mem::replace(&mut p.velocity, before))
            });
            (result, sent)
        })?;
        self.send_guest_motion(host, conn, sent);
        Some(result)
    }

    pub(super) fn send_guest_motion(&self, host: &LanHost, conn: u32, velocity: Option<Vec3>) {
        if let (Some(v), Some(g)) = (velocity, host.hub.guest(conn)) {
            g.send(&C::SetEntityMotion(play::SetEntityMotion {
                id: g.entity_id,
                velocity: [v.x, v.y, v.z],
            }));
        }
    }
}

pub(super) struct Present {
    pub uuid: [i32; 4],
    pub position: [f64; 3],
    pub eye: [f64; 3],
    pub height: f32,
    pub creative: bool,
    pub spectator: bool,
    pub alive: bool,
    pub omen: Option<vise_client::mobs::trial_spawner::Omen>,
}

impl Game<'_> {
    pub(super) fn lan_present(&self) -> Vec<Present> {
        let guests = self
            .lan
            .iter()
            .flat_map(|h| h.hub.guests.iter().filter(|g| h.hub.here(g)));
        guests
            .filter(|g| g.loaded)
            .map(|g| {
                let [x, y, z] = g.position;
                let effects = if self.lan_acting == Some(g.conn) {
                    &self.potions.effects
                } else {
                    &g.effects
                };
                Present {
                    uuid: vise_client::lan::playerdata::uuid_ints(g.uuid),
                    position: g.position,
                    eye: [x, y + g.eye_height(), z],
                    height: g.dimensions().height,
                    creative: g.creative(),
                    spectator: g.spectator(),
                    alive: !g.dead,
                    omen: omen_of(effects),
                }
            })
            .collect()
    }
    pub(super) fn acting_uuid(&self) -> [i32; 4] {
        self.lan_acting_uuid
            .map(vise_client::lan::playerdata::uuid_ints)
            .or(self.level.player_uuid)
            .unwrap_or_default()
    }
}

pub(super) fn omen_of(
    effects: &vise_client::effects::PlayerEffects,
) -> Option<vise_client::mobs::trial_spawner::Omen> {
    use vise_client::mobs::trial_spawner::Omen;
    if effects.get("trial_omen").is_some() {
        Some(Omen::Trial)
    } else {
        effects.get("bad_omen").map(|_| Omen::Bad)
    }
}

pub(super) fn guest_bounds(g: &vise_client::lan::guest::Guest) -> Aabb {
    g.bounds()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(x: f64, creative: bool) -> PlayerView {
        PlayerView {
            position: Vec3::new(x, 64.0, 0.0),
            bounds: Aabb::new(Vec3::new(x, 64.0, 0.0), Vec3::new(x, 65.8, 0.0)),
            eye: [x, 65.62, 0.0],
            alive: true,
            creative,
            sneaking: false,
            swimming: false,
            look: [0.0; 3],
            main_hand: None,
            off_hand: None,
            head: None,
            armor_cover: 0.0,
            invisible: false,
            spectator: false,
            gold_armor: false,
            hurt_by: None,
            last_hurt: None,
            velocity: Vec3::ZERO,
            yaw: 0.0,
            sprinting: false,
            riding: false,
            hero: false,
            effects: Vec::new(),
            uuid: [0; 4],
            allies: Default::default(),
            remote: false,
            sleeping: false,
            sleep_timer: 0,
            passenger: false,
        }
    }

    const MOB: Vec3 = Vec3::new(0.0, 64.0, 0.0);

    #[test]
    fn the_nearest_survival_player_is_seen() {
        let host = view(10.0, false);
        let guests = vec![(7, view(3.0, false)), (8, view(-5.0, false))];
        assert_eq!(choose(MOB, false, Some(&host), &guests, None), Some(7));
        let guests = vec![(7, view(3.0, true))];
        assert_eq!(choose(MOB, false, Some(&host), &guests, None), None);
        let host = view(10.0, true);
        assert_eq!(choose(MOB, false, Some(&host), &guests, None), Some(7));
        assert_eq!(choose(MOB, false, None, &Vec::new(), None), None);
    }

    #[test]
    fn a_targeted_guest_stays_the_target() {
        let host = view(1.0, false);
        let guests = vec![(7, view(12.0, false))];
        assert_eq!(choose(MOB, true, Some(&host), &guests, Some(7)), Some(7));
        let guests = vec![(7, view(12.0, true))];
        assert_eq!(choose(MOB, true, Some(&host), &guests, Some(7)), None);
        let guests = vec![(7, view(12.0, false))];
        assert_eq!(choose(MOB, false, Some(&host), &guests, Some(7)), None);
    }
}
