use crate::{
    GameWorld, GameWorldError, MovementOrder, game_path::segment_length, game_world::facing_for,
};
use aoe_core::WorldPosition;
use std::collections::VecDeque;

impl GameWorld {
    /// Returns the retained fixed-route queue length for diagnostics.
    pub fn queued_waypoint_count(&self, id: aoe_core::EntityId) -> Option<usize> {
        self.lookup
            .get(&id)
            .map(|index| self.units[*index].route.len())
    }

    /// Issues one predetermined tile-center route as a single movement order.
    ///
    /// Every waypoint and tile edge is validated before state changes. The
    /// remaining waypoints are retained in the unit's existing route queue so
    /// segment transitions use the same fractional speed-carry loop as an
    /// ordinary map route.
    pub fn issue_move_waypoints(
        &mut self,
        id: aoe_core::EntityId,
        waypoints: &[WorldPosition],
    ) -> Result<bool, GameWorldError> {
        let index = *self.lookup.get(&id).ok_or(GameWorldError::UnknownEntity)?;
        if waypoints.is_empty() {
            return Err(GameWorldError::InvalidPosition);
        }
        let destination = *waypoints.last().ok_or(GameWorldError::InvalidPosition)?;
        let origin = self.units[index].state.position;
        self.validate_fixed_waypoints(origin, waypoints)?;

        let speed_carry = self.units[index].order.map_or(0, |order| order.speed_carry);
        self.clear_planner(index);
        self.units[index].last_movement_error = None;
        let start_index = waypoints
            .iter()
            .position(|waypoint| *waypoint != origin)
            .unwrap_or(waypoints.len());
        if start_index == waypoints.len() {
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].state.moving = false;
            self.units[index].state.planning = false;
            return Ok(false);
        }

        let waypoint = waypoints[start_index];
        let route = waypoints[start_index + 1..]
            .iter()
            .map(|position| position.tile_floor())
            .collect::<VecDeque<_>>();
        let target_tile = destination.tile_floor();
        let dx = i64::from(waypoint.x) - i64::from(origin.x);
        let dy = i64::from(waypoint.y) - i64::from(origin.y);
        self.units[index].state.previous_position = origin;
        self.units[index].state.facing = facing_for(dx, dy, self.units[index].state.facing);
        self.units[index].state.moving = true;
        self.units[index].state.planning = false;
        self.units[index].route = route;
        self.units[index].order = Some(MovementOrder {
            origin,
            destination,
            waypoint,
            target_tile,
            segment_length: segment_length(dx, dy),
            travelled: 0,
            speed_carry,
        });
        if self.active_movers.binary_search(&id).is_err() {
            let insert_at = self
                .active_movers
                .binary_search(&id)
                .unwrap_or_else(|index| index);
            self.active_movers.insert(insert_at, id);
        }
        Ok(true)
    }

    fn validate_fixed_waypoints(
        &self,
        origin: WorldPosition,
        waypoints: &[WorldPosition],
    ) -> Result<(), GameWorldError> {
        let mut previous = origin;
        for (index, waypoint) in waypoints.iter().copied().enumerate() {
            let tile = waypoint.tile_floor();
            if waypoint != WorldPosition::from_tile_center(tile)?
                || !self.config.valid_ground_position(waypoint)
                || !self
                    .terrain
                    .passable_with_cancel(tile, self.config, &|| false)?
            {
                return Err(GameWorldError::InvalidPosition);
            }
            if index > 0 && previous == waypoint {
                return Err(GameWorldError::InvalidPosition);
            }
            if previous != waypoint {
                self.validate_fixed_edge(previous, waypoint)?;
            }
            previous = waypoint;
        }
        Ok(())
    }

    fn validate_fixed_edge(
        &self,
        from: WorldPosition,
        to: WorldPosition,
    ) -> Result<(), GameWorldError> {
        let start = from.tile_floor();
        let end = to.tile_floor();
        let dx = i64::from(end.x) - i64::from(start.x);
        let dy = i64::from(end.y) - i64::from(start.y);
        let steps = dx.unsigned_abs().max(dy.unsigned_abs());
        if steps == 0 {
            return Ok(());
        }
        let mut previous = start;
        for step in 1..=steps {
            let tile = aoe_core::TileCoord::new(
                start.x
                    + i32::try_from(dx * i64::try_from(step).unwrap_or(i64::MAX) / steps as i64)
                        .unwrap_or(i32::MAX),
                start.y
                    + i32::try_from(dy * i64::try_from(step).unwrap_or(i64::MAX) / steps as i64)
                        .unwrap_or(i32::MAX),
            );
            if !self
                .terrain
                .passable_with_cancel(tile, self.config, &|| false)?
                || !self
                    .terrain
                    .crossable_with_cancel(previous, tile, self.config, &|| false)?
            {
                return Err(GameWorldError::InvalidPosition);
            }
            previous = tile;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig};

    fn centered(tile: TileCoord) -> WorldPosition {
        WorldPosition::from_tile_center(tile).expect("tile center")
    }

    #[test]
    fn waypoint_orders_require_tile_centers_and_walkable_edges() {
        let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
        let (mut world, _) = GameWorld::with_cavalry(config).expect("world");
        let unit = world
            .spawn_unit(PlayerId(0), centered(TileCoord::new(10, 10)))
            .expect("unit");
        let off_center = WorldPosition::new(11_000, 10_512);
        assert!(matches!(
            world.issue_move_waypoints(unit, &[off_center, centered(TileCoord::new(12, 10))]),
            Err(GameWorldError::InvalidPosition)
        ));
        assert!(matches!(
            world.issue_move_waypoints(
                unit,
                &[
                    centered(TileCoord::new(11, 10)),
                    centered(TileCoord::new(11, 10))
                ],
            ),
            Err(GameWorldError::InvalidPosition)
        ));
        assert!(world.movement_order(unit).is_none());
    }

    #[test]
    fn one_order_retains_the_final_destination_and_remaining_route() {
        let config = WorldConfig::new(64, 64, Seed(7)).expect("config");
        let (mut world, _) = GameWorld::with_cavalry(config).expect("world");
        let unit = world
            .spawn_unit(PlayerId(0), centered(TileCoord::new(10, 10)))
            .expect("unit");
        let waypoints = [
            centered(TileCoord::new(12, 10)),
            centered(TileCoord::new(10, 10)),
            centered(TileCoord::new(12, 10)),
        ];

        assert!(
            world
                .issue_move_waypoints(unit, &waypoints)
                .expect("waypoint route")
        );
        let index = world.lookup[&unit];
        let order = world.movement_order(unit).expect("order");
        assert_eq!(order.waypoint, waypoints[0]);
        assert_eq!(order.destination, waypoints[2]);
        assert_eq!(
            world.units[index].route,
            VecDeque::from([waypoints[1].tile_floor(), waypoints[2].tile_floor()])
        );
        assert_eq!(world.active_mover_count(), 1);
    }
}
