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
        // Every refusal counts: members, destination and the route's issue.
        self.formation_move(player, ids, form, destination, queued, attack_move)
            .inspect_err(|_| self.route_metrics.attribution.group_orders_rejected += 1)
    }

    #[track_caller]
    fn formation_move(
        &mut self,
        player: PlayerId,
        ids: &[EntityId],
        form: Formation,
        destination: WorldPosition,
        queued: bool,
        attack_move: bool,
    ) -> Result<(), EconomyError> {
        let named = ids.len();
        let (ids, centroid) = self.formation_members(player, ids)?;
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
        for &id in &ids {
            self.demand_attempt(id, destination);
        }
        let leader = ids[0];
        let existing = self.units[self.lookup[&leader]].formation.unwrap_or(leader);
        if !queued
            && self.formations.get(&existing).is_some_and(|group| {
                !group.settled
                    && group.form == form
                    && group.attack_move == attack_move
                    // Combat firing points are contacts, like individual
                    // attack approaches; a nearby point may enter range.
                    && if attack_move {
                        group.destination == destination
                    } else {
                        near_goal(group.destination, destination)
                    }
                    && group.members.iter().map(|m| m.id).eq(ids.iter().copied())
            })
        {
            return Ok(());
        }
        let member_pending = if queued {
            ids.iter()
                .filter_map(|id| self.move_queues.get(id))
                .map(|queue| queue.len())
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        if queued
            && member_pending > 0
            && let Some(group) = self.formations.get(&existing)
            && group.members.iter().map(|m| m.id).eq(ids.iter().copied())
            && !group.settled
        {
            // A group leg would run before a member's earlier scalar legs.
            // Keep every member's order by appending to the scalar FIFOs,
            // which carry the attack-move flag of each leg.
            let pending = group.queue.len();
            let width = Self::move_queue_width();
            if ids.iter().any(|id| {
                self.move_queues.get(id).map_or(0, |queue| queue.len()) + 1 + pending >= width
            }) {
                return Err(EconomyError::OrderTooLarge);
            }
            for &id in &ids {
                self.move_queues
                    .entry(id)
                    .or_default()
                    .push_back((destination, attack_move));
                self.vassal_player_override(id);
            }
            return Ok(());
        }
        if queued
            && let Some(group) = self.formations.get_mut(&existing)
            && group.members.iter().map(|m| m.id).eq(ids.iter().copied())
            && !group.settled
        {
            if group.queue.len() + 1 + member_pending >= MAX_MOVE_DESTINATIONS {
                return Err(EconomyError::OrderTooLarge);
            }
            group.queue.push_back((destination, attack_move));
            group.direct = false;
            for &id in &ids {
                self.vassal_player_override(id);
            }
            return Ok(());
        }
        let origin = self.formation_origin(&ids, centroid)?;
        let requested_destination = destination;
        let destination = self.formation_goal(player, origin, destination, form, &ids)?;
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
        self.route_metrics.attribution.group_stopped_short +=
            u64::from(destination != requested_destination);
        // Validation and the shared route preparation precede every task change.
        for &id in &ids {
            self.demand_accepted(id);
        }
        self.leave_groups(&ids);
        for &id in &ids {
            let i = self.lookup[&id];
            self.stop_movement(i);
            self.stop_gathering(id);
            self.cancel_attack(id);
            self.cancel_auto_scout(id);
            self.discard_move_queue_for_order(id);
            self.economy.builders.remove(&id);
            self.units[i].formation = Some(leader);
            self.units[i].state.task = UnitTask::Moving;
            self.units[i].state.moving = true;
            self.set_movement_error(i, None);
            self.active_movers.remove(id);
            self.refresh_spacing_order(id);
        }
        let heading = WorldPosition::new(destination.x - origin.x, destination.y - origin.y);
        let heading = if heading == WorldPosition::new(0, 0) {
            WorldPosition::new(1024, 0)
        } else {
            heading
        };
        let facing = facing_for(i64::from(heading.x), i64::from(heading.y), Facing::South);
        let (gap, narrow_gap) = self.formation_gaps(&ids);
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
            speed_limit: None,
            settled: false,
            direct: false,
            gap,
            narrow_gap,
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
        group.direct = self.direct_order(origin, destination, &group);
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
        self.active_movers.insert(id);
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
                let pace = group.limited_speed(own);
                self.road_speed(u.kind, u.position, pace)
            })
            .min_by(|a, b| {
                (u128::from(a.0) * u128::from(b.1)).cmp(&(u128::from(b.0) * u128::from(a.1)))
            })
            .unwrap_or((0, 1));
        (speed, front_near_end)
    }

    /// The carrier of `leader`'s group proved `target` unreachable. Rather
    /// than leave every member steering at its slot across the water, the
    /// group's next leg goes to `nearest` (the reachable tile nearest the
    /// destination that the search proved; the anchor when it proved none),
    /// where the group forms. False when a queued waypoint follows, or the
    /// failed leg already aimed there (so it cannot repeat).
    pub(crate) fn stop_group_short(
        &mut self,
        leader: EntityId,
        target: TileCoord,
        nearest: Option<TileCoord>,
    ) -> bool {
        let Some(group) = self.formations.get_mut(&leader) else {
            return false;
        };
        let stop = nearest.unwrap_or_else(|| group.anchor.tile_floor());
        if !group.queue.is_empty() || stop == target {
            return false;
        }
        let Ok(stop) = WorldPosition::from_tile_center(stop) else {
            return false;
        };
        group.queue.push_front((stop, group.attack_move));
        true
    }

    pub(super) fn next_formation_order(&mut self, leader: EntityId, group: &mut Group) {
        let index = self.lookup[&leader];
        let player = self.units[index].state.player;
        while let Some((destination, attack_move)) = group.queue.pop_front() {
            let ids: Vec<_> = group.members.iter().map(|member| member.id).collect();
            let requested_destination = destination;
            let Ok(destination) =
                self.formation_goal(player, group.anchor, destination, group.form, &ids)
            else {
                continue;
            };
            if let Ok((waypoint, route, planner, planning, fast_tried)) =
                self.order_route(index, group.anchor, destination)
            {
                self.route_metrics.attribution.group_stopped_short +=
                    u64::from(destination != requested_destination);
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
                // Rear members still need the bends behind this waypoint.
                // Extend the common distance frame instead of extrapolating
                // a new first edge backwards through the old wall or river.
                let cursors: Vec<_> = group.members.iter().map(|m| (m.cursor, m.carry)).collect();
                self.group_work.members(group.members.len() as u64);
                group.path.continue_from(
                    group.progress,
                    group.anchor,
                    std::iter::once(waypoint).chain(
                        route
                            .iter()
                            .filter_map(|&tile| WorldPosition::from_tile_center(tile).ok()),
                    ),
                );
                group.path_cursor = group.path.cursor_at(group.progress);
                group.taut_end = None;
                self.reset_group_detours(group);
                self.assign_group_slots(group);
                for (member, (cursor, carry)) in group.members.iter_mut().zip(cursors) {
                    self.group_work.members(1);
                    member.cursor = cursor;
                    let position = self.units[self.lookup[&member.id]].state.position;
                    member.progress = group.path.relocate(position, &mut member.cursor);
                    member.detour_progress = member.progress;
                    member.carry = carry;
                    member.following_corridor = true;
                }
                group.direct =
                    group.queue.is_empty() && self.direct_order(group.anchor, destination, group);
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
/// their own, so it is only direct when every straight line is clear: each
/// member's line to its assigned slot, and to the destination centre.
const DIRECT_LINE_TILES: i32 = 64;

impl GameWorld {
    /// Called once the group's slots are assigned (`member.goal`).
    fn direct_order(
        &self,
        origin: WorldPosition,
        destination: WorldPosition,
        group: &Group,
    ) -> bool {
        let members = group
            .members
            .iter()
            .map(|m| {
                let unit = self.units[self.lookup[&m.id]].state;
                (unit.kind, unit.position, m.goal)
            })
            .collect::<Vec<_>>();
        let travel = path::distance(origin, destination);
        let (mut min, mut max) = (origin, origin);
        for &(_, p, _) in &members {
            min.x = min.x.min(p.x);
            min.y = min.y.min(p.y);
            max.x = max.x.max(p.x);
            max.y = max.y.max(p.y);
        }
        (travel < 20 * 1024 || path::distance(min, max) > travel)
            && members.iter().all(|&(kind, from, slot)| {
                self.straight_line_clear(kind, from, slot)
                    && self.straight_line_clear(kind, from, destination)
            })
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
