//! Resumable reverse flood proving a small destination pocket is sealed.
//! Every neighbour probe spends the caller's current allowance; no batch can
//! run thousands of uncharged terrain reads inside a one-work poll.
use super::search::{checked_walkable, neighbor_offset};
use super::{MAX_ROUTE_PLANNER_NODES, RoutePlanner, RouteTerrain, Terminal};
use crate::ResourceOverlay;
use aoe_core::TileCoord;
use std::collections::{BTreeSet, VecDeque};

pub(super) const POCKET_CHECK_WORK: u32 = 1_024;
const POCKET_TILES: usize = 1_024;

fn step_allowed(
    terrain: &dyn RouteTerrain,
    overlay: &ResourceOverlay,
    from: TileCoord,
    offset: (i32, i32),
    cancelled: &dyn Fn() -> bool,
) -> Result<bool, Terminal> {
    let (dx, dy) = offset;
    let next = TileCoord::new(from.x + dx, from.y + dy);
    if dx != 0 && dy != 0 {
        let horizontal = TileCoord::new(from.x + dx, from.y);
        let vertical = TileCoord::new(from.x, from.y + dy);
        if !(checked_walkable(terrain, overlay, horizontal, cancelled)?
            && checked_walkable(terrain, overlay, vertical, cancelled)?
            && terrain.route_edge_clear(from, horizontal, cancelled)?
            && terrain.route_edge_clear(from, vertical, cancelled)?)
        {
            return Ok(false);
        }
    }
    Ok(checked_walkable(terrain, overlay, next, cancelled)?
        && terrain.route_edge_clear(from, next, cancelled)?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Proof {
    seen: BTreeSet<TileCoord>,
    queue: VecDeque<TileCoord>,
    active: Option<(TileCoord, u8)>,
    min: TileCoord,
    max: TileCoord,
    peak: usize,
}
impl Proof {
    pub(super) fn new(destination: TileCoord) -> Self {
        Self {
            seen: BTreeSet::from([destination]),
            queue: VecDeque::from([destination]),
            active: None,
            min: destination,
            max: destination,
            peak: 2,
        }
    }
    pub(super) fn bounds(&self) -> [TileCoord; 2] {
        [self.min, self.max]
    }
    /// Transfer a completed proof without copying its flood. The bounds stay
    /// live so an opened entrance invalidates the resulting nearest-point trip.
    pub(super) fn take_sealed_tiles(&mut self) -> BTreeSet<TileCoord> {
        std::mem::take(&mut self.seen)
    }
    pub(super) fn opening_touches(&self, area: aoe_core::TileRect) -> bool {
        self.min.x.saturating_sub(2) < area.max.x
            && area.min.x < self.max.x.saturating_add(3)
            && self.min.y.saturating_sub(2) < area.max.y
            && area.min.y < self.max.y.saturating_add(3)
    }
    pub(super) fn touches(&self, area: aoe_core::TileRect) -> bool {
        self.seen.iter().any(|tile| {
            tile.x >= area.min.x
                && tile.x < area.max.x
                && tile.y >= area.min.y
                && tile.y < area.max.y
        })
    }
    pub(super) fn retained_entries(&self) -> usize {
        self.peak
    }
    pub(super) fn poll(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        origin: TileCoord,
        remaining: &mut u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<bool>, Terminal> {
        let _span = terrain
            .route_generator()
            .and_then(|generator| generator.measure_navigation(crate::NavigationPhase::Pocket));
        while *remaining > 0 {
            if cancelled() {
                return Err(Terminal::Environment(
                    crate::EnvironmentPageError::Cancelled,
                ));
            }
            let (tile, direction) = match self.active {
                Some((_, 8)) => {
                    self.active = None;
                    continue;
                }
                Some(active) => active,
                None => {
                    let Some(tile) = self.queue.pop_front() else {
                        return Ok(Some(true));
                    };
                    self.min = TileCoord::new(self.min.x.min(tile.x), self.min.y.min(tile.y));
                    self.max = TileCoord::new(self.max.x.max(tile.x), self.max.y.max(tile.y));
                    (tile, 0)
                }
            };
            self.active = Some((tile, direction + 1));
            *remaining -= 1;
            let (dx, dy) = neighbor_offset(direction);
            let previous = TileCoord::new(tile.x - dx, tile.y - dy);
            if self.seen.contains(&previous)
                || !checked_walkable(terrain, overlay, previous, cancelled)?
                || !step_allowed(terrain, overlay, previous, (dx, dy), cancelled)?
            {
                continue;
            }
            if previous == origin || self.seen.len() >= POCKET_TILES {
                return Ok(Some(false));
            }
            self.seen.insert(previous);
            self.queue.push_back(previous);
            self.peak = self.peak.max(self.seen.len() + self.queue.len() + 1);
        }
        Ok(None)
    }
    pub(super) fn update_hash(&self, hash: &mut blake3::Hasher) {
        hash.update(b"pocket-frontier");
        let tile = |hash: &mut blake3::Hasher, tile: TileCoord| {
            hash.update(&tile.x.to_le_bytes());
            hash.update(&tile.y.to_le_bytes());
        };
        tile(hash, self.min);
        tile(hash, self.max);
        hash.update(&(self.peak as u64).to_le_bytes());
        hash.update(&(self.seen.len() as u64).to_le_bytes());
        for at in &self.seen {
            tile(hash, *at);
        }
        hash.update(&(self.queue.len() as u64).to_le_bytes());
        for at in &self.queue {
            tile(hash, *at);
        }
        hash.update(&[u8::from(self.active.is_some())]);
        if let Some((at, direction)) = self.active {
            tile(hash, at);
            hash.update(&[direction]);
        }
    }
}

impl RoutePlanner {
    /// The same charged reverse proof precedes both a difficult local search
    /// and global guidance. A small sealed far goal must not first flood the
    /// origin's whole continent through portals.
    pub(super) fn check_destination_pocket(
        &mut self,
        terrain: &dyn RouteTerrain,
        overlay: &ResourceOverlay,
        remaining: &mut u32,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, Terminal> {
        let before = *remaining;
        let destination = self.effective_destination();
        let proof = self.pocket.get_or_insert_with(|| Proof::new(destination));
        let outcome = proof.poll(terrain, overlay, self.leg_origin, remaining, cancelled)?;
        let bounds = proof.bounds();
        self.work = self.work.saturating_add(before - *remaining);
        for corner in bounds {
            self.include_bounds(corner);
        }
        self.update_peak_retained_entries();
        if self.retained_entries() > MAX_ROUTE_PLANNER_NODES {
            return Err(Terminal::SearchLimit);
        }
        let Some(sealed) = outcome else {
            return Ok(false);
        };
        self.pocket_checked = true;
        if !sealed {
            self.pocket = None;
            return Ok(true);
        }
        let Some(generator) = terrain.route_generator() else {
            return Err(Terminal::Unreachable);
        };
        self.hierarchy = None;
        self.guidance.clear();
        generator.prepare_navigation(overlay, cancelled)?;
        self.begin_components(generator, overlay, cancelled)?;
        if let (Some(resolution), Some(proof)) = (self.resolution.as_mut(), self.pocket.as_mut()) {
            resolution.exclude_sealed(proof.take_sealed_tiles());
        }
        Ok(true)
    }
}
