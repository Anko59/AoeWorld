//! Derived destination occupancy, updated by order issuance and changed movers.
//! It carries no gameplay state: every lookup validates against the live order.
use super::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

#[derive(Debug, Default)]
pub(crate) struct Destinations {
    by_tile: BTreeMap<TileCoord, BTreeSet<EntityId>>,
    by_unit: BTreeMap<EntityId, TileCoord>,
}
impl Destinations {
    pub(crate) fn remove(&mut self, id: EntityId) {
        if let Some(tile) = self.by_unit.remove(&id)
            && let Some(ids) = self.by_tile.get_mut(&tile)
        {
            ids.remove(&id);
            if ids.is_empty() {
                self.by_tile.remove(&tile);
            }
        }
    }
    fn insert(&mut self, id: EntityId, tile: TileCoord) {
        self.remove(id);
        self.by_unit.insert(id, tile);
        self.by_tile.entry(tile).or_default().insert(id);
    }
}

impl GameWorld {
    pub(crate) fn index_destination(&mut self, id: EntityId) {
        self.destinations.remove(id);
        let order = self.movement_order(id);
        self.refresh_spacing_order(id);
        if let Some(order) = order {
            self.destinations.insert(id, order.target_tile);
        }
    }

    fn destination_reserved(&self, tile: TileCoord, except: EntityId) -> bool {
        self.destinations.by_tile.get(&tile).is_some_and(|ids| {
            ids.iter().any(|&id| {
                id != except
                    && self
                        .movement_order(id)
                        .is_some_and(|o| o.target_tile == tile)
            })
        })
    }

    /// All normal move commands use the same compact, deterministic ring scan.
    /// Reservations survive tick boundaries, including commands split by transport.
    /// At most 289 slots are visited; congestion never rejects a legal click.
    pub(crate) fn spread_destination(
        &self,
        id: EntityId,
        click: WorldPosition,
    ) -> Result<WorldPosition, GameWorldError> {
        self.destination_slot(id, click, false)
    }

    pub(crate) fn free_arrival_destination(
        &self,
        id: EntityId,
        click: WorldPosition,
    ) -> Result<WorldPosition, GameWorldError> {
        self.destination_slot(id, click, true)
    }

    fn destination_slot(
        &self,
        id: EntityId,
        click: WorldPosition,
        occupants: bool,
    ) -> Result<WorldPosition, GameWorldError> {
        let unit = self.unit(id).ok_or(GameWorldError::UnknownEntity)?;
        let click = self.config.snap_ground_position(click);
        let target = click.tile_floor();
        if !self.unit_tile_passable(unit.kind, target, &|| false)?
            || self.economy.blocked_tiles.contains(&target)
        {
            return Err(GameWorldError::InvalidPosition);
        }
        let busy = |tile| {
            if occupants {
                // Replacing an occupied short order with its own tile cancels
                // it before the mover takes a step. Seek another arrival;
                // bounded obstruction settling handles a truly boxed-in unit.
                tile == unit.position.tile_floor() || self.work_spot_busy(tile, id)
            } else {
                self.destination_reserved(tile, id)
            }
        };
        if !busy(target) {
            return Ok(click);
        }
        // Walk outward from the click, rather than jumping over walls to a
        // geometrically near but disconnected tile. Both the frontier and the
        // visited set are bounded by the 17x17 local square.
        let mut frontier = BinaryHeap::from([Reverse((0, target))]);
        let mut visited = BTreeSet::from([target]);
        while let Some(Reverse((_, tile))) = frontier.pop() {
            if !busy(tile) {
                return WorldPosition::from_tile_center(tile).map_err(Into::into);
            }
            for (dx, dy) in [
                (-1, 0),
                (0, -1),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                if next.x.abs_diff(target.x) > 8
                    || next.y.abs_diff(target.y) > 8
                    || visited.contains(&next)
                {
                    continue;
                }
                let Ok(position) = WorldPosition::from_tile_center(next) else {
                    continue;
                };
                if self.config.valid_ground_position(position)
                    && self.building_step_clear(tile, next)
                    && self.unit_step_clear(unit.kind, tile, next)?
                {
                    visited.insert(next);
                    let dx = next.x - target.x;
                    let dy = next.y - target.y;
                    frontier.push(Reverse((dx * dx + dy * dy, next)));
                }
            }
        }
        Ok(click)
    }

    /// Work spots prefer unreserved adjacent tiles, then distance and tile order.
    /// Idle workers are found in the existing local grid, including those whose
    /// approach order has just completed. No population-wide scan is needed.
    pub(crate) fn work_spot_busy(&self, tile: TileCoord, except: EntityId) -> bool {
        self.destination_reserved(tile, except)
            || WorldPosition::from_tile_center(tile).is_ok_and(|at| {
                self.spacing_neighbours(at, u64::from(except.0)).any(|id| {
                    id != except
                        && self.unit(id).is_some_and(|u| {
                            u.hp > 0 && !u.moving && !u.planning && u.position.tile_floor() == tile
                        })
                })
            })
    }

    /// The actor already occupies this spot. A future arrival reservation
    /// cannot make it keep walking; that arrival must find another free slot.
    pub(crate) fn arrival_position_free(&self, index: usize) -> bool {
        self.arrival_point_free(index, self.units[index].state.position)
    }

    pub(crate) fn arrival_point_free(&self, index: usize, at: WorldPosition) -> bool {
        let unit = self.units[index].state;
        !self.local_position_occupied(index, at)
            && !self
                .spacing_units(at, u64::from(unit.id.0), unit.id)
                .any(|other| {
                    other.kind.is_naval() == unit.kind.is_naval()
                        && other.position.tile_floor() == at.tile_floor()
                })
    }

    pub(crate) fn arrival_tile_has_idle(&self, index: usize, at: WorldPosition) -> bool {
        let unit = self.units[index].state;
        self.spacing_units(at, u64::from(unit.id.0), unit.id)
            .any(|other| {
                let state = self.units[self.lookup[&other.id]].state;
                other.kind.is_naval() == unit.kind.is_naval()
                    && other.position.tile_floor() == at.tile_floor()
                    && !state.moving
                    && !state.planning
            })
    }
}
