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
            .filter(|member| !member.detached && !member.arrived && member.blocked >= DETACH_AFTER)
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
        let goal = group.members[at].goal;
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
        self.units[index].obstruction = None;
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
    }

    pub(super) fn rejoin_detached_members(&mut self) {
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
                let timed_out = !navigating
                    && (self.units[index]
                        .obstruction
                        .is_some_and(|o| o.blocked >= GIVE_UP_AFTER_TICKS)
                        || (self.units[index].order.is_none() && !unit.planning));
                let near = path::squared(unit.position, member.ideal) <= (2 * 1024_i64).pow(2)
                    && self.body_step_clear(
                        unit.kind,
                        unit.position,
                        path::step(unit.position, member.ideal, 64),
                    );
                if unit.position == member.goal || near || timed_out {
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
