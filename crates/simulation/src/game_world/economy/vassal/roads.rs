//! Tick-local queries against the live world; no validity cache crosses ticks.
use super::plan::Site;
use super::road_cache::Memo;
use super::road_job::Network;
use super::*;
use std::ops::{Deref, DerefMut};
pub(super) const TIER_GATE_ROAD: u8 = 5;
pub(super) const MAX_NODES: usize = 64;
pub(super) const MAX_BRIDGE: i32 = 14;
const BRIDGE_SPACING: i32 = 30;
pub(super) const STUB: i32 = 3;
pub(super) type Out = Option<(i32, i32)>;
pub(super) fn trim_nodes<T>(nodes: &mut Vec<T>) {
    nodes.truncate(MAX_NODES);
}
pub(super) fn bank_steps() -> std::ops::RangeInclusive<i32> {
    2..=MAX_BRIDGE
}
pub(super) fn footprint_of(site: &Site) -> TileRect {
    let r = site.kind.rules();
    let (w, h) = if site.vertical {
        (r.depth, r.side)
    } else {
        (r.side, r.depth)
    };
    TileRect::from_xywh(site.tile.x, site.tile.y, w, h)
}

pub(super) fn center_of(r: TileRect) -> TileCoord {
    TileCoord::new((r.min.x + r.max.x) / 2, (r.min.y + r.max.y) / 2)
}

pub(super) struct View<'a> {
    pub(super) w: &'a GameWorld,
    net: &'a mut Network,
    passability: Memo<TileCoord>,
    free_tiles: Memo<TileCoord>,
    existing_tiles: Memo<TileCoord>,
    edges: Memo<(TileCoord, TileCoord)>,
    bridge_ok: BTreeMap<(TileCoord, TileCoord), bool>,
}
impl Deref for View<'_> {
    type Target = Network;
    fn deref(&self) -> &Network {
        self.net
    }
}
impl DerefMut for View<'_> {
    fn deref_mut(&mut self) -> &mut Network {
        self.net
    }
}
impl<'a> View<'a> {
    pub(super) fn new(w: &'a GameWorld, net: &'a mut Network) -> Self {
        #[cfg(test)]
        super::super::bridge::work::begin_plan();
        Self {
            w,
            net,
            passability: Memo::default(),
            free_tiles: Memo::default(),
            existing_tiles: Memo::default(),
            edges: Memo::default(),
            bridge_ok: BTreeMap::new(),
        }
    }
    pub(super) fn passable(&self, tile: TileCoord) -> bool {
        self.passability.get(tile, || {
            #[cfg(test)]
            super::super::bridge::work::tile_read(tile);
            self.w.tile_passable(tile)
        })
    }

    pub(super) fn crossable(&self, from: TileCoord, to: TileCoord) -> bool {
        self.edges.get((from, to), || {
            self.w.terrain.crossable(from, to, self.w.config)
        })
    }
    /// Free open ground a road tile may be laid on.
    pub(super) fn free(&self, tile: TileCoord) -> bool {
        self.free_tiles.get(tile, || {
            !self.avoid.contains(&tile)
                && !self.gate_tiles.contains(&tile)
                && self.w.building_at(tile).is_none()
                && self.passable(tile)
        })
    }

    /// An own road, finished gate or bridge (a foundation too: re-planning
    /// must keep the span it already laid, not lay a parallel one).
    pub(super) fn existing(&self, tile: TileCoord) -> bool {
        self.existing_tiles.get(tile, || {
            self.w
                .building_at(tile)
                .and_then(|id| self.w.building(id))
                .is_some_and(|b| {
                    b.player == self.player
                        && (b.kind == BuildingKind::Road
                            || b.kind == BuildingKind::Bridge
                            || b.kind.is_gate() && b.is_complete())
                })
        })
    }

    pub(super) fn walkable(&self, tile: TileCoord) -> bool {
        self.free(tile) || self.existing(tile) || self.gate_tiles.contains(&tile)
    }

    pub(super) fn lay(&mut self, tile: TileCoord, tier: u8) {
        if self.free(tile) && self.roads.insert(tile) {
            self.out.push(Site::new(BuildingKind::Road, tile, tier));
            self.goals.insert(tile);
        } else if self.existing(tile) {
            self.goals.insert(tile);
            // An unfinished road stays a deficit until it is complete.
            let unfinished = self
                .w
                .building_at(tile)
                .and_then(|id| self.w.building(id))
                .is_some_and(|b| b.kind == BuildingKind::Road && !b.is_complete());
            if unfinished && self.roads.insert(tile) {
                self.out.push(Site::new(BuildingKind::Road, tile, tier));
            }
        }
    }

    /// A bridge from bank `a` to bank `b`, checked once per pair.
    pub(super) fn bridgeable(&mut self, a: TileCoord, b: TileCoord) -> bool {
        // A second span near a laid or planned one is never wanted: cross by
        // that one and walk the bank.
        let near =
            |t: TileCoord, o: TileCoord| (t.x - o.x).abs().max((t.y - o.y).abs()) <= BRIDGE_SPACING;
        if self
            .fixed
            .keys()
            .chain(self.planned_spans.iter())
            .any(|&f| near(f, a) || near(f, b))
        {
            return false;
        }
        let w = self.w;
        *self
            .bridge_ok
            .entry((a, b))
            .or_insert_with(|| w.bridge_site_valid(a, b))
    }
}
