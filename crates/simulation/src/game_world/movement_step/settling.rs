//! Hysteresis counts obstruction until real progress, never sidestep distance.
use crate::{GameWorld, UnitTask};
use aoe_core::WorldPosition;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Obstruction {
    pub(crate) goal: WorldPosition,
    pub(crate) remaining: u32,
    pub(crate) blocked: u16,
    pub(crate) side: i8,
    pub(crate) last_tick: u64,
    pub(crate) search_tick: u64,
}

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
        });
        if obstruction.last_tick != self.tick.0 {
            if obstruction.blocked > 0 && obstruction.blocked.is_multiple_of(48) {
                obstruction.side = -obstruction.side;
            }
            obstruction.blocked = obstruction.blocked.saturating_add(1);
            obstruction.last_tick = self.tick.0;
        }
    }

    pub(crate) fn take_local_retry(&mut self, index: usize) -> bool {
        let unit = self.units[index].state;
        if !self.local_retry(unit.id)
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
        if obstruction.blocked < 16 {
            return false;
        }
        let unit = self.units[index].state;
        let plain_move = self.can_adjust_arrival(index);
        let nearby = (plain_move
            && distance(unit.position, obstruction.goal) <= 768
            && self.position_occupied(index, obstruction.goal, true)
            || self.at_gather_arrival(index))
            && self.arrival_position_free(index);
        if !nearby {
            if self.local_retry(unit.id) && obstruction.blocked >= 16 {
                self.redirect_gather_arrival(index);
            }
            // A unit obstruction is temporary. Keep the order while yielding
            // and re-pathing; only static navigation may prove unreachable.
            return false;
        }
        self.clear_planner(index);
        // Congestion is temporary, not a static-navigation unreachable proof.
        let stored = &mut self.units[index];
        stored.order = None;
        stored.route.clear();
        stored.obstruction = None;
        stored.state.previous_position = unit.position;
        stored.state.moving = false;
        stored.state.planning = false;
        stored.last_movement_error = None;
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
