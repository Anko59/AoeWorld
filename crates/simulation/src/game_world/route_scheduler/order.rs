//! Issuing a movement order. A refused order changes nothing: the unit keeps
//! its current order *and* the route search already spent on it. (Clearing
//! the search first made every refused order restart a waiting unit's search
//! from nothing, so a unit whose owner kept sending refused orders, as the AI
//! does, waited for its route forever and held a search slot all along.)
use super::issue::MapRoutePlan;
use crate::game_path::{next_waypoint, route_waypoint, segment_length};
use crate::game_world::facing_for;
use crate::{GameWorld, GameWorldError, MovementOrder};
use aoe_core::{EntityId, TileCoord, WorldPosition};
use aoe_map::{MovementOutcome, RoutePlanner};
use std::collections::VecDeque;

/// How long (ticks, 10 s at 20 Hz) an unreachable verdict is remembered.
const UNREACHABLE_MEMO_TICKS: u64 = 200;

/// Where a new order starts: the first waypoint, the planned tiles after it,
/// the planner that continues the route, whether it still waits for one, and
/// whether its fast attempt was made in full.
pub(crate) type OrderRoute = (
    WorldPosition,
    VecDeque<aoe_core::TileCoord>,
    Option<RoutePlanner>,
    bool,
    bool,
);

impl GameWorld {
    pub(crate) fn issue_task_move(
        &mut self,
        id: EntityId,
        destination: WorldPosition,
    ) -> Result<bool, GameWorldError> {
        let index = *self.lookup.get(&id).ok_or(GameWorldError::UnknownEntity)?;
        if self.units[index].state.hp == 0 || self.embarked(id) {
            return Err(GameWorldError::UnknownEntity);
        }
        let destination = self.config.snap_ground_position(destination);
        let kind = self.units[index].state.kind;
        let passable = self
            .unit_tile_passable(kind, destination.tile_floor(), &|| false)
            .map_err(GameWorldError::Environment)?;
        if !passable
            || self
                .economy
                .blocked_tiles
                .contains(&destination.tile_floor())
        {
            return Err(GameWorldError::InvalidPosition);
        }
        let origin = self.units[index].state.position;
        if self.recently_unreachable(index, origin.tile_floor(), destination.tile_floor()) {
            return Err(GameWorldError::Unreachable);
        }
        if origin == destination {
            self.break_formation(id);
            self.move_queues.remove(&id);
            self.clear_planner(index);
            self.units[index].last_movement_error = None;
            self.units[index].order = None;
            self.units[index].obstruction = None;
            self.units[index].route.clear();
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            self.index_destination(id);
            return Ok(false);
        }
        // The current planner is set aside, not dropped: its slot is free
        // while the new route is planned, and it comes back if that fails.
        let previous = self.take_planner(index);
        let (waypoint, route, planner, planning, fast_tried) =
            match self.order_route(index, origin, destination) {
                Ok(route) => route,
                Err(error) => {
                    self.store_planner(index, previous);
                    if error == GameWorldError::Unreachable {
                        self.remember_unreachable(index, destination.tile_floor());
                    }
                    return Err(error);
                }
            };
        let speed_carry = self.units[index].order.map_or(0, |order| order.speed_carry);
        self.break_formation(id);
        self.move_queues.remove(&id);
        let dx = i64::from(waypoint.x) - i64::from(origin.x);
        let dy = i64::from(waypoint.y) - i64::from(origin.y);
        let unit = &mut self.units[index];
        unit.last_movement_error = None;
        unit.obstruction = None;
        unit.state.previous_position = origin;
        unit.state.facing = facing_for(dx, dy, unit.state.facing);
        unit.state.moving = !planning;
        unit.state.planning = planning;
        unit.planning_since = self.tick.0;
        unit.fast_route_tried = fast_tried;
        unit.route = route;
        unit.order = Some(MovementOrder {
            origin,
            destination,
            waypoint,
            target_tile: destination.tile_floor(),
            segment_length: segment_length(dx, dy),
            travelled: 0,
            speed_carry,
        });
        self.store_planner(index, planner);
        if let Err(at) = self.active_movers.binary_search(&id) {
            self.active_movers.insert(at, id);
        }
        self.index_destination(id);
        Ok(true)
    }

    /// Remembers that the unit, where it stands, could not get to `target`.
    pub(crate) fn remember_unreachable(&mut self, index: usize, target: TileCoord) {
        let at = self.units[index].state.position.tile_floor();
        self.units[index].unreachable_memo = Some((at, target, self.tick.0));
    }

    /// True when the unit, still where it stood, was refused `target` in the
    /// last `UNREACHABLE_MEMO_TICKS` and no ground has opened since.
    /// A chase or a re-sent order to the same unreachable spot (a soldier
    /// facing an enemy across a river re-plans every second) is refused at
    /// once instead of costing a whole fast attempt each time.
    fn recently_unreachable(&self, index: usize, from: TileCoord, target: TileCoord) -> bool {
        self.units[index]
            .unreachable_memo
            .is_some_and(|(at, to, tick)| {
                at == from
                    && to == target
                    && self.tick.0.saturating_sub(tick) < UNREACHABLE_MEMO_TICKS
            })
    }

    pub(crate) fn order_route(
        &mut self,
        index: usize,
        origin: WorldPosition,
        destination: WorldPosition,
    ) -> Result<OrderRoute, GameWorldError> {
        let (kind, player) = (self.units[index].state.kind, self.units[index].state.player);
        let target_tile = destination.tile_floor();
        Ok(
            match self.next_map_route(origin.tile_floor(), target_tile, kind, player) {
                Some(MapRoutePlan::Outcome(MovementOutcome::Path(path))) => {
                    let (waypoint, route) = route_waypoint(path.tiles, origin, destination)?;
                    (waypoint, route, None, false, false)
                }
                Some(MapRoutePlan::Segment(path, planner)) => {
                    let (waypoint, route) = route_waypoint(path.tiles, origin, destination)?;
                    (waypoint, route, planner, false, false)
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::InvalidDestination)) => {
                    return Err(GameWorldError::InvalidPosition);
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::Unreachable)) => {
                    return Err(GameWorldError::Unreachable);
                }
                Some(MapRoutePlan::Outcome(MovementOutcome::BudgetExceeded))
                | Some(MapRoutePlan::SearchLimit) => {
                    return Err(GameWorldError::PathBudgetExceeded);
                }
                Some(MapRoutePlan::Pending(planner, fast_tried)) => {
                    (origin, VecDeque::new(), planner, true, fast_tried)
                }
                Some(MapRoutePlan::Environment(error)) => {
                    return Err(GameWorldError::Environment(error));
                }
                None => (
                    next_waypoint(origin, target_tile, destination),
                    VecDeque::new(),
                    None,
                    false,
                    false,
                ),
            },
        )
    }
}
