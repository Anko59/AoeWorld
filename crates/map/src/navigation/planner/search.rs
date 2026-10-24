use super::{MAX_ROUTE_PLANNER_NODES, RouteTerrain, Terminal};
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::TileCoord;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SearchState {
    pub(super) open: BTreeSet<OpenNode>,
    pub(super) records: BTreeMap<TileCoord, SearchRecord>,
    pub(super) active: Option<ActiveExpansion>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SearchRecord {
    pub(super) cost: u64,
    pub(super) parent: Option<TileCoord>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ActiveExpansion {
    pub(super) tile: TileCoord,
    pub(super) cost: u64,
    pub(super) next_neighbor: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OpenNode {
    pub(super) total: u64,
    pub(super) cost: u64,
    pub(super) tile: TileCoord,
}

impl OpenNode {
    fn new(total: u64, cost: u64, tile: TileCoord) -> Self {
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

impl SearchState {
    pub(super) fn new(origin: TileCoord, destination: TileCoord) -> Self {
        let mut open = BTreeSet::new();
        open.insert(OpenNode::new(
            super::heuristic(origin, destination),
            super::super::priority_cost(0),
            origin,
        ));
        Self {
            open,
            records: BTreeMap::from([(
                origin,
                SearchRecord {
                    cost: 0,
                    parent: None,
                },
            )]),
            active: None,
        }
    }
}

pub(super) struct ExpansionContext<'a> {
    destination: TileCoord,
    terrain: &'a dyn RouteTerrain,
    overlay: &'a ResourceOverlay,
    cancelled: &'a dyn Fn() -> bool,
    proof_overlay: Option<&'a ResourceOverlay>,
}

impl<'a> ExpansionContext<'a> {
    pub(super) fn new(
        destination: TileCoord,
        terrain: &'a dyn RouteTerrain,
        overlay: &'a ResourceOverlay,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            destination,
            terrain,
            overlay,
            cancelled,
            proof_overlay: None,
        }
    }

    pub(super) fn with_proof_overlay(mut self, overlay: Option<&'a ResourceOverlay>) -> Self {
        self.proof_overlay = overlay;
        self
    }

    pub(super) fn expand_neighbor(
        &self,
        search: &mut SearchState,
        active: ActiveExpansion,
        offset: (i32, i32),
    ) -> Result<(), Terminal> {
        let (delta_x, delta_y) = offset;
        let next = TileCoord::new(active.tile.x + delta_x, active.tile.y + delta_y);
        let diagonal_ok = if delta_x != 0 && delta_y != 0 {
            diagonal_clear_checked(
                self.terrain,
                self.overlay,
                active.tile,
                delta_x,
                delta_y,
                self.cancelled,
            )?
        } else {
            true
        };
        if !checked_walkable(self.terrain, self.overlay, next, self.cancelled)?
            || !self
                .terrain
                .route_edge_clear(active.tile, next, self.cancelled)?
            || !diagonal_ok
        {
            return Ok(());
        }
        if let Some(snapshot) = self.proof_overlay
            && (!checked_walkable(self.terrain, snapshot, next, self.cancelled)?
                || !self
                    .terrain
                    .route_edge_clear(next, active.tile, self.cancelled)?
                || (delta_x != 0
                    && delta_y != 0
                    && (!diagonal_clear_checked(
                        self.terrain,
                        self.overlay,
                        next,
                        -delta_x,
                        -delta_y,
                        self.cancelled,
                    )? || !diagonal_clear_checked(
                        self.terrain,
                        snapshot,
                        active.tile,
                        delta_x,
                        delta_y,
                        self.cancelled,
                    )? || !diagonal_clear_checked(
                        self.terrain,
                        snapshot,
                        next,
                        -delta_x,
                        -delta_y,
                        self.cancelled,
                    )?)))
        {
            return Ok(());
        }
        let next_cost = active
            .cost
            .saturating_add(u64::from(self.terrain.route_step_cost(
                active.tile,
                next,
                self.cancelled,
            )?));
        if search
            .records
            .get(&next)
            .is_some_and(|known| known.cost <= next_cost)
        {
            return Ok(());
        }
        let new_record = !search.records.contains_key(&next);
        let retained_after_insert = search
            .open
            .len()
            .saturating_add(search.records.len())
            .saturating_add(usize::from(search.active.is_some()))
            .saturating_add(1)
            .saturating_add(usize::from(new_record));
        if retained_after_insert > MAX_ROUTE_PLANNER_NODES {
            return Err(Terminal::SearchLimit);
        }
        search.records.insert(
            next,
            SearchRecord {
                cost: next_cost,
                parent: Some(active.tile),
            },
        );
        search.open.insert(OpenNode::new(
            next_cost.saturating_add(super::heuristic(next, self.destination)),
            super::super::priority_cost(next_cost),
            next,
        ));
        Ok(())
    }
}

pub(super) fn neighbor_offset(index: u8) -> (i32, i32) {
    match index {
        0 => (-1, -1),
        1 => (0, -1),
        2 => (1, -1),
        3 => (-1, 0),
        4 => (1, 0),
        5 => (-1, 1),
        6 => (0, 1),
        _ => (1, 1),
    }
}

pub(super) fn direction_index(delta_x: i32, delta_y: i32) -> Option<u8> {
    match (delta_x, delta_y) {
        (-1, -1) => Some(0),
        (0, -1) => Some(1),
        (1, -1) => Some(2),
        (-1, 0) => Some(3),
        (1, 0) => Some(4),
        (-1, 1) => Some(5),
        (0, 1) => Some(6),
        (1, 1) => Some(7),
        _ => None,
    }
}

pub(super) fn checked_walkable(
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    tile: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, Terminal> {
    if cancelled() {
        return Err(Terminal::Environment(EnvironmentPageError::Cancelled));
    }
    Ok(terrain.route_walkable(overlay, tile, cancelled)?)
}

fn diagonal_clear_checked(
    terrain: &dyn RouteTerrain,
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
        && terrain.route_edge_clear(tile, horizontal, cancelled)?
        && terrain.route_edge_clear(tile, vertical, cancelled)?)
}

#[path = "search/tests.rs"]
#[cfg(test)]
mod tests;
