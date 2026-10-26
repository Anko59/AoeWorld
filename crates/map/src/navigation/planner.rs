use super::heuristic;
use crate::{EnvironmentPageError, Path, ResourceOverlay};
use aoe_core::TileCoord;

#[path = "planner_hash.rs"]
mod hash;
#[path = "planner/legs.rs"]
mod legs;
#[path = "planner/memo.rs"]
mod memo;
#[path = "planner/pocket.rs"]
mod pocket;
use pocket::POCKET_CHECK_WORK;
#[path = "planner/reconstruction.rs"]
mod reconstruction;
#[path = "planner/search.rs"]
mod search;
use reconstruction::{ReconstructionPhase, ReconstructionState};
use search::SearchState;
#[path = "planner/guidance.rs"]
mod guidance;
mod terrain;
pub use terrain::RouteTerrain;

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
    resolved_destination: Option<TileCoord>,
    max_work: u32,
    work: u32,
    /// Where the planned route ends and the next leg search starts.
    leg_origin: TileCoord,
    leg_work: u32,
    /// Closed tile nearest the destination in this leg, with its heuristic.
    leg_best: Option<(TileCoord, u64)>,
    /// The sealed-destination proof has run (see `pocket`).
    pocket_checked: bool,
    pocket: Option<pocket::Proof>,
    guidance_checked: bool,
    guidance: std::collections::VecDeque<TileCoord>,
    hierarchy: Option<super::hierarchy::Search>,
    resolution: Option<super::hierarchy::Resolution>,
    components_checked: bool,
    /// Smallest box holding every tile this planner expanded or flooded
    /// (diagnostic and cache scope only; derived from the search history).
    bounds: Option<(TileCoord, TileCoord)>,
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
            resolved_destination: None,
            max_work: max_work.min(MAX_ROUTE_PLANNER_WORK),
            work: 0,
            leg_origin: origin,
            leg_work: 0,
            leg_best: None,
            pocket_checked: false,
            pocket: None,
            guidance_checked: false,
            guidance: Default::default(),
            hierarchy: None,
            resolution: None,
            components_checked: false,
            bounds: None,
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

    pub fn origin(&self) -> TileCoord {
        self.origin
    }

    pub fn destination(&self) -> TileCoord {
        self.destination
    }

    /// Nearest endpoint certified by the prepared live land components.
    pub fn certified_destination(&self) -> Option<TileCoord> {
        self.resolved_destination
    }

    pub fn work(&self) -> u32 {
        self.work
    }

    /// Endpoint preparation is separate from a hard local tile search.
    pub fn has_endpoint_refinement(&self) -> bool {
        self.components_checked
    }

    /// Global guidance has its own bounded work and expansion limits. A local
    /// search waiting for it has not exhausted its local progress allowance.
    pub fn preparing_guidance(&self) -> bool {
        self.hierarchy.is_some() || self.resolution.is_some()
    }

    /// Work spent on the leg being searched now (reset when a leg commits).
    pub fn leg_work(&self) -> u32 {
        self.leg_work
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
                || self.hierarchy.is_some()
                || self.resolution.is_some()
                || self.reconstruction.is_some())
    }

    /// True when any tile of `area` is part of this planner's retained
    /// state: a searched tile, a tile being reconstructed, or a tile of the
    /// planned route not yet emitted. A planner that does not touch a newly
    /// blocked area stays exact; one that does must be planned again.
    pub fn touches(&self, area: aoe_core::TileRect) -> bool {
        let inside = |tile: TileCoord| {
            tile.x >= area.min.x
                && tile.x < area.max.x
                && tile.y >= area.min.y
                && tile.y < area.max.y
        };
        if self
            .pocket
            .as_ref()
            .is_some_and(|proof| proof.touches(area))
        {
            return true;
        }
        if self
            .resolution
            .as_ref()
            .is_some_and(|resolution| resolution.touches(area))
            || self
                .hierarchy
                .as_ref()
                .is_some_and(|search| search.touches(area))
            || self.guidance.iter().any(|tile| {
                let x = tile.x.div_euclid(crate::CHUNK_TILES) * crate::CHUNK_TILES;
                let y = tile.y.div_euclid(crate::CHUNK_TILES) * crate::CHUNK_TILES;
                x < area.max.x
                    && y < area.max.y
                    && x + crate::CHUNK_TILES > area.min.x
                    && y + crate::CHUNK_TILES > area.min.y
            })
        {
            return true;
        }
        if self.search.as_ref().is_some_and(|search| {
            (area.min.y..area.max.y).any(|y| {
                (area.min.x..area.max.x).any(|x| search.records.contains_key(&TileCoord::new(x, y)))
            })
        }) {
            return true;
        }
        if self.reconstruction.as_ref().is_some_and(|reconstruction| {
            reconstruction
                .reverse_tiles
                .iter()
                .any(|tile| inside(*tile))
        }) {
            return true;
        }
        let Some(directions) = self.route_directions.as_ref() else {
            return false;
        };
        let mut at = self.route_position;
        if inside(at) {
            return true;
        }
        directions.iter().skip(self.route_cursor).any(|direction| {
            let (dx, dy) = search::neighbor_offset(*direction);
            at = TileCoord::new(at.x + dx, at.y + dy);
            inside(at)
        })
    }

    /// The area whose ground this planner's result depends on: every tile it
    /// expanded or flooded, grown by the two tiles a step rule can read
    /// around one. Ground changes outside it cannot change the result.
    pub fn explored_area(&self) -> aoe_core::TileRect {
        let (mut min, mut max) = self.bounds.unwrap_or((self.origin, self.origin));
        for tile in [self.origin, self.destination] {
            min = TileCoord::new(min.x.min(tile.x), min.y.min(tile.y));
            max = TileCoord::new(max.x.max(tile.x), max.y.max(tile.y));
        }
        aoe_core::TileRect::new(
            TileCoord::new(min.x - 2, min.y - 2),
            TileCoord::new(max.x + 3, max.y + 3),
        )
    }

    /// A suspended reverse flood may have rejected newly walkable ground.
    /// Callers must restart that planner before it can prove a stale seal.
    /// The padded bounds include rejected neighbours; overlap is checked
    /// in constant time without polling or scanning the retained frontier.
    pub fn opening_invalidates_proof(&self, area: aoe_core::TileRect) -> bool {
        self.pocket
            .as_ref()
            .is_some_and(|proof| proof.opening_touches(area))
    }

    fn include_bounds(&mut self, tile: TileCoord) {
        let (min, max) = self.bounds.get_or_insert((tile, tile));
        *min = TileCoord::new(min.x.min(tile.x), min.y.min(tile.y));
        *max = TileCoord::new(max.x.max(tile.x), max.y.max(tile.y));
    }

    /// Drops the frontier of the leg being searched; the next poll searches
    /// that leg again from its start. Planned segments are kept. A caller
    /// that cannot grant a search slot uses this to hold only the compact
    /// route continuation.
    pub fn abandon_leg_search(&mut self) {
        self.pocket = None;
        if self.resolution.take().is_some() {
            self.components_checked = false;
        }
        self.hierarchy = None;
        if self.guidance.is_empty() {
            self.guidance_checked = false;
        }
        self.search = None;
        self.reconstruction = None;
        self.leg_work = 0;
        self.leg_best = None;
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
        terrain: &dyn RouteTerrain,
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
            .saturating_add(self.guidance.len())
            .saturating_add(
                self.pocket
                    .as_ref()
                    .map_or(0, |proof| proof.retained_entries()),
            )
            .saturating_add(
                self.resolution
                    .as_ref()
                    .map_or(0, |resolution| resolution.retained_entries()),
            )
            .saturating_add(
                self.hierarchy
                    .as_ref()
                    .map_or(0, |search| search.retained_entries()),
            )
    }

    fn update_peak_retained_entries(&mut self) {
        self.peak_retained_entries = self.peak_retained_entries.max(self.retained_entries());
    }

    /// Ends the search. Planned segments not yet emitted stay valid: `poll`
    /// hands them out before it reports the terminal result.
    fn finish_terminal(&mut self, terminal: Terminal) -> RoutePlannerPoll {
        self.pocket = None;
        self.resolution = None;
        self.hierarchy = None;
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
