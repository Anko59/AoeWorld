//! One tick's shared, read-only unit data: every unit's wire state, an id
//! index, and each viewer's own units. Sessions then gather their view from
//! this instead of querying the world once per session. A unit whose inputs
//! equal last tick's keeps last tick's state without a conversion, and is
//! marked unchanged so sessions holding that frame skip it in their diffs.
use super::economy::wire_unit;
use aoe_core::{EntityId, PlayerId, TileRect};
use aoe_protocol::{GameplayUnitState, MAX_SUBSCRIBED_UNITS};
use aoe_simulation::{GameUnit, GameWorld};
use rayon::prelude::*;
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

fn convert(world: &GameWorld, input: &Input) -> GameplayUnitState {
    let unit = &input.unit;
    let mut state = wire_unit(
        unit,
        resource_of(input.gather),
        (input.hunt != ABSENT).then_some(EntityId(input.hunt)),
    );
    state.cargo_count = input.cargo;
    if input.commanded {
        super::economy::command_state(world, unit, &mut state);
    }
    state
}

/// Units are converted in fixed-size parts on shared workers; a unit's slot is
/// `part * PART + position in part`, which is also its storage index.
const PART: usize = 1 << 12;

/// Everything a unit's wire state is computed from. A unit no command report
/// can name is a pure function of these, so equal inputs give an equal state.
#[derive(Clone, Copy, PartialEq)]
struct Input {
    unit: GameUnit,
    gather: u8,
    hunt: u32,
    cargo: u8,
    /// A General's army, controller or Steward may be reported for it: its
    /// state reads other entities, so it is converted every tick.
    commanded: bool,
}

struct Part {
    converted: usize,
    states: Vec<GameplayUnitState>,
    inputs: Vec<Input>,
    changed: Vec<bool>,
    /// `(viewer list, slot)` for the units a requested viewer owns.
    owned: Vec<(u32, u32)>,
}

pub(super) struct UnitFrame {
    parts: Vec<Vec<GameplayUnitState>>,
    inputs: Vec<Vec<Input>>,
    /// Owner of each slot, to tell a viewer's own units without loading them.
    owner: Vec<u16>,
    index: IdTable<u32>,
    /// Per viewer, the first `MAX_SUBSCRIBED_UNITS + 1` units it owns (in
    /// storage order, as the per-session scan took them) as `id << 32 | slot`,
    /// sorted.
    own: BTreeMap<PlayerId, Vec<u64>>,
    /// This frame's number: fan-outs count up, never reusing one.
    generation: u64,
    /// The frame `changed` compares against, when there was one.
    base: Option<u64>,
    /// Per slot: the unit's state differs from `base`'s, or it is new there.
    changed: Vec<bool>,
    /// Units converted afresh (the rest kept last frame's state).
    converted: usize,
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
    #[cfg(test)]
    pub(super) fn part_count(&self) -> usize {
        self.parts.len()
    }

    /// `None` when a worker failed, so no half-built frame is ever used.
    /// `previous` is the last tick's frame: every slot records whether its
    /// unit's state differs from there, so sessions that hold that frame skip
    /// the units that did not change.
    pub(super) fn build(
        world: &GameWorld,
        viewers: &[PlayerId],
        threads: usize,
        generation: u64,
        previous: Option<&Self>,
    ) -> Option<Self> {
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
        let mut cargo = IdTable::new(bound, 0u8);
        for (id, count) in world.loaded_transports() {
            cargo.set(id, count as u8);
        }
        let mut commanded = IdTable::new(bound, false);
        for id in world.command_report_units() {
            commanded.set(id, true);
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
            let mut inputs = Vec::with_capacity(range.len());
            let mut changed = Vec::with_capacity(range.len());
            let mut owned = Vec::new();
            let mut converted = 0;
            for (n, unit) in world.units_range(range.clone()).enumerate() {
                let input = Input {
                    unit,
                    gather: gather.get(unit.id),
                    hunt: hunt.get(unit.id),
                    cargo: cargo.get(unit.id),
                    commanded: commanded.get(unit.id),
                };
                let before = previous
                    .map(|previous| (previous, previous.index.get(unit.id)))
                    .filter(|(_, slot)| *slot != ABSENT);
                match before {
                    Some((previous, slot))
                        if !input.commanded && *previous.input(slot) == input =>
                    {
                        states.push(*previous.state(slot));
                        changed.push(false);
                    }
                    _ => {
                        converted += 1;
                        let state = convert(world, &input);
                        changed.push(
                            before.is_none_or(|(previous, slot)| *previous.state(slot) != state),
                        );
                        states.push(state);
                    }
                }
                inputs.push(input);
                if let Some(list) = list_of
                    .get(usize::from(unit.player.0))
                    .filter(|list| **list != ABSENT)
                    && world.unit_visible_to(unit.player, unit.id)
                {
                    owned.push((*list, (range.start + n) as u32));
                }
            }
            Part {
                converted,
                states,
                inputs,
                changed,
                owned,
            }
        };
        let wanted = count.div_ceil(PART);
        let parts = if threads <= 1 || wanted <= 1 {
            (0..wanted).map(part).collect::<Vec<_>>()
        } else {
            (0..wanted)
                .into_par_iter()
                // At most `threads` concurrent parts, even in a larger pool.
                .with_min_len(wanted.div_ceil(threads))
                .map(|at| std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| part(at))).ok())
                .collect::<Option<Vec<_>>>()?
        };
        let converted = parts.iter().map(|part| part.converted).sum();
        let mut index = IdTable::new(bound, ABSENT);
        let mut owner = Vec::with_capacity(count);
        let mut changed = Vec::with_capacity(count);
        let mut lists = vec![Vec::new(); viewers.len()];
        for (at, part) in parts.iter().enumerate() {
            for (n, state) in part.states.iter().enumerate() {
                index.set(state.id, (at * PART + n) as u32);
                owner.push(state.player.0);
            }
            changed.extend_from_slice(&part.changed);
            for (list, slot) in &part.owned {
                let list = &mut lists[*list as usize];
                if list.len() <= MAX_SUBSCRIBED_UNITS {
                    list.push(*slot);
                }
            }
        }
        let (parts, inputs): (Vec<_>, Vec<_>) = parts
            .into_iter()
            .map(|part| (part.states, part.inputs))
            .unzip();
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
            inputs,
            owner,
            index,
            own,
            generation,
            base: previous.map(|previous| previous.generation),
            changed,
            converted,
        })
    }

    /// Units this frame converted.
    pub(super) fn unit_count(&self) -> usize {
        self.owner.len()
    }

    /// Units converted afresh and units whose state changed this frame.
    pub(super) fn work(&self) -> (usize, usize) {
        let changed = self.changed.iter().filter(|changed| **changed).count();
        (self.converted, changed)
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether `slot`'s unit is as it was in frame `held`, so a baseline
    /// taken from that frame needs no comparison for it.
    pub(super) fn unchanged_since(&self, held: Option<u64>, slot: u32) -> bool {
        held.is_some() && held == self.base && !self.changed[slot as usize]
    }

    fn input(&self, slot: u32) -> &Input {
        &self.inputs[slot as usize / PART][slot as usize % PART]
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
