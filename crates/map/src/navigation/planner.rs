use super::heuristic;
use crate::{EnvironmentPageError, MapChunkGenerator, Path, ResourceOverlay};
use aoe_core::TileCoord;

#[path = "planner_hash.rs"]
mod hash;
#[path = "planner/legs.rs"]
mod legs;
#[path = "planner/reconstruction.rs"]
mod reconstruction;
#[path = "planner/search.rs"]
mod search;
use reconstruction::{ReconstructionPhase, ReconstructionState};
use search::SearchState;

/// Maximum logical entries retained by one route planner across its search
/// frontier, score/parent records, backtrace, and route continuation.
pub const MAX_ROUTE_PLANNER_NODES: usize = 524_288;
/// Maximum deterministic queue pops and neighbor probes of one leg search.
/// Reconstruction steps are charged as work but bounded by the route length.
pub const MAX_ROUTE_PLANNER_WORK: u32 = 12_800_000;
/// A leg is committed as soon as the search closes a tile this many straight
/// tiles nearer the destination, so an order of any length starts from the
/// work of one short search and plans the rest while the unit walks.
pub const ROUTE_LEG_PROGRESS_TILES: u32 = 48;
/// A leg search that has spent this much work commits the nearest tile it has
/// closed (the best partial route) instead of keeping the unit waiting. With no
/// nearer tile yet, it keeps searching and commits the first one it closes.
pub const ROUTE_LEG_COMMIT_WORK: u32 = 65_536;
/// A retained route can contain at most this many tile centers.
pub const MAX_ROUTE_TILES: usize = 65_536;
const MAX_SEGMENT_STEPS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePlanner {
    origin: TileCoord,
    destination: TileCoord,
    max_work: u32,
    work: u32,
    /// Where the planned route ends and the next leg search starts.
    leg_origin: TileCoord,
    leg_work: u32,
    /// Closed tile nearest the destination in this leg, with its heuristic.
    leg_best: Option<(TileCoord, u64)>,
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
    /// A far order is planned leg by leg: its segments first cover a proven
    /// partial route toward the destination.
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
            leg_origin: origin,
            leg_work: 0,
            leg_best: None,
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

    /// True while this planner occupies a heavy search slot: before its first
    /// leg and while a later leg's frontier is retained. Between legs it holds
    /// only its compact route continuation.
    pub fn requires_search_slot(&self) -> bool {
        self.terminal.is_none()
            && (self.route_directions.is_none()
                || self.search.is_some()
                || self.reconstruction.is_some())
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

    /// Emits the next proven segment, searching the next leg with `budget`
    /// only when no planned segment is left.
    pub fn poll(
        &mut self,
        terrain: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> RoutePlannerPoll {
        if let Some(terminal) = self.terminal
            && !self.has_route_continuation()
        {
            return terminal.result();
        }
        if cancelled() {
            return self.cancel();
        }
        if self.has_route_continuation() || self.route_is_complete() {
            return self.emit_segment();
        }
        if budget == 0 {
            return RoutePlannerPoll::Pending;
        }
        if let Err(terminal) = self.search_leg(terrain, overlay, budget, cancelled) {
            return self.finish_terminal(terminal);
        }
        if !self.has_route_continuation() && !self.route_is_complete() {
            return RoutePlannerPoll::Pending;
        }
        if cancelled() {
            return self.cancel();
        }
        if self.origin == self.destination {
            self.terminal = Some(Terminal::Complete);
            return RoutePlannerPoll::Path(Path {
                tiles: vec![self.origin],
                cost: 0,
            });
        }
        self.emit_segment()
    }

    fn emit_segment(&mut self) -> RoutePlannerPoll {
        let Some(directions) = self.route_directions.as_ref() else {
            return RoutePlannerPoll::Pending;
        };
        if self.route_cursor >= directions.len() {
            self.terminal.get_or_insert(Terminal::Complete);
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
        if self.route_cursor == directions.len() && self.leg_origin == self.destination {
            self.terminal.get_or_insert(Terminal::Complete);
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

    /// Ends the search. Planned segments not yet emitted stay valid: `poll`
    /// hands them out before it reports the terminal result.
    fn finish_terminal(&mut self, terminal: Terminal) -> RoutePlannerPoll {
        self.search = None;
        self.reconstruction = None;
        self.leg_best = None;
        if !self.has_route_continuation() {
            self.route_directions = None;
            self.route_costs.clear();
            self.route_cursor = 0;
        }
        self.terminal = Some(terminal);
        terminal.result()
    }

    fn cancel(&mut self) -> RoutePlannerPoll {
        self.finish_terminal(Terminal::Environment(EnvironmentPageError::Cancelled))
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
