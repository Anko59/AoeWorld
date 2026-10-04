//! Bounded browser-only interpolation; authoritative coordinates never change.
use super::state::Sample;
use aoe_core::{EntityId, FIXED_SUBUNITS_PER_TILE};
use aoe_protocol::GameplayUnitState;
use std::collections::{BTreeMap, VecDeque};

fn push_sample(history: &mut VecDeque<Sample>, sample: Sample) {
    // Drop first: pushing a ninth entry would grow the allocation to 16 slots.
    if history.len() == 8 {
        history.pop_front();
    }
    history.push_back(sample);
}

#[derive(Default)]
pub(crate) struct Presentation {
    history: BTreeMap<EntityId, VecDeque<Sample>>,
    anchor: Option<(u64, f64)>,
    tick_hz: f64,
    tick: f64,
    last_draw: Option<f64>,
}

impl Presentation {
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
            let target =
                (latest as f64 + elapsed * self.tick_hz / 1_000.0 - 2.0).clamp(0.0, latest as f64);
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
        if let Some(last) = history.back() {
            if tick < last.tick {
                return;
            }
            let gap = tick - last.tick;
            if idle_jump
                || gap > 8
                || distance(last.position, state.position)
                    > gap.max(1) as i64 * i64::from(FIXED_SUBUNITS_PER_TILE)
            {
                // Missing route corners or a discontinuity cannot be reconstructed.
                history.clear();
            }
        }
        if let Some(last) = history.back()
            && last.tick == tick
        {
            history.pop_back();
        }
        if let Some(old) = idle
            && !idle_jump
            && let Some(anchor) = tick.checked_sub(1)
            && history.back().is_none_or(|last| last.tick < anchor)
        {
            // Heartbeats kept an idle unit stationary, even after a long gap.
            push_sample(
                history,
                Sample {
                    tick: anchor,
                    position: old.position,
                },
            );
        }
        push_sample(
            history,
            Sample {
                tick,
                position: state.position,
            },
        );
    }

    pub(crate) fn remove(&mut self, id: EntityId) {
        self.history.remove(&id);
    }

    #[inline(never)]
    pub(crate) fn position(&self, state: GameplayUnitState) -> [f64; 2] {
        let Some(history) = self.history.get(&state.id) else {
            return state.position.as_tiles();
        };
        let mut previous = None::<Sample>;
        for next in history {
            if next.tick as f64 >= self.tick {
                let Some(previous) = previous else {
                    return next.position.as_tiles();
                };
                if next.tick as f64 == self.tick {
                    return next.position.as_tiles();
                }
                let amount =
                    (self.tick - previous.tick as f64) / (next.tick - previous.tick) as f64;
                let start = previous.position.as_tiles();
                let end = next.position.as_tiles();
                return [
                    start[0] + (end[0] - start[0]) * amount,
                    start[1] + (end[1] - start[1]) * amount,
                ];
            }
            previous = Some(*next);
        }
        state.position.as_tiles()
    }
}

fn distance(left: aoe_core::WorldPosition, right: aoe_core::WorldPosition) -> i64 {
    (i64::from(left.x) - i64::from(right.x))
        .abs()
        .max((i64::from(left.y) - i64::from(right.y)).abs())
}

#[path = "presentation/tests.rs"]
#[cfg(test)]
mod tests;
