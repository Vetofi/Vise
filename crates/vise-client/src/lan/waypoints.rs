use crate::fast_hash::FxHashMap as HashMap;
use vise_protocol::packets::play::{
    Clientbound as C, TrackedWaypoint, WaypointId, WaypointTarget, waypoint_operation,
};

pub const PLAYER_RANGE: f64 = 6.0e7;
const REALLY_FAR: f32 = 332.0;
const AZIMUTH_STEP: f32 = 0.008_726_646;

pub const HIDING_HEADS: [&str; 8] = [
    "minecraft:carved_pumpkin",
    "minecraft:skeleton_skull",
    "minecraft:wither_skeleton_skull",
    "minecraft:player_head",
    "minecraft:zombie_head",
    "minecraft:creeper_head",
    "minecraft:dragon_head",
    "minecraft:piglin_head",
];

pub fn team_rgb(color: &str) -> Option<i32> {
    let rgb = match color {
        "black" => return Some(-13_619_152),
        "dark_blue" => 0x0000AA,
        "dark_green" => 0x00AA00,
        "dark_aqua" => 0x00AAAA,
        "dark_red" => 0xAA0000,
        "dark_purple" => 0xAA00AA,
        "gold" => 0xFFAA00,
        "gray" => 0xAAAAAA,
        "dark_gray" => 0x555555,
        "blue" => 0x5555FF,
        "green" => 0x55FF55,
        "aqua" => 0x55FFFF,
        "red" => 0xFF5555,
        "light_purple" => 0xFF55FF,
        "yellow" => 0xFFFF55,
        "white" => 0xFFFFFF,
        _ => return None,
    };
    Some(rgb)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub style: String,
    pub color: Option<i32>,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            style: "minecraft:default".into(),
            color: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Player {
    pub uuid: u128,
    pub conn: Option<u32>,
    pub position: [f64; 3],
    pub spectator: bool,
    pub transmit_range: f64,
    pub receive_range: f64,
    pub first_tick: bool,
    pub view_distance: i32,
    pub team: Option<(String, Option<i32>)>,
    pub icon: Icon,
}

impl Player {
    fn block(&self) -> [i32; 3] {
        self.position.map(|v| v.floor() as i32)
    }
    fn chunk(&self) -> [i32; 2] {
        let [x, _, z] = self.block();
        [x >> 4, z >> 4]
    }
    fn distance_to(&self, other: &Player) -> f32 {
        let d2: f64 = (0..3)
            .map(|k| (self.position[k] - other.position[k]).powi(2))
            .sum();
        d2.sqrt() as f32
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Block([i32; 3]),
    Chunk([i32; 2]),
    Azimuth(f32),
}

#[derive(Clone, Debug)]
struct Connection {
    kind: Kind,
    icon: Icon,
}

struct Memo {
    position: [f64; 3],
    view_center: [i32; 2],
    transmitting: bool,
    team: Option<(String, Option<i32>)>,
}

#[derive(Default)]
pub struct Waypoints {
    waypoints: Vec<u128>,
    players: Vec<u128>,
    connections: HashMap<u128, Vec<(u128, Connection)>>,
    memo: HashMap<u128, Memo>,
    enabled: Option<bool>,
}

struct Tick<'a> {
    players: HashMap<u128, &'a Player>,
    away: &'a [(u128, u32)],
    view: HashMap<u128, ([i32; 2], i32)>,
    fresh: &'a [u128],
    enabled: bool,
    out: Vec<(u32, C)>,
}

impl<'a> Tick<'a> {
    fn send(
        &mut self,
        receiver: u128,
        source: u128,
        operation: i32,
        icon: &Icon,
        target: WaypointTarget,
    ) {
        let Some(conn) = self
            .players
            .get(&receiver)
            .and_then(|p| p.conn)
            .or_else(|| {
                self.away
                    .iter()
                    .find(|(uuid, _)| *uuid == receiver)
                    .map(|&(_, conn)| conn)
            })
        else {
            return;
        };
        self.out
            .push((conn, waypoint(source, operation, icon, target)));
    }
    fn connect(&mut self, receiver: u128, source: u128, c: &Connection) {
        let target = match c.kind {
            Kind::Block(p) => WaypointTarget::Block(p),
            Kind::Chunk(p) => WaypointTarget::Chunk(p),
            Kind::Azimuth(a) => WaypointTarget::Azimuth(a),
        };
        self.send(receiver, source, waypoint_operation::TRACK, &c.icon, target);
    }
    fn disconnect(&mut self, receiver: u128, source: u128) {
        self.send(
            receiver,
            source,
            waypoint_operation::UNTRACK,
            &Icon::default(),
            WaypointTarget::Empty,
        );
    }
    fn chunk_visible(&self, [x, z]: [i32; 2], receiver: u128) -> bool {
        let Some(&([cx, cz], r)) = self.view.get(&receiver) else {
            return false;
        };
        let dx = i64::from(((x - cx).abs() - 1).max(0));
        let dz = i64::from(((z - cz).abs() - 1).max(0));
        dx * dx + dz * dz < i64::from(r) * i64::from(r)
    }
    fn ignores(&self, source: &Player, receiver: &Player) -> bool {
        if receiver.spectator {
            return false;
        }
        if source.spectator {
            return true;
        }
        let range = source.transmit_range.min(receiver.receive_range);
        f64::from(source.distance_to(receiver)) >= range
    }
    fn really_far(source: &Player, receiver: &Player) -> bool {
        source.distance_to(receiver) > REALLY_FAR
    }
    fn azimuth(source: &Player, receiver: &Player) -> f32 {
        let d = [0, 1, 2].map(|k| receiver.position[k] - source.position[k]);
        vise_core::geometry::trig::atan2(d[0], -d[2]) as f32
    }
    fn pair(&self, source: u128, receiver: u128) -> Option<(&'a Player, &'a Player)> {
        Some((*self.players.get(&source)?, *self.players.get(&receiver)?))
    }
    fn make(&self, (s, r): (&Player, &Player)) -> Option<Connection> {
        if s.first_tick || self.fresh.contains(&s.uuid) || s.uuid == r.uuid || self.ignores(s, r) {
            return None;
        }
        let mut icon = s.icon.clone();
        if icon.color.is_none() {
            icon.color = s.team.as_ref().and_then(|t| t.1);
        }
        let kind = if Self::really_far(s, r) {
            Kind::Azimuth(Self::azimuth(s, r))
        } else if !self.chunk_visible(s.chunk(), r.uuid) {
            Kind::Chunk(s.chunk())
        } else {
            Kind::Block(s.block())
        };
        Some(Connection { kind, icon })
    }
    fn broken(&self, c: &Connection, (s, r): (&Player, &Player)) -> bool {
        let receiver = r.uuid;
        match c.kind {
            Kind::Block(last) => {
                let now = s.block();
                (0..3).map(|k| (now[k] - last[k]).abs()).sum::<i32>() > 1 || self.ignores(s, r)
            }
            Kind::Chunk(last) => {
                let now = s.chunk();
                let moved = (now[0] - last[0]).abs().max((now[1] - last[1]).abs());
                moved > 1 || self.ignores(s, r) || self.chunk_visible(last, receiver)
            }
            Kind::Azimuth(_) => {
                self.ignores(s, r)
                    || self.chunk_visible(s.chunk(), receiver)
                    || !Self::really_far(s, r)
            }
        }
    }
    fn update(&mut self, c: &mut Connection, (s, r): (&Player, &Player)) {
        let target = match &mut c.kind {
            Kind::Block(last) => {
                let now = s.block();
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Block(now)
            }
            Kind::Chunk(last) => {
                let now = s.chunk();
                if now == *last {
                    return;
                }
                *last = now;
                WaypointTarget::Chunk(now)
            }
            Kind::Azimuth(last) => {
                let now = Self::azimuth(s, r);
                if (now - *last).abs() <= AZIMUTH_STEP {
                    return;
                }
                *last = now;
                WaypointTarget::Azimuth(now)
            }
        };
        if let Some(conn) = r.conn {
            self.out.push((
                conn,
                waypoint(s.uuid, waypoint_operation::UPDATE, &c.icon, target),
            ));
        }
    }
}

fn waypoint(source: u128, operation: i32, icon: &Icon, target: WaypointTarget) -> C {
    C::TrackedWaypoint(TrackedWaypoint {
        operation,
        id: WaypointId::Uuid(source),
        style: icon.style.clone(),
        color: icon.color,
        target,
    })
}

impl Waypoints {
    fn put(&mut self, receiver: u128, source: u128, c: Connection) {
        let row = self.connections.entry(receiver).or_default();
        match row.iter_mut().find(|(s, _)| *s == source) {
            Some(slot) => slot.1 = c,
            None => row.push((source, c)),
        }
    }
    fn take(&mut self, receiver: u128, source: u128) -> Option<Connection> {
        let row = self.connections.get_mut(&receiver)?;
        let i = row.iter().position(|(s, _)| *s == source)?;
        Some(row.remove(i).1)
    }

    fn create_connection(&mut self, t: &mut Tick, receiver: u128, source: u128) {
        if receiver == source || !t.enabled {
            return;
        }
        match t.pair(source, receiver).and_then(|pair| t.make(pair)) {
            Some(c) => {
                t.connect(receiver, source, &c);
                self.put(receiver, source, c);
            }
            None => {
                if self.take(receiver, source).is_some() {
                    t.disconnect(receiver, source);
                }
            }
        }
    }
    fn update_or_create(&mut self, t: &mut Tick, receiver: u128, source: u128) {
        if receiver == source || !t.enabled {
            return;
        }
        let Some(row) = self.connections.get_mut(&receiver) else {
            return self.create_connection(t, receiver, source);
        };
        let Some(i) = row.iter().position(|(s, _)| *s == source) else {
            return self.create_connection(t, receiver, source);
        };
        let c = &mut row[i].1;
        let pair = t.pair(source, receiver);
        if let Some(pair) = pair
            && !t.broken(c, pair)
        {
            t.update(c, pair);
            return;
        }
        match pair.and_then(|pair| t.make(pair)) {
            Some(new) => {
                t.connect(receiver, source, &new);
                *c = new;
            }
            None => {
                t.disconnect(receiver, source);
                row.remove(i);
            }
        }
    }
    fn track(&mut self, t: &mut Tick, source: u128) {
        if !self.waypoints.contains(&source) {
            self.waypoints.push(source);
        }
        for receiver in self.players.clone() {
            self.create_connection(t, receiver, source);
        }
    }
    fn update_waypoint(&mut self, t: &mut Tick, source: u128) {
        if !self.waypoints.contains(&source) {
            return;
        }
        for k in 0..self.players.len() {
            self.update_or_create(t, self.players[k], source);
        }
    }
    fn untrack(&mut self, t: &mut Tick, source: u128) {
        for receiver in self.players.clone() {
            if self.take(receiver, source).is_some() {
                t.disconnect(receiver, source);
            }
        }
        self.waypoints.retain(|w| *w != source);
    }
    fn add_player(&mut self, t: &mut Tick, player: u128, transmitting: bool) {
        if !self.players.contains(&player) {
            self.players.push(player);
        }
        for source in self.waypoints.clone() {
            self.create_connection(t, player, source);
        }
        if transmitting {
            self.track(t, player);
        }
    }
    fn update_player(&mut self, t: &mut Tick, player: u128) {
        for k in 0..self.waypoints.len() {
            self.update_or_create(t, player, self.waypoints[k]);
        }
    }
    fn remove_player(&mut self, t: &mut Tick, player: u128) {
        for (source, _) in self.connections.remove(&player).unwrap_or_default() {
            t.disconnect(player, source);
        }
        self.untrack(t, player);
        self.players.retain(|p| *p != player);
    }
    fn break_all(&mut self, t: &mut Tick) {
        for receiver in self.players.clone() {
            for (source, _) in self.connections.remove(&receiver).unwrap_or_default() {
                t.disconnect(receiver, source);
            }
        }
        self.connections.clear();
    }

    pub fn tick(
        &mut self,
        players: &[Player],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        self.tick_level(players, &[], respawned, locator_bar)
    }

    pub fn tick_level(
        &mut self,
        players: &[Player],
        away: &[(u128, u32)],
        respawned: &[u128],
        locator_bar: bool,
    ) -> Vec<(u32, C)> {
        let mut t = Tick {
            players: players.iter().map(|p| (p.uuid, p)).collect(),
            away,
            view: HashMap::default(),
            fresh: respawned,
            enabled: locator_bar,
            out: Vec::new(),
        };
        let was = self.enabled.replace(locator_bar);
        for (uuid, memo) in &self.memo {
            if let Some(p) = t.players.get(uuid) {
                t.view.insert(*uuid, (memo.view_center, p.view_distance));
            }
        }
        let gone: Vec<u128> = self
            .players
            .iter()
            .copied()
            .filter(|uuid| !t.players.contains_key(uuid) || respawned.contains(uuid))
            .collect();
        for uuid in gone {
            self.remove_player(&mut t, uuid);
            self.memo.remove(&uuid);
        }
        if was == Some(true) && !locator_bar {
            self.break_all(&mut t);
        }
        for p in players {
            if self.memo.contains_key(&p.uuid) {
                continue;
            }
            let center = p.chunk();
            t.view.insert(p.uuid, (center, p.view_distance));
            let transmitting = p.transmit_range > 0.0;
            self.memo.insert(
                p.uuid,
                Memo {
                    position: p.position,
                    view_center: center,
                    transmitting,
                    team: p.team.clone(),
                },
            );
            if p.receive_range > 0.0 {
                self.add_player(&mut t, p.uuid, transmitting);
            } else if transmitting {
                self.track(&mut t, p.uuid);
            }
        }
        if was == Some(false) && locator_bar {
            for player in self.players.clone() {
                self.update_player(&mut t, player);
            }
        }
        for p in players {
            let memo = &self.memo[&p.uuid];
            if memo.position != p.position {
                let moved_view = memo.view_center != p.chunk();
                let transmitting = memo.transmitting;
                self.memo.get_mut(&p.uuid).unwrap().position = p.position;
                if !p.first_tick {
                    if transmitting {
                        self.update_waypoint(&mut t, p.uuid);
                    }
                    if p.receive_range > 0.0 {
                        self.update_player(&mut t, p.uuid);
                    }
                }
                if moved_view {
                    self.memo.get_mut(&p.uuid).unwrap().view_center = p.chunk();
                    t.view.insert(p.uuid, (p.chunk(), p.view_distance));
                }
                if p.receive_range > 0.0 {
                    self.update_player(&mut t, p.uuid);
                }
            }
            let transmitting = p.transmit_range > 0.0;
            if transmitting != self.memo[&p.uuid].transmitting {
                self.memo.get_mut(&p.uuid).unwrap().transmitting = transmitting;
                if transmitting {
                    self.track(&mut t, p.uuid);
                } else {
                    self.untrack(&mut t, p.uuid);
                }
            }
            if p.team != self.memo[&p.uuid].team {
                self.memo.get_mut(&p.uuid).unwrap().team = p.team.clone();
                for receiver in self.players.clone() {
                    self.create_connection(&mut t, receiver, p.uuid);
                }
            }
        }
        t.out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(uuid: u128, conn: u32, position: [f64; 3]) -> Player {
        Player {
            uuid,
            conn: Some(conn),
            position,
            spectator: false,
            transmit_range: PLAYER_RANGE,
            receive_range: PLAYER_RANGE,
            first_tick: false,
            view_distance: 4,
            team: None,
            icon: Icon::default(),
        }
    }

    fn to(out: &[(u32, C)], conn: u32) -> Vec<(i32, WaypointTarget)> {
        out.iter()
            .filter(|(c, _)| *c == conn)
            .map(|(_, p)| match p {
                C::TrackedWaypoint(w) => (w.operation, w.target),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_new_player_shows_once_it_moves() {
        let mut w = Waypoints::default();
        let a = player(1, 10, [0.5, 100.0, 0.5]);
        let mut b = player(2, 20, [5.5, 100.0, 0.5]);
        b.first_tick = true;
        w.tick(std::slice::from_ref(&a), &[], true);
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 20), [(0, WaypointTarget::Block([0, 100, 0]))]);
        assert!(to(&out, 10).is_empty());
        b.first_tick = false;
        assert!(w.tick(&[a.clone(), b.clone()], &[], true).is_empty());
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([6, 100, 0]))]);
        b.position[0] += 1.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(2, WaypointTarget::Block([7, 100, 0]))]);
        b.position[0] += 3.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Block([10, 100, 0]))]);
        b.position[0] = 100.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(0, WaypointTarget::Chunk([6, 0]))]);
        b.position[0] = 1000.5;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 10)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        b.transmit_range = 0.0;
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert_eq!(to(&out, 10), [(1, WaypointTarget::Empty)]);
        let out = w.tick(&[a.clone(), b.clone()], &[], false);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
        assert!(to(&out, 10).is_empty());
        let out = w.tick(&[a.clone(), b.clone()], &[], true);
        assert!(matches!(
            to(&out, 20)[..],
            [(0, WaypointTarget::Azimuth(_))]
        ));
        let out = w.tick(&[b.clone()], &[], true);
        assert_eq!(to(&out, 20), [(1, WaypointTarget::Empty)]);
    }

    #[test]
    fn a_player_leaving_the_level_loses_its_waypoints() {
        let (a, b) = (
            player(1, 10, [0.5, 64.0, 0.5]),
            player(2, 20, [4.5, 64.0, 0.5]),
        );
        let mut w = Waypoints::default();
        w.tick(&[a.clone(), b.clone()], &[], true);
        let mut moved = b.clone();
        moved.position = [5.5, 64.0, 0.5];
        w.tick(&[a.clone(), moved], &[], true);
        let out = w.tick_level(std::slice::from_ref(&a), &[(2, 20)], &[], true);
        assert_eq!(
            to(&out, 10),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
        assert_eq!(
            to(&out, 20),
            vec![(waypoint_operation::UNTRACK, WaypointTarget::Empty)]
        );
    }

    #[test]
    fn team_colours() {
        assert_eq!(team_rgb("red"), Some(0xFF5555));
        assert_eq!(team_rgb("black"), Some(0xFF30_3030_u32 as i32));
        assert_eq!(team_rgb("reset"), None);
    }
}
