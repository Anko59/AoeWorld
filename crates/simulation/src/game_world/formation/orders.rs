use super::*;
use crate::game_path::segment_length;

impl GameWorld {
    /// One route, one scheduler slot and one bounded waypoint FIFO per army.
    #[track_caller]
    pub fn issue_formation_move(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        form: Formation,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        let named = ids.len();
        let (ids, centroid) = self.formation_members(player, ids).inspect_err(|_| {
            self.route_metrics.attribution.group_orders_rejected += 1;
        })?;
        self.route_metrics.attribution.group_members_left_out += (named - ids.len()) as u64;
        if !self.config.valid_map_position(destination) {
            return Err(GameWorldError::InvalidPosition.into());
        }
        let destination = self.config.snap_ground_position(destination);
        if !self
            .unit_tile_passable(UnitKind::Militia, destination.tile_floor(), &|| false)
            .map_err(GameWorldError::Environment)?
            || self
                .economy
                .blocked_tiles
                .contains(&destination.tile_floor())
        {
            return Err(GameWorldError::InvalidPosition.into());
        }
        let leader = ids[0];
        let existing = self.units[self.lookup[&leader]].formation.unwrap_or(leader);
        if queued
            && let Some(group) = self.formations.get_mut(&existing)
            && group.members.iter().map(|m| m.id).eq(ids.iter().copied())
            && !group.settled
        {
            if group.queue.len() + 1 >= MAX_MOVE_DESTINATIONS {
                return Err(EconomyError::OrderTooLarge);
            }
            group.queue.push_back((destination, attack_move));
            for &id in &ids {
                self.vassal_player_override(id);
            }
            return Ok(());
        }
        let origin = self.formation_origin(&ids, centroid)?;
        let index = self.lookup[&leader];
        let previous = self.take_planner(index);
        self.route_metrics.attribution.group_orders += 1;
        let planned = self.order_route(index, origin, destination);
        let (waypoint, route, planner, planning, fast_tried) = match planned {
            Ok(plan) => plan,
            Err(error) => {
                self.store_planner(index, previous);
                return Err(error.into());
            }
        };
        let (waypoint, route) = if self.has_route_navigation() {
            (waypoint, route)
        } else {
            (destination, VecDeque::new())
        };
        // Validation and the shared route preparation precede every task change.
        self.leave_groups(&ids);
        for &id in &ids {
            let i = self.lookup[&id];
            self.stop_movement(i);
            self.stop_gathering(id);
            self.cancel_attack(id);
            self.cancel_auto_scout(id);
            self.move_queues.remove(&id);
            self.economy.builders.remove(&id);
            self.units[i].formation = Some(leader);
            self.units[i].state.task = UnitTask::Moving;
            self.units[i].state.moving = true;
            self.units[i].last_movement_error = None;
            if let Ok(at) = self.active_movers.binary_search(&id) {
                self.active_movers.remove(at);
            }
            self.refresh_spacing_order(id);
        }
        let heading = WorldPosition::new(destination.x - origin.x, destination.y - origin.y);
        let heading = if heading == WorldPosition::new(0, 0) {
            WorldPosition::new(1024, 0)
        } else {
            heading
        };
        let facing = facing_for(i64::from(heading.x), i64::from(heading.y), Facing::South);
        let positions = ids
            .iter()
            .map(|id| {
                let unit = self.units[self.lookup[id]].state;
                (unit.kind, unit.position)
            })
            .collect::<Vec<_>>();
        let (diameter, step) = ids.iter().fold((0, 0), |(diameter, step), id| {
            let unit = self.units[self.lookup[id]].state;
            let (speed, denominator) = self.movement_speed(unit.kind, unit.player);
            (
                diameter.max(unit.kind.collision_radius() * 2),
                step.max(speed.div_ceil(denominator).min(512) as i32),
            )
        });
        // Hard bodies need room for a late member to pass between occupied slots,
        // plus one snapshot step. Narrow files use just diameter plus a step.
        let gap = diameter * 2 + step + 16;
        let members = ids
            .iter()
            .copied()
            .map(|id| Member {
                id,
                offset: WorldPosition::new(0, 0),
                start_offset: WorldPosition::new(0, 0),
                goal: destination,
                target: origin,
                center: origin,
                ideal: origin,
                target_progress: 0,
                carry: 0,
                rank: 0,
                cursor: 0,
                progress: 0,
                blocked: 0,
                detour_progress: 0,
                arrived: false,
                detached: false,
                following_corridor: false,
                stalled: 0,
                retries: 0,
                remaining: i64::MAX,
            })
            .collect();
        let mut group = Group {
            form,
            members,
            anchor: origin,
            heading,
            travel_heading: heading,
            facing,
            speed: (0, 1),
            pace: (0, 1),
            settled: false,
            direct: self.direct_order(origin, destination, &positions),
            gap,
            narrow_gap: diameter + step + 16,
            slots_tight: false,
            packing_files: ids.len(),
            long_files: false,
            form_start: 0,
            form_distance: 0,
            half_width: 0,
            static_area: None,
            destination,
            queue: VecDeque::new(),
            attack_move,
            path: path::Path::new(
                std::iter::once(origin)
                    .chain(std::iter::once(waypoint))
                    .chain(
                        route
                            .iter()
                            .filter_map(|&tile| WorldPosition::from_tile_center(tile).ok()),
                    ),
                self.group_work.clone(),
            ),
            taut_end: None,
            progress: 0,
            path_cursor: 0,
        };
        group.speed = self.group_speed(&group).0;
        group.pace = group.speed;
        self.assign_group_slots(&mut group);
        self.formations.insert(leader, group);
        self.install_formation_route(
            index,
            origin,
            destination,
            waypoint,
            route,
            planner,
            planning,
            fast_tried,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_formation_route(
        &mut self,
        index: usize,
        origin: WorldPosition,
        destination: WorldPosition,
        waypoint: WorldPosition,
        route: VecDeque<TileCoord>,
        planner: Option<RoutePlanner>,
        planning: bool,
        fast_tried: bool,
    ) {
        let dx = i64::from(waypoint.x) - i64::from(origin.x);
        let dy = i64::from(waypoint.y) - i64::from(origin.y);
        self.units[index].order = Some(MovementOrder {
            origin,
            destination,
            waypoint,
            target_tile: destination.tile_floor(),
            segment_length: segment_length(dx, dy),
            travelled: 0,
            speed_carry: 0,
        });
        self.units[index].route = route
            .into_iter()
            .filter_map(|t| WorldPosition::from_tile_center(t).ok())
            .collect();
        self.units[index].state.planning = planning;
        self.units[index].planning_since = self.tick.0;
        self.units[index].fast_route_tried = fast_tried;
        self.store_planner(index, planner);
        let id = self.units[index].state.id;
        if let Err(at) = self.active_movers.binary_search(&id) {
            self.active_movers.insert(at, id);
        }
    }

    pub(super) fn group_speed(&self, group: &Group) -> ((u64, u64), bool) {
        let end = group.path.points.last().copied().unwrap_or(group.anchor);
        let mut front_near_end = path::squared(group.anchor, end) <= (4 * 1024_i64).pow(2);
        let speed = group
            .members
            .iter()
            .inspect(|_| self.group_work.members(1))
            .filter_map(|m| self.unit(m.id).map(|u| (m, u)))
            .map(|(member, u)| {
                front_near_end |= member.following_corridor
                    && path::squared(u.position, end) <= (4 * 1024_i64).pow(2);
                let own = self.movement_speed(u.kind, u.player);
                let pace = self
                    .general_squad_pace(u.id)
                    .filter(|pace| {
                        u128::from(pace.0) * u128::from(own.1)
                            < u128::from(own.0) * u128::from(pace.1)
                    })
                    .unwrap_or(own);
                self.road_speed(u.kind, u.position, pace)
            })
            .min_by(|a, b| {
                (u128::from(a.0) * u128::from(b.1)).cmp(&(u128::from(b.0) * u128::from(a.1)))
            })
            .unwrap_or((0, 1));
        (speed, front_near_end)
    }

    pub(super) fn next_formation_order(&mut self, leader: EntityId, group: &mut Group) {
        let index = self.lookup[&leader];
        while let Some((destination, attack_move)) = group.queue.pop_front() {
            if let Ok((waypoint, route, planner, planning, fast_tried)) =
                self.order_route(index, group.anchor, destination)
            {
                let (waypoint, route) = if self.has_route_navigation() {
                    (waypoint, route)
                } else {
                    (destination, VecDeque::new())
                };
                group.destination = destination;
                group.attack_move = attack_move;
                group.heading = WorldPosition::new(
                    destination.x - group.anchor.x,
                    destination.y - group.anchor.y,
                );
                if group.heading == WorldPosition::new(0, 0) {
                    group.heading = group.travel_heading;
                }
                group.travel_heading = group.heading;
                group.facing = facing_for(
                    i64::from(group.heading.x),
                    i64::from(group.heading.y),
                    group.facing,
                );
                let positions = group
                    .members
                    .iter()
                    .map(|m| {
                        let unit = self.units[self.lookup[&m.id]].state;
                        (unit.kind, unit.position)
                    })
                    .collect::<Vec<_>>();
                group.direct = self.direct_order(group.anchor, destination, &positions);
                group.path = path::Path::new(
                    std::iter::once(group.anchor)
                        .chain(std::iter::once(waypoint))
                        .chain(
                            route
                                .iter()
                                .filter_map(|&tile| WorldPosition::from_tile_center(tile).ok()),
                        ),
                    self.group_work.clone(),
                );
                group.progress = 0;
                group.path_cursor = 0;
                group.taut_end = None;
                self.reset_group_detours(group);
                self.assign_group_slots(group);
                group.settled = false;
                self.install_formation_route(
                    index,
                    group.anchor,
                    destination,
                    waypoint,
                    route,
                    planner,
                    planning,
                    fast_tried,
                );
                return;
            }
        }
    }
}

/// Members of a direct order steer straight at their slots with no route of
/// their own, so it is only direct when every straight line is clear.
const DIRECT_LINE_TILES: i32 = 64;

impl GameWorld {
    fn direct_order(
        &self,
        origin: WorldPosition,
        destination: WorldPosition,
        members: &[(UnitKind, WorldPosition)],
    ) -> bool {
        let travel = path::distance(origin, destination);
        let (mut min, mut max) = (origin, origin);
        for &(_, p) in members {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        (travel < 20 * 1024 || path::distance(min, max) > travel)
            && members
                .iter()
                .all(|&(kind, from)| self.straight_line_clear(kind, from, destination))
    }

    /// Static terrain, trees and buildings along the half-tile steps of a
    /// straight walk; at most `DIRECT_LINE_TILES` long, longer is not clear.
    fn straight_line_clear(&self, kind: UnitKind, from: WorldPosition, to: WorldPosition) -> bool {
        let length = path::distance(from, to);
        if length > i64::from(DIRECT_LINE_TILES * 1024) {
            return false;
        }
        let parts = (length / 512).max(1);
        let (dx, dy) = (i64::from(to.x - from.x), i64::from(to.y - from.y));
        let mut previous = from;
        for n in 1..=parts {
            self.group_work.members(1);
            let next = WorldPosition::new(
                from.x + (dx * n / parts) as i32,
                from.y + (dy * n / parts) as i32,
            );
            // Terrain and trees come from the step check; buildings block tiles
            // only in the economy, which that check does not read.
            if self.economy.blocked_tiles.contains(&next.tile_floor())
                || !self
                    .unit_position_step_clear(kind, previous, next)
                    .unwrap_or(false)
            {
                return false;
            }
            previous = next;
        }
        true
    }
}
