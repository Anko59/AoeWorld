//! A bounded, complete source component. Rejected targets need no new terrain
//! search. Its tiny graph projects the ordinary A* work charge exactly, keeping
//! deterministic route admission unchanged while avoiding repeated world probes.
use super::search::{OpenNode, checked_walkable, neighbor_clear, neighbor_offset};
use super::{POCKET_CHECK_WORK, RouteTerrain, Terminal, heuristic};
use crate::{EnvironmentPageError, ResourceOverlay};
use aoe_core::{TileCoord, TileRect};
use std::collections::BTreeSet;

const MAX_TILES: usize = 32;
const RADIUS: u32 = 4;

#[derive(Debug)]
struct Node {
    tile: TileCoord,
    edges: Vec<(usize, u32)>,
}

/// A sealed local component, derived from the current route terrain and overlay.
/// Use only while that ground is unchanged; no failed environment read is retained.
#[derive(Debug)]
pub struct RouteOriginPocket {
    origin: TileCoord,
    guided: bool,
    nodes: Vec<Node>,
}

impl RouteOriginPocket {
    /// The worker tile whose complete component was certified.
    pub fn origin(&self) -> TileCoord {
        self.origin
    }

    /// None means unknown: the flood escaped its small tile/radius bound.
    pub fn build(
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Self>, EnvironmentPageError> {
        match Self::build_checked(terrain, overlay, origin, cancelled) {
            Ok(pocket) => Ok(pocket),
            Err(Terminal::Environment(error)) => Err(error),
            Err(_) => Ok(None),
        }
    }

    fn build_checked(
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<Self>, Terminal> {
        if !checked_walkable(terrain, overlay, origin, cancelled)? {
            return Ok(None);
        }
        let mut nodes = vec![Node {
            tile: origin,
            edges: Vec::new(),
        }];
        let mut cursor = 0;
        while cursor < nodes.len() {
            let from = nodes[cursor].tile;
            for direction in 0..8 {
                let offset = neighbor_offset(direction);
                if !neighbor_clear(terrain, overlay, from, offset, cancelled)? {
                    continue;
                }
                let next = TileCoord::new(from.x + offset.0, from.y + offset.1);
                let index = match nodes.iter().position(|n| n.tile == next) {
                    Some(index) => index,
                    None => {
                        if nodes.len() == MAX_TILES
                            || origin.x.abs_diff(next.x).max(origin.y.abs_diff(next.y)) > RADIUS
                        {
                            return Ok(None);
                        }
                        nodes.push(Node {
                            tile: next,
                            edges: Vec::new(),
                        });
                        nodes.len() - 1
                    }
                };
                let cost = terrain.route_step_cost(from, next, cancelled)?;
                nodes[cursor].edges.push((index, cost));
            }
            cursor += 1;
        }
        Ok(Some(Self {
            origin,
            guided: terrain.route_generator().is_some(),
            nodes,
        }))
    }

    /// For an already validated walkable destination, the exact work of an
    /// ordinary unsuccessful local A* poll. No route planner,
    /// terrain query or preparation is needed: only priorities on at most 32
    /// already certified tiles. Unknown cases use the ordinary scheduler.
    pub fn failure_work(&self, destination: TileCoord) -> Option<u32> {
        if self.nodes.iter().any(|n| n.tile == destination)
            // Far map searches can enter global guidance and approach a nearest
            // reachable point. They must retain that behavior and its charge.
            || (self.guided && self.origin.x.abs_diff(destination.x)
                .max(self.origin.y.abs_diff(destination.y)) > 64)
        {
            return None;
        }
        let mut costs = vec![u64::MAX; self.nodes.len()];
        costs[0] = 0;
        let mut open = BTreeSet::from([OpenNode {
            total: heuristic(self.origin, destination),
            cost: super::super::priority_cost(0),
            tile: self.origin,
        }]);
        let mut work = 2; // The ordinary origin and destination validations.
        while let Some(current) = open.pop_first() {
            let index = self.nodes.iter().position(|n| n.tile == current.tile)?;
            work += 1;
            if current.cost != super::super::priority_cost(costs[index]) {
                continue;
            }
            work += 8;
            if work >= POCKET_CHECK_WORK {
                return None;
            }
            for &(next, edge) in &self.nodes[index].edges {
                let cost = costs[index].saturating_add(u64::from(edge));
                if cost >= costs[next] {
                    continue;
                }
                costs[next] = cost;
                open.insert(OpenNode {
                    total: cost.saturating_add(heuristic(self.nodes[next].tile, destination)),
                    cost: super::super::priority_cost(cost),
                    tile: self.nodes[next].tile,
                });
            }
        }
        Some(work)
    }

    /// The same invalidation area as a fully exhausted local route planner.
    pub fn explored_area(&self, destination: TileCoord) -> TileRect {
        let (mut min, mut max) = (destination, destination);
        for node in &self.nodes {
            min.x = min.x.min(node.tile.x);
            min.y = min.y.min(node.tile.y);
            max.x = max.x.max(node.tile.x);
            max.y = max.y.max(node.tile.y);
        }
        TileRect::new(
            TileCoord::new(min.x - 2, min.y - 2),
            TileCoord::new(max.x + 3, max.y + 3),
        )
    }
}
