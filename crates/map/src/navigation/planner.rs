use super::heuristic;
use crate::{EnvironmentPageError, MapChunkGenerator, Path, ResourceOverlay};
use aoe_core::TileCoord;

#[path = "planner_hash.rs"]
mod hash;
#[path = "planner/reconstruction.rs"]
mod reconstruction;
#[path = "planner/search.rs"]
mod search;
use reconstruction::{ReconstructionPhase, ReconstructionState};
use search::{ActiveExpansion, SearchState};

/// Maximum logical entries retained by one route planner across its search
/// frontier, score/parent records, backtrace, and route continuation.
pub const MAX_ROUTE_PLANNER_NODES: usize = 524_288;
/// Maximum deterministic queue pops, neighbor probes, and reconstruction steps per order.
pub const MAX_ROUTE_PLANNER_WORK: u32 = 12_800_000;
/// A retained route can contain at most this many tile centers.
pub const MAX_ROUTE_TILES: usize = 65_536;
const MAX_SEGMENT_STEPS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePlanner {
    origin: TileCoord,
    destination: TileCoord,
    max_work: u32,
    work: u32,
    expansions: u32,
    validation_step: u8,
    search: Option<SearchState>,
    reconstruction: Option<ReconstructionState>,
    route_directions: Option<Vec<u8>>,
    route_costs: Vec<u16>,
    route_cursor: usize,
    route_position: TileCoord,
    peak_retained_entries: usize,
    terminal: Option<Terminal>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Terminal {
    Complete,
    InvalidDestination,
    Unreachable,
    SearchLimit,
    Environment(EnvironmentPageError),
}

impl From<EnvironmentPageError> for Terminal {
    fn from(error: EnvironmentPageError) -> Self {
        Self::Environment(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutePlannerPoll {
    /// The exact sparse search retained its frontier and can continue later.
    Pending,
    /// One proven path segment. The planner remains live until the final
    /// segment is emitted; callers keep it to continue without replanning.
    Path(Path),
    /// The final segment was already emitted on an earlier poll.
    Complete,
    InvalidDestination,
    Unreachable,
    /// The total deterministic work or retained-state cap ended this attempt.
    SearchLimit,
    Environment(EnvironmentPageError),
}

impl RoutePlanner {
    pub fn new(origin: TileCoord, destination: TileCoord, max_work: u32) -> Self {
        Self {
            origin,
            destination,
            max_work: max_work.min(MAX_ROUTE_PLANNER_WORK),
            work: 0,
            expansions: 0,
            validation_step: 0,
            search: None,
            reconstruction: None,
            route_directions: None,
            route_costs: Vec::new(),
            route_cursor: 0,
            route_position: origin,
            peak_retained_entries: 0,
            terminal: None,
        }
    }

    /// Number of fully processed queue nodes, retained for existing progress
    /// reporting. `work()` also includes stale queue pops and transition probes.
    pub fn expansions(&self) -> u32 {
        self.expansions
    }

    pub fn work(&self) -> u32 {
        self.work
    }

    pub fn peak_retained_entries(&self) -> usize {
        self.peak_retained_entries
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal.is_some()
    }

    /// True while this planner occupies a heavy search slot. A completed
    /// compact route continuation does not consume a search slot.
    pub fn requires_search_slot(&self) -> bool {
        self.terminal.is_none() && self.route_directions.is_none()
    }

    pub fn has_route_continuation(&self) -> bool {
        self.route_directions
            .as_ref()
            .is_some_and(|route| self.route_cursor < route.len())
    }

    #[cfg(test)]
    pub(super) fn is_reconstructing(&self) -> bool {
        self.reconstruction.is_some()
    }

    pub fn poll(
        &mut self,
        terrain: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> RoutePlannerPoll {
        if let Some(terminal) = self.terminal {
            return terminal.result();
        }
        if cancelled() {
            return self.finish_terminal(Terminal::Environment(EnvironmentPageError::Cancelled));
        }
        if self.route_directions.is_some() {
            return self.emit_segment();
        }
        if budget == 0 {
            return RoutePlannerPoll::Pending;
        }

        let mut remaining = budget;
        while remaining > 0 {
            if self.work >= self.max_work && !self.reconstruction_has_free_step() {
                return self.finish_terminal(Terminal::SearchLimit);
            }
            if self.reconstruction.is_some() {
                let previous_work = self.work;
                if let Err(error) = self.advance_reconstruction(cancelled) {
                    return self.finish_terminal(error);
                }
                if self.work > previous_work {
                    remaining -= 1;
                }
                if self.route_directions.is_some() {
                    if cancelled() {
                        return self.finish_terminal(Terminal::Environment(
                            EnvironmentPageError::Cancelled,
                        ));
                    }
                    if self.route_directions.as_ref().is_some_and(Vec::is_empty) {
                        self.terminal = Some(Terminal::Complete);
                        return RoutePlannerPoll::Path(Path {
                            tiles: vec![self.origin],
                            cost: 0,
                        });
                    }
                    return self.emit_segment();
                }
                continue;
            }
            if self.validation_step < 2 {
                let tile = if self.validation_step == 0 {
                    self.origin
                } else {
                    self.destination
                };
                self.work += 1;
                remaining -= 1;
                self.validation_step += 1;
                match search::checked_walkable(terrain, overlay, tile, cancelled) {
                    Ok(true) => {}
                    Ok(false) => {
                        return self.finish_terminal(Terminal::InvalidDestination);
                    }
                    Err(error) => return self.finish_terminal(error),
                }
                continue;
            }
            if self.search.is_none() {
                self.search = Some(SearchState::new(self.origin, self.destination));
                self.update_peak_retained_entries();
            }
            let Some(mut search) = self.search.take() else {
                return self.finish_terminal(Terminal::SearchLimit);
            };

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
                self.work += 1;
                remaining -= 1;
                let context =
                    search::ExpansionContext::new(self.destination, terrain, overlay, cancelled);
                match context.expand_neighbor(&mut search, active, offset) {
                    Ok(()) => {}
                    Err(error) => return self.finish_terminal(error),
                }
                self.search = Some(search);
                self.update_peak_retained_entries();
                if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
                    return self.finish_terminal(Terminal::SearchLimit);
                }
                continue;
            }

            let Some(current) = search.open.pop_first() else {
                return self.finish_terminal(Terminal::Unreachable);
            };
            self.work += 1;
            remaining -= 1;
            let Some(record) = search.records.get(&current.tile).copied() else {
                self.search = Some(search);
                continue;
            };
            if current.cost != record.cost {
                self.search = Some(search);
                continue;
            }
            if current.tile == self.destination {
                self.search = Some(search);
                if self.retained_entries().saturating_add(1) > MAX_ROUTE_PLANNER_NODES {
                    return self.finish_terminal(Terminal::SearchLimit);
                }
                self.reconstruction = Some(ReconstructionState::new(self.destination));
                self.update_peak_retained_entries();
                if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
                    return self.finish_terminal(Terminal::SearchLimit);
                }
                continue;
            }
            search.active = Some(ActiveExpansion {
                tile: current.tile,
                cost: current.cost,
                next_neighbor: 0,
            });
            self.search = Some(search);
            self.update_peak_retained_entries();
        }
        RoutePlannerPoll::Pending
    }

    fn emit_segment(&mut self) -> RoutePlannerPoll {
        let Some(directions) = self.route_directions.as_ref() else {
            return RoutePlannerPoll::Pending;
        };
        if self.route_cursor >= directions.len() {
            self.terminal = Some(Terminal::Complete);
            return RoutePlannerPoll::Complete;
        }
        let end = self
            .route_cursor
            .saturating_add(MAX_SEGMENT_STEPS)
            .min(directions.len());
        let mut tiles = Vec::with_capacity(end - self.route_cursor + 1);
        tiles.push(self.route_position);
        let mut cost = 0_u64;
        for (index, direction) in directions
            .iter()
            .enumerate()
            .take(end)
            .skip(self.route_cursor)
        {
            let (dx, dy) = search::neighbor_offset(*direction);
            self.route_position =
                TileCoord::new(self.route_position.x + dx, self.route_position.y + dy);
            tiles.push(self.route_position);
            cost = cost.saturating_add(u64::from(self.route_costs[index]));
        }
        self.route_cursor = end;
        if self.route_cursor == directions.len() {
            self.terminal = Some(Terminal::Complete);
        }
        RoutePlannerPoll::Path(Path { tiles, cost })
    }

    fn retained_entries(&self) -> usize {
        self.search
            .as_ref()
            .map_or(0, |search| {
                search
                    .open
                    .len()
                    .saturating_add(search.records.len())
                    .saturating_add(usize::from(search.active.is_some()))
            })
            .saturating_add(self.reconstruction.as_ref().map_or(0, |reconstruction| {
                reconstruction
                    .reverse_tiles
                    .len()
                    .saturating_add(reconstruction.directions.len())
                    .saturating_add(reconstruction.costs.len())
            }))
            .saturating_add(self.route_directions.as_ref().map_or(0, Vec::len))
            .saturating_add(self.route_costs.len())
    }

    fn update_peak_retained_entries(&mut self) {
        self.peak_retained_entries = self.peak_retained_entries.max(self.retained_entries());
    }

    fn finish_terminal(&mut self, terminal: Terminal) -> RoutePlannerPoll {
        self.search = None;
        self.reconstruction = None;
        self.route_directions = None;
        self.route_costs.clear();
        self.route_cursor = 0;
        self.terminal = Some(terminal);
        terminal.result()
    }
}

impl Terminal {
    fn result(self) -> RoutePlannerPoll {
        match self {
            Self::Complete => RoutePlannerPoll::Complete,
            Self::InvalidDestination => RoutePlannerPoll::InvalidDestination,
            Self::Unreachable => RoutePlannerPoll::Unreachable,
            Self::SearchLimit => RoutePlannerPoll::SearchLimit,
            Self::Environment(error) => RoutePlannerPoll::Environment(error),
        }
    }
}
