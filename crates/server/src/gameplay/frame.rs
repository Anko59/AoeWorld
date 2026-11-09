//! Retained shared unit states and owner lists. After the initial build only
//! the simulation's changed ids are read; idle units and lists stay in place.
use aoe_core::{EntityId, PlayerId, Tick, TileRect};
use aoe_protocol::{GameplayUnitState, MAX_SUBSCRIBED_UNITS};
use aoe_simulation::GameWorld;
use rayon::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap};

const ABSENT: u32 = u32::MAX;
const PART: usize = 1 << 12;

#[derive(Default)]
struct Owned {
    selected: BTreeMap<usize, u64>,
    overflow: BTreeMap<usize, u64>,
    keys: BTreeSet<u64>,
}
impl Owned {
    fn insert(&mut self, rank: usize, key: u64) {
        if self.selected.len() > MAX_SUBSCRIBED_UNITS
            && let Some((&last, _)) = self.selected.last_key_value()
        {
            if rank > last {
                self.overflow.insert(rank, key);
                return;
            }
            if let Some((rank, key)) = self.selected.pop_last() {
                self.keys.remove(&key);
                self.overflow.insert(rank, key);
            }
        }
        self.selected.insert(rank, key);
        self.keys.insert(key);
    }
    fn remove(&mut self, rank: usize) {
        if let Some(key) = self.selected.remove(&rank) {
            self.keys.remove(&key);
            if let Some((rank, key)) = self.overflow.pop_first() {
                self.selected.insert(rank, key);
                self.keys.insert(key);
            }
        } else {
            self.overflow.remove(&rank);
        }
    }
}

pub(super) struct UnitFrame {
    parts: Vec<Vec<GameplayUnitState>>,
    owner: Vec<u16>,
    rank: Vec<usize>,
    index: HashMap<EntityId, u32>,
    own: BTreeMap<PlayerId, Owned>,
    free: Vec<u32>,
    tick: Tick,
    generation: u64,
    base: Option<u64>,
    changed: BTreeSet<u32>,
    visited: usize,
    converted: usize,
}

pub(super) struct Lens {
    pub viewer: PlayerId,
    pub primary: EntityId,
    pub region: TileRect,
}
#[derive(Default)]
pub(super) struct Scratch {
    ids: Vec<EntityId>,
    keys: Vec<u64>,
}
fn key(id: EntityId, slot: u32) -> u64 {
    u64::from(id.0) << 32 | u64::from(slot)
}

impl UnitFrame {
    /// Ownership transfers between ticks so retaining an idle frame does not
    /// copy its states or indexes. A missed tick rebuilds from authoritative
    /// state; the single-tick journal cannot cover a gap.
    pub(super) fn build(
        world: &GameWorld,
        threads: usize,
        generation: u64,
        previous: Option<Self>,
    ) -> Option<Self> {
        let Some(mut frame) = previous.filter(|p| p.tick.0 + 1 == world.tick().0) else {
            return Self::initial(world, threads, generation);
        };
        frame.base = Some(frame.generation);
        frame.generation = generation;
        frame.tick = world.tick();
        frame.changed.clear();
        frame.visited = world.unit_changes().len();
        frame.converted = 0;
        // Remove old list entries first: swap-removal may assign their ranks
        // to another changed id in this same batch.
        for id in world.unit_changes() {
            if let Some(&slot) = frame.index.get(id) {
                let owner = PlayerId(frame.owner[slot as usize]);
                if let Some(own) = frame.own.get_mut(&owner) {
                    own.remove(frame.rank[slot as usize]);
                }
                if world.unit(*id).is_none() {
                    frame.index.remove(id);
                    frame.free.push(slot);
                }
            }
        }
        for id in world.unit_changes() {
            let Some(unit) = world.unit(*id) else {
                continue;
            };
            let state = super::economy::unit_state(world, &unit);
            frame.converted += 1;
            let slot = if let Some(&slot) = frame.index.get(id) {
                if *frame.state(slot) != state {
                    frame.changed.insert(slot);
                }
                frame.parts[slot as usize / PART][slot as usize % PART] = state;
                slot
            } else {
                let slot = frame.insert(state);
                frame.changed.insert(slot);
                slot
            };
            let rank = world.unit_storage_slot(*id)?;
            frame.owner[slot as usize] = unit.player.0;
            frame.rank[slot as usize] = rank;
            if world.unit_visible_to(unit.player, *id) {
                frame
                    .own
                    .entry(unit.player)
                    .or_default()
                    .insert(rank, key(*id, slot));
            }
        }
        Some(frame)
    }

    fn initial(world: &GameWorld, threads: usize, generation: u64) -> Option<Self> {
        let count = world.unit_count();
        let wanted = count.div_ceil(PART);
        let part = |at: usize| {
            world
                .units_range(at * PART..((at + 1) * PART).min(count))
                .map(|u| super::economy::unit_state(world, &u))
                .collect::<Vec<_>>()
        };
        let parts = if threads <= 1 || wanted <= 1 {
            (0..wanted).map(part).collect()
        } else {
            (0..wanted)
                .into_par_iter()
                .with_min_len(wanted.div_ceil(threads))
                .map(|at| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| part(at))).ok())
                .collect::<Option<Vec<_>>>()?
        };
        let mut frame = Self {
            parts,
            owner: Vec::with_capacity(count),
            rank: (0..count).collect(),
            index: HashMap::with_capacity(count),
            own: BTreeMap::new(),
            free: Vec::new(),
            tick: world.tick(),
            generation,
            base: None,
            changed: (0..count as u32).collect(),
            visited: count,
            converted: count,
        };
        for (slot, state) in frame.parts.iter().flatten().enumerate() {
            frame.index.insert(state.id, slot as u32);
            frame.owner.push(state.player.0);
            if world.unit_visible_to(state.player, state.id) {
                frame
                    .own
                    .entry(state.player)
                    .or_default()
                    .insert(slot, key(state.id, slot as u32));
            }
        }
        Some(frame)
    }

    fn insert(&mut self, state: GameplayUnitState) -> u32 {
        let slot = if let Some(slot) = self.free.pop() {
            self.parts[slot as usize / PART][slot as usize % PART] = state;
            slot
        } else {
            let slot = self.owner.len() as u32;
            if (slot as usize).is_multiple_of(PART) {
                self.parts.push(Vec::new());
            }
            self.parts[slot as usize / PART].push(state);
            self.owner.push(state.player.0);
            self.rank.push(0);
            slot
        };
        self.index.insert(state.id, slot);
        slot
    }

    pub(super) fn visited(&self) -> usize {
        self.visited
    }
    pub(super) fn work(&self) -> (usize, usize) {
        (self.converted, self.changed.len())
    }
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }
    pub(super) fn unchanged_since(&self, held: Option<u64>, slot: u32) -> bool {
        held.is_some() && held == self.base && !self.changed.contains(&slot)
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
        let visible = |state: &GameplayUnitState| world.unit_visible_to(lens.viewer, state.id);
        let economy = world.player_economy(lens.viewer).is_some();
        let own = self
            .own
            .get(&lens.viewer)
            .filter(|_| economy)
            .map(|own| &own.keys);
        let own_len = own.map_or(0, BTreeSet::len);
        // A complete list already holds every unit the viewer owns.
        let own_complete = economy && own_len <= MAX_SUBSCRIBED_UNITS;
        let Scratch { ids, keys } = scratch;
        ids.clear();
        keys.clear();
        world.collect_chunk_unit_ids(lens.region, ids);
        for id in ids.iter() {
            let slot = self.index.get(id).copied().unwrap_or(ABSENT);
            if slot == ABSENT || (own_complete && self.owner[slot as usize] == lens.viewer.0) {
                continue;
            }
            let state = self.state(slot);
            if lens.region.contains(state.position.tile_floor()) && visible(state) {
                keys.push(key(state.id, slot));
            }
        }
        if !economy {
            let slot = self.index.get(&lens.primary).copied().unwrap_or(ABSENT);
            if slot != ABSENT && visible(self.state(slot)) {
                keys.push(key(lens.primary, slot));
            }
        }
        keys.sort_unstable();
        // The primary unit may already be in the region scan.
        keys.dedup();
        let mut slots = Vec::with_capacity(keys.len() + own_len);
        let (mut a, mut b) = (keys.iter().peekable(), own.into_iter().flatten().peekable());
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

#[path = "frame/tests.rs"]
#[cfg(test)]
mod tests;
