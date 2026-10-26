use aoe_core::{EntityId, PlayerId, ScreenPoint};
use aoe_protocol::{GameplayUnitState, UnitKind, UnitTask};
use std::{collections::BTreeSet, ops::Bound};

/// Updated only by authoritative snapshots, unit deltas and removals.
#[derive(Default)]
pub(crate) struct IdleFinder {
    ids: BTreeSet<EntityId>,
    last: Option<EntityId>,
}

impl IdleFinder {
    pub fn clear(&mut self) {
        // A camera subscription can replace its snapshot after every jump.
        // Keep the cycle cursor; Welcome/WorldReset replace the whole Village.
        self.ids.clear();
    }

    pub fn update(&mut self, unit: &GameplayUnitState, eligible: bool) {
        if eligible
            && unit.hp > 0
            && matches!(unit.task, UnitTask::Idle | UnitTask::Carrying)
            && !unit.moving
            && !unit.planning
            && !unit.auto_scout
            && unit.attack_target.is_none()
            && unit.hunt_target.is_none()
            && unit.gathering.is_none()
        {
            self.ids.insert(unit.id);
        } else {
            self.remove(unit.id);
        }
    }

    pub fn remove(&mut self, id: EntityId) {
        self.ids.remove(&id);
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn next(&mut self) -> Option<EntityId> {
        let next = self
            .last
            .and_then(|last| {
                self.ids
                    .range((Bound::Excluded(last), Bound::Unbounded))
                    .next()
                    .copied()
            })
            .or_else(|| self.ids.first().copied());
        self.last = next;
        next
    }
}

pub(in super::super) fn idle(client: &mut super::Client, military: bool) {
    let finder = if military {
        &mut client.village.idle_military
    } else {
        &mut client.village.idle_villagers
    };
    let Some(id) = finder.next() else {
        return;
    };
    let Some(unit) = client.units.get(&id) else {
        return;
    };
    let position = unit.position.as_tiles();
    super::construction::cancel(client);
    client.village.move_cursor = false;
    client.village.unload_cursor = false;
    client.village.attack_move = false;
    client.pointer = None;
    client.drag = None;
    client.selected.units(vec![id]);
    crate::playground::game::map::center_on_world(client, position);
    client.camera = client.camera.clamp_center(client.config);
    crate::playground::game::subscribe(client);
    let _ = client.canvas.focus();
}

#[derive(Default)]
pub(crate) struct Selection {
    pub units: Vec<EntityId>,
    pub resource_id: Option<u64>,
    pub building_id: Option<EntityId>,
    pub buildings: Vec<EntityId>,
}
impl Selection {
    pub fn building(&mut self, id: EntityId) {
        self.resource_id = None;
        self.units.clear();
        self.building_id = Some(id);
        self.buildings = vec![id];
    }
    pub fn units(&mut self, units: Vec<EntityId>) {
        self.resource_id = None;
        self.units = units;
        self.building_id = None;
        self.buildings.clear();
    }
    pub fn resource(&mut self, id: u64) {
        self.units.clear();
        self.building_id = None;
        self.buildings.clear();
        self.resource_id = Some(id);
    }
    pub fn has_building(&self, id: EntityId) -> bool {
        self.building_id == Some(id) || self.buildings.binary_search(&id).is_ok()
    }
    pub fn same_buildings(
        &mut self,
        candidates: impl IntoIterator<Item = (EntityId, PlayerId, aoe_protocol::BuildingKind, bool)>,
        owner: PlayerId,
        kind: aoe_protocol::BuildingKind,
        primary: EntityId,
    ) {
        self.units.clear();
        self.resource_id = None;
        self.buildings = candidates
            .into_iter()
            .filter_map(|(id, player, candidate, visible)| {
                (player == owner && candidate == kind && visible).then_some(id)
            })
            .collect();
        self.buildings.sort_unstable();
        self.buildings.dedup();
        self.building_id = self
            .buildings
            .binary_search(&primary)
            .is_ok()
            .then_some(primary)
            .or_else(|| self.buildings.first().copied());
    }
    pub fn same_kind(
        &mut self,
        candidates: impl IntoIterator<Item = (EntityId, PlayerId, UnitKind, bool)>,
        owner: PlayerId,
        kind: UnitKind,
    ) {
        self.units(
            candidates
                .into_iter()
                .filter_map(|(id, player, candidate, visible)| {
                    (player == owner && candidate == kind && visible).then_some(id)
                })
                .collect(),
        );
    }
}

pub(crate) fn box_members(
    candidates: impl IntoIterator<Item = (EntityId, PlayerId, ScreenPoint)>,
    owner: PlayerId,
    start: ScreenPoint,
    end: ScreenPoint,
) -> Vec<EntityId> {
    candidates
        .into_iter()
        .filter_map(|(id, player, p)| {
            (player == owner
                && p.x >= start.x.min(end.x)
                && p.x <= start.x.max(end.x)
                && p.y >= start.y.min(end.y)
                && p.y <= start.y.max(end.y))
            .then_some(id)
        })
        .collect()
}
