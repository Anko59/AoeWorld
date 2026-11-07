//! A settled friendly body steps aside for a blocked group member, as AoE2 units
//! nudge an idle friend. The mover asks; the bodies in its way walk clear of its
//! line, wait for it to pass and walk back only if they hold a slot. A body yields
//! once per cooldown and nobody else wakes up: its group stays settled and arrived.
//! Fights, work and live orders are never disturbed.
use crate::{GameWorld, UnitTask};
use aoe_core::{EntityId, WorldPosition};
use std::collections::BTreeMap;

/// Blocked ticks before a mover asks, and how often it asks after that.
const ASK_AFTER: u16 = 120;
const ASK_EVERY: u64 = 8;
const ASKS_PER_TICK: usize = 8;
const BODIES_PER_ASK: usize = 2;
/// Contact margin in subunits: a body this close ahead counts as in the way.
const IN_THE_WAY: i64 = 64;
/// The walk aside, the wait for the mover to pass, the walk back.
const ASIDE_TICKS: u64 = 40;
const HOLD_TICKS: u64 = 56;
const RETURN_TICKS: u64 = 200;
/// Ticks from an ask until the same body may be asked again.
const COOLDOWN: u64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Aside,
    Hold,
    Return,
    /// Finished or disturbed: only the cooldown is left.
    Rest,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Yielding {
    phase: Phase,
    to: WorldPosition,
    home: WorldPosition,
    /// When the current phase ends.
    until: u64,
    asked: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Yields {
    /// Movers that asked this tick, consumed by `advance_yields` in the same tick.
    asks: Vec<EntityId>,
    active: BTreeMap<EntityId, Yielding>,
}

impl Yields {
    pub(crate) fn update_hash(&self, hash: &mut blake3::Hasher) {
        if self.active.is_empty() {
            return;
        }
        hash.update(b"yields\0");
        hash.update(&(self.active.len() as u64).to_le_bytes());
        for (id, entry) in &self.active {
            hash.update(&id.0.to_le_bytes());
            hash.update(&[entry.phase as u8]);
            for point in [entry.to, entry.home] {
                hash.update(&point.x.to_le_bytes());
                hash.update(&point.y.to_le_bytes());
            }
            hash.update(&entry.until.to_le_bytes());
            hash.update(&entry.asked.to_le_bytes());
        }
    }
}

impl GameWorld {
    /// Called once per blocked tick. Only group members ask: a lone unit keeps
    /// its own bounded give-up rule.
    pub(super) fn note_blocked_for_yield(&mut self, index: usize, blocked: u16) {
        let id = self.units[index].state.id;
        if blocked >= ASK_AFTER
            && self.units[index].formation.is_some()
            && (self.tick.0 + u64::from(id.0)).is_multiple_of(ASK_EVERY)
        {
            self.yields.asks.push(id);
        }
    }

    /// Standing still with nothing to do: no order, fight, job or route.
    fn may_yield(&self, index: usize) -> bool {
        let stored = &self.units[index];
        let unit = &stored.state;
        unit.hp > 0
            && !unit.moving
            && !unit.planning
            && unit.task == UnitTask::Idle
            && unit.attack_target.is_none()
            && !unit.kind.is_naval()
            && stored.order.is_none()
            && !self.economy.gatherers.contains_key(&unit.id)
            && !self.economy.builders.contains_key(&unit.id)
            && !self.economy.garrison_orders.contains_key(&unit.id)
            && !self.combat.attackers.contains_key(&unit.id)
            && self.hunting_target(unit.id).is_none()
    }

    /// Where the mover is trying to go right now.
    fn yield_heading(&self, index: usize) -> Option<(i64, i64)> {
        let from = self.units[index].state.position;
        let to = self
            .formation_member_target(index)
            .or_else(|| self.units[index].order.map(|order| order.waypoint))?;
        let heading = (i64::from(to.x - from.x), i64::from(to.y - from.y));
        (heading != (0, 0)).then_some(heading)
    }

    /// The nearest bodies ahead of the mover that are free to step aside.
    fn bodies_in_the_way(&self, index: usize, heading: (i64, i64)) -> Vec<usize> {
        let mover = self.units[index].state;
        let radius = mover.kind.collision_radius();
        let reach = radius + self.collisions.maximum_radii()[0] + IN_THE_WAY as i32;
        let mut found: Vec<(i64, EntityId, usize)> = Vec::new();
        self.collisions.blocked(
            (mover.position, reach),
            false,
            self.tick.0.wrapping_add(u64::from(mover.id.0)),
            &self.group_work,
            |id| {
                let at = self.collisions.unit_index(id);
                let other = &self.units[at].state;
                let (x, y) = (
                    i64::from(other.position.x - mover.position.x),
                    i64::from(other.position.y - mover.position.y),
                );
                let contact = i64::from(radius + other.kind.collision_radius()) + IN_THE_WAY;
                let squared = x * x + y * y;
                if id != mover.id
                    && other.player == mover.player
                    && squared < contact * contact
                    && x * heading.0 + y * heading.1 > 0
                    && self.may_yield(at)
                {
                    found.push((squared, id, at));
                }
                false
            },
            |_| false,
        );
        found.sort_unstable_by_key(|&(squared, id, _)| (squared, id));
        found.truncate(BODIES_PER_ASK);
        found.into_iter().map(|(_, _, at)| at).collect()
    }

    /// The nearest open ground clear of the mover's line, on the body's own side.
    fn yield_spot(&self, at: usize, mover: usize, heading: (i64, i64)) -> Option<WorldPosition> {
        let body = self.units[at].state;
        let from = self.units[mover].state;
        let length = i64::from(crate::game_path::segment_length(heading.0, heading.1)).max(1);
        let (x, y) = (
            i64::from(body.position.x - from.position.x),
            i64::from(body.position.y - from.position.y),
        );
        let lateral = (heading.0 * y - heading.1 * x) / length;
        let side = match lateral.signum() {
            0 if body.id.0.is_multiple_of(2) => 1,
            0 => -1,
            sign => sign,
        };
        let clear = i64::from(from.kind.collision_radius() + body.kind.collision_radius()) + 32;
        let base = (clear - lateral.abs()).max(0);
        for extra in [0, 256, 512, 768, 1024] {
            for along in [0, 384, -384, 768, -768] {
                let out = (base + extra) * side;
                let spot = WorldPosition::new(
                    body.position.x + ((-heading.1 * out + heading.0 * along) / length) as i32,
                    body.position.y + ((heading.0 * out + heading.1 * along) / length) as i32,
                );
                if spot != body.position
                    && self.body_position_clear(body.kind, spot)
                    && !self.economy.blocked_tiles.contains(&spot.tile_floor())
                    && self
                        .unit_tile_passable(body.kind, spot.tile_floor(), &|| false)
                        .unwrap_or(false)
                    && !self.collision_occupied(at, spot, false)
                {
                    return Some(spot);
                }
            }
        }
        None
    }

    fn answer_yield_asks(&mut self) {
        let mut asks = std::mem::take(&mut self.yields.asks);
        asks.sort_unstable();
        asks.dedup();
        let start = (self.tick.0 % asks.len().max(1) as u64) as usize;
        for turn in 0..asks.len().min(ASKS_PER_TICK) {
            let Some(&index) = self.lookup.get(&asks[(start + turn) % asks.len()]) else {
                continue;
            };
            let Some(heading) = self.yield_heading(index) else {
                continue;
            };
            for at in self.bodies_in_the_way(index, heading) {
                let id = self.units[at].state.id;
                if self.yields.active.contains_key(&id) {
                    continue;
                }
                let Some(to) = self.yield_spot(at, index, heading) else {
                    continue;
                };
                self.yields.active.insert(
                    id,
                    Yielding {
                        phase: Phase::Aside,
                        to,
                        home: self.units[at].state.position,
                        until: self.tick.0 + ASIDE_TICKS,
                        asked: self.tick.0,
                    },
                );
            }
        }
        asks.clear();
        self.yields.asks = asks;
    }

    /// One walking tick. False when the way is shut: the body keeps its place.
    fn yield_step(&mut self, index: usize, to: WorldPosition, changed: &mut Vec<EntityId>) -> bool {
        let unit = self.units[index].state;
        let (speed, denominator) = self.movement_speed(unit.kind, unit.player);
        let budget = (speed / denominator).clamp(1, 512) as i32;
        let next = crate::game_world::formation::group_step(unit.position, to, budget);
        if next == unit.position
            || !self.local_unit_step_clear(index, unit.position, next)
            || !self
                .unit_position_step_clear(unit.kind, unit.position, next)
                .unwrap_or(false)
            || !self.building_step_clear(unit.position.tile_floor(), next.tile_floor())
        {
            return false;
        }
        let stored = &mut self.units[index].state;
        stored.previous_position = unit.position;
        stored.position = next;
        stored.facing = super::facing_for(
            i64::from(next.x - unit.position.x),
            i64::from(next.y - unit.position.y),
            unit.facing,
        );
        let after = *stored;
        self.track_unit_step(unit.id, unit.position.tile_floor(), &after);
        let bucket = aoe_core::ChunkCoord::from_position(next);
        if self.units[index].bucket != bucket {
            self.move_bucket(index, bucket);
        }
        if unit.position.tile_floor() != next.tile_floor() {
            self.update_unit_vision(unit.id);
        }
        changed.push(unit.id);
        true
    }

    /// A slot if it is free, else the place the body gave up.
    fn yield_return_point(&self, index: usize, home: WorldPosition) -> Option<WorldPosition> {
        let slot = self.formation_destination_slot(self.units[index].state.id)?;
        Some(if self.collision_occupied(index, slot, false) {
            home
        } else {
            slot
        })
    }

    #[inline(never)] // a symbol of its own for the Callgrind stage counters
    pub(crate) fn advance_yields(&mut self, changed: &mut Vec<EntityId>) {
        if !self.yields.asks.is_empty() {
            self.answer_yield_asks();
        }
        if self.yields.active.is_empty() {
            return;
        }
        let tick = self.tick.0;
        let entries = self
            .yields
            .active
            .iter()
            .map(|(&id, &entry)| (id, entry))
            .collect::<Vec<_>>();
        for (id, mut entry) in entries {
            let Some(&index) = self
                .lookup
                .get(&id)
                .filter(|_| tick < entry.asked + COOLDOWN)
            else {
                self.yields.active.remove(&id);
                continue;
            };
            if entry.phase != Phase::Rest && !self.may_yield(index) {
                entry.phase = Phase::Rest;
            }
            match entry.phase {
                Phase::Aside => {
                    if self.units[index].state.position == entry.to
                        || tick >= entry.until
                        || !self.yield_step(index, entry.to, changed)
                    {
                        entry.phase = Phase::Hold;
                        entry.until = tick + HOLD_TICKS;
                        let stored = &mut self.units[index].state;
                        stored.previous_position = stored.position;
                    }
                }
                Phase::Hold if tick >= entry.until => {
                    if let Some(to) = self.yield_return_point(index, entry.home) {
                        entry.phase = Phase::Return;
                        entry.to = to;
                        entry.until = tick + RETURN_TICKS;
                    } else {
                        entry.phase = Phase::Rest;
                    }
                }
                Phase::Return => {
                    let there = self.units[index].state.position == entry.to;
                    if there || tick >= entry.until {
                        entry.phase = Phase::Rest;
                        let stored = &mut self.units[index].state;
                        stored.previous_position = stored.position;
                    } else {
                        // A body in the way may leave: wait, within the deadline.
                        self.yield_step(index, entry.to, changed);
                    }
                }
                Phase::Hold | Phase::Rest => {}
            }
            self.yields.active.insert(id, entry);
        }
    }
}

#[cfg(test)]
pub(crate) fn yield_candidate(world: &GameWorld, id: EntityId) -> bool {
    world.may_yield(world.lookup[&id])
}
