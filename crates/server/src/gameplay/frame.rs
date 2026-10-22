//! One tick's shared, read-only unit data: every unit's wire state computed
//! once, an id index, and each viewer's own units. Sessions then gather their
//! view from this instead of querying the world once per session.
use super::economy::wire_unit;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{GameplayUnitState, MAX_SUBSCRIBED_UNITS};
use aoe_simulation::{GameWorld, TileVisibility};
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;

/// Ids are dense small integers, so a flat table beats a map; ids beyond the
/// dense bound (a very long game) fall back to a hash map.
const DENSE_IDS: usize = 1 << 24;
const ABSENT: u32 = u32::MAX;

struct IdTable<T: Copy> {
    dense: Vec<T>,
    sparse: HashMap<u32, T>,
    empty: T,
}

impl<T: Copy> IdTable<T> {
    fn new(bound: usize, empty: T) -> Self {
        let dense = if bound <= DENSE_IDS {
            vec![empty; bound]
        } else {
            Vec::new()
        };
        Self {
            dense,
            sparse: HashMap::new(),
            empty,
        }
    }

    fn set(&mut self, id: EntityId, value: T) {
        match self.dense.get_mut(id.0 as usize) {
            Some(slot) => *slot = value,
            None => {
                self.sparse.insert(id.0, value);
            }
        }
    }

    fn get(&self, id: EntityId) -> T {
        match self.dense.get(id.0 as usize) {
            Some(value) => *value,
            None => self.sparse.get(&id.0).copied().unwrap_or(self.empty),
        }
    }
}

fn resource_code(kind: aoe_map::ResourceKind) -> u8 {
    match kind {
        aoe_map::ResourceKind::Food => 1,
        aoe_map::ResourceKind::Wood => 2,
        aoe_map::ResourceKind::Gold => 3,
        aoe_map::ResourceKind::Stone => 4,
    }
}

fn resource_of(code: u8) -> Option<aoe_map::ResourceKind> {
    match code {
        1 => Some(aoe_map::ResourceKind::Food),
        2 => Some(aoe_map::ResourceKind::Wood),
        3 => Some(aoe_map::ResourceKind::Gold),
        4 => Some(aoe_map::ResourceKind::Stone),
        _ => None,
    }
}

/// Units are converted in fixed-size parts, one worker each; a unit's slot is
/// `part * PART + position in part`, which is also its storage index.
const PART: usize = 1 << 14;

struct Part {
    states: Vec<GameplayUnitState>,
    /// `(viewer list, slot)` for the units a requested viewer owns.
    owned: Vec<(u32, u32)>,
}

pub(super) struct UnitFrame {
    parts: Vec<Vec<GameplayUnitState>>,
    /// Owner of each slot, to tell a viewer's own units without loading them.
    owner: Vec<u16>,
    index: IdTable<u32>,
    /// Per viewer, the first `MAX_SUBSCRIBED_UNITS + 1` units it owns (in
    /// storage order, as the per-session scan took them) as `id << 32 | slot`,
    /// sorted.
    own: BTreeMap<PlayerId, Vec<u64>>,
}

/// What one session looks through.
pub(super) struct Lens {
    pub viewer: PlayerId,
    pub primary: EntityId,
    pub region: TileRect,
}

/// Scratch space a worker reuses across sessions.
#[derive(Default)]
pub(super) struct Scratch {
    ids: Vec<EntityId>,
    keys: Vec<u64>,
}

fn key(id: EntityId, slot: u32) -> u64 {
    u64::from(id.0) << 32 | u64::from(slot)
}

impl UnitFrame {
    /// `None` when a worker failed, so no half-built frame is ever used.
    pub(super) fn build(world: &GameWorld, viewers: &[PlayerId], threads: usize) -> Option<Self> {
        let bound = world.entity_id_bound();
        let mut gather = IdTable::new(bound, 0u8);
        let mut hunt = IdTable::new(bound, ABSENT);
        for (id, kind, animal) in world.gatherers() {
            gather.set(id, resource_code(kind));
            if let Some(animal) = animal {
                hunt.set(id, animal.0);
            }
        }
        // A hunt order outranks the carcass a gatherer works.
        for (id, animal) in world.hunts() {
            hunt.set(id, animal.0);
        }
        // `viewers` are distinct; their lists are found by player id.
        let mut list_of = vec![
            ABSENT;
            viewers
                .iter()
                .map(|p| usize::from(p.0) + 1)
                .max()
                .unwrap_or(0)
        ];
        for (at, viewer) in viewers.iter().enumerate() {
            list_of[usize::from(viewer.0)] = at as u32;
        }
        let count = world.unit_count();
        let part = |at: usize| -> Part {
            let range: Range<usize> = at * PART..((at + 1) * PART).min(count);
            let mut states = Vec::with_capacity(range.len());
            let mut owned = Vec::new();
            for (n, unit) in world.units_range(range.clone()).enumerate() {
                let target = hunt.get(unit.id);
                states.push(wire_unit(
                    &unit,
                    resource_of(gather.get(unit.id)),
                    (target != ABSENT).then_some(EntityId(target)),
                ));
                if let Some(list) = list_of
                    .get(usize::from(unit.player.0))
                    .filter(|list| **list != ABSENT)
                {
                    owned.push((*list, (range.start + n) as u32));
                }
            }
            Part { states, owned }
        };
        let wanted = count.div_ceil(PART);
        let parts = if threads <= 1 || wanted <= 1 {
            (0..wanted).map(part).collect::<Vec<_>>()
        } else {
            std::thread::scope(|scope| {
                let handles = (0..wanted)
                    .map(|at| scope.spawn(move || part(at)))
                    .collect::<Vec<_>>();
                handles
                    .into_iter()
                    .map(|handle| handle.join().ok())
                    .collect::<Option<Vec<_>>>()
            })?
        };
        let mut index = IdTable::new(bound, ABSENT);
        let mut owner = Vec::with_capacity(count);
        let mut lists = vec![Vec::new(); viewers.len()];
        for (at, part) in parts.iter().enumerate() {
            for (n, state) in part.states.iter().enumerate() {
                index.set(state.id, (at * PART + n) as u32);
                owner.push(state.player.0);
            }
            for (list, slot) in &part.owned {
                let list = &mut lists[*list as usize];
                if list.len() <= MAX_SUBSCRIBED_UNITS {
                    list.push(*slot);
                }
            }
        }
        let parts = parts
            .into_iter()
            .map(|part| part.states)
            .collect::<Vec<_>>();
        let own = viewers
            .iter()
            .zip(lists)
            .map(|(viewer, slots)| {
                let mut keys = slots
                    .into_iter()
                    .map(|slot| key(parts[slot as usize / PART][slot as usize % PART].id, slot))
                    .collect::<Vec<_>>();
                keys.sort_unstable();
                (*viewer, keys)
            })
            .collect();
        Some(Self {
            parts,
            owner,
            index,
            own,
        })
    }

    pub(super) fn state(&self, slot: u32) -> &GameplayUnitState {
        &self.parts[slot as usize / PART][slot as usize % PART]
    }

    /// The units a session sees this tick, as slots sorted by id: whatever
    /// stands in its region and is visible to its viewer, plus the viewer's
    /// own economy (or just the primary unit of a population-only world).
    pub(super) fn current(
        &self,
        world: &GameWorld,
        lens: &Lens,
        scratch: &mut Scratch,
    ) -> Vec<u32> {
        let visible = |state: &GameplayUnitState| {
            state.player == lens.viewer
                || world.visibility(lens.viewer, state.position.tile_floor())
                    == TileVisibility::Visible
        };
        let economy = world.player_economy(lens.viewer).is_some();
        let own = self
            .own
            .get(&lens.viewer)
            .filter(|_| economy)
            .map_or(&[][..], |own| own.as_slice());
        // A complete list already holds every unit the viewer owns.
        let own_complete = economy && own.len() <= MAX_SUBSCRIBED_UNITS;
        let Scratch { ids, keys } = scratch;
        ids.clear();
        keys.clear();
        world.collect_chunk_unit_ids(lens.region, ids);
        for id in ids.iter() {
            let slot = self.index.get(*id);
            if slot == ABSENT || (own_complete && self.owner[slot as usize] == lens.viewer.0) {
                continue;
            }
            let state = self.state(slot);
            if lens.region.contains(state.position.tile_floor()) && visible(state) {
                keys.push(key(state.id, slot));
            }
        }
        if !economy {
            let slot = self.index.get(lens.primary);
            if slot != ABSENT && visible(self.state(slot)) {
                keys.push(key(lens.primary, slot));
            }
        }
        keys.sort_unstable();
        // The primary unit may already be in the region scan.
        keys.dedup();
        let mut slots = Vec::with_capacity(keys.len() + own.len());
        let (mut a, mut b) = (keys.iter().peekable(), own.iter().peekable());
        loop {
            let next = match (a.peek(), b.peek()) {
                (Some(x), Some(y)) if x <= y => {
                    if x == y {
                        b.next();
                    }
                    a.next()
                }
                (Some(_), Some(_)) | (None, Some(_)) => b.next(),
                (Some(_), None) => a.next(),
                (None, None) => break,
            };
            if let Some(key) = next {
                slots.push(*key as u32);
            }
        }
        slots
    }
}
