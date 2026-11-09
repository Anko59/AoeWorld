//! Derived tile ownership: completed buildings and active administrative domains.
//! Integer discs, clipped to the map; nearest source wins (ID breaks ties).
//! Row sweeps avoid work proportional to the map area or the unit population.
use crate::{BuildingKind, ConstructionState, GameWorld};
use aoe_core::{PlayerId, TileCoord};
use std::collections::BTreeMap;
use std::sync::Arc;
mod raster;
use raster::{coalesce, raster};

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
    sources: Vec<(PlayerId, TileCoord, i32)>,
    own_sources: BTreeMap<PlayerId, Vec<(PlayerId, TileCoord, i32)>>,
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
    fn refresh_land(&self, cache: &mut Cache) {
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
            if cache.epoch.is_none() || cache.sources != sources {
                cache.rows = raster(&sources, self.config.width_tiles, self.config.height_tiles);
                cache.totals.clear();
                let mut totals = BTreeMap::new();
                for s in cache.rows.values().flatten() {
                    *totals.entry(s.player).or_default() += (s.x1 - s.x0) as u64;
                }
                cache.totals = totals;
                cache.own_sources.clear();
                for source in &sources {
                    cache.own_sources.entry(source.0).or_default().push(*source);
                }
                cache.sources = sources;
                cache.rebuilds += 1;
            }
            cache.viewers.clear();
            cache.epoch = Some(epoch);
        }
    }
    /// Scalar reports share one global source scan, without building geometry.
    pub fn held_land(&self, viewer: PlayerId) -> (u64, u32) {
        let mut cache = self.economy.land.lock().unwrap_or_else(|e| e.into_inner());
        self.refresh_land(&mut cache);
        let tiles = cache.totals.get(&viewer).copied().unwrap_or(0);
        (
            tiles,
            (tiles * 1_000_000 / (self.config.width_tiles as u64 * self.config.height_tiles as u64))
                as u32,
        )
    }
    /// The cache lock covers only the shared refresh and the memo: viewers'
    /// clipped reports are built outside it, so a fan-out can build many in
    /// parallel. Two racing builds of one report are equal; the first stored
    /// is the one every caller shares.
    pub fn land_control(&self, viewer: PlayerId) -> Arc<LandControl> {
        let key = (viewer, self.fog_disabled());
        let (tiles, epoch, mut shapes, own) = {
            let mut cache = self.economy.land.lock().unwrap_or_else(|e| e.into_inner());
            self.refresh_land(&mut cache);
            if let Some(report) = cache.viewers.get(&key) {
                return report.clone();
            }
            let tiles = cache.totals.get(&viewer).copied().unwrap_or(0);
            let (shapes, own) = if self.fog_disabled() {
                (cache.rows.values().flatten().copied().collect(), Vec::new())
            } else {
                let own = cache.own_sources.get(&viewer).cloned().unwrap_or_default();
                (Vec::new(), own)
            };
            (tiles, cache.epoch, shapes, own)
        };
        if !self.fog_disabled() {
            // Intersect sparse explored runs with held rows, never extrapolating
            // an entire influence disc from a single explored building.
            let mut sources = self
                .remembered_land_buildings(viewer)
                .filter(|b| b.hp > 0 && b.state == ConstructionState::Complete)
                .filter_map(|b| {
                    let r = radius(b.kind);
                    (r > 0).then_some((
                        b.player,
                        TileCoord::new(
                            (b.footprint.min.x + b.footprint.max.x) / 2,
                            (b.footprint.min.y + b.footprint.max.y) / 2,
                        ),
                        r,
                    ))
                })
                .collect::<Vec<_>>();
            // Enemy administrators/garrisons are private: never infer their
            // domains from live state. Building influence uses last sightings.
            sources.extend(own);
            let known = raster(&sources, self.config.width_tiles, self.config.height_tiles);
            for span in known.values().flatten() {
                self.clip_land_span(viewer, *span, &mut shapes);
            }
            coalesce(&mut shapes);
        }
        let report = Arc::new(LandControl {
            tiles,
            share_ppm: (tiles * 1_000_000
                / (self.config.width_tiles as u64 * self.config.height_tiles as u64))
                as u32,
            shapes,
        });
        let mut cache = self.economy.land.lock().unwrap_or_else(|e| e.into_inner());
        if cache.epoch != epoch {
            // A refresh ran meanwhile (only possible outside a read-only
            // fan-out): this report belongs to the old epoch, so do not keep it.
            return report;
        }
        cache.viewers.entry(key).or_insert(report).clone()
    }
    pub fn land_control_rebuilds(&self) -> u64 {
        self.economy
            .land
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .rebuilds
    }
}

#[cfg(test)]
mod tests;
