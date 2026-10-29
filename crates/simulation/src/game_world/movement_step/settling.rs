//! Hysteresis counts obstruction until real progress, never sidestep distance.
use crate::{GameWorld, GameWorldError, UnitTask};
use aoe_core::WorldPosition;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Obstruction {
    pub(crate) goal: WorldPosition,
    pub(crate) remaining: u32,
    pub(crate) blocked: u16,
    pub(crate) side: i8,
    pub(crate) last_tick: u64,
    pub(crate) search_tick: u64,
    /// Until this tick the mover may overlap other bodies down to their hard
    /// distance (`collision::hard_distance`).
    pub(crate) pass_until: u64,
}

/// Obstructed ticks without progress before a mover may overlap other bodies
/// down to their hard distance, as AoE2 units shuffle past each other instead
/// of waiting in a gridlock (design 4.3). 32 ticks is 1.6 s: briefly blocked
/// traffic still waits for the gap, a head-on pair separates at once.
pub(crate) const PASS_AFTER_TICKS: u16 = 32;
/// How long one squeeze lasts once granted, so the mover clears the crowd
/// rather than colliding again after its first half tile of progress.
const PASS_TICKS: u64 = 40;
/// Obstructed ticks without progress after which a mover that cannot reach
/// its goal gives up (12 s at 20 Hz), so a permanently boxed-in unit does not
/// stay an active mover forever. The 160-tick
/// waits in the congestion tests and a normal queue all end well before it.
pub(crate) const GIVE_UP_AFTER_TICKS: u16 = 240;

fn distance(a: WorldPosition, b: WorldPosition) -> u32 {
    crate::game_path::segment_length(
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    )
}

impl GameWorld {
    /// Read-only congestion census; the profiler never changes retry state.
    pub fn movement_obstruction_ticks(&self, id: aoe_core::EntityId) -> Option<u16> {
        self.units
            .get(*self.lookup.get(&id)?)?
            .obstruction
            .map(|o| o.blocked)
    }

    /// Work traffic may leave a narrow pocket near its goal, but never
    /// compress into a physically occupied work endpoint.
    pub(super) fn free_work_approach(&self, index: usize, goal: WorldPosition) -> bool {
        let id = self.units[index].state.id;
        match self.work_approach_clearance {
            Some((cached, at, clear)) if cached == id && at == goal => clear,
            _ => self.has_work_task(id) && !self.local_position_occupied(index, goal),
        }
    }

    /// No other body moves during this unit's sequential turn. Clear the
    /// derived answer before the next mover and before formation/spacing work.
    pub(crate) fn cache_work_approach(&mut self, index: usize) {
        let unit = self.units[index].state;
        self.melee_traffic_clearance = unit
            .attack_target
            .and_then(|target| self.attack_goal(target, unit.player))
            .map(|goal| {
                (
                    unit.id,
                    unit.position,
                    self.melee_contact_traffic(unit.id, &goal),
                )
            });
        self.work_approach_clearance = self.units[index]
            .obstruction
            .filter(|o| o.pass_until > self.tick.0 && distance(unit.position, o.goal) <= 2048)
            .map(|o| {
                (
                    unit.id,
                    o.goal,
                    self.has_work_task(unit.id) && !self.local_position_occupied(index, o.goal),
                )
            });
    }

    pub(crate) fn record_unit_obstruction(&mut self, index: usize) {
        let Some(order) = self.units[index].order else {
            return;
        };
        self.record_obstruction_towards(index, order.destination);
    }

    pub(crate) fn record_obstruction_towards(&mut self, index: usize, goal: WorldPosition) {
        let unit = &mut self.units[index];
        let obstruction = unit.obstruction.get_or_insert(Obstruction {
            goal,
            remaining: distance(unit.state.position, goal),
            blocked: 0,
            side: 1,
            last_tick: u64::MAX,
            search_tick: u64::MAX,
            pass_until: 0,
        });
        if obstruction.last_tick != self.tick.0 {
            if obstruction.blocked > 0 && obstruction.blocked.is_multiple_of(48) {
                obstruction.side = -obstruction.side;
            }
            obstruction.blocked = obstruction.blocked.saturating_add(1);
            obstruction.last_tick = self.tick.0;
            if obstruction.blocked >= PASS_AFTER_TICKS {
                obstruction.pass_until = self.tick.0 + PASS_TICKS;
            }
        }
    }

    pub(crate) fn take_local_retry(&mut self, index: usize) -> bool {
        let unit = self.units[index].state;
        // A mover can alternate a clear step and a blocked sidestep. An
        // absolute eight-tick phase can then miss every blocked step forever.
        // Seed the first retry by id, then measure elapsed time between tries.
        let due = self.units[index].obstruction.is_some_and(|o| {
            if o.search_tick == u64::MAX {
                self.local_retry(unit.id) || o.blocked >= 8
            } else {
                self.tick.0.saturating_sub(o.search_tick) >= 8
            }
        });
        if !due
            && !(self.has_work_task(unit.id) && self.local_position_occupied(index, unit.position))
        {
            return false;
        }
        let Some(obstruction) = &mut self.units[index].obstruction else {
            return false;
        };
        if obstruction.search_tick == self.tick.0 {
            return false;
        }
        obstruction.search_tick = self.tick.0;
        true
    }

    pub(crate) fn observe_unit_progress(&mut self, index: usize) {
        let unit = &mut self.units[index];
        if unit.order.is_none() && unit.formation.is_none() {
            unit.obstruction = None;
        } else if let Some(obstruction) = &mut unit.obstruction {
            let remaining = distance(unit.state.position, obstruction.goal);
            // A sideways oscillation cannot replenish the avoidance budget.
            if remaining.saturating_add(512) <= obstruction.remaining {
                obstruction.remaining = remaining;
                obstruction.blocked = 0;
            }
        }
    }

    pub(crate) fn settle_obstructed_unit(&mut self, index: usize) -> bool {
        let Some(obstruction) = self.units[index].obstruction else {
            return false;
        };
        if self.units[index].order.is_none() {
            return false;
        }
        // A group leader's order is the whole group's route. Its own body
        // being blocked while it steers to its slot must not end that route;
        // the group's per-member stall wait covers a boxed-in unit.
        if self.units[index].formation.is_some() {
            return false;
        }
        if obstruction.blocked < 16 {
            return false;
        }
        let unit = self.units[index].state;
        let plain_move = self.can_adjust_arrival(index);
        // A builder at a legal fractional contact must hand back a taken
        // tile so construction can retry a different perimeter work spot.
        let build_arrival = self
            .economy
            .builders
            .get(&unit.id)
            .and_then(|task| self.building(task.building))
            .is_some_and(|building| {
                super::super::economy::placement::adjacent(
                    building.footprint,
                    unit.position.tile_floor(),
                )
            });
        let nearby = ((plain_move || build_arrival)
            && distance(unit.position, obstruction.goal) <= 768
            && self.position_occupied(index, obstruction.goal, true)
            || self.at_gather_arrival(index))
            && self.arrival_position_free(index);
        if !nearby && obstruction.blocked < GIVE_UP_AFTER_TICKS {
            let unavailable = if self.local_retry(unit.id) && obstruction.blocked >= 16 {
                let land_work = self.land_resource_approach(index);
                self.redirect_gather_arrival(index) == Some(false) && land_work
            } else {
                false
            };
            // End a proved unavailable work approach. Other obstructions keep
            // yielding/re-pathing until the ordinary bounded give-up below.
            if !unavailable {
                return false;
            }
        }
        self.clear_planner(index);
        // Giving up reports the goal as unreachable for now, as on dev; an
        // arrival beside a taken goal (`nearby`) is a success and has no error.
        let stored = &mut self.units[index];
        stored.order = None;
        stored.route.clear();
        stored.obstruction = None;
        stored.state.previous_position = unit.position;
        stored.state.moving = false;
        stored.state.planning = false;
        stored.last_movement_error = (!nearby).then_some(GameWorldError::Unreachable);
        if plain_move && stored.state.task == UnitTask::Moving {
            stored.state.task = if unit.carried.is_some() {
                UnitTask::Carrying
            } else {
                UnitTask::Idle
            };
        }
        self.index_destination(unit.id);
        true
    }
}
