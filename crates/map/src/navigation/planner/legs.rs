//! Leg-wise search. One exact sparse A* runs from the end of the planned route
//! toward the destination and is committed as soon as it has proven a leg:
//! the destination, a tile `ROUTE_LEG_PROGRESS_TILES` nearer, or, once
//! `ROUTE_LEG_COMMIT_WORK` is spent, the nearest tile closed so far. Every leg
//! ends strictly nearer the destination than it starts, so legs cannot cycle,
//! and the frontier of a finished leg is dropped before the next one starts.
use super::reconstruction::ReconstructionState;
use super::search::{self, ActiveExpansion, SearchState};
use super::{
    MAX_ROUTE_PLANNER_NODES, ROUTE_LEG_COMMIT_WORK, ROUTE_LEG_PROGRESS_TILES, RoutePlanner,
    RouteTerrain, Terminal, heuristic,
};
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::TileCoord;

/// The next leg is planned ahead only while fewer planned steps than this wait
/// to be emitted: enough to hide the search behind the walk, small enough that
/// a redirected order wastes little work and retains a short route.
const LOOKAHEAD_STEPS: usize = 64;
const LEG_PROGRESS_COST: u64 =
    ROUTE_LEG_PROGRESS_TILES as u64 * super::super::ORTHOGONAL_COST as u64;

impl RoutePlanner {
    /// True once the planned route ends at the destination.
    pub(super) fn route_is_complete(&self) -> bool {
        self.route_directions.is_some() && self.leg_origin == self.destination
    }

    /// True while a later leg can usefully be searched before the unit has
    /// used up the segments already planned.
    pub fn wants_lookahead(&self) -> bool {
        self.terminal.is_none()
            && !self.route_is_complete()
            && self.route_directions.as_ref().is_some_and(|route| {
                route.len().saturating_sub(self.route_cursor) <= LOOKAHEAD_STEPS
            })
    }

    /// Spends `budget` on the next leg without emitting a segment, so a moving
    /// unit does not stop at the end of its current leg. A failed search is
    /// reported by `poll` after the segments already planned.
    pub fn plan_ahead(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) {
        if budget == 0 || !self.wants_lookahead() {
            return;
        }
        let result = if cancelled() {
            Err(Terminal::Environment(EnvironmentPageError::Cancelled))
        } else {
            self.search_leg(terrain, overlay, budget, cancelled)
        };
        if let Err(terminal) = result {
            self.finish_terminal(terminal);
        }
    }

    fn charge(&mut self, remaining: &mut u32) {
        self.work = self.work.saturating_add(1);
        self.leg_work = self.leg_work.saturating_add(1);
        *remaining -= 1;
    }

    fn begin_reconstruction(&mut self, target: TileCoord) -> Result<(), Terminal> {
        if self.retained_entries().saturating_add(1) > MAX_ROUTE_PLANNER_NODES {
            return Err(Terminal::SearchLimit);
        }
        self.reconstruction = Some(ReconstructionState::new(target));
        self.update_peak_retained_entries();
        Ok(())
    }

    /// Advances the current leg by at most `budget` work units and returns
    /// when the budget is spent or the leg has joined the planned route.
    pub(super) fn search_leg(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        let memo = super::memo::MemoTerrain::new(terrain);
        self.search_leg_with(&memo, overlay, budget, cancelled)
    }

    fn search_leg_with(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        let mut remaining = budget;
        while remaining > 0 {
            if self.reconstruction.is_some() {
                let previous_work = self.work;
                self.advance_reconstruction(cancelled)?;
                if self.work > previous_work {
                    remaining -= 1;
                }
                if self.reconstruction.is_none() {
                    return Ok(());
                }
                continue;
            }
            if self.leg_work >= self.max_work {
                // Out of work: the best partial route, when one exists.
                let Some((best, _)) = self.leg_best else {
                    return Err(Terminal::SearchLimit);
                };
                self.begin_reconstruction(best)?;
                continue;
            }
            if self.validation_step < 2 {
                let tile = if self.validation_step == 0 {
                    self.origin
                } else {
                    self.destination
                };
                self.charge(&mut remaining);
                self.validation_step += 1;
                if !search::checked_walkable(terrain, overlay, tile, cancelled)? {
                    return Err(Terminal::InvalidDestination);
                }
                continue;
            }
            if !self.pocket_checked && self.leg_work >= super::POCKET_CHECK_WORK {
                let mut pocket = self
                    .pocket
                    .take()
                    .unwrap_or_else(|| super::pocket::PocketSearch::new(self.destination));
                // Reverse proof work spends the poll allowance but is separate
                // from the forward leg's search-limit/commit counters, as before.
                self.work = self.work.saturating_add(1);
                remaining -= 1;
                let result = pocket.step(terrain, overlay, self.leg_origin, cancelled)?;
                if let Some(&tile) = pocket.queue.back() {
                    self.include_bounds(tile);
                }
                match result {
                    Some(true) => return Err(Terminal::Unreachable),
                    Some(false) => self.pocket_checked = true,
                    None => self.pocket = Some(pocket),
                }
                self.update_peak_retained_entries();
                if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
                    return Err(Terminal::SearchLimit);
                }
                continue;
            }
            let mut search = self
                .search
                .take()
                .unwrap_or_else(|| SearchState::new(self.leg_origin, self.destination));

            if let Some(mut active) = search.active {
                if active.next_neighbor == 8 {
                    search.active = None;
                    self.expansions = self.expansions.saturating_add(1);
                    self.search = Some(search);
                    continue;
                }
                let offset = search::neighbor_offset(active.next_neighbor);
                active.next_neighbor += 1;
                search.active = Some(active);
                self.charge(&mut remaining);
                let context =
                    search::ExpansionContext::new(self.destination, terrain, overlay, cancelled);
                context.expand_neighbor(&mut search, active, offset)?;
                self.search = Some(search);
                self.update_peak_retained_entries();
                if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
                    return Err(Terminal::SearchLimit);
                }
                continue;
            }

            let Some(current) = search.open.pop_first() else {
                return Err(Terminal::Unreachable);
            };
            self.charge(&mut remaining);
            let record = search.records.get(&current.tile).copied();
            self.search = Some(search);
            let Some(record) = record else {
                continue;
            };
            if current.cost != super::super::priority_cost(record.cost) {
                continue;
            }
            self.include_bounds(current.tile);
            if let Some(target) = self.leg_end(current.tile) {
                self.begin_reconstruction(target)?;
                continue;
            }
            if let Some(search) = self.search.as_mut() {
                search.active = Some(ActiveExpansion {
                    tile: current.tile,
                    cost: record.cost,
                    next_neighbor: 0,
                });
            }
            self.update_peak_retained_entries();
        }
        Ok(())
    }

    /// Decides, for a tile the search has just closed, where this leg ends.
    /// Closed tiles carry their final cost, so the route to them is exact.
    fn leg_end(&mut self, closed: TileCoord) -> Option<TileCoord> {
        if closed == self.destination {
            return Some(closed);
        }
        let from_origin = heuristic(self.leg_origin, self.destination);
        let remaining = heuristic(closed, self.destination);
        if remaining < self.leg_best.map_or(from_origin, |(_, best)| best) {
            self.leg_best = Some((closed, remaining));
        }
        if remaining.saturating_add(LEG_PROGRESS_COST) <= from_origin {
            return Some(closed);
        }
        if self.leg_work >= ROUTE_LEG_COMMIT_WORK {
            return self.leg_best.map(|(best, _)| best);
        }
        None
    }
}
