use super::tracking;
use vise_protocol::{
    buf::pack_degrees,
    entity_data::{Value, serializer},
    item::Stack,
    packets::play::{
        self, AddEntity, AttributeSnapshot, EntityPositionSync, MoveEntity, MoveEntityPos,
        MoveEntityPosRot, MoveEntityRot, PositionPath, RotateHead, SetEntityData, SetEntityLink,
        SetEntityMotion, SetEquipment, SetPassengers, UpdateAttributes, VecDelta,
    },
};

pub fn changed_attributes(
    now: &[AttributeSnapshot],
    sent: &[AttributeSnapshot],
) -> Vec<AttributeSnapshot> {
    now.iter()
        .filter(|a| {
            sent.iter()
                .find(|b| b.attribute == a.attribute)
                .is_none_or(|b| b != *a)
        })
        .cloned()
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityState {
    pub id: i32,
    pub uuid: u128,
    pub kind: String,
    pub position: [f64; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub head_yaw: f32,
    pub on_ground: bool,
    pub velocity: [f64; 3],
    pub data: i32,
    pub meta: Vec<(u8, i32, Value)>,
    pub defaults: Vec<(u8, i32, Value)>,
    pub equipment: Vec<(u8, Option<Stack>)>,
    pub attributes: Vec<AttributeSnapshot>,
    pub precise: bool,
    pub needs_sync: bool,
    pub leash: Option<i32>,
    pub spectator: bool,
}

impl EntityState {
    pub fn reset(&mut self, id: i32, uuid: u128, kind: &str) {
        self.id = id;
        self.uuid = uuid;
        self.kind.clear();
        self.kind.push_str(kind);
        self.position = [0.0; 3];
        (self.yaw, self.pitch, self.head_yaw) = (0.0, 0.0, 0.0);
        self.on_ground = false;
        self.velocity = [0.0; 3];
        self.data = 0;
        self.meta.clear();
        self.defaults.clear();
        self.equipment.clear();
        self.attributes.clear();
        self.precise = false;
        self.needs_sync = false;
        self.leash = None;
        self.spectator = false;
    }
}

#[derive(Default)]
pub struct States {
    list: Vec<EntityState>,
    len: usize,
}

impl States {
    pub fn clear(&mut self) {
        self.len = 0;
    }
    pub fn add(&mut self, id: i32, uuid: u128, kind: &str) -> &mut EntityState {
        if self.len == self.list.len() {
            self.list.push(EntityState::default());
        }
        let e = &mut self.list[self.len];
        self.len += 1;
        e.reset(id, uuid, kind);
        e
    }
    pub fn push(&mut self, e: EntityState) {
        match self.list.get_mut(self.len) {
            Some(slot) => *slot = e,
            None => self.list.push(e),
        }
        self.len += 1;
    }
    pub fn as_slice(&self) -> &[EntityState] {
        &self.list[..self.len]
    }
    pub fn as_mut_slice(&mut self) -> &mut [EntityState] {
        &mut self.list[..self.len]
    }
}

impl Extend<EntityState> for States {
    fn extend<I: IntoIterator<Item = EntityState>>(&mut self, states: I) {
        for e in states {
            self.push(e);
        }
    }
}

fn is_arrow(kind: &str) -> bool {
    matches!(
        kind,
        "minecraft:arrow" | "minecraft:spectral_arrow" | "minecraft:trident"
    )
}

fn acceleration_power(kind: &str) -> Option<f64> {
    match kind {
        "minecraft:fireball"
        | "minecraft:small_fireball"
        | "minecraft:dragon_fireball"
        | "minecraft:wither_skull" => Some(0.1),
        "minecraft:wind_charge" | "minecraft:breeze_wind_charge" => Some(0.0),
        _ => None,
    }
}

#[derive(Default)]
pub struct Riding {
    pub passengers: crate::fast_hash::FxHashMap<i32, Vec<i32>>,
    pub vehicle: crate::fast_hash::FxHashMap<i32, i32>,
}

impl Riding {
    pub fn clear(&mut self) {
        self.passengers.clear();
        self.vehicle.clear();
    }
    pub fn add(&mut self, vehicle: i32, passenger: i32) {
        self.passengers.entry(vehicle).or_default().push(passenger);
        self.vehicle.insert(passenger, vehicle);
    }
    pub fn passengers(&self, id: i32) -> &[i32] {
        self.passengers.get(&id).map_or(&[], Vec::as_slice)
    }
    fn set_passengers(&self, vehicle: i32) -> play::Clientbound {
        play::Clientbound::SetPassengers(SetPassengers {
            vehicle,
            passengers: self.passengers(vehicle).to_vec(),
        })
    }
}

struct ServerEntity {
    kind: Option<i32>,
    range: i32,
    interval: u32,
    track_deltas: bool,
    arrow: bool,
    power: Option<f64>,
    base: [f64; 3],
    rotation: (i8, i8),
    head: i8,
    velocity: [f64; 3],
    meta: Vec<(u8, i32, Value)>,
    equipment: Vec<(u8, Option<Stack>)>,
    attributes: Vec<AttributeSnapshot>,
    was_on_ground: bool,
    tick: u32,
    teleport_delay: u32,
    passengers: Vec<i32>,
    was_riding: bool,
    leash: Option<i32>,
    seen: u32,
    index: usize,
    added: bool,
    section: [i32; 3],
    spectator: bool,
}

fn section_of(position: [f64; 3]) -> [i32; 3] {
    position.map(|v| (v.floor() as i32) >> 4)
}

fn encode(v: f64) -> i64 {
    (v * 4096.0).round() as i64
}

impl ServerEntity {
    fn new(e: &EntityState, kind: Option<i32>, riding: &Riding) -> Self {
        let (range, interval, track_deltas) = tracking::settings(&e.kind);
        Self {
            kind,
            range: i32::from(range),
            interval,
            track_deltas,
            arrow: is_arrow(&e.kind),
            power: acceleration_power(&e.kind),
            base: e.position,
            rotation: (pack_degrees(e.pitch), pack_degrees(e.yaw)),
            head: pack_degrees(e.head_yaw),
            velocity: e.velocity,
            meta: e.meta.clone(),
            equipment: e.equipment.clone(),
            attributes: e.attributes.clone(),
            was_on_ground: e.on_ground,
            tick: 0,
            teleport_delay: 0,
            passengers: riding.passengers(e.id).to_vec(),
            was_riding: false,
            leash: e.leash,
            seen: 0,
            index: 0,
            added: true,
            section: section_of(e.position),
            spectator: e.spectator,
        }
    }

    fn pairing(
        &self,
        e: &EntityState,
        kind: i32,
        riding: &Riding,
        out: &mut Vec<play::Clientbound>,
    ) {
        out.push(play::Clientbound::AddEntity(AddEntity {
            id: e.id,
            uuid: e.uuid,
            kind,
            position: e.position,
            velocity: e.velocity,
            pitch: pack_degrees(e.pitch),
            yaw: pack_degrees(e.yaw),
            head_yaw: pack_degrees(e.head_yaw),
            data: e.data,
        }));
        if !self.meta.is_empty() {
            out.push(play::Clientbound::SetEntityData(SetEntityData {
                id: e.id,
                values: self.meta.clone(),
            }));
        }
        if !e.attributes.is_empty() {
            out.push(play::Clientbound::UpdateAttributes(UpdateAttributes {
                id: e.id,
                values: e.attributes.clone(),
            }));
        }
        if !self.equipment.is_empty() {
            out.push(play::Clientbound::SetEquipment(SetEquipment {
                id: e.id,
                slots: self.equipment.clone(),
            }));
        }
        if !riding.passengers(e.id).is_empty() {
            out.push(riding.set_passengers(e.id));
        }
        if let Some(&vehicle) = riding.vehicle.get(&e.id) {
            out.push(riding.set_passengers(vehicle));
        }
        if let Some(dest) = e.leash {
            out.push(play::Clientbound::SetEntityLink(SetEntityLink {
                source: e.id,
                dest,
            }));
        }
    }

    fn send_changes(&mut self, e: &EntityState, riding: &Riding, out: &mut Vec<play::Clientbound>) {
        let passengers = riding.passengers(e.id);
        if passengers != self.passengers.as_slice() {
            out.push(riding.set_passengers(e.id));
            self.passengers.clear();
            self.passengers.extend_from_slice(passengers);
        }
        if e.leash != self.leash {
            out.push(play::Clientbound::SetEntityLink(SetEntityLink {
                source: e.id,
                dest: e.leash.unwrap_or(0),
            }));
            self.leash = e.leash;
        }
        let dirty = e.meta != self.meta;
        let due = self.interval != 0 && self.tick.is_multiple_of(self.interval);
        if due || dirty || e.needs_sync {
            let rotation = (pack_degrees(e.pitch), pack_degrees(e.yaw));
            let turned = |a: i8, b: i8| (i32::from(a) - i32::from(b)).abs() >= 1;
            let rotate = self.arrow
                || turned(rotation.1, self.rotation.1)
                || turned(rotation.0, self.rotation.0);
            if riding.vehicle.contains_key(&e.id) {
                if rotate {
                    out.push(play::Clientbound::MoveEntityRot(MoveEntityRot(
                        MoveEntity {
                            id: e.id,
                            delta: None,
                            rotation: Some((rotation.1, rotation.0)),
                            on_ground: e.on_ground,
                        },
                    )));
                    self.rotation = rotation;
                }
                self.base = e.position;
                self.was_riding = true;
                self.send_dirty(e, dirty, out);
                self.send_head(e, out);
                self.tick += 1;
                self.send_equipment(e, out);
                return;
            }
            self.teleport_delay += 1;
            let d: f64 = (0..3).map(|k| (e.position[k] - self.base[k]).powi(2)).sum();
            let send_position = self.arrow || d >= 7.629_394_5e-6 || self.tick.is_multiple_of(60);
            let sync = EntityPositionSync {
                id: e.id,
                path: PositionPath::Linear(e.position),
                yaw: e.yaw,
                pitch: e.pitch,
                on_ground: e.on_ground,
            };
            let packet = if e.precise
                || self.teleport_delay > 400
                || self.was_riding
                || self.was_on_ground != e.on_ground
            {
                self.was_on_ground = e.on_ground;
                self.teleport_delay = 0;
                self.was_riding = false;
                Some(play::Clientbound::EntityPositionSync(sync))
            } else if send_position {
                let delta = [0, 1, 2].map(|k| encode(e.position[k]) - encode(self.base[k]));
                if delta.iter().any(|v| i16::try_from(*v).is_err()) {
                    Some(play::Clientbound::EntityPositionSync(sync))
                } else {
                    let m = MoveEntity {
                        id: e.id,
                        delta: Some(VecDelta::Linear(delta.map(|v| v as i16))),
                        rotation: rotate.then_some((rotation.1, rotation.0)),
                        on_ground: e.on_ground,
                    };
                    Some(if rotate {
                        play::Clientbound::MoveEntityPosRot(MoveEntityPosRot(m))
                    } else {
                        play::Clientbound::MoveEntityPos(MoveEntityPos(m))
                    })
                }
            } else if rotate {
                Some(play::Clientbound::MoveEntityRot(MoveEntityRot(
                    MoveEntity {
                        id: e.id,
                        delta: None,
                        rotation: Some((rotation.1, rotation.0)),
                        on_ground: e.on_ground,
                    },
                )))
            } else {
                None
            };
            if self.track_deltas || e.needs_sync {
                let diff: f64 = (0..3)
                    .map(|k| (e.velocity[k] - self.velocity[k]).powi(2))
                    .sum();
                let still = e.velocity.iter().all(|v| *v == 0.0);
                if diff > 1.0e-7 || diff > 0.0 && still {
                    self.velocity = e.velocity;
                    let motion = play::Clientbound::SetEntityMotion(SetEntityMotion {
                        id: e.id,
                        velocity: e.velocity,
                    });
                    match self.power {
                        Some(power) => out.extend([
                            play::Clientbound::BundleDelimiter(vise_protocol::packets::Empty),
                            motion,
                            play::Clientbound::ProjectilePower(play::ProjectilePower {
                                id: e.id,
                                power,
                            }),
                            play::Clientbound::BundleDelimiter(vise_protocol::packets::Empty),
                        ]),
                        None => out.push(motion),
                    }
                }
            }
            if let Some(packet) = packet {
                let (position, rotated) = match &packet {
                    play::Clientbound::EntityPositionSync(_)
                    | play::Clientbound::MoveEntityPosRot(_) => (true, true),
                    play::Clientbound::MoveEntityPos(_) => (true, false),
                    _ => (false, true),
                };
                out.push(packet);
                if position {
                    self.base = e.position;
                }
                if rotated {
                    self.rotation = rotation;
                }
            }
            self.send_dirty(e, dirty, out);
            self.send_head(e, out);
        }
        self.tick += 1;
        self.send_equipment(e, out);
    }

    fn send_dirty(&mut self, e: &EntityState, dirty: bool, out: &mut Vec<play::Clientbound>) {
        if dirty {
            let mut changed: Vec<_> = e
                .meta
                .iter()
                .filter(|m| !self.meta.contains(m))
                .cloned()
                .collect();
            let frame = e.kind.ends_with("item_frame");
            for (index, kind, _) in &self.meta {
                let default = match (*index, *kind) {
                    (0, serializer::BYTE) => Value::Byte(0),
                    (6, serializer::POSE) => Value::Id(0),
                    (14, serializer::OPTIONAL_BLOCK_POS) => Value::OptPos(None),
                    (9, serializer::ITEM_STACK) if frame => Value::Item(None),
                    (10, serializer::INT) if frame => Value::Int(0),
                    _ => match e.defaults.iter().find(|d| (d.0, d.1) == (*index, *kind)) {
                        Some(d) => d.2.clone(),
                        None => continue,
                    },
                };
                if !e.meta.iter().any(|m| m.0 == *index) {
                    changed.push((*index, *kind, default));
                }
            }
            if !changed.is_empty() {
                out.push(play::Clientbound::SetEntityData(SetEntityData {
                    id: e.id,
                    values: changed,
                }));
            }
            self.meta.clone_from(&e.meta);
        }
        if e.attributes != self.attributes {
            let changed = changed_attributes(&e.attributes, &self.attributes);
            if !changed.is_empty() {
                out.push(play::Clientbound::UpdateAttributes(UpdateAttributes {
                    id: e.id,
                    values: changed,
                }));
            }
            self.attributes.clone_from(&e.attributes);
        }
    }

    fn send_head(&mut self, e: &EntityState, out: &mut Vec<play::Clientbound>) {
        let head = pack_degrees(e.head_yaw);
        if (i32::from(head) - i32::from(self.head)).abs() >= 1 {
            out.push(play::Clientbound::RotateHead(RotateHead {
                id: e.id,
                head_yaw: head,
            }));
            self.head = head;
        }
    }

    fn send_equipment(&mut self, e: &EntityState, out: &mut Vec<play::Clientbound>) {
        if e.equipment != self.equipment {
            let mut slots: Vec<(u8, Option<Stack>)> = e
                .equipment
                .iter()
                .filter(|m| !self.equipment.contains(m))
                .cloned()
                .collect();
            for (slot, _) in &self.equipment {
                if !e.equipment.iter().any(|(k, _)| k == slot) {
                    slots.push((*slot, None));
                }
            }
            if !slots.is_empty() {
                out.push(play::Clientbound::SetEquipment(SetEquipment {
                    id: e.id,
                    slots,
                }));
            }
            self.equipment.clone_from(&e.equipment);
        }
    }
}

#[derive(Default)]
pub struct Tracker {
    seen: crate::fast_hash::FxHashSet<i32>,
}

impl Tracker {
    pub fn is_tracking(&self, id: i32) -> bool {
        self.seen.contains(&id)
    }
    pub fn ids(&self) -> Vec<i32> {
        self.seen.iter().copied().collect()
    }
    pub fn clear(&mut self) {
        self.seen.clear();
    }
    pub fn forget(&mut self, id: i32) -> bool {
        self.seen.remove(&id)
    }
    pub fn len(&self) -> usize {
        self.seen.len()
    }
    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

pub struct Viewer<'a> {
    pub entity_id: i32,
    pub spectator: bool,
    pub moved: bool,
    pub position: [f64; 3],
    pub view_distance: i32,
    pub chunks: &'a crate::fast_hash::FxHashSet<(i32, i32)>,
    pub tracker: &'a mut Tracker,
    pub out: &'a mut Vec<Vec<u8>>,
}

#[derive(Clone, Copy)]
struct Probe {
    x: f64,
    z: f64,
    id: i32,
    range: i32,
    chunk: u32,
    spectator: bool,
}

fn sees(v: &Viewer, p: &Probe, chunks: &[(i32, i32)], held: &mut [u8]) -> bool {
    let range = f64::from(p.range.min(v.view_distance * 16));
    let (dx, dz) = (v.position[0] - p.x, v.position[2] - p.z);
    dx * dx + dz * dz <= range * range && (v.spectator || !p.spectator) && {
        let held = &mut held[p.chunk as usize];
        if *held == 0 {
            *held = 1 + u8::from(v.chunks.contains(&chunks[p.chunk as usize]));
        }
        *held == 2
    }
}

#[derive(Default)]
pub struct Entities {
    pub range_percent: Option<i32>,
    states: crate::fast_hash::FxHashMap<i32, ServerEntity>,
    tick: u32,
    probes: Vec<Probe>,
    chunks: Vec<(i32, i32)>,
    chunk_index: crate::fast_hash::FxHashMap<(i32, i32), u32>,
    held: Vec<u8>,
    flags: Vec<Vec<u8>>,
    pairs: Vec<bool>,
    moved: Vec<usize>,
    is_moved: Vec<bool>,
    packets: Vec<play::Clientbound>,
    bodies: Vec<Vec<u8>>,
}

const SEES: u8 = 1;
const TRACKS: u8 = 2;

fn encode_all(packets: &[play::Clientbound], bodies: &mut Vec<Vec<u8>>) {
    bodies.clear();
    bodies.extend(packets.iter().map(play::Clientbound::encode));
}

impl Entities {
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn tick(
        &mut self,
        entities: &[&EntityState],
        riding: &Riding,
        viewers: &mut [Viewer],
        mut kind: impl FnMut(&str) -> Option<i32>,
    ) {
        self.tick = self.tick.wrapping_add(1);
        let now = self.tick;
        self.probes.clear();
        self.chunks.clear();
        self.chunk_index.clear();
        self.moved.clear();
        self.is_moved.clear();
        self.is_moved.resize(entities.len(), false);
        for (index, e) in entities.iter().enumerate() {
            let mut added = false;
            let s = self.states.entry(e.id).or_insert_with(|| {
                added = true;
                ServerEntity::new(e, kind(&e.kind), riding)
            });
            s.seen = now;
            s.index = index;
            let section = section_of(e.position);
            if added || section != s.section || e.spectator != s.spectator {
                s.section = section;
                s.spectator = e.spectator;
                self.moved.push(index);
                self.is_moved[index] = true;
            }
            let at = (
                (e.position[0].floor() as i32) >> 4,
                (e.position[2].floor() as i32) >> 4,
            );
            let next = self.chunks.len() as u32;
            let chunk = *self.chunk_index.entry(at).or_insert(next);
            if chunk == next {
                self.chunks.push(at);
            }
            self.probes.push(Probe {
                x: e.position[0],
                z: e.position[2],
                id: e.id,
                range: self
                    .range_percent
                    .map_or(s.range * 16, |p| p * (s.range * 16) / 100),
                chunk,
                spectator: e.spectator,
            });
        }
        for (&vehicle, passengers) in &riding.passengers {
            let Some(own) = self.states.get(&vehicle).filter(|s| s.seen == now) else {
                continue;
            };
            let (index, mut range) = (own.index, own.range);
            let mut pending = passengers.clone();
            while let Some(id) = pending.pop() {
                if let Some(s) = self.states.get(&id).filter(|s| s.seen == now) {
                    range = range.max(s.range);
                }
                pending.extend_from_slice(riding.passengers(id));
            }
            self.probes[index].range = self
                .range_percent
                .map_or(range * 16, |p| p * (range * 16) / 100);
        }
        self.flags.resize_with(viewers.len(), Vec::new);
        self.pairs.clear();
        self.pairs.resize(entities.len(), false);
        for (v, flags) in viewers.iter_mut().zip(&mut self.flags) {
            flags.clear();
            self.held.clear();
            self.held.resize(self.chunks.len(), 0);
            let (chunks, held, pairs) = (&self.chunks, &mut self.held, &mut self.pairs);
            let mut kept = 0;
            let all = v.moved
                || self
                    .states
                    .get(&v.entity_id)
                    .is_none_or(|s| s.seen != now || self.is_moved[s.index]);
            if all {
                flags.extend(self.probes.iter().zip(pairs.iter_mut()).map(|(p, pairs)| {
                    if p.id == v.entity_id || !sees(v, p, chunks, held) {
                        0
                    } else if v.tracker.seen.contains(&p.id) {
                        kept += 1;
                        SEES | TRACKS
                    } else {
                        *pairs = true;
                        SEES
                    }
                }));
            } else if v.tracker.seen.len() * 4 > self.probes.len() {
                let is_moved = &self.is_moved;
                flags.extend(self.probes.iter().zip(pairs.iter_mut()).zip(is_moved).map(
                    |((p, pairs), &moved)| {
                        let tracks = v.tracker.seen.contains(&p.id);
                        if p.id == v.entity_id || (moved && !sees(v, p, chunks, held)) {
                            0
                        } else if tracks {
                            kept += 1;
                            SEES | TRACKS
                        } else if moved {
                            *pairs = true;
                            SEES
                        } else {
                            0
                        }
                    },
                ));
            } else {
                flags.resize(self.probes.len(), 0);
                for id in &v.tracker.seen {
                    if let Some(s) = self.states.get(id).filter(|s| s.seen == now) {
                        flags[s.index] = SEES | TRACKS;
                        kept += 1;
                    }
                }
                for &index in &self.moved {
                    let p = &self.probes[index];
                    let tracks = flags[index] & TRACKS != 0;
                    if p.id != v.entity_id && sees(v, p, chunks, held) {
                        if !tracks {
                            flags[index] = SEES;
                            pairs[index] = true;
                        }
                    } else if tracks {
                        flags[index] = 0;
                        kept -= 1;
                    }
                }
            }
            let mut removed = Vec::new();
            if kept != v.tracker.seen.len() {
                let states = &self.states;
                v.tracker.seen.retain(|id| match states.get(id) {
                    Some(s) if s.seen == now && flags[s.index] & TRACKS != 0 => true,
                    _ => {
                        removed.push(*id);
                        false
                    }
                });
            }
            if !removed.is_empty() {
                v.out.push(
                    play::Clientbound::RemoveEntities(play::RemoveEntities { ids: removed })
                        .encode(),
                );
            }
        }
        for (index, e) in entities.iter().enumerate() {
            let Some(s) = self.states.get_mut(&e.id) else {
                continue;
            };
            self.packets.clear();
            if !s.added {
                s.send_changes(e, riding, &mut self.packets);
            }
            s.added = false;
            if self.packets.is_empty() && !self.pairs[index] {
                continue;
            }
            let mut encoded = false;
            let mut pairing: Option<Vec<Vec<u8>>> = None;
            for (v, flags) in viewers.iter_mut().zip(&self.flags) {
                let f = flags[index];
                if f & TRACKS != 0 {
                    if self.packets.is_empty() {
                        continue;
                    }
                    if !encoded {
                        encode_all(&self.packets, &mut self.bodies);
                        encoded = true;
                    }
                    v.out.extend(self.bodies.iter().cloned());
                } else if f & SEES != 0
                    && let Some(kind) = s.kind
                {
                    let bodies = pairing.get_or_insert_with(|| {
                        let mut packets = Vec::new();
                        s.pairing(e, kind, riding, &mut packets);
                        packets.iter().map(play::Clientbound::encode).collect()
                    });
                    v.out.extend(bodies.iter().cloned());
                    v.tracker.seen.insert(e.id);
                }
            }
        }
        self.states.retain(|_, s| s.seen == now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(x: f64) -> EntityState {
        EntityState {
            id: 7,
            uuid: 1,
            kind: "minecraft:zombie".into(),
            position: [x, 64.0, 0.0],
            yaw: 0.0,
            pitch: 0.0,
            head_yaw: 0.0,
            on_ground: true,
            velocity: [0.0; 3],
            data: 0,
            meta: Vec::new(),
            defaults: Vec::new(),
            equipment: Vec::new(),
            attributes: Vec::new(),
            precise: false,
            needs_sync: false,
            leash: None,
            spectator: false,
        }
    }

    #[test]
    fn states_are_written_over_last_ticks_in_place() {
        let mut states = States::default();
        let e = states.add(1, 1, "minecraft:zombie");
        e.position = [1.0, 2.0, 3.0];
        e.meta.push((9, serializer::FLOAT, Value::Float(20.0)));
        e.equipment.push((0, None));
        let (kind, meta) = (e.kind.as_ptr(), e.meta.as_ptr());
        states.push(state(2.0));
        states.clear();
        let e = states.add(3, 3, "minecraft:husk");
        let fresh = EntityState {
            id: 3,
            uuid: 3,
            kind: "minecraft:husk".into(),
            ..EntityState::default()
        };
        assert_eq!(*e, fresh);
        assert_eq!((e.kind.as_ptr(), e.meta.as_ptr()), (kind, meta));
        assert_eq!(states.as_slice(), std::slice::from_ref(&fresh));
        states.extend([state(5.0), state(6.0)]);
        assert_eq!(states.as_slice(), [fresh, state(5.0), state(6.0)]);
    }

    fn added(e: &EntityState) -> ServerEntity {
        ServerEntity::new(e, Some(100), &Riding::default())
    }
    fn changes(s: &mut ServerEntity, e: &EntityState) -> Vec<play::Clientbound> {
        let mut out = Vec::new();
        s.send_changes(e, &Riding::default(), &mut out);
        out
    }

    #[test]
    fn furnace_cart_sends_unpowered_metadata_when_fuel_runs_out() {
        let mut powered = state(0.0);
        powered.kind = "minecraft:furnace_minecart".into();
        powered
            .defaults
            .push((13, serializer::BOOLEAN, Value::Bool(false)));
        powered
            .meta
            .push((13, serializer::BOOLEAN, Value::Bool(true)));
        let mut tracker = added(&powered);
        let mut empty = powered.clone();
        empty.meta.clear();
        let mut packets = Vec::new();
        tracker.send_dirty(&empty, true, &mut packets);
        assert!(packets.iter().any(|packet| matches!(packet,
            play::Clientbound::SetEntityData(data)
                if data.values.contains(&(13, serializer::BOOLEAN, Value::Bool(false)))
        )));
    }

    #[test]
    fn moves_go_out_every_update_interval() {
        let mut s = added(&state(0.0));
        let mut spawn = Vec::new();
        s.pairing(&state(0.0), 100, &Riding::default(), &mut spawn);
        assert!(matches!(spawn[..], [play::Clientbound::AddEntity(_)]));
        assert!(matches!(
            changes(&mut s, &state(0.0))[..],
            [play::Clientbound::MoveEntityPos(_)]
        ));
        assert!(changes(&mut s, &state(0.5)).is_empty());
        assert!(changes(&mut s, &state(0.5)).is_empty());
        assert!(matches!(
            changes(&mut s, &state(0.5))[..],
            [play::Clientbound::MoveEntityPos(_)]
        ));
        for _ in 0..2 {
            changes(&mut s, &state(40.0));
        }
        let jump = changes(&mut s, &state(40.0));
        assert!(matches!(jump[0], play::Clientbound::EntityPositionSync(_)));
    }

    #[test]
    fn a_knocked_back_mob_is_sent_at_once() {
        let mut s = added(&state(0.0));
        changes(&mut s, &state(0.0));
        let mut knocked = state(0.3);
        knocked.velocity = [0.2, 0.3, 0.0];
        knocked.needs_sync = true;
        let out = changes(&mut s, &knocked);
        assert!(
            matches!(
                out[..],
                [
                    play::Clientbound::SetEntityMotion(_),
                    play::Clientbound::MoveEntityPos(_)
                ]
            ),
            "{out:?}"
        );
        assert!(changes(&mut s, &state(0.5)).is_empty());
        assert!(matches!(
            changes(&mut s, &state(0.6))[..],
            [
                play::Clientbound::SetEntityMotion(_),
                play::Clientbound::MoveEntityPos(_)
            ]
        ));
    }

    #[test]
    fn fireballs_bundle_their_motion_with_their_power() {
        let mut e = state(0.0);
        e.kind = "minecraft:fireball".into();
        e.on_ground = false;
        let mut s = added(&e);
        e.velocity = [0.0, 0.0, 0.5];
        let out = changes(&mut s, &e);
        assert!(matches!(
            out[..],
            [
                play::Clientbound::BundleDelimiter(_),
                play::Clientbound::SetEntityMotion(_),
                play::Clientbound::ProjectilePower(play::ProjectilePower { power, .. }),
                play::Clientbound::BundleDelimiter(_),
                play::Clientbound::MoveEntityPos(_),
            ] if power == 0.1
        ));
    }

    #[test]
    fn arrows_send_position_and_rotation_every_update() {
        let mut e = state(0.0);
        e.kind = "minecraft:arrow".into();
        let mut s = added(&e);
        assert!(matches!(
            changes(&mut s, &e)[..],
            [play::Clientbound::MoveEntityPosRot(_)]
        ));
    }

    #[test]
    fn precise_entities_sync_their_position() {
        let mut e = state(0.0);
        e.kind = "minecraft:tnt".into();
        e.precise = true;
        let mut s = added(&e);
        assert!(matches!(
            changes(&mut s, &e)[..],
            [play::Clientbound::EntityPositionSync(_)]
        ));
    }

    #[test]
    fn landing_sends_a_position_sync() {
        let mut e = state(0.0);
        let mut s = added(&e);
        changes(&mut s, &e);
        e.on_ground = false;
        changes(&mut s, &e);
        changes(&mut s, &e);
        assert!(matches!(
            changes(&mut s, &e)[..],
            [play::Clientbound::EntityPositionSync(_)]
        ));
    }

    #[test]
    fn attributes_pair_whole_then_send_the_changed_instances_when_due() {
        let speed = |base| AttributeSnapshot {
            attribute: 26,
            base,
            modifiers: Vec::new(),
        };
        let armor = AttributeSnapshot {
            attribute: 1,
            base: 0.0,
            modifiers: Vec::new(),
        };
        let mut e = state(0.0);
        e.attributes = vec![speed(0.23), armor];
        let mut s = added(&e);
        let mut spawn = Vec::new();
        s.pairing(&e, 100, &Riding::default(), &mut spawn);
        let Some(play::Clientbound::UpdateAttributes(p)) = spawn.get(1) else {
            panic!("expected the attributes after the add");
        };
        assert_eq!(p.values, e.attributes);
        changes(&mut s, &e);
        e.attributes[0] = speed(0.3);
        assert!(changes(&mut s, &e).is_empty());
        assert!(changes(&mut s, &e).is_empty());
        let out = changes(&mut s, &e);
        let Some(play::Clientbound::UpdateAttributes(p)) = out
            .iter()
            .find(|p| matches!(p, play::Clientbound::UpdateAttributes(_)))
        else {
            panic!("expected the changed attribute");
        };
        assert_eq!(p.values, vec![speed(0.3)]);
    }

    #[test]
    fn cleared_flags_and_pose_go_back_to_their_default() {
        let mut e = state(0.0);
        let mut s = added(&e);
        changes(&mut s, &e);
        e.meta = vec![
            (0, serializer::BYTE, Value::Byte(2)),
            (6, serializer::POSE, Value::Id(5)),
        ];
        changes(&mut s, &e);
        e.meta.clear();
        let data = changes(&mut s, &e)
            .into_iter()
            .find_map(|p| match p {
                play::Clientbound::SetEntityData(d) => Some(d),
                _ => None,
            })
            .expect("standing up is sent");
        assert_eq!(
            data.values,
            vec![
                (0, serializer::BYTE, Value::Byte(0)),
                (6, serializer::POSE, Value::Id(0)),
            ]
        );
        assert!(changes(&mut s, &e).is_empty());
    }

    #[test]
    fn an_emptied_item_frame_sends_its_empty_item_and_rotation() {
        let mut e = state(0.0);
        e.kind = "minecraft:item_frame".into();
        let mut s = added(&e);
        changes(&mut s, &e);
        e.meta = vec![
            (
                9,
                serializer::ITEM_STACK,
                Value::Item(Some(Stack::new(5, 1))),
            ),
            (10, serializer::INT, Value::Int(3)),
        ];
        changes(&mut s, &e);
        e.meta.clear();
        let data = changes(&mut s, &e)
            .into_iter()
            .find_map(|p| match p {
                play::Clientbound::SetEntityData(d) => Some(d),
                _ => None,
            })
            .expect("the empty frame is sent");
        assert_eq!(
            data.values,
            vec![
                (9, serializer::ITEM_STACK, Value::Item(None)),
                (10, serializer::INT, Value::Int(0)),
            ]
        );
    }

    #[test]
    fn a_value_back_at_a_listed_default_is_sent() {
        let mut e = state(0.0);
        e.defaults = vec![(16, serializer::BOOLEAN, Value::Bool(false))];
        e.meta = vec![(16, serializer::BOOLEAN, Value::Bool(true))];
        let mut s = added(&e);
        changes(&mut s, &e);
        e.meta.clear();
        let data = changes(&mut s, &e)
            .into_iter()
            .find_map(|p| match p {
                play::Clientbound::SetEntityData(d) => Some(d),
                _ => None,
            })
            .expect("growing up is sent");
        assert_eq!(
            data.values,
            vec![(16, serializer::BOOLEAN, Value::Bool(false))]
        );
        assert!(changes(&mut s, &e).is_empty());
    }

    #[test]
    fn metadata_and_equipment_send_only_the_changes() {
        let mut e = state(0.0);
        let mut s = added(&e);
        changes(&mut s, &e);
        e.meta.push((0, 0, Value::Byte(1)));
        e.equipment.push((0, Some(Stack::new(5, 1))));
        let out = changes(&mut s, &e);
        assert!(
            out.iter()
                .any(|p| matches!(p, play::Clientbound::SetEntityData(_)))
        );
        assert!(
            out.iter()
                .any(|p| matches!(p, play::Clientbound::SetEquipment(_)))
        );
        assert!(changes(&mut s, &e).is_empty());
        e.equipment.clear();
        let out = changes(&mut s, &e);
        let Some(play::Clientbound::SetEquipment(eq)) = out.first() else {
            panic!("expected equipment");
        };
        assert_eq!(eq.slots, vec![(0, None)]);
    }

    #[test]
    fn a_lead_tied_and_untied_reaches_the_clients() {
        let link = |out: &[play::Clientbound]| {
            out.iter().find_map(|p| match p {
                play::Clientbound::SetEntityLink(l) => Some((l.source, l.dest)),
                _ => None,
            })
        };
        let mut e = state(0.0);
        let mut s = added(&e);
        changes(&mut s, &e);
        e.leash = Some(7);
        assert_eq!(link(&changes(&mut s, &e)), Some((e.id, 7)));
        assert_eq!(link(&changes(&mut s, &e)), None);
        e.leash = None;
        assert_eq!(link(&changes(&mut s, &e)), Some((e.id, 0)));
    }

    struct Clients {
        chunks: crate::fast_hash::FxHashSet<(i32, i32)>,
        trackers: [Tracker; 2],
        out: [Vec<Vec<u8>>; 2],
        positions: [[f64; 3]; 2],
        moved: [bool; 2],
        spectators: [bool; 2],
    }

    impl Clients {
        fn new() -> Self {
            Self {
                chunks: (-4..4).flat_map(|x| (-4..4).map(move |z| (x, z))).collect(),
                trackers: Default::default(),
                out: Default::default(),
                positions: [[0.0, 64.0, 0.0]; 2],
                moved: [false; 2],
                spectators: [false; 2],
            }
        }
        fn tick(&mut self, level: &mut Entities, entities: &[&EntityState]) {
            for out in &mut self.out {
                out.clear();
            }
            let [t0, t1] = &mut self.trackers;
            let [o0, o1] = &mut self.out;
            let mut viewers = [
                Viewer {
                    entity_id: 1,
                    spectator: self.spectators[0],
                    moved: self.moved[0],
                    position: self.positions[0],
                    view_distance: 10,
                    chunks: &self.chunks,
                    tracker: t0,
                    out: o0,
                },
                Viewer {
                    entity_id: 2,
                    spectator: self.spectators[1],
                    moved: self.moved[1],
                    position: self.positions[1],
                    view_distance: 10,
                    chunks: &self.chunks,
                    tracker: t1,
                    out: o1,
                },
            ];
            level.tick(entities, &Riding::default(), &mut viewers, |_| Some(100));
        }
    }

    fn body(p: play::Clientbound) -> Vec<u8> {
        p.encode()
    }

    #[test]
    fn pairs_are_checked_again_when_a_side_moves() {
        let mut level = Entities::default();
        let mut clients = Clients::new();
        clients.positions[0] = [-108.0, 64.0, 0.0];
        let mut me = state(-108.0);
        (me.id, me.kind) = (1, "minecraft:player".into());
        let tracks = |clients: &Clients| clients.trackers[0].is_tracking(7);
        clients.tick(&mut level, &[&me, &state(20.5)]);
        assert!(!tracks(&clients));
        clients.tick(&mut level, &[&me, &state(19.5)]);
        assert!(!tracks(&clients));
        clients.moved[0] = true;
        clients.tick(&mut level, &[&me, &state(19.5)]);
        assert!(tracks(&clients));
        clients.moved[0] = false;
        clients.tick(&mut level, &[&me, &state(20.5)]);
        assert!(tracks(&clients));
        clients.tick(&mut level, &[&me, &state(40.0)]);
        assert!(!tracks(&clients));
        assert!(clients.trackers[1].is_tracking(7));
    }

    #[test]
    fn every_client_gets_the_same_changes_and_late_ones_are_paired() {
        let mut level = Entities::default();
        let mut c = Clients::new();
        c.positions[1] = [500.0, 64.0, 0.0];
        let mut e = state(0.0);
        c.tick(&mut level, &[&e]);
        assert!(c.trackers[0].is_tracking(7) && !c.trackers[1].is_tracking(7));
        assert!(c.out[1].is_empty());
        c.tick(&mut level, &[&e]);
        assert_eq!(c.out[0].len(), 1);
        c.positions[1] = [0.0, 64.0, 0.0];
        e.position[0] = 1.0;
        c.tick(&mut level, &[&e]);
        assert!(c.trackers[1].is_tracking(7));
        assert!(c.out[0].is_empty());
        assert_eq!(c.out[1].len(), 1);
        c.tick(&mut level, &[&e]);
        e.position[0] = 2.0;
        c.tick(&mut level, &[&e]);
        assert!(!c.out[0].is_empty());
        assert_eq!(c.out[0], c.out[1]);
    }

    #[test]
    fn clients_lose_what_leaves_range_or_the_level_and_never_track_themselves() {
        let mut level = Entities::default();
        let mut c = Clients::new();
        let mut me = state(0.0);
        me.id = 1;
        me.kind = "minecraft:player".into();
        let mut e = state(0.0);
        c.tick(&mut level, &[&me, &e]);
        assert!(!c.trackers[0].is_tracking(1) && c.trackers[1].is_tracking(1));
        assert!(c.trackers[0].is_tracking(7));
        e.position[0] = 200.0;
        c.tick(&mut level, &[&me, &e]);
        assert!(!c.trackers[0].is_tracking(7));
        let removed = body(play::Clientbound::RemoveEntities(play::RemoveEntities {
            ids: vec![7],
        }));
        assert_eq!(c.out[0].first(), Some(&removed));
        e.position[0] = 0.0;
        c.tick(&mut level, &[&me, &e]);
        assert!(c.trackers[0].is_tracking(7));
        c.tick(&mut level, &[&me]);
        assert!(!c.trackers[0].is_tracking(7) && !c.trackers[1].is_tracking(7));
        assert_eq!(c.out[0].first(), Some(&removed));
        assert_eq!(level.len(), 1);
    }

    #[test]
    fn entities_in_chunks_a_client_lacks_wait_for_them() {
        let mut level = Entities::default();
        let mut c = Clients::new();
        let mut far = state(70.0);
        let mut near = state(66.0);
        near.id = 8;
        let mut home = state(1.0);
        home.id = 9;
        c.tick(&mut level, &[&far, &near, &home]);
        for t in &c.trackers {
            assert!(!t.is_tracking(7) && !t.is_tracking(8) && t.is_tracking(9));
        }
        c.chunks.insert((4, 0));
        c.tick(&mut level, &[&far, &near, &home]);
        for t in &c.trackers {
            assert!(t.is_tracking(7) && t.is_tracking(8) && t.is_tracking(9));
        }
        far.position[0] = 200.0;
        near.position[0] = 60.0;
        c.tick(&mut level, &[&far, &near, &home]);
        for t in &c.trackers {
            assert!(!t.is_tracking(7) && t.is_tracking(8) && t.is_tracking(9));
        }
    }

    #[test]
    fn spectators_are_tracked_only_by_spectators() {
        let mut level = Entities::default();
        let mut c = Clients::new();
        let player = |id: i32| {
            let mut p = state(0.0);
            (p.id, p.uuid, p.kind) = (id, id as u128, "minecraft:player".into());
            p
        };
        let (mut one, mut two) = (player(1), player(2));
        let mut three = player(3);
        three.spectator = true;
        c.tick(&mut level, &[&one, &two, &three]);
        c.tick(&mut level, &[&one, &two, &three]);
        assert!(c.trackers[0].is_tracking(2) && c.trackers[1].is_tracking(1));
        assert!(!c.trackers[0].is_tracking(3) && !c.trackers[1].is_tracking(3));
        (two.spectator, c.spectators[1]) = (true, true);
        c.tick(&mut level, &[&one, &two, &three]);
        assert!(!c.trackers[0].is_tracking(2));
        let removed = body(play::Clientbound::RemoveEntities(play::RemoveEntities {
            ids: vec![2],
        }));
        assert_eq!(c.out[0].first(), Some(&removed));
        assert!(c.trackers[1].is_tracking(1) && c.trackers[1].is_tracking(3));
        two.equipment.push((5, Some(Stack::new(5, 1))));
        c.tick(&mut level, &[&one, &two, &three]);
        assert!(c.out[0].is_empty());
        (one.spectator, c.spectators[0]) = (true, true);
        c.tick(&mut level, &[&one, &two, &three]);
        assert!(c.trackers[0].is_tracking(2) && c.trackers[0].is_tracking(3));
        (one.spectator, c.spectators[0]) = (false, false);
        (two.spectator, c.spectators[1]) = (false, false);
        c.tick(&mut level, &[&one, &two, &three]);
        assert!(c.trackers[0].is_tracking(2) && c.trackers[1].is_tracking(1));
        assert!(!c.trackers[0].is_tracking(3) && !c.trackers[1].is_tracking(3));
    }

    #[test]
    fn a_forgotten_entity_is_paired_again_at_the_next_tick() {
        let mut level = Entities::default();
        let mut c = Clients::new();
        let mut player = state(0.0);
        player.id = 1;
        player.kind = "minecraft:player".into();
        c.tick(&mut level, &[&player]);
        c.tick(&mut level, &[&player]);
        assert!(c.trackers[1].forget(1));
        assert!(!c.trackers[1].forget(1));
        c.tick(&mut level, &[&player]);
        assert!(c.trackers[1].is_tracking(1));
        let paired = c.out[1]
            .first()
            .and_then(|p| play::Clientbound::decode(i32::from(p[0]), &p[1..]).ok());
        assert!(
            matches!(paired, Some(play::Clientbound::AddEntity(ref a)) if a.id == 1),
            "{paired:?}"
        );
    }
}
