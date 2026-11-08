use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct DecorationOrder {
    rank: HashMap<(i32, i32), u64>,
    next: u64,
}

impl DecorationOrder {
    pub fn rank(&self, x: i32, z: i32) -> Option<u64> {
        self.rank.get(&(x, z)).copied()
    }

    pub fn reassign(
        &mut self,
        center: (i32, i32),
        radius: i32,
        decorated: impl Fn(i32, i32) -> bool,
    ) {
        for (x, z) in vanilla_order(center, radius) {
            if !decorated(x, z) {
                self.rank.insert((x, z), self.next);
                self.next += 1;
            }
        }
    }

    pub fn rank_or_append(&mut self, x: i32, z: i32) -> u64 {
        let next = &mut self.next;
        *self.rank.entry((x, z)).or_insert_with(|| {
            *next += 1;
            *next - 1
        })
    }

    pub fn blocked(&self, x: i32, z: i32, pending: impl Fn(i32, i32) -> bool) -> bool {
        let Some(own) = self.rank(x, z) else {
            return false;
        };
        (-2..=2).any(|dz| {
            (-2..=2).any(|dx| {
                (dx, dz) != (0, 0)
                    && self.rank(x + dx, z + dz).is_some_and(|r| r < own)
                    && pending(x + dx, z + dz)
            })
        })
    }

    pub fn retain(&mut self, mut keep: impl FnMut(i32, i32) -> bool) {
        self.rank.retain(|&(x, z), _| keep(x, z));
    }

    pub fn ordered(&self) -> Vec<(i32, i32)> {
        let mut all: Vec<_> = self.rank.iter().map(|(&k, &r)| (r, k)).collect();
        all.sort_unstable();
        all.into_iter().map(|(_, k)| k).collect()
    }
}

const TICKETS_IN_FLIGHT: usize = 4;
const TICKET_SET_CAPACITY: u64 = 8192;
const TICKET_LEVEL: i32 = 31;
const FULL_LEVEL: i32 = 33;
const LIGHT_LEVEL: i32 = 34;

fn ticket_hash(x: i32, z: i32) -> u64 {
    let packed = u64::from(x as u32) | (u64::from(z as u32) << 32);
    let h = packed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let h = h ^ (h >> 32);
    h ^ (h >> 16)
}

pub fn vanilla_order(center: (i32, i32), radius: i32) -> Vec<(i32, i32)> {
    use std::cmp::Reverse;
    let mut tickets: Vec<(i32, i32)> = (-radius..=radius)
        .flat_map(|dx| (-radius..=radius).map(move |dz| (center.0 + dx, center.1 + dz)))
        .collect();
    tickets.sort_by_key(|&(x, z)| {
        (
            (x - center.0).abs().max((z - center.1).abs()),
            Reverse(ticket_hash(x, z) & (TICKET_SET_CAPACITY - 1)),
        )
    });
    let half = radius.max(0) + LIGHT_LEVEL - TICKET_LEVEL + 1;
    ticket_order(
        tickets.into(),
        TICKETS_IN_FLIGHT,
        (center.0 - half, center.1 - half),
        (2 * half + 1) as usize,
    )
}

fn ticket_order(
    mut tickets: VecDeque<(i32, i32)>,
    in_flight: usize,
    origin: (i32, i32),
    side: usize,
) -> Vec<(i32, i32)> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let reach = LIGHT_LEVEL - TICKET_LEVEL;
    let cell = |(x, z): (i32, i32)| ((x - origin.0) as usize) * side + (z - origin.1) as usize;
    let mut active: Vec<(i32, i32)> = Vec::new();
    const NONE: (i32, u64) = (i32::MAX, 0);
    let mut queued = vec![NONE; side * side];
    let mut heap = BinaryHeap::new();
    let mut sequence = 0u64;
    let mut full = vec![false; side * side];
    let mut seen = vec![false; side * side];
    let mut order = Vec::new();
    let mut levels = vec![i32::MAX; side * side];
    let mut add = |ticket: (i32, i32),
                   active: &mut Vec<(i32, i32)>,
                   queued: &mut [(i32, u64)],
                   heap: &mut BinaryHeap<Reverse<(i32, u64, (i32, i32))>>,
                   full: &[bool]| {
        active.push(ticket);
        for x in ticket.0 - reach..=ticket.0 + reach {
            for z in ticket.1 - reach..=ticket.1 + reach {
                let own = TICKET_LEVEL + (x - ticket.0).abs().max((z - ticket.1).abs());
                let i = cell((x, z));
                levels[i] = levels[i].min(own);
                let l = levels[i];
                if full[i] {
                    continue;
                }
                if l < queued[i].0 {
                    sequence += 1;
                    queued[i] = (l, sequence);
                    heap.push(Reverse((l, sequence, (x, z))));
                }
            }
        }
    };
    while active.len() < in_flight {
        let Some(t) = tickets.pop_front() else { break };
        add(t, &mut active, &mut queued, &mut heap, &full);
    }
    while let Some(Reverse((l, s, chunk))) = heap.pop() {
        let i = cell(chunk);
        if queued[i] != (l, s) {
            continue;
        }
        queued[i] = NONE;
        let r = i32::from(l <= FULL_LEVEL);
        for x in chunk.0 - r..=chunk.0 + r {
            for z in chunk.1 - r..=chunk.1 + r {
                let seen = &mut seen[cell((x, z))];
                if !*seen {
                    *seen = true;
                    order.push((x, z));
                }
            }
        }
        if l <= FULL_LEVEL {
            full[i] = true;
        }
        active.retain(|t| !(-2..=2).all(|dx| (-2..=2).all(|dz| full[cell((t.0 + dx, t.1 + dz))])));
        while active.len() < in_flight {
            let Some(t) = tickets.pop_front() else { break };
            add(t, &mut active, &mut queued, &mut heap, &full);
        }
    }
    order
}

pub fn forced_order(x0: i32, z0: i32, x1: i32, z1: i32) -> Vec<(i32, i32)> {
    let tickets = (x0..=x1)
        .flat_map(|x| (z0..=z1).map(move |z| (x, z)))
        .collect();
    let pad = LIGHT_LEVEL - TICKET_LEVEL + 1;
    let side = ((x1 - x0).max(z1 - z0) + 2 * pad + 1) as usize;
    ticket_order(tickets, usize::MAX, (x0 - pad, z0 - pad), side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_chunks_decorate_x_outer_z_inner() {
        let rank = |order: &[(i32, i32)], c| order.iter().position(|&o| o == c).unwrap();
        let west = forced_order(-32, -16, -17, -1);
        assert!(rank(&west, (-26, -1)) < rank(&west, (-26, 0)));
        let east = forced_order(0, -32, 15, -17);
        assert!(rank(&east, (14, -20)) < rank(&east, (14, -19)));
        assert_eq!(west[0], (-33, -17));
        assert_eq!(west.len(), 22 * 22);
    }

    fn hashed_order(center: (i32, i32), radius: i32) -> Vec<(i32, i32)> {
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashSet, VecDeque};
        let mut tickets: Vec<(i32, i32)> = (-radius..=radius)
            .flat_map(|dx| (-radius..=radius).map(move |dz| (center.0 + dx, center.1 + dz)))
            .collect();
        tickets.sort_by_key(|&(x, z)| {
            (
                (x - center.0).abs().max((z - center.1).abs()),
                Reverse(ticket_hash(x, z) & (TICKET_SET_CAPACITY - 1)),
            )
        });
        let mut tickets: VecDeque<_> = tickets.into();
        let mut active: Vec<(i32, i32)> = Vec::new();
        let mut queued: HashMap<(i32, i32), (i32, u64)> = HashMap::new();
        let mut heap = BinaryHeap::new();
        let mut sequence = 0u64;
        let mut full = HashSet::new();
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut levels: HashMap<(i32, i32), i32> = HashMap::new();
        let mut add = |ticket: (i32, i32),
                       active: &mut Vec<(i32, i32)>,
                       queued: &mut HashMap<(i32, i32), (i32, u64)>,
                       heap: &mut BinaryHeap<Reverse<(i32, u64, (i32, i32))>>,
                       full: &HashSet<(i32, i32)>| {
            active.push(ticket);
            let reach = LIGHT_LEVEL - TICKET_LEVEL;
            for x in ticket.0 - reach..=ticket.0 + reach {
                for z in ticket.1 - reach..=ticket.1 + reach {
                    let own = TICKET_LEVEL + (x - ticket.0).abs().max((z - ticket.1).abs());
                    let l = levels.entry((x, z)).or_insert(i32::MAX);
                    *l = (*l).min(own);
                    let l = *l;
                    if full.contains(&(x, z)) {
                        continue;
                    }
                    if queued.get(&(x, z)).is_none_or(|&(old, _)| l < old) {
                        sequence += 1;
                        queued.insert((x, z), (l, sequence));
                        heap.push(Reverse((l, sequence, (x, z))));
                    }
                }
            }
        };
        for _ in 0..TICKETS_IN_FLIGHT {
            if let Some(t) = tickets.pop_front() {
                add(t, &mut active, &mut queued, &mut heap, &full);
            }
        }
        while let Some(Reverse((l, s, chunk))) = heap.pop() {
            if queued.get(&chunk) != Some(&(l, s)) {
                continue;
            }
            queued.remove(&chunk);
            let r = i32::from(l <= FULL_LEVEL);
            for x in chunk.0 - r..=chunk.0 + r {
                for z in chunk.1 - r..=chunk.1 + r {
                    if seen.insert((x, z)) {
                        order.push((x, z));
                    }
                }
            }
            if l <= FULL_LEVEL {
                full.insert(chunk);
            }
            active.retain(|t| {
                !(-2..=2).all(|dx| (-2..=2).all(|dz| full.contains(&(t.0 + dx, t.1 + dz))))
            });
            while active.len() < TICKETS_IN_FLIGHT {
                let Some(t) = tickets.pop_front() else { break };
                add(t, &mut active, &mut queued, &mut heap, &full);
            }
        }
        order
    }

    #[test]
    fn dense_grids_keep_the_hashed_order() {
        for (center, radius) in [
            ((0, 0), -1),
            ((0, 0), 0),
            ((0, 0), 1),
            ((12, -15), 8),
            ((-30_000, 29_999), 5),
            ((7, 3), 12),
            ((-61, 1234), 32),
        ] {
            assert_eq!(
                vanilla_order(center, radius),
                hashed_order(center, radius),
                "{center:?} radius {radius}"
            );
        }
    }

    #[test]
    fn first_wave_matches_vanilla() {
        let order = vanilla_order((12, -15), 8);
        let rel: Vec<_> = order
            .iter()
            .take(18)
            .map(|&(x, z)| (x - 12, z + 15))
            .collect();
        assert_eq!(
            rel,
            [
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 0),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1),
                (-1, 2),
                (0, 2),
                (1, 2),
                (-2, -1),
                (-2, 0),
                (-2, 1),
                (2, 0),
                (2, 1),
                (2, 2),
            ]
        );
        let unique: std::collections::HashSet<_> = order.iter().collect();
        assert_eq!(unique.len(), order.len());
        assert_eq!(order.len(), 23 * 23);
    }

    #[test]
    fn blocked_by_lower_ranks_within_two() {
        let mut order = DecorationOrder::default();
        order.reassign((0, 0), 2, |_, _| false);
        let first = order.ordered()[0];
        assert!(!order.blocked(first.0, first.1, |_, _| true));
        let last = *order.ordered().last().unwrap();
        assert!(order.blocked(last.0, last.1, |_, _| true));
        assert!(!order.blocked(last.0, last.1, |_, _| false));
    }

    #[test]
    fn decorated_chunks_keep_their_place() {
        let mut order = DecorationOrder::default();
        order.reassign((0, 0), 1, |_, _| false);
        let before = order.rank(0, 0);
        order.reassign((10, 0), 1, |x, _| x < 5);
        assert_eq!(order.rank(0, 0), before);
        assert!(order.rank(10, 0) > order.rank(2, 2));
    }
}
