//! Only persistently blocked members ask the fair scheduler for their own
//! route. Their destination slot and group membership survive the detour.
use super::*;
use crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS;

const DETACH_AFTER: u16 = 120;

impl GameWorld {
    pub(super) fn reset_group_detours(&mut self, group: &mut Group) {
        for member in group.members.iter_mut().filter(|member| member.detached) {
            let index = self.lookup[&member.id];
            self.clear_planner(index);
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].obstruction = None;
            self.units[index].last_movement_error = None;
            self.units[index].state.planning = false;
            if let Ok(at) = self.active_movers.binary_search(&member.id) {
                self.active_movers.remove(at);
            }
            member.detached = false;
        }
    }

    /// The group a member marches with in single file, when it does: its
    /// files squeeze past each other at the friendly hard distance, so a knot
    /// at the mouth of a one-lane passage cannot lock the whole column.
    pub(crate) fn single_file_group(&self, index: usize) -> Option<EntityId> {
        let unit = &self.units[index];
        let leader = unit.formation?;
        let group = self.formations.get(&leader)?;
        (group.packing_files == 1
            && group.members.len() > 1
            && group
                .member_index(unit.state.id)
                .is_some_and(|at| !group.members[at].detached && !group.members[at].arrived))
        .then_some(leader)
    }

    /// A grouped member's current steering target, uncounted: traffic
    /// queries read it without charging group work.
    pub(crate) fn formation_member_target(&self, index: usize) -> Option<WorldPosition> {
        let unit = &self.units[index];
        let group = self.formations.get(&unit.formation?)?;
        let member = &group.members[group.member_index(unit.state.id)?];
        (!member.detached).then_some(member.target)
    }

    pub(crate) fn is_detached_member(&self, index: usize) -> bool {
        let unit = &self.units[index];
        unit.formation
            .and_then(|leader| self.formations.get(&leader))
            .and_then(|group| {
                group
                    .member_index(unit.state.id)
                    .map(|at| group.members[at].detached)
            })
            .unwrap_or(false)
    }

    pub(super) fn detach_blocked_members(&mut self) {
        let ids = self
            .formations
            .values()
            .filter(|group| !group.settled)
            .flat_map(|group| group.members.iter())
            .inspect(|_| self.group_work.members(1))
            .filter(|member| {
                !member.detached
                    && !member.arrived
                    && (member.blocked >= DETACH_AFTER || member.stalled >= 60 || {
                        let index = self.lookup[&member.id];
                        let unit = &self.units[index];
                        unit.state.position == unit.state.previous_position
                            && unit.obstruction.is_some_and(|o| o.blocked >= 32)
                            && self.stationary_external_traffic(index)
                    })
            })
            .map(|member| member.id)
            .take(4)
            .collect::<Vec<_>>();
        for id in ids {
            self.detach_member(id);
        }
    }

    fn detach_member(&mut self, id: EntityId) {
        let index = self.lookup[&id];
        let Some(leader) = self.units[index].formation else {
            return;
        };
        let group = &self.formations[&leader];
        let Some(at) = group.member_index(id) else {
            return;
        };
        let member = &group.members[at];
        let mut cursor = member.cursor;
        let goal = if self.units[self.lookup[&leader]].order.is_some() {
            group
                .path
                .point(
                    (member.progress + 8 * 1024).min(group.path.end()),
                    &mut cursor,
                )
                .0
        } else {
            member.goal
        };
        let replacement = group
            .members
            .iter()
            .find(|member| member.id != id && !member.detached)
            .map(|member| member.id);
        if let Some(group) = self.formations.get_mut(&leader) {
            group.members[at].blocked = 0;
        }
        if id == leader {
            let Some(replacement) = replacement else {
                return;
            };
            let Some(group) = self.formations.remove(&leader) else {
                return;
            };
            self.hand_over_route(leader, replacement);
            for member in &group.members {
                self.units[self.lookup[&member.id]].formation = Some(replacement);
            }
            self.formations.insert(replacement, group);
        }
        let origin = self.units[index].state.position;
        let previous = self.take_planner(index);
        let Ok((waypoint, route, planner, planning, fast_tried)) =
            self.order_route(index, origin, goal)
        else {
            self.store_planner(index, previous);
            return;
        };
        self.install_formation_route(
            index, origin, goal, waypoint, route, planner, planning, fast_tried,
        );
        self.units[index].state.planning = planning;
        self.units[index].state.moving = !planning;
        self.units[index].state.task = UnitTask::Moving;
        // Recovery must retain the traffic wait already earned. Clearing it
        // here makes a member wait for friendly passing permission twice.
        if let Some(obstruction) = &mut self.units[index].obstruction {
            obstruction.goal = goal;
        }
        self.units[index].last_movement_error = None;
        let Some(group) = self.units[index]
            .formation
            .and_then(|leader| self.formations.get_mut(&leader))
        else {
            return;
        };
        let Some(at) = group.member_index(id) else {
            return;
        };
        group.members[at].detached = true;
        group.members[at].following_corridor = true;
    }

    pub(crate) fn rejoin_detached_members(&mut self) {
        let mut ready = Vec::new();
        for (&leader, group) in &self.formations {
            if group.settled {
                continue;
            }
            let navigating = self.units[self.lookup[&leader]].order.is_some();
            for member in group
                .members
                .iter()
                .inspect(|_| self.group_work.members(1))
                .filter(|member| member.detached)
            {
                self.group_work.members(1);
                let index = self.lookup[&member.id];
                let unit = self.units[index].state;
                // A traffic wait is not an unreachable verdict. In particular,
                // successful completion of an intermediate checkpoint must
                // rejoin the corridor even after the carrier has arrived.
                let timed_out = !navigating && self.units[index].last_movement_error.is_some();
                let near = self.units[index].order.is_some_and(|order| {
                    navigating && path::squared(unit.position, order.destination) <= 512_i64.pow(2)
                }) || (!navigating
                    && path::squared(unit.position, member.goal) <= 512_i64.pow(2)
                    && self.clear_edge(index, unit.position, member.goal, false));
                let ended = self.units[index].order.is_none() && !unit.planning;
                if unit.position == member.goal || near || ended || timed_out {
                    ready.push((member.id, timed_out));
                }
            }
        }
        for (id, timed_out) in ready {
            let index = self.lookup[&id];
            self.clear_planner(index);
            self.units[index].order = None;
            self.units[index].route.clear();
            self.units[index].obstruction = None;
            self.units[index].last_movement_error = None;
            self.units[index].state.planning = false;
            if let Ok(at) = self.active_movers.binary_search(&id) {
                self.active_movers.remove(at);
            }
            let Some(group) = self.units[index]
                .formation
                .and_then(|leader| self.formations.get_mut(&leader))
            else {
                continue;
            };
            let Some(at) = group.member_index(id) else {
                continue;
            };
            let member = &mut group.members[at];
            member.detached = false;
            member.blocked = 0;
            member.stalled = if timed_out { GIVE_UP_AFTER_TICKS } else { 0 };
            member.arrived = self.units[index].state.position == member.goal;
            self.units[index].state.moving = !member.arrived && !timed_out;
            self.units[index].state.task = if self.units[index].state.moving {
                UnitTask::Moving
            } else {
                UnitTask::Idle
            };
        }
    }
}
