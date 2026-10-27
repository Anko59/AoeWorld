//! Derived per-target reservations. Returning workers retain their job/slot.
//! Mutations go through insert/remove so cancellation cannot leak occupancy.
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Deref;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
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
    targets: BTreeMap<WorkTarget, BTreeSet<EntityId>>,
}

impl Gatherers {
    pub(crate) fn set_work_position(&mut self, id: EntityId, at: aoe_core::WorldPosition) {
        if let Some(task) = self.tasks.get_mut(&id) {
            task.work_position = Some(at);
        }
    }

    pub(crate) fn count_target(&self, target: WorkTarget) -> usize {
        self.targets.get(&target).map_or(0, BTreeSet::len)
    }

    pub(crate) fn occupied(&self, target: WorkTarget, except: EntityId) -> usize {
        self.targets
            .get(&target)
            .map_or(0, |ids| ids.len() - usize::from(ids.contains(&except)))
    }

    pub(crate) fn insert(&mut self, id: EntityId, task: GatherTask) -> Option<GatherTask> {
        let old = self.remove(&id);
        self.targets.entry(task.target()).or_default().insert(id);
        self.tasks.insert(id, task);
        old
    }

    pub(crate) fn remove(&mut self, id: &EntityId) -> Option<GatherTask> {
        let task = self.tasks.remove(id)?;
        let key = task.target();
        if let Some(ids) = self.targets.get_mut(&key) {
            ids.remove(id);
            if ids.is_empty() {
                self.targets.remove(&key);
            }
        }
        Some(task)
    }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        self.tasks.clear();
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
        let radius = unit.kind.collision_radius();
        let side = 1024 + 2 * (radius + 8);
        let slots = (4 * (side / (2 * radius + 16))) as usize;
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
