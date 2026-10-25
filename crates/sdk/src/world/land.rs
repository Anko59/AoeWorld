//! Bounded queries over explored public terrain. Unknown ground never proves
//! a connection, and a one-tile river remains a barrier between components.
use crate::World;
use aoe_core::TileCoord;
use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
};

const MAX_LAND_NODES: usize = 8_192;
type Records = HashMap<TileCoord, (u32, Option<TileCoord>)>;

fn distance(a: TileCoord, b: TileCoord) -> u32 {
    let dx = a.x.abs_diff(b.x);
    let dy = a.y.abs_diff(b.y);
    10 * dx.max(dy) + 4 * dx.min(dy)
}

fn trace(records: &Records, mut tile: TileCoord) -> Vec<TileCoord> {
    let mut path = vec![tile];
    while let Some((_, Some(parent))) = records.get(&tile) {
        tile = *parent;
        path.push(tile);
    }
    path.reverse();
    path
}

impl World {
    /// A route through explored land, ignoring temporary unit/building
    /// occupancy. None means unknown, disconnected, or the fixed query bound
    /// was reached; it is never permission to march across unknown water.
    pub fn land_route(&self, from: TileCoord, to: TileCoord) -> Option<Vec<TileCoord>> {
        if self.map_hash.is_none() {
            return self.flat_land_route(from, to);
        }
        self.search_land(from, to, false)
    }

    /// A reachable explored tile toward an exploration goal. Walking there
    /// reveals more terrain without sending an army across an unproven river.
    pub fn land_frontier(&self, from: TileCoord, toward: TileCoord) -> Option<TileCoord> {
        if !self.in_bounds(toward) {
            return None;
        }
        if self.map_hash.is_none() {
            return self
                .flat_land_route(from, toward)
                .and_then(|path| path.last().copied());
        }
        self.search_land(from, toward, true)
            .and_then(|path| path.last().copied())
    }

    fn in_bounds(&self, tile: TileCoord) -> bool {
        tile.x >= 0 && tile.y >= 0 && tile.x < self.width && tile.y < self.height
    }

    fn flat_land_route(&self, from: TileCoord, to: TileCoord) -> Option<Vec<TileCoord>> {
        if !self.in_bounds(from) || !self.in_bounds(to) {
            return None;
        }
        let steps = from.x.abs_diff(to.x).max(from.y.abs_diff(to.y));
        if steps as usize >= MAX_LAND_NODES {
            return None;
        }
        let mut path = vec![from];
        let mut at = from;
        while at != to {
            at = TileCoord::new(at.x + (to.x - at.x).signum(), at.y + (to.y - at.y).signum());
            path.push(at);
        }
        Some(path)
    }

    fn known_land(&self, tile: TileCoord) -> bool {
        self.in_bounds(tile)
            && self.fog.explored_exactly(tile)
            && self.catalog.passable(tile) == Some(true)
    }

    fn land_step(&self, from: TileCoord, to: TileCoord) -> bool {
        if !self.known_land(to) || !self.catalog.land_edge(from, to) {
            return false;
        }
        if from.x != to.x && from.y != to.y {
            for corner in [TileCoord::new(from.x, to.y), TileCoord::new(to.x, from.y)] {
                if !self.known_land(corner) || !self.catalog.land_edge(from, corner) {
                    return false;
                }
            }
        }
        true
    }

    fn search_land(
        &self,
        from: TileCoord,
        goal: TileCoord,
        frontier: bool,
    ) -> Option<Vec<TileCoord>> {
        if !self.known_land(from) || (!frontier && !self.known_land(goal)) {
            return None;
        }
        let mut records = Records::from([(from, (0, None))]);
        let mut open = BinaryHeap::from([Reverse((
            2 * distance(from, goal),
            u32::MAX,
            from.y,
            from.x,
        ))]);
        let mut best = from;
        while let Some(Reverse((_, inverse, y, x))) = open.pop() {
            let at = TileCoord::new(x, y);
            let cost = u32::MAX - inverse;
            if records.get(&at).map(|record| record.0) != Some(cost) {
                continue;
            }
            if at == goal {
                return Some(trace(&records, at));
            }
            if (distance(at, goal), at.y, at.x) < (distance(best, goal), best.y, best.x) {
                best = at;
            }
            for (dx, dy) in [
                (0, -1),
                (-1, 0),
                (1, 0),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let next = TileCoord::new(x + dx, y + dy);
                let next_cost = cost + if dx == 0 || dy == 0 { 10 } else { 14 };
                if records
                    .get(&next)
                    .is_some_and(|record| record.0 <= next_cost)
                    || !self.land_step(at, next)
                {
                    continue;
                }
                if records.len() >= MAX_LAND_NODES || open.len() >= MAX_LAND_NODES {
                    return frontier
                        .then(|| trace(&records, best))
                        .filter(|path| path.len() > 1);
                }
                records.insert(next, (next_cost, Some(at)));
                open.push(Reverse((
                    next_cost + 2 * distance(next, goal),
                    u32::MAX - next_cost,
                    next.y,
                    next.x,
                )));
            }
        }
        frontier
            .then(|| trace(&records, best))
            .filter(|path| path.len() > 1)
    }
}
