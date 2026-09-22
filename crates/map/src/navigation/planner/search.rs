use super::{MAX_ROUTE_PLANNER_NODES, Terminal};
use crate::{
    EdgePassability, EnvironmentPageError, GroundMaterial, MapChunkGenerator, ResourceOverlay,
};
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
            0,
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
    terrain: &'a MapChunkGenerator,
    overlay: &'a ResourceOverlay,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> ExpansionContext<'a> {
    pub(super) fn new(
        destination: TileCoord,
        terrain: &'a MapChunkGenerator,
        overlay: &'a ResourceOverlay,
        cancelled: &'a dyn Fn() -> bool,
    ) -> Self {
        Self {
            destination,
            terrain,
            overlay,
            cancelled,
        }
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
            || !matches!(
                self.terrain
                    .edge_between_with_cancel(active.tile, next, self.cancelled)?,
                EdgePassability::Passable
            )
            || !diagonal_ok
        {
            return Ok(());
        }
        let next_cost = active.cost.saturating_add(u64::from(checked_movement_cost(
            self.terrain,
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
            next_cost,
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
        super::super::ORTHOGONAL_COST
    } else {
        super::super::DIAGONAL_COST
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
