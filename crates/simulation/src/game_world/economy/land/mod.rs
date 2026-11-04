//! Derived tile ownership: completed buildings and active administrative domains.
//! Integer discs, clipped to the map; overlapping players tie to the lower ID.
//! Row sweeps avoid work proportional to the map area or the unit population.
use crate::{BuildingKind, ConstructionState, GameWorld, TileVisibility};
use aoe_core::{PlayerId, TileCoord};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HoldingSpan {
    pub player: PlayerId,
    pub y: i32,
    pub x0: i32,
    pub x1: i32,
}
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LandControl {
    pub tiles: u64,
    pub share_ppm: u32,
    pub shapes: Vec<HoldingSpan>,
}
#[derive(Debug, Default)]
pub(crate) struct Cache {
    epoch: Option<u64>,
    rows: BTreeMap<i32, Vec<HoldingSpan>>,
    totals: BTreeMap<PlayerId, u64>,
    viewers: BTreeMap<(PlayerId, bool), Arc<LandControl>>,
    rebuilds: u64,
}

fn radius(kind: BuildingKind) -> i32 {
    match kind {
        BuildingKind::TownCenter => 12,
        BuildingKind::Castle => 16,
        BuildingKind::WatchTower => 8,
        BuildingKind::Outpost => 6,
        BuildingKind::Bridge => 0,
        BuildingKind::PalisadeWall
        | BuildingKind::StoneWall
        | BuildingKind::PalisadeGate
        | BuildingKind::StoneGate => 1,
        BuildingKind::Farm => 2,
        _ => 4,
    }
}
impl GameWorld {
    /// At most one shared rebuild and one clipped report per viewer per second.
    /// New exploration is included next second. Fog mode changes clip immediately.
    /// Derived caches are deliberately excluded from the canonical replay hash.
    pub fn land_control(&self, viewer: PlayerId) -> Arc<LandControl> {
        let mut cache = self.economy.land.lock().unwrap_or_else(|e| e.into_inner());
        let epoch = self.tick.0 / 20;
        if cache.epoch != Some(epoch) {
            let mut sources = Vec::new();
            for b in self
                .buildings()
                .filter(|b| b.hp > 0 && b.state == ConstructionState::Complete)
            {
                let r = radius(b.kind);
                if r > 0 {
                    sources.push((
                        b.player,
                        TileCoord::new(
                            (b.footprint.min.x + b.footprint.max.x) / 2,
                            (b.footprint.min.y + b.footprint.max.y) / 2,
                        ),
                        r,
                    ));
                }
            }
            sources.extend(self.steward_land_sources());
            sources.extend(self.vassal_land_sources());
            cache.rows = raster(&sources, self.config.width_tiles, self.config.height_tiles);
            cache.totals.clear();
            let mut totals = BTreeMap::new();
            for s in cache.rows.values().flatten() {
                *totals.entry(s.player).or_default() += (s.x1 - s.x0) as u64;
            }
            cache.totals = totals;
            cache.viewers.clear();
            cache.epoch = Some(epoch);
            cache.rebuilds += 1;
        }
        let key = (viewer, self.fog_disabled());
        if let Some(report) = cache.viewers.get(&key) {
            return report.clone();
        }
        let tiles = cache.totals.get(&viewer).copied().unwrap_or(0);
        let mut shapes = Vec::new();
        if self.fog_disabled() {
            shapes.extend(cache.rows.values().flatten().copied());
        } else {
            // Intersect sparse explored runs with held rows, never extrapolating
            // an entire influence disc from a single explored building.
            for (y, x0, x1) in self.explored_land_runs(viewer) {
                if let Some(row) = cache.rows.get(&y) {
                    let first = row.partition_point(|s| s.x1 <= x0);
                    for s in row[first..].iter().take_while(|s| s.x0 < x1) {
                        let span = HoldingSpan {
                            x0: x0.max(s.x0),
                            x1: x1.min(s.x1),
                            ..*s
                        };
                        debug_assert!(
                            (span.x0..span.x1)
                                .all(|x| self.visibility(viewer, TileCoord::new(x, y))
                                    != TileVisibility::Unexplored)
                        );
                        shapes.push(span);
                    }
                }
            }
            shapes.sort_unstable_by_key(|s| (s.y, s.x0, s.player));
        }
        let report = Arc::new(LandControl {
            tiles,
            share_ppm: (tiles * 1_000_000
                / (self.config.width_tiles as u64 * self.config.height_tiles as u64))
                as u32,
            shapes,
        });
        cache.viewers.insert(key, report.clone());
        report
    }
    pub fn land_control_rebuilds(&self) -> u64 {
        self.economy
            .land
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rebuilds
    }
}

fn raster(
    sources: &[(PlayerId, TileCoord, i32)],
    width: i32,
    height: i32,
) -> BTreeMap<i32, Vec<HoldingSpan>> {
    let mut events = BTreeMap::<i32, Vec<(i32, PlayerId, i32)>>::new();
    for &(player, c, r) in sources {
        for y in (c.y - r).max(0)..=(c.y + r).min(height - 1) {
            let dy = y - c.y;
            let mut dx = r;
            while dx * dx + dy * dy > r * r {
                dx -= 1;
            }
            let x0 = (c.x - dx).max(0);
            let x1 = (c.x + dx + 1).min(width);
            if x0 < x1 {
                events
                    .entry(y)
                    .or_default()
                    .extend([(x0, player, 1), (x1, player, -1)]);
            }
        }
    }
    events
        .into_iter()
        .map(|(y, mut events)| {
            events.sort_unstable();
            let mut active = BTreeMap::<PlayerId, i32>::new();
            let mut row: Vec<HoldingSpan> = Vec::new();
            let mut previous = 0;
            for (x, player, delta) in events {
                if previous < x
                    && let Some((&owner, _)) = active.first_key_value()
                {
                    if let Some(last) = row
                        .last_mut()
                        .filter(|s| s.player == owner && s.x1 == previous)
                    {
                        last.x1 = x;
                    } else {
                        row.push(HoldingSpan {
                            player: owner,
                            y,
                            x0: previous,
                            x1: x,
                        });
                    }
                }
                let count = active.entry(player).or_default();
                *count += delta;
                if *count == 0 {
                    active.remove(&player);
                }
                previous = x;
            }
            (y, row)
        })
        .collect()
}

#[cfg(test)]
mod tests;
