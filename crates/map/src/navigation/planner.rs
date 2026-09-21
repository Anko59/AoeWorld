use super::{heuristic, same_fine_chunk};
use crate::route_hierarchy::{portal_candidates_checked, same_intermediate_region};
use crate::{
    EdgePassability, EnvironmentPageError, GroundMaterial, MapChunkGenerator, Path, ResourceOverlay,
};
use aoe_core::TileCoord;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[path = "planner_hash.rs"]
mod hash;

/// Maximum retained search nodes for one in-flight route. This cap includes
/// frontier, score, and parent entries and makes the retained planner state
/// bounded independently of map dimensions.
pub const MAX_ROUTE_PLANNER_NODES: usize = 2_048;
const MAX_PORTAL_ATTEMPTS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutePlanner {
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
    expansions: u32,
    portals: Option<Vec<TileCoord>>,
    portal_index: usize,
    direct_search_started: bool,
    search: Option<SearchState>,
    terminal: Option<Terminal>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Terminal {
    Path(Path),
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
struct SearchState {
    goal: TileCoord,
    open: BTreeSet<OpenNode>,
    scores: BTreeMap<TileCoord, u64>,
    parents: BTreeMap<TileCoord, TileCoord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct OpenNode {
    total: u64,
    cost: u64,
    tile: TileCoord,
}

impl OpenNode {
    const fn new(total: u64, cost: u64, tile: TileCoord) -> Self {
        Self { total, cost, tile }
    }
}

impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.total, self.cost, self.tile.y, self.tile.x).cmp(&(
            other.total,
            other.cost,
            other.tile.y,
            other.tile.x,
        ))
    }
}

impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutePlannerPoll {
    /// The search retained its frontier and can continue in a later poll.
    Pending,
    Path(Path),
    InvalidDestination,
    Unreachable,
    /// The planner's total deterministic work or retained-node cap was
    /// reached. This is terminal for this attempt but is not cacheable.
    SearchLimit,
    Environment(EnvironmentPageError),
}

impl RoutePlanner {
    pub fn new(origin: TileCoord, destination: TileCoord, max_expansions: u32) -> Self {
        Self {
            origin,
            destination,
            max_expansions,
            expansions: 0,
            portals: None,
            portal_index: 0,
            direct_search_started: false,
            search: None,
            terminal: None,
        }
    }

    pub fn expansions(&self) -> u32 {
        self.expansions
    }

    pub fn is_terminal(&self) -> bool {
        self.terminal.is_some()
    }

    pub fn poll(
        &mut self,
        terrain: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        budget: u32,
        cancelled: &dyn Fn() -> bool,
    ) -> RoutePlannerPoll {
        if let Some(terminal) = &self.terminal {
            return terminal.result();
        }
        if let Err(error) = self.initialize(terrain, overlay, cancelled) {
            return self.finish_terminal(error);
        }
        if let Some(terminal) = &self.terminal {
            return terminal.result();
        }
        if cancelled() {
            return self.finish_terminal(Terminal::Environment(EnvironmentPageError::Cancelled));
        }
        if budget == 0 {
            return RoutePlannerPoll::Pending;
        }
        let mut remaining = budget;
        while remaining > 0 {
            if self.expansions >= self.max_expansions {
                return self.finish_terminal(Terminal::SearchLimit);
            }
            let Some(mut search) = self.search.take() else {
                if !self.advance_portal(terrain) {
                    self.terminal = Some(Terminal::Unreachable);
                    return RoutePlannerPoll::Unreachable;
                }
                continue;
            };
            let Some(current) = search.open.pop_first() else {
                self.abandon_search();
                continue;
            };
            remaining = remaining.saturating_sub(1);
            let Some(cost) = search.scores.get(&current.tile).copied() else {
                self.search = Some(search);
                continue;
            };
            if current.cost != cost {
                self.search = Some(search);
                continue;
            }
            if current.tile == search.goal {
                let result =
                    self.finish_path(terrain, search.goal, cost, &search.parents, cancelled);
                match result {
                    Ok(path) => {
                        self.terminal = Some(Terminal::Path(path.clone()));
                        return RoutePlannerPoll::Path(path);
                    }
                    Err(error) => {
                        return self.finish_terminal(error);
                    }
                }
            }
            self.expansions = self.expansions.saturating_add(1);
            let expand_result =
                self.expand(terrain, overlay, cancelled, &mut search, current.tile, cost);
            let search_empty = search.open.is_empty();
            match expand_result {
                Ok(()) => self.search = Some(search),
                Err(error) => {
                    return self.finish_terminal(error);
                }
            }
            if search_empty {
                if self
                    .portals
                    .as_ref()
                    .is_some_and(|portals| !portals.is_empty())
                {
                    self.portal_index = self.portal_index.saturating_add(1);
                }
                self.search = None;
            }
            if self.retained_nodes() > MAX_ROUTE_PLANNER_NODES {
                return self.finish_terminal(Terminal::SearchLimit);
            }
        }
        RoutePlannerPoll::Pending
    }

    fn initialize(
        &mut self,
        terrain: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), Terminal> {
        if self.portals.is_some() {
            return Ok(());
        }
        if !checked_walkable(terrain, overlay, self.origin, cancelled)?
            || !checked_walkable(terrain, overlay, self.destination, cancelled)?
        {
            self.terminal = Some(Terminal::InvalidDestination);
            return Ok(());
        }
        let portals = if same_intermediate_region(self.origin, self.destination) {
            Vec::new()
        } else {
            portal_candidates_checked(terrain, overlay, self.origin, self.destination, cancelled)?
                .into_iter()
                .take(MAX_PORTAL_ATTEMPTS)
                .collect()
        };
        if !same_intermediate_region(self.origin, self.destination) && portals.is_empty() {
            self.terminal = Some(Terminal::Unreachable);
            return Ok(());
        }
        self.portals = Some(portals);
        self.advance_portal(terrain);
        Ok(())
    }

    fn advance_portal(&mut self, _terrain: &MapChunkGenerator) -> bool {
        let Some(portals) = self.portals.as_ref() else {
            return false;
        };
        let goal = portals
            .get(self.portal_index)
            .copied()
            .unwrap_or(self.destination);
        if self.portal_index >= portals.len() && portals.is_empty() {
            if self.direct_search_started {
                return false;
            }
            self.direct_search_started = true;
            self.search = Some(SearchState::new(self.origin, self.destination));
            return true;
        }
        if self.portal_index >= portals.len() {
            return false;
        }
        if self.search.is_none() {
            self.search = Some(SearchState::new(self.origin, goal));
            return true;
        }
        false
    }

    fn abandon_search(&mut self) {
        if self
            .portals
            .as_ref()
            .is_some_and(|portals| !portals.is_empty())
        {
            self.portal_index = self.portal_index.saturating_add(1);
        }
        self.search = None;
    }

    fn expand(
        &self,
        terrain: &MapChunkGenerator,
        overlay: &ResourceOverlay,
        cancelled: &dyn Fn() -> bool,
        search: &mut SearchState,
        tile: TileCoord,
        cost: u64,
    ) -> Result<(), Terminal> {
        for delta_y in -1..=1 {
            for delta_x in -1..=1 {
                if delta_x == 0 && delta_y == 0 {
                    continue;
                }
                let next = TileCoord::new(tile.x + delta_x, tile.y + delta_y);
                let diagonal_ok = if delta_x != 0 && delta_y != 0 {
                    self.allowed(TileCoord::new(tile.x + delta_x, tile.y), search.goal)
                        && self.allowed(TileCoord::new(tile.x, tile.y + delta_y), search.goal)
                        && diagonal_clear_checked(
                            terrain, overlay, tile, delta_x, delta_y, cancelled,
                        )?
                } else {
                    true
                };
                if !self.allowed(next, search.goal)
                    || !checked_walkable(terrain, overlay, next, cancelled)?
                    || !matches!(
                        terrain.edge_between_with_cancel(tile, next, cancelled)?,
                        EdgePassability::Passable
                    )
                    || !diagonal_ok
                {
                    continue;
                }
                let next_cost = cost.saturating_add(u64::from(checked_movement_cost(
                    terrain, tile, next, cancelled,
                )?));
                if search
                    .scores
                    .get(&next)
                    .is_some_and(|known| *known <= next_cost)
                {
                    continue;
                }
                let retained_after_insert = search
                    .open
                    .len()
                    .saturating_add(search.scores.len())
                    .saturating_add(search.parents.len())
                    .saturating_add(1)
                    .saturating_add(usize::from(!search.scores.contains_key(&next)))
                    .saturating_add(usize::from(!search.parents.contains_key(&next)));
                if retained_after_insert > MAX_ROUTE_PLANNER_NODES {
                    return Err(Terminal::SearchLimit);
                }
                search.scores.insert(next, next_cost);
                search.parents.insert(next, tile);
                search.open.insert(OpenNode::new(
                    next_cost.saturating_add(heuristic(next, search.goal)),
                    next_cost,
                    next,
                ));
            }
        }
        Ok(())
    }

    fn allowed(&self, tile: TileCoord, goal: TileCoord) -> bool {
        self.portals.as_ref().is_none_or(|portals| {
            portals.is_empty() || tile == goal || same_fine_chunk(self.origin, tile)
        })
    }

    fn finish_path(
        &mut self,
        terrain: &MapChunkGenerator,
        goal: TileCoord,
        cost: u64,
        parents: &BTreeMap<TileCoord, TileCoord>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Path, Terminal> {
        let mut tiles = vec![goal];
        let mut current = goal;
        while current != self.origin {
            let Some(parent) = parents.get(&current).copied() else {
                return Err(Terminal::Unreachable);
            };
            current = parent;
            tiles.push(current);
        }
        tiles.reverse();
        Ok(super::segment_path_checked(
            terrain,
            Path { tiles, cost },
            Some(32),
            cancelled,
        )?)
    }

    fn retained_nodes(&self) -> usize {
        self.search.as_ref().map_or(0, |search| {
            search
                .open
                .len()
                .saturating_add(search.scores.len())
                .saturating_add(search.parents.len())
        })
    }

    fn finish_terminal(&mut self, terminal: Terminal) -> RoutePlannerPoll {
        let result = terminal.result();
        self.search = None;
        self.terminal = Some(terminal);
        result
    }
}

impl SearchState {
    fn new(origin: TileCoord, goal: TileCoord) -> Self {
        let mut open = BTreeSet::new();
        open.insert(OpenNode::new(heuristic(origin, goal), 0, origin));
        Self {
            goal,
            open,
            scores: BTreeMap::from([(origin, 0)]),
            parents: BTreeMap::new(),
        }
    }
}

impl Terminal {
    fn result(&self) -> RoutePlannerPoll {
        match self {
            Self::Path(path) => RoutePlannerPoll::Path(path.clone()),
            Self::InvalidDestination => RoutePlannerPoll::InvalidDestination,
            Self::Unreachable => RoutePlannerPoll::Unreachable,
            Self::SearchLimit => RoutePlannerPoll::SearchLimit,
            Self::Environment(error) => RoutePlannerPoll::Environment(*error),
        }
    }
}

#[cfg(test)]
#[path = "planner_tests.rs"]
mod tests;

fn checked_walkable(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, Terminal> {
    if cancelled() {
        return Err(Terminal::Environment(EnvironmentPageError::Cancelled));
    }
    let Some(sample) = terrain.tile_at_with_cancel(tile, cancelled)? else {
        return Ok(false);
    };
    let object = terrain.object_at_with_cancel(tile, cancelled)?;
    Ok(sample.passable && object.is_none_or(|node| !overlay.blocks_node(node)))
}

fn diagonal_clear_checked(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    tile: TileCoord,
    delta_x: i32,
    delta_y: i32,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, Terminal> {
    let horizontal = TileCoord::new(tile.x + delta_x, tile.y);
    let vertical = TileCoord::new(tile.x, tile.y + delta_y);
    Ok(checked_walkable(terrain, overlay, horizontal, cancelled)?
        && checked_walkable(terrain, overlay, vertical, cancelled)?
        && matches!(
            terrain.edge_between_with_cancel(tile, horizontal, cancelled)?,
            EdgePassability::Passable
        )
        && matches!(
            terrain.edge_between_with_cancel(tile, vertical, cancelled)?,
            EdgePassability::Passable
        ))
}

fn checked_movement_cost(
    terrain: &MapChunkGenerator,
    from: TileCoord,
    to: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<u32, Terminal> {
    let base = if from.x == to.x || from.y == to.y {
        super::ORTHOGONAL_COST
    } else {
        super::DIAGONAL_COST
    };
    let multiplier = matches!(
        terrain
            .tile_at_with_cancel(to, cancelled)?
            .map(|sample| sample.material),
        Some(GroundMaterial::Mud)
    )
    .then_some(3_u32)
    .unwrap_or(2);
    Ok(base * multiplier / 2)
}
