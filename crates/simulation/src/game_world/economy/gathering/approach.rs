//! Work positions reserve circles around the resource footprint, not whole tiles.
use super::*;
use crate::game_world::movement_step::slots::{perimeter_positions, squared, within_work_reach};
use std::collections::BTreeSet;

impl GameWorld {
    pub(crate) fn resource_has_work_spot(&self, id: EntityId, target: TileCoord) -> bool {
        self.resource_work_spot(id, target).is_some()
    }

    /// The closest free work position, the first one an approach tries.
    pub(crate) fn resource_work_spot(
        &self,
        id: EntityId,
        target: TileCoord,
    ) -> Option<aoe_core::WorldPosition> {
        self.resource_work_positions(id, target, None)
            .into_iter()
            .next()
    }

    fn resource_work_positions(
        &self,
        id: EntityId,
        target: TileCoord,
        reached: Option<&BTreeSet<TileCoord>>,
    ) -> Vec<aoe_core::WorldPosition> {
        let Some(unit) = self.unit(id) else {
            return Vec::new();
        };
        let rect = aoe_core::TileRect::from_xywh(target.x, target.y, 1, 1);
        let mut candidates = if unit.kind.is_naval() {
            // Half-tile hull separation with quarter-tile shore clearance:
            // sample both legal lanes, instead of two outer hull corners.
            let mut points = Vec::new();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    for y in [256, 768] {
                        for x in [256, 768] {
                            let at = aoe_core::WorldPosition::new(
                                (target.x + dx) * 1024 + x,
                                (target.y + dy) * 1024 + y,
                            );
                            if within_work_reach(unit.kind, at, target) {
                                points.push(at);
                            }
                        }
                    }
                }
            }
            points
        } else {
            perimeter_positions(rect, unit.kind)
        };
        // A worker already in contact need not walk to a canonical candidate.
        if self.at_work_position(id, target) {
            candidates.push(unit.position);
        }
        candidates.retain(|&at| {
            reached.is_none_or(|tiles| tiles.contains(&at.tile_floor()))
                && self.interaction_point_free(id, at)
        });
        candidates.sort_unstable_by_key(|&at| (squared(unit.position, at), at.y, at.x));
        candidates
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
        reached: Option<&BTreeSet<TileCoord>>,
    ) -> Result<(), EconomyError> {
        let candidates = self.resource_work_positions(id, target, reached);
        let mut failure = GameWorldError::Unreachable;
        for at in candidates {
            match self.issue_task_move(id, at) {
                Ok(_) => return Ok(()),
                Err(error) => {
                    failure = error;
                    if matches!(
                        error,
                        GameWorldError::PathBudgetExceeded | GameWorldError::Environment(_)
                    ) {
                        break;
                    }
                }
            }
        }
        Err(failure.into())
    }
}
