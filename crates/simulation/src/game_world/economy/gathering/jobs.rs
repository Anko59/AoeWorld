//! Derived per-target reservations. Returning workers retain their job/slot.
//! Mutations go through insert/remove so cancellation cannot leak occupancy.
use super::*;
use crate::game_world::movement_step::CellHasher;
use std::collections::{BTreeMap, HashMap};
use std::hash::BuildHasherDefault;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum WorkTarget {
    Node(u64),
    Farm(EntityId),
    Animal(EntityId),
}

impl GatherTask {
    fn target(&self) -> WorkTarget {
        if let Some(animal) = self.animal {
            WorkTarget::Animal(animal)
        } else if let Some(farm) = self.farm {
            WorkTarget::Farm(farm)
        } else {
            WorkTarget::Node(self.node)
        }
    }

    pub(super) fn new(
        node: u64,
        tile: TileCoord,
        kind: ResourceKind,
        farm: Option<EntityId>,
    ) -> Self {
        Self {
            node,
            tile,
            kind,
            farm,
            animal: None,
            carcass_kind: None,
            fish: None,
            work_tile: None,
            work_position: None,
            phase: GatherPhase::Approaching,
            approach_failures: 0,
            crowded: false,
            credits: 0,
            dropoff: None,
            return_target: None,
            failed_dropoffs: Default::default(),
            retry_tick: 0,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Gatherers {
    tasks: BTreeMap<EntityId, GatherTask>,
    members: HashMap<u64, u64, BuildHasherDefault<CellHasher>>,
    targets: HashMap<WorkTarget, usize, BuildHasherDefault<CellHasher>>,
}

impl Gatherers {
    /// Sparse words bound memory by live jobs, even for ever-increasing ids.
    pub(crate) fn contains_key(&self, id: &EntityId) -> bool {
        self.members
            .get(&(u64::from(id.0) >> 6))
            .is_some_and(|word| word & (1_u64 << (id.0 % 64)) != 0)
    }

    pub(super) fn observe_crowd(&mut self, id: EntityId) {
        let mark = self.tasks.get(&id).is_some_and(|task| {
            !task.crowded
                && task.farm.is_none()
                && task.animal.is_none()
                && task.fish.is_none()
                && self.count_target(task.target()) > 10
        });
        if mark && let Some(task) = self.tasks.get_mut(&id) {
            task.crowded = true;
        }
    }

    pub(crate) fn set_work_position(&mut self, id: EntityId, at: aoe_core::WorldPosition) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.work_position = Some(at);
        }
    }

    pub(crate) fn count_target(&self, target: WorkTarget) -> usize {
        self.targets.get(&target).copied().unwrap_or(0)
    }

    pub(super) fn reserved_target(&self, id: &EntityId) -> Option<WorkTarget> {
        self.tasks.get(id).map(GatherTask::target)
    }

    pub(crate) fn occupied(&self, target: WorkTarget, except: EntityId) -> usize {
        self.count_target(target) - usize::from(self.reserved_target(&except) == Some(target))
    }

    pub(crate) fn insert(&mut self, id: EntityId, task: GatherTask) -> Option<GatherTask> {
        let target = task.target();
        let old = self.tasks.insert(id, task);
        let previous = old.as_ref().map(GatherTask::target);
        // Progress and phase writes keep the same reservation and membership.
        if previous == Some(target) {
            return old;
        }
        if let Some(previous) = previous {
            self.release_target(previous);
        } else {
            *self.members.entry(u64::from(id.0) >> 6).or_default() |= 1_u64 << (id.0 % 64);
        }
        *self.targets.entry(target).or_default() += 1;
        old
    }

    fn release_target(&mut self, target: WorkTarget) {
        if let Some(count) = self.targets.get_mut(&target) {
            *count -= 1;
            if *count == 0 {
                self.targets.remove(&target);
            }
        }
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<GatherTask> {
        let task = self.tasks.remove(id)?;
        let word = u64::from(id.0) >> 6;
        if let Some(bits) = self.members.get_mut(&word) {
            *bits &= !(1_u64 << (id.0 % 64));
            if *bits == 0 {
                self.members.remove(&word);
            }
        }
        self.release_target(task.target());
        Some(task)
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.tasks.clear();
        self.members.clear();
        self.targets.clear();
    }
}

impl GameWorld {
    pub(crate) fn node_has_work_slot(&self, node: u64, id: EntityId) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        // Broad admission uses the number of radius-spaced perimeter samples.
        // Exact terrain, occupancy and reservation checks belong to approach.
        let slots = slot_capacity(unit.kind);
        self.economy.gatherers.occupied(WorkTarget::Node(node), id) < slots
    }

    pub(crate) fn animal_has_work_slot(
        &self,
        animal: EntityId,
        kind: crate::AnimalKind,
        worker: EntityId,
    ) -> bool {
        let spots = match kind {
            crate::AnimalKind::Deer => 4,
            crate::AnimalKind::Sheep | crate::AnimalKind::Boar => 8,
            crate::AnimalKind::Wolf => 0,
        };
        self.economy
            .gatherers
            .occupied(WorkTarget::Animal(animal), worker)
            < spots
    }
}

impl Deref for Gatherers {
    type Target = BTreeMap<EntityId, GatherTask>;
    fn deref(&self) -> &Self::Target {
        &self.tasks
    }
}

impl<'a> IntoIterator for &'a Gatherers {
    type Item = (&'a EntityId, &'a GatherTask);
    type IntoIter = std::collections::btree_map::Iter<'a, EntityId, GatherTask>;
    fn into_iter(self) -> Self::IntoIter {
        self.tasks.iter()
    }
}

pub(super) fn slot_capacity(kind: UnitKind) -> usize {
    let radius = kind.collision_radius();
    let side = 1024 + 2 * (radius + 8);
    (4 * (side / (2 * radius + 16))) as usize
}

#[cfg(test)]
mod tests;
