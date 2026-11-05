//! Bounded browser-only interpolation; authoritative coordinates never change.
use super::state::Sample;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE};
use aoe_protocol::GameplayUnitState;
use std::collections::{HashMap, VecDeque};

/// Minimum distance, in subunits, between the ends of a heading segment.
const HEADING_BASELINE: i64 = FIXED_SUBUNITS_PER_TILE as i64 / 10;

fn push_sample(history: &mut VecDeque<Sample>, sample: Sample) {
    // Drop first: pushing a ninth entry would grow the allocation to 16 slots.
    if history.len() == 8 {
        history.pop_front();
    }
    history.push_back(sample);
}

#[derive(Default)]
pub(crate) struct Presentation {
    history: HashMap<EntityId, VecDeque<Sample>>,
    anchor: Option<(u64, f64)>,
    tick_hz: f64,
    tick: f64,
    last_draw: Option<f64>,
}

impl Presentation {
    /// DE's sixteen clockwise headings from east, using the latest actual
    /// movement segment rather than reducing it to eight mirrored directions.
    pub(crate) fn native_heading(&self, unit: GameplayUnitState) -> Option<u8> {
        if !unit.moving || unit.hp == 0 {
            return None;
        }
        let history = self.history.get(&unit.id)?;
        let end = history.back()?;
        // A baseline of a tenth of a tile steadies the heading against the
        // one-tick jitter of collision sidesteps; a shorter history falls
        // back to its oldest differing sample.
        let far = |s: &&Sample| distance(s.position, end.position) >= HEADING_BASELINE;
        let differs = |s: &&Sample| s.position != end.position;
        let start = history
            .iter()
            .rev()
            .find(far)
            .or_else(|| history.iter().find(differs))?;
        let dx = f64::from(end.position.x) - f64::from(start.position.x);
        let dy = f64::from(end.position.y) - f64::from(start.position.y);
        // DE headings use the same unsquished axes as authoritative facings;
        // applying the screen's vertical scale again turns when a unit stops.
        let angle = (dx + dy).atan2(dx - dy);
        Some(
            (angle * 8.0 / std::f64::consts::PI)
                .round()
                .rem_euclid(16.0) as u8,
        )
    }

    pub(crate) fn resident_bytes(&self) -> usize {
        self.history
            .values()
            .map(|h| {
                std::mem::size_of::<aoe_core::EntityId>()
                    + std::mem::size_of_val(h)
                    + h.capacity() * std::mem::size_of::<Sample>()
            })
            .sum()
    }

    pub(crate) fn reset(&mut self, tick_hz: u32) {
        *self = Self {
            tick_hz: f64::from(tick_hz.clamp(1, 60)),
            ..Self::default()
        };
    }

    pub(crate) fn receive(&mut self, tick: u64, time: f64) -> bool {
        if !time.is_finite() || self.anchor.is_some_and(|(old, _)| tick < old) {
            return false;
        }
        if self.anchor.is_none() {
            self.tick = tick.saturating_sub(2) as f64;
        }
        if self.anchor.is_none_or(|(old, _)| tick > old) {
            self.anchor = Some((tick, time));
        }
        true
    }

    /// The newest received tick advanced by the wall time since it arrived,
    /// at most one tick and a half, for effects that are not lagged like
    /// unit positions. `None` before any snapshot.
    pub(crate) fn live_tick(&self, time: f64) -> Option<f64> {
        let (tick, received) = self.anchor?;
        let ahead = ((time - received) * self.tick_hz / 1_000.0).clamp(0.0, 1.5);
        Some(tick as f64 + ahead)
    }

    /// Leave positions unchanged between Canvas presentations, including picking.
    pub(crate) fn advance(&mut self, time: f64, canvas: bool) {
        if !time.is_finite() || self.last_draw.is_some_and(|old| time < old) {
            return;
        }
        if canvas
            && self
                .last_draw
                .is_some_and(|old| time - old < 1_000.0 / 30.0)
        {
            return;
        }
        self.last_draw = Some(time);
        if let Some((latest, received)) = self.anchor {
            let elapsed = (time - received).max(0.0);
            let target = aoe_core::clamp_f64(
                latest as f64 + elapsed * self.tick_hz / 1_000.0 - 2.0,
                0.0,
                latest as f64,
            );
            self.tick = self.tick.max(target);
        }
    }

    pub(crate) fn remember(
        &mut self,
        old: Option<&GameplayUnitState>,
        state: &GameplayUnitState,
        tick: u64,
    ) {
        if self.history.len() >= aoe_protocol::MAX_SUBSCRIBED_UNITS
            && !self.history.contains_key(&state.id)
        {
            return;
        }
        let idle = old.filter(|old| !old.moving);
        let idle_jump = idle.is_some_and(|old| {
            distance(old.position, state.position) > i64::from(FIXED_SUBUNITS_PER_TILE)
        });
        let history = self.history.entry(state.id).or_default();
        // The odometer survives a cleared history, but a jump adds nothing.
        let mut travel = history.back().map_or(0.0, |last| last.travel);
        let mut step = true;
        if let Some(last) = history.back() {
            if tick < u64::from(last.tick) {
                return;
            }
            let gap = tick - u64::from(last.tick);
            if idle_jump
                || gap > 8
                || distance(last.position, state.position)
                    > gap.max(1) as i64 * i64::from(FIXED_SUBUNITS_PER_TILE)
            {
                // Missing route corners or a discontinuity cannot be reconstructed.
                history.clear();
                step = false;
            }
        }
        if let Some(last) = history.back()
            && u64::from(last.tick) == tick
        {
            history.pop_back();
            // Replace the sample: count from the one before it.
            travel = history.back().map_or(0.0, |before| before.travel);
        }
        let before = travel;
        if step && let Some(last) = history.back() {
            travel += tiles_between(last.position, state.position);
        }
        if let Some(old) = idle
            && !idle_jump
            && let Some(anchor) = tick.checked_sub(1)
            && history
                .back()
                .is_none_or(|last| u64::from(last.tick) < anchor)
        {
            // Heartbeats kept an idle unit stationary, even after a long gap.
            push_sample(
                history,
                Sample {
                    tick: anchor as u32,
                    position: old.position,
                    travel: before,
                },
            );
        }
        push_sample(
            history,
            Sample {
                tick: tick as u32,
                position: state.position,
                travel,
            },
        );
    }

    pub(crate) fn remove(&mut self, id: EntityId) {
        self.history.remove(&id);
    }

    #[inline(never)]
    pub(crate) fn position(&self, state: GameplayUnitState) -> [f64; 2] {
        self.pose(state).0
    }

    /// The tiles a unit has moved over the last `ticks` ticks of presentation
    /// time, or `None` while its history does not reach back that far.
    pub(crate) fn travel_over(&self, id: EntityId, ticks: f64) -> Option<f64> {
        let history = self.history.get(&id)?;
        if (history.front()?.tick as f64) > self.tick - ticks {
            return None;
        }
        Some(travel_at(history, self.tick) - travel_at(history, self.tick - ticks))
    }

    /// The drawn position and the tiles the unit has actually moved by then.
    #[inline(never)]
    pub(crate) fn pose(&self, state: GameplayUnitState) -> ([f64; 2], f64) {
        let Some(history) = self.history.get(&state.id) else {
            return (state.position.as_tiles(), 0.0);
        };
        let mut previous = None::<Sample>;
        for next in history {
            if next.tick as f64 >= self.tick {
                let Some(previous) = previous else {
                    return (next.position.as_tiles(), f64::from(next.travel));
                };
                if next.tick as f64 == self.tick {
                    return (next.position.as_tiles(), f64::from(next.travel));
                }
                let amount =
                    (self.tick - previous.tick as f64) / (next.tick - previous.tick) as f64;
                let start = previous.position.as_tiles();
                let end = next.position.as_tiles();
                let travel = f64::from(previous.travel)
                    + (f64::from(next.travel) - f64::from(previous.travel)) * amount;
                return (
                    [
                        start[0] + (end[0] - start[0]) * amount,
                        start[1] + (end[1] - start[1]) * amount,
                    ],
                    travel,
                );
            }
            previous = Some(*next);
        }
        let travel = history.back().map_or(0.0, |last| f64::from(last.travel));
        (state.position.as_tiles(), travel)
    }
}

/// The tiles a unit had moved at `tick`, interpolated between its samples and
/// held at the first or last one outside them.
fn travel_at(history: &VecDeque<Sample>, tick: f64) -> f64 {
    let mut previous = None::<Sample>;
    for next in history {
        if next.tick as f64 >= tick {
            let Some(previous) = previous else {
                return f64::from(next.travel);
            };
            let amount = (tick - previous.tick as f64) / (next.tick - previous.tick) as f64;
            return f64::from(previous.travel)
                + (f64::from(next.travel) - f64::from(previous.travel)) * amount;
        }
        previous = Some(*next);
    }
    history.back().map_or(0.0, |last| f64::from(last.travel))
}

fn tiles_between(left: aoe_core::WorldPosition, right: aoe_core::WorldPosition) -> f32 {
    let dx = f64::from(right.x) - f64::from(left.x);
    let dy = f64::from(right.y) - f64::from(left.y);
    (dx.hypot(dy) / f64::from(FIXED_SUBUNITS_PER_TILE)) as f32
}

fn distance(left: aoe_core::WorldPosition, right: aoe_core::WorldPosition) -> i64 {
    (i64::from(left.x) - i64::from(right.x))
        .abs()
        .max((i64::from(left.y) - i64::from(right.y)).abs())
}

#[path = "presentation/tests.rs"]
#[cfg(test)]
mod tests;
