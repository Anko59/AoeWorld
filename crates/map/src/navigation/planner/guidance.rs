//! Portal guidance and exact component proofs both resume under the route
//! allowance; ordinary far orders first emit their cheap local leg.
use super::{RoutePlanner, RouteTerrain, Terminal};
use crate::{DEFAULT_HIERARCHY_EXPANSIONS, HierarchicalOutcome, ResourceOverlay};
use aoe_core::TileCoord;
#[path = "guidance/components.rs"]
mod components;

const GUIDANCE_AFTER_WORK: u32 = 16_384;
// Keep first-use local legs cheap, then certify connectivity while a long
// trip still has walking distance to hide the bounded global work.
const GUIDANCE_AFTER_TILES: i32 = 64;

impl RoutePlanner {
    /// Immutable terrain the next polls may visit. Adapters may warm these
    /// caches without changing planner work, ordering or results.
    pub fn preparation_tiles(&self) -> Vec<TileCoord> {
        let mut tiles = vec![self.leg_origin];
        if let Some(search) = &self.hierarchy {
            tiles.extend(search.preparation_tiles());
        }
        if let Some(resolution) = &self.resolution {
            tiles.extend(resolution.preparation_tile());
        }
        tiles.extend(self.guidance.front().copied());
        tiles
    }

    pub(super) fn effective_destination(&self) -> TileCoord {
        self.resolved_destination.unwrap_or(self.destination)
    }

    pub(super) fn leg_target(&self) -> TileCoord {
        self.guidance
            .front()
            .copied()
            .unwrap_or(self.effective_destination())
    }

    /// Before connectivity is proved, a provisional far leg must preserve
    /// the origin's reachable component rather than taking a one-way drop.
    pub(super) fn provisional_map_leg(&self, terrain: &dyn RouteTerrain) -> bool {
        terrain.route_generator().is_some()
            && self.guidance.is_empty()
            && self.resolved_destination.is_none()
            && (self.origin.x - self.destination.x)
                .abs()
                .max((self.origin.y - self.destination.y).abs())
                > 64
    }

    pub(super) fn begin_guidance(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        self.guidance_checked = true;
        if let Some(generator) = terrain.route_generator()
            && (self.leg_work >= GUIDANCE_AFTER_WORK
                || generator.refresh_navigation(overlay, cancelled)?)
        {
            // A difficult local trip also needs an atlas when the adapter
            // has not prepared one yet. Capturing collision composes no tiles.
            generator.prepare_navigation(overlay, cancelled)?;
            self.hierarchy = Some(super::super::hierarchy::Search::new(
                self.leg_origin,
                self.effective_destination(),
                DEFAULT_HIERARCHY_EXPANSIONS,
            ));
        }
        Ok(())
    }

    pub(super) fn needs_guidance(&self, terrain: &dyn RouteTerrain) -> bool {
        self.hierarchy.is_none()
            && self.resolution.is_none()
            && !self.guidance_checked
            && (self.leg_work >= GUIDANCE_AFTER_WORK
                || (self.work < GUIDANCE_AFTER_WORK
                    && terrain.route_generator().is_some()
                    && (self.origin.x - self.leg_origin.x)
                        .abs()
                        .max((self.origin.y - self.leg_origin.y).abs())
                        >= GUIDANCE_AFTER_TILES))
        // Straight-line proximity says nothing about the required detour.
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
        if self.needs_guidance(terrain) {
            self.begin_guidance(terrain, overlay, cancelled)?;
        }
        if (self.hierarchy.is_some() || self.resolution.is_some())
            && !self.pocket_checked
            && self.resolved_destination.is_none()
            && terrain.route_walkable(overlay, self.destination, cancelled)?
            && !self.check_destination_pocket(terrain, overlay, &mut remaining, cancelled)?
        {
            return Ok(0);
        }
        if self.resolution.is_some() {
            let Some(generator) = terrain.route_generator() else {
                return Err(Terminal::SearchLimit);
            };
            let local = if self.needs_local_lookahead() && self.leg_work < GUIDANCE_AFTER_WORK {
                remaining / 2
            } else {
                0
            };
            let mut proof = remaining - local;
            let ready = self.poll_components(generator, &mut proof, cancelled, &|| {
                terrain.prepare_navigation_chunk()
            })?;
            remaining = local + proof;
            if !ready {
                return Ok(local);
            }
        }
        let hierarchy_local = if self.needs_local_lookahead() && self.leg_work < GUIDANCE_AFTER_WORK
        {
            remaining / 2
        } else {
            0
        };
        if let Some(hierarchy) = self.hierarchy.as_mut() {
            let Some(generator) = terrain.route_generator() else {
                return Err(Terminal::SearchLimit);
            };
            // Pending global guidance must not stop cheap local legs. Keep
            // half of this poll for lookahead, as endpoint refinement does.
            let local = hierarchy_local;
            let mut proof = remaining - local;
            let before = proof;
            let outcome =
                hierarchy.poll_budgeted(generator, overlay, &mut proof, cancelled, &|| {
                    terrain.prepare_navigation_chunk()
                });
            self.work = self.work.saturating_add(before - proof);
            self.update_peak_retained_entries();
            remaining = local + proof;
            let Some(outcome) = outcome? else {
                return Ok(local);
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
