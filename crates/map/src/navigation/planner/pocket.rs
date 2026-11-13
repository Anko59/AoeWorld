//! Resumable reverse flood proving a small destination pocket is sealed.
//! Every neighbour probe spends the caller's current allowance; no batch can
//! run thousands of uncharged terrain reads inside a one-work poll.
use super::super::tile_hash::TileHasher;
use super::search::{checked_walkable, neighbor_offset};
use super::{MAX_ROUTE_PLANNER_NODES, RoutePlanner, RouteTerrain, Terminal};
use crate::{RegionReachability, ResourceOverlay};
use aoe_core::TileCoord;
use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    hash::BuildHasherDefault,
};

pub(super) const POCKET_CHECK_WORK: u32 = 1_024;
const POCKET_TILES: usize = 1_024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RegionProof {
    pub(super) from: TileCoord,
    pub(super) to: TileCoord,
    pub(super) stamp: [u8; 32],
    pub(super) reachable: RegionReachability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RegionPocket {
    pub(super) to: TileCoord,
    pub(super) snapshot: ResourceOverlay,
    pub(super) discharged: bool,
}

#[cfg(test)]
#[path = "pocket/tests/regions.rs"]
mod region_tests;

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
            && terrain.route_overlay_edge_clear(overlay, from, horizontal, cancelled)?
            && terrain.route_overlay_edge_clear(overlay, from, vertical, cancelled)?)
        {
            return Ok(false);
        }
    }
    Ok(checked_walkable(terrain, overlay, next, cancelled)?
        && terrain.route_overlay_edge_clear(overlay, from, next, cancelled)?)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Proof {
    seen: HashSet<TileCoord, BuildHasherDefault<TileHasher>>,
    queue: VecDeque<TileCoord>,
    active: Option<(TileCoord, u8)>,
    min: TileCoord,
    max: TileCoord,
    peak: usize,
}
impl Proof {
    pub(super) fn new(destination: TileCoord) -> Self {
        Self {
            seen: [destination].into_iter().collect(),
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
    /// Transfer completed membership into the ordered resolution set. The bounds stay
    /// live so an opened entrance invalidates the resulting nearest-point trip.
    pub(super) fn take_sealed_tiles(&mut self) -> BTreeSet<TileCoord> {
        std::mem::take(&mut self.seen).into_iter().collect()
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
        // Membership never determines flood order. Sort only for canonical
        // serialization, outside ordinary navigation polls.
        let mut seen: Vec<_> = self.seen.iter().copied().collect();
        seen.sort_unstable();
        for at in seen {
            tile(hash, at);
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
    fn current_region_proof(&self, overlay: &ResourceOverlay) -> bool {
        self.region_proof.as_ref().is_some_and(|proof| {
            proof.from == self.leg_origin
                && proof.to == self.effective_destination()
                && (!proof.reachable.is_reachable() || proof.stamp == overlay.navigation_stamp())
        })
    }

    pub(super) fn region_endpoint_direct(&self, overlay: &ResourceOverlay) -> bool {
        self.current_region_proof(overlay)
            && self
                .region_proof
                .as_ref()
                .is_some_and(|proof| proof.reachable == RegionReachability::Straight)
    }

    pub(super) fn refresh_region_proof(&mut self, overlay: &ResourceOverlay) {
        let goal = self.effective_destination();
        let invalid = self.region_pocket.as_ref().is_some_and(|proof| {
            proof.to != goal || !overlay.navigation_preserves_walkability(&proof.snapshot)
        });
        if invalid {
            if self
                .region_pocket
                .take()
                .is_some_and(|proof| proof.discharged)
            {
                self.pocket_checked = false;
                self.pocket = None;
            }
        } else if let Some(proof) = &mut self.region_pocket
            && proof.snapshot.navigation_stamp() != overlay.navigation_stamp()
        {
            proof.snapshot = overlay.navigation_snapshot();
        }
        if self.region_proof.as_ref().is_some_and(|proof| {
            proof.from != self.leg_origin
                || proof.to != goal
                || (proof.reachable.is_reachable() && self.region_pocket.is_none())
        }) {
            self.region_proof = None;
        } else if let Some(proof) = &mut self.region_proof
            && proof.reachable.is_reachable()
        {
            proof.stamp = overlay.navigation_stamp();
        }
    }

    /// Query a bounded region witness at most once per leg. Positive witnesses
    /// survive openings; negative and unknown answers leave path finding intact.
    /// Unknown and negative answers leave path finding and refinement intact.
    pub fn pending_region_proof(
        &mut self,
        overlay: &ResourceOverlay,
    ) -> Option<(TileCoord, TileCoord)> {
        self.refresh_region_proof(overlay);
        if self.terminal.is_some() || self.current_region_proof(overlay) {
            return None;
        }
        (self.pending_destination_proof().is_some()
            || (self.resolved_destination.is_some()
                && self.hierarchy.is_some()
                && self.resolution.is_none()
                && self.guidance.is_empty())
            || (self.early_guidance_due()
                && self.guidance.is_empty()
                && self.resolution.is_none()
                && self.resolved_destination.is_none()))
        .then(|| (self.leg_origin, self.effective_destination()))
    }

    /// A certificate replaces connectivity work, never the local path search.
    pub fn record_region_proof(
        &mut self,
        from: TileCoord,
        to: TileCoord,
        reachable: RegionReachability,
        overlay: &ResourceOverlay,
    ) {
        if from != self.leg_origin || to != self.effective_destination() {
            return;
        }
        self.region_proof = Some(RegionProof {
            from,
            to,
            stamp: overlay.navigation_stamp(),
            reachable,
        });
        if reachable.is_reachable() {
            let discharged = !self.pocket_checked
                || self
                    .region_pocket
                    .as_ref()
                    .is_some_and(|proof| proof.discharged);
            self.region_pocket = Some(RegionPocket {
                to,
                snapshot: overlay.navigation_snapshot(),
                discharged,
            });
            self.confirm_reachable_destination(from, to);
            if reachable == RegionReachability::Straight
                && self.hierarchy.is_some()
                && self.resolution.is_none()
                && self.guidance.is_empty()
                && (self.resolved_destination.is_some()
                    || (self.early_guidance_due() && !self.components_checked))
            {
                self.hierarchy = None;
                self.guidance_checked = false;
            }
        }
    }

    /// Endpoints of a reverse proof that is due or already suspended. Cheap
    /// first legs need no global query; adapters can reuse their region layer
    /// when a difficult leg would otherwise start another destination flood.
    fn pending_destination_proof(&self) -> Option<(TileCoord, TileCoord)> {
        (self.terminal.is_none()
            && !self.pocket_checked
            && (self.pocket.is_some() || self.leg_work >= POCKET_CHECK_WORK))
            .then(|| (self.leg_origin, self.effective_destination()))
    }

    /// Reuse a current directed reachability proof for this leg's endpoint.
    /// A certificate for an older leg or a different endpoint cannot discharge
    /// its reverse flood. Geometry changes remain the caller's responsibility,
    /// just as after this planner finishes its own pocket check.
    pub fn confirm_reachable_destination(&mut self, from: TileCoord, to: TileCoord) {
        if from == self.leg_origin && to == self.effective_destination() {
            let keep_seal = self.pocket_checked && self.resolved_destination.is_some();
            self.pocket_checked = true;
            if !keep_seal {
                self.pocket = None;
            }
        }
    }

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
