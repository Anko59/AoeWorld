//! Work positions reserve circles around the resource footprint, not whole tiles.
use super::*;
use crate::game_world::movement_step::slots::{perimeter_positions, squared};
use std::collections::BTreeSet;

impl GameWorld {
    pub(crate) fn resource_has_work_spot(&self, id: EntityId, target: TileCoord) -> bool {
        self.resource_work_positions(id, target, None)
            .into_iter()
            .next()
            .is_some()
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
        let mut candidates = perimeter_positions(rect, unit.kind);
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
