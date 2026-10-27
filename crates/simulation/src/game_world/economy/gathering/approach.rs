//! Bounded work-spot admission; replacement can require a certified local tile.
use super::*;
use std::collections::BTreeSet;

impl GameWorld {
    pub(crate) fn resource_has_work_spot(&self, id: EntityId, target: TileCoord) -> bool {
        let Some(unit) = self.unit(id) else {
            return false;
        };
        perimeter(aoe_core::TileRect::from_xywh(target.x, target.y, 1, 1))
            .into_iter()
            .any(|tile| {
                self.unit_tile_passable(unit.kind, tile, &|| false)
                    .unwrap_or(false)
                    && !self.economy.blocked_tiles.contains(&tile)
                    && !self.work_spot_busy(tile, id)
            })
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
        let unit = self.unit(id).ok_or(EconomyError::UnknownEntity)?;
        let mut candidates = perimeter(aoe_core::TileRect::from_xywh(target.x, target.y, 1, 1));
        candidates.retain(|tile| {
            reached.is_none_or(|tiles| tiles.contains(tile))
                && self
                    .unit_tile_passable(unit.kind, *tile, &|| false)
                    .unwrap_or(false)
                && !self.economy.blocked_tiles.contains(tile)
                && !self.work_spot_busy(*tile, id)
        });
        candidates.sort_by_key(|tile| {
            (
                distance_squared(unit.position.tile_floor(), *tile),
                tile.y,
                tile.x,
            )
        });
        let mut failure = GameWorldError::Unreachable;
        for tile in candidates {
            match self.issue_task_move(id, position(tile)?) {
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
