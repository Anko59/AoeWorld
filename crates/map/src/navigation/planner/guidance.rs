//! Portal guidance and exact component proofs both resume under the route
//! allowance; ordinary far orders first emit their cheap local leg.
use super::{RoutePlanner, RouteTerrain, Terminal};
use crate::{DEFAULT_HIERARCHY_EXPANSIONS, HierarchicalOutcome, ResourceOverlay};
use aoe_core::TileCoord;
#[path = "guidance/components.rs"]
mod components;

const GUIDANCE_AFTER_WORK: u32 = 16_384;

impl RoutePlanner {
    pub(super) fn effective_destination(&self) -> TileCoord {
        self.resolved_destination.unwrap_or(self.destination)
    }

    pub(super) fn leg_target(&self) -> TileCoord {
        self.guidance
            .front()
            .copied()
            .unwrap_or(self.effective_destination())
    }

    pub(super) fn needs_guidance(&self) -> bool {
        !self.approach
            && self.hierarchy.is_none()
            && self.resolution.is_none()
            && !self.guidance_checked
            && (self.leg_work >= GUIDANCE_AFTER_WORK
                || (self.route_directions.is_some() && self.work < GUIDANCE_AFTER_WORK))
            && (self.origin.x - self.destination.x)
                .abs()
                .max((self.origin.y - self.destination.y).abs())
                > 64
    }

    pub(super) fn guide(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<u32, Terminal> {
        let mut remaining = budget;
        if self.validation_step == 0
            && let Some(generator) = terrain.route_generator()
        {
            generator.refresh_navigation(overlay, cancelled)?;
            if !terrain.route_walkable(overlay, self.destination, cancelled)? {
                self.begin_components(generator, overlay, cancelled)?;
            }
        }
        if self.resolution.is_some() {
            let Some(generator) = terrain.route_generator() else {
                return Err(Terminal::SearchLimit);
            };
            let local = if self.needs_local_lookahead() {
                remaining / 2
            } else {
                0
            };
            let mut proof = remaining - local;
            let ready = self.poll_components(generator, &mut proof, cancelled)?;
            remaining = local + proof;
            if !ready {
                return Ok(remaining);
            }
        }
        if self.needs_guidance() {
            self.guidance_checked = true;
            if let Some(generator) = terrain.route_generator()
                && (self.leg_work >= GUIDANCE_AFTER_WORK
                    || generator.refresh_navigation(overlay, cancelled)?)
            {
                generator.refresh_navigation(overlay, cancelled)?;
                self.hierarchy = Some(super::super::hierarchy::Search::new(
                    self.leg_origin,
                    self.effective_destination(),
                    DEFAULT_HIERARCHY_EXPANSIONS,
                ));
            }
        }
        if let Some(hierarchy) = self.hierarchy.as_mut() {
            let Some(generator) = terrain.route_generator() else {
                return Err(Terminal::SearchLimit);
            };
            let before = remaining;
            let outcome = hierarchy.poll_overlay(generator, overlay, &mut remaining, cancelled);
            self.work = self.work.saturating_add(before - remaining);
            self.update_peak_retained_entries();
            let Some(outcome) = outcome? else {
                return Ok(0);
            };
            let finished = self.hierarchy.take().ok_or(Terminal::SearchLimit)?;
            match outcome {
                HierarchicalOutcome::Path { waypoints, .. } => {
                    self.guidance = waypoints.into();
                    self.guidance.push_back(self.effective_destination());
                    self.search = None;
                    self.leg_work = 0;
                    self.leg_best = None;
                }
                HierarchicalOutcome::Unreachable
                | HierarchicalOutcome::Bounded
                | HierarchicalOutcome::Invalid => {
                    self.guidance.clear();
                    if !self.components_checked {
                        self.resolution = finished.into_resolution(self.destination);
                        self.components_checked = self.resolution.is_some();
                    }
                    self.begin_components(generator, overlay, cancelled)?;
                    if self.resolution.is_some() {
                        return Ok(0);
                    }
                }
            }
        }
        while let Some(&target) = self.guidance.front() {
            if target != self.leg_origin && terrain.route_walkable(overlay, target, cancelled)? {
                break;
            }
            self.guidance.pop_front();
        }
        Ok(remaining)
    }
}

impl RoutePlanner {
    /// Gives the destination up, as Age of Empires does with a target a unit
    /// cannot reach: the route goes on to the closed tile nearest the
    /// destination (the best this leg proved) and then ends, and the planner
    /// reports `Unreachable` once that route is used. With no tile nearer
    /// than where the leg starts, it reports `Unreachable` at once.
    pub fn approach_closest(&mut self) {
        if self.terminal.is_some() || self.approach {
            return;
        }
        self.approach = true;
        if self.reconstruction.is_some() {
            return;
        }
        let best = self.leg_best.map(|(best, _)| best);
        match best {
            Some(best) if self.search.is_some() => {
                if self.begin_reconstruction(best).is_err() {
                    self.finish_terminal(Terminal::SearchLimit);
                }
            }
            _ => {
                self.finish_terminal(Terminal::Unreachable);
            }
        }
    }

    /// True once `approach_closest` gave the destination up.
    pub fn is_approach(&self) -> bool {
        self.approach
    }
}
