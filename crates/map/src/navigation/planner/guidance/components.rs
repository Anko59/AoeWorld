//! Exact endpoint proofs share the planner's ordinary fair work allowance.
use super::*;
use crate::navigation::hierarchy::ResolutionOutcome;

impl RoutePlanner {
    pub(in crate::navigation::planner) fn begin_components(
        &mut self,
        generator: &crate::MapChunkGenerator,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        if !self.components_checked && self.resolution.is_none() {
            self.resolution = generator.navigation_resolution(
                overlay,
                self.leg_origin,
                self.destination,
                cancelled,
            )?;
            self.components_checked = self.resolution.is_some();
        }
        if self.resolution.is_some() {
            self.search = None;
        }
        Ok(())
    }

    pub(super) fn poll_components(
        &mut self,
        generator: &crate::MapChunkGenerator,
        remaining: &mut u32,
        cancelled: &dyn Fn() -> bool,
        prepare: &dyn Fn() -> bool,
    ) -> Result<bool, Terminal> {
        let Some(resolution) = self.resolution.as_mut() else {
            return Ok(true);
        };
        let before = *remaining;
        let outcome = resolution.poll_budgeted(
            generator,
            remaining,
            Some((
                super::super::MAX_ROUTE_PLANNER_WORK,
                super::super::MAX_ROUTE_PLANNER_NODES,
            )),
            cancelled,
            prepare,
        );
        self.work = self.work.saturating_add(before - *remaining);
        self.update_peak_retained_entries();
        let Some(outcome) = outcome? else {
            return Ok(false);
        };
        let finished = self.resolution.take().ok_or(Terminal::SearchLimit)?;
        match outcome {
            ResolutionOutcome::Bounded => return Err(Terminal::SearchLimit),
            ResolutionOutcome::Destination(None) => return Err(Terminal::InvalidDestination),
            ResolutionOutcome::Destination(Some(target)) => {
                self.resolved_destination = (target != self.destination).then_some(target);
                if target == self.leg_origin && target != self.destination {
                    return Err(Terminal::Unreachable);
                }
            }
        }
        self.search = None;
        self.leg_work = 0;
        self.leg_best = None;
        self.guidance_checked = true;
        self.hierarchy = Some(finished.into_search(
            self.leg_origin,
            self.effective_destination(),
            DEFAULT_HIERARCHY_EXPANSIONS,
        ));
        Ok(true)
    }
}
