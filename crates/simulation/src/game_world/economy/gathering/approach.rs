//! Work positions reserve circles around the resource footprint, not whole tiles.
use super::*;
use crate::game_world::movement_step::slots::{squared, within_work_reach};

impl GameWorld {
    pub(crate) fn crowded_resource_approach(&self, id: EntityId) -> bool {
        self.economy.gatherers.get(&id).is_some_and(|task| {
            task.farm.is_none()
                && task.animal.is_none()
                && task.fish.is_none()
                && (task.crowded
                    || self
                        .economy
                        .gatherers
                        .count_target(super::WorkTarget::Node(task.node))
                        >= 11)
        })
    }

    pub(crate) fn constrained_worksite(&self, kind: UnitKind, tile: TileCoord) -> bool {
        self.static_work_positions(kind, tile).constrained
    }

    pub(super) fn refine_gather_approach(&mut self, id: EntityId) {
        if !(self.tick.0 + u64::from(id.0)).is_multiple_of(u64::from(self.config.tick_hz)) {
            return;
        }
        let refine = self.economy.gatherers.get(&id).is_some_and(|task| {
            task.phase == GatherPhase::Approaching
                && task.crowded
                && task.farm.is_none()
                && task.animal.is_none()
                && task.fish.is_none()
                && task.work_position.is_some_and(|at| {
                    self.unit(id).is_some_and(|u| {
                        u.position.tile_floor().x.abs_diff(task.tile.x) <= 8
                            && u.position.tile_floor().y.abs_diff(task.tile.y) <= 8
                            && squared(u.position, at) > 768 * 768
                            && self.constrained_worksite(u.kind, task.tile)
                            && !self.interaction_line_clear(u.kind, u.position, at)
                    })
                })
        });
        if refine {
            self.repick_obstructed_gatherer(id);
        }
    }

    pub(super) fn admit_resource_approach(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        (self.node_has_work_slot(task.node, id)
            && (self.approach_resource(id, task.tile).is_ok()
                || self.queue_resource_approach(id, task.tile)))
            || self.replacement_resource(id, task)
    }

    /// Nearby friendly traffic can clear; an isolated static failure ends the job.
    pub(super) fn gather_traffic_blocked(&self, id: EntityId) -> bool {
        let Some(&index) = self.lookup.get(&id) else {
            return false;
        };
        self.friendly_contact_pressure(index, 2)
    }

    /// Reserved work points are preferences. Physical contact and full body
    /// clearance, rather than another worker's future point, decide arrival.
    pub(super) fn gather_contact(&self, index: usize, task: &GatherTask) -> bool {
        let unit = self.units[index].state;
        task.phase == GatherPhase::Approaching
            && unit.kind == UnitKind::Villager
            && task.farm.is_none()
            && task.animal.is_none()
            && within_work_reach(unit.kind, unit.position, task.tile)
            && self.body_position_clear(unit.kind, unit.position)
            && self.work_contact_clear(index)
    }

    pub(crate) fn resource_has_work_spot(&self, id: EntityId, target: TileCoord) -> bool {
        !self.resource_work_positions(id, target, None).is_empty()
    }

    /// The closest free work position, the first one an approach tries.
    pub(crate) fn resource_work_spot(
        &self,
        id: EntityId,
        target: TileCoord,
    ) -> Option<aoe_core::WorldPosition> {
        let unit = self.unit(id)?;
        let points = self.resource_work_positions(id, target, None);
        points
            .iter()
            .copied()
            .find(|&at| self.interaction_line_clear(unit.kind, unit.position, at))
            .or_else(|| points.first().copied())
    }

    fn resource_work_positions(
        &self,
        id: EntityId,
        target: TileCoord,
        reached: Option<&[TileCoord]>,
    ) -> Vec<aoe_core::WorldPosition> {
        let Some(unit) = self.unit(id) else {
            return Vec::new();
        };
        let index = self.lookup[&id];
        if !unit.kind.is_naval()
            && self.at_work_position(id, target)
            && self.interaction_static_clear(unit.kind, unit.position)
            && self.work_point_clear(index, unit.position)
        {
            return vec![unit.position];
        }
        let points = self.static_work_positions(unit.kind, target);
        let mut candidates = points.inner.clone();
        // A worker already in contact need not walk to a canonical candidate.
        if self.at_work_position(id, target)
            && self.interaction_static_clear(unit.kind, unit.position)
        {
            candidates.push(unit.position);
        }
        let free = |at: aoe_core::WorldPosition| {
            reached.is_none_or(|tiles| tiles.binary_search(&at.tile_floor()).is_ok())
                && !self.destination_point_reserved(at, id)
                && !self.position_occupied(index, at, true)
        };
        candidates.retain(|&at| free(at));
        if !unit.kind.is_naval() {
            let compressed = self.crowded_resource_approach(id)
                || (candidates.is_empty()
                    && aoe_core::WorldPosition::from_tile_center(target)
                        .ok()
                        .is_some_and(|at| self.friendly_contact_pressure_at(index, at, 11)));
            if candidates.is_empty() || compressed {
                candidates.extend(points.outer.iter().copied().filter(|&at| {
                    free(at)
                        || (compressed
                            && reached
                                .is_none_or(|tiles| tiles.binary_search(&at.tile_floor()).is_ok())
                            && self.work_point_clear(index, at))
                }));
            }
        }
        // Sort the cheap, total distance key once. Callers inspect direct
        // candidates lazily, preserving the direct-first order without rays
        // inside O(n log n) sort comparisons.
        candidates.sort_unstable_by_key(|&at| (squared(unit.position, at), at.y, at.x));
        candidates
    }

    /// A crowded job may queue at a statically reachable contact point. Unit
    /// reservations cannot reject the order; actual bodies still block steps.
    pub(super) fn queue_resource_approach(&mut self, id: EntityId, target: TileCoord) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        let mut points = self.static_work_positions(unit.kind, target).inner.clone();
        points.sort_unstable_by_key(|&at| (squared(unit.position, at), at.y, at.x));
        if !unit.kind.is_naval() {
            let index = self.lookup[&id];
            points.retain(|&at| self.work_queue_point_clear(index, at));
        }
        points
            .into_iter()
            .take(8)
            .any(|at| self.issue_task_move(id, at).is_ok())
    }

    pub(crate) fn approach_resource(
        &mut self,
        id: EntityId,
        target: TileCoord,
    ) -> Result<(), EconomyError> {
        self.approach_resource_in(id, target, None)
    }

    /// Traffic is temporary. Retry a valid fishing job at most once a second,
    /// retaining its reservation instead of dropping the profession to Idle.
    pub(super) fn retry_fishing_approach(&mut self, id: EntityId, task: &mut GatherTask) -> bool {
        let Some(left) = self.resource_remaining(task.node) else {
            return false;
        };
        if left == 0 {
            return self.resume_resource(id, task);
        }
        if self.tick.0 < task.retry_tick {
            return true;
        }
        task.retry_tick = self.tick.0 + u64::from(self.config.tick_hz);
        if self.approach_resource(id, task.tile).is_ok() {
            task.work_position = self.movement_order(id).map(|o| o.destination);
            task.approach_failures = 0;
            task.retry_tick = 0;
            if let Some(&index) = self.lookup.get(&id) {
                self.units[index].state.task = UnitTask::Moving;
            }
        }
        true
    }

    pub(super) fn approach_resource_in(
        &mut self,
        id: EntityId,
        target: TileCoord,
        reached: Option<&[TileCoord]>,
    ) -> Result<(), EconomyError> {
        let candidates = self.resource_work_positions(id, target, reached);
        let Some(unit) = self.unit(id) else {
            return Err(GameWorldError::Unreachable.into());
        };
        let mut failure = GameWorldError::Unreachable;
        for direct in [true, false] {
            for &at in &candidates {
                if self.interaction_line_clear(unit.kind, unit.position, at) != direct {
                    continue;
                }
                match self.issue_task_move(id, at) {
                    Ok(_) => return Ok(()),
                    Err(error) => {
                        failure = error;
                        if matches!(
                            error,
                            GameWorldError::PathBudgetExceeded | GameWorldError::Environment(_)
                        ) {
                            return Err(error.into());
                        }
                    }
                }
            }
        }
        Err(failure.into())
    }
}
