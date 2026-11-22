//! A lone mover no screen shows, with nothing to fight near, rides its route
//! (#978 4.3, slice 4b): it sits out the movement phase and every `RIDE_TICKS`
//! it is moved along its legs by the time that passed, with no proposal, sweep
//! or step check. Its legs came from the route planner, clear of terrain and
//! buildings; a ride stops on a tile that is no longer passable. It walks its
//! last leg on the stride tier, so arriving is unchanged. A rider is checked
//! every tick and anything that changes its order or what it may do ends the
//! ride. Not saved: after a load a mover starts riding again on its turn.
use super::*;
use crate::game_path::segment_length;
use crate::game_world::{facing_for, interpolate};

/// Reference ticks between two moves of a rider.
pub(crate) const RIDE_TICKS: u64 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ride {
    /// The tick from which the movement is owed.
    since: u64,
    /// The tick it is moved.
    wake: u64,
    /// The order it rides, and the leg it was on when the ride began or the
    /// unit was last moved: anything else ends the ride.
    destination: WorldPosition,
    leg: (WorldPosition, WorldPosition),
}

impl GameWorld {
    /// True when the mover at `index` rides this tick instead of stepping.
    pub(crate) fn ride(&mut self, index: usize, changed: &mut Vec<EntityId>) -> bool {
        let now = self.tick.0;
        if let Some(ride) = self.units[index].ride.as_deref().copied() {
            // Checked every tick: an order, a stop, a view or something to
            // fight ends the ride at once, before any credit is applied.
            let same = self.units[index].order.is_some_and(|o| {
                o.destination == ride.destination && (o.origin, o.waypoint) == ride.leg
            });
            if !same || self.units[index].formation.is_some() || !self.unseen_bystander(index) {
                self.units.set_ride(index, None);
                return false;
            }
            if now < ride.wake {
                return true;
            }
            self.units.set_ride(index, None);
            if !self.advance_ride(index, ride, changed) {
                return false;
            }
        }
        let Some(order) = self.units[index].order.filter(|_| self.rides(index)) else {
            return false;
        };
        let ride = Ride {
            since: now,
            wake: now + self.config.ticks(RIDE_TICKS),
            destination: order.destination,
            leg: (order.origin, order.waypoint),
        };
        self.units.set_ride(index, Some(ride));
        true
    }

    /// Lone, unwatched, nothing to fight near, and a leg before its last. A
    /// planner planning further ahead may stay: the last known leg, where the
    /// planner extends the route, is always stepped.
    fn rides(&self, index: usize) -> bool {
        let stored = &self.units[index];
        stored.formation.is_none()
            && !stored.route.is_empty()
            && stored.obstruction.is_none()
            && !stored.state.planning
            && !stored.state.kind.is_naval()
            && stored.order.is_some_and(|o| o.travelled < o.segment_length)
            && self.unseen_bystander(index)
    }

    /// Moves the rider along its legs by the ticks since `ride.since`, up to
    /// the start of its last leg. False when it does not move (another order,
    /// or a blocked tile): it then steps on the ordinary path.
    fn advance_ride(&mut self, index: usize, ride: Ride, changed: &mut Vec<EntityId>) -> bool {
        let stored = &self.units[index];
        let Some(mut order) = stored.order else {
            return false;
        };
        let unit = stored.state;
        let own = self.movement_speed(unit.kind, unit.player);
        let (speed, denominator) = self.road_speed(unit.kind, unit.position, own);
        let elapsed = self.tick.0.saturating_sub(ride.since);
        let mut credit = u128::from(order.speed_carry) + u128::from(speed) * u128::from(elapsed);
        let (mut popped, mut facing) = (0, unit.facing);
        loop {
            let remaining = order.segment_length.saturating_sub(order.travelled);
            let step = u128::from(remaining).min(credit / u128::from(denominator)) as u32;
            order.travelled += step;
            credit -= u128::from(step) * u128::from(denominator);
            if order.travelled < order.segment_length {
                break;
            }
            // At a waypoint: the last leg is walked, not ridden.
            let Some(&next) = stored.route.get(popped) else {
                break;
            };
            popped += 1;
            let dx = i64::from(next.x) - i64::from(order.waypoint.x);
            let dy = i64::from(next.y) - i64::from(order.waypoint.y);
            facing = facing_for(dx, dy, facing);
            order = MovementOrder {
                origin: order.waypoint,
                waypoint: next,
                segment_length: segment_length(dx, dy),
                travelled: 0,
                ..order
            };
            if popped == stored.route.len() {
                break;
            }
        }
        // What is left carries into the stepped last leg, at most two ticks'
        // worth so its first step stays on the parallel path.
        order.speed_carry = credit.min(2 * u128::from(speed)) as u64;
        let position = if order.travelled >= order.segment_length {
            order.waypoint
        } else {
            interpolate(order, order.travelled)
        };
        let (previous_tile, tile) = (unit.position.tile_floor(), position.tile_floor());
        if !self.tile_passable(tile) {
            return false;
        }
        let stored = &mut self.units[index];
        stored.route.drain(..popped);
        stored.order = Some(order);
        stored.state.previous_position = unit.position;
        stored.state.position = position;
        stored.state.facing = facing;
        let after = stored.state;
        self.track_unit_step(unit.id, previous_tile, &after);
        let bucket = ChunkCoord::from_position(position);
        if self.units[index].bucket != bucket {
            self.move_bucket(index, bucket);
        }
        if previous_tile != tile {
            self.update_unit_vision(unit.id);
        }
        changed.push(unit.id);
        true
    }
}
