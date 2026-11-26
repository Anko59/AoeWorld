//! Static resource perimeters are derived once per node/kind and ground change.
//! Dynamic reservations and body clearance are still checked for each worker.
use crate::game_world::movement_step::slots::{perimeter_positions, within_work_reach};
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, WorldPosition};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub(super) struct Positions {
    pub inner: Vec<WorldPosition>,
    pub outer: Vec<WorldPosition>,
    pub contact_tiles: Vec<TileCoord>,
    pub constrained: bool,
}

#[derive(Debug, Default)]
pub(crate) struct WorkPositions {
    entries: Mutex<BTreeMap<(u8, TileCoord), Arc<Positions>>>,
    steps: Mutex<super::steps::Steps>,
}

impl WorkPositions {
    pub(crate) fn steps(&self) -> std::sync::MutexGuard<'_, super::steps::Steps> {
        self.steps.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub(crate) fn clear(&mut self) {
        self.steps
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.entries
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub(crate) fn invalidate(&mut self, area: aoe_core::TileRect) {
        self.steps
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .invalidate(area);
        // Perimeter bodies can touch the resource's neighbouring ground.
        self.entries
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|&(_, tile), _| {
                tile.x + 2 < area.min.x
                    || tile.y + 2 < area.min.y
                    || tile.x - 2 >= area.max.x
                    || tile.y - 2 >= area.max.y
            });
    }
}

impl GameWorld {
    pub(super) fn static_work_positions(
        &self,
        kind: UnitKind,
        target: TileCoord,
    ) -> Arc<Positions> {
        let key = (kind as u8, target);
        let mut entries = self
            .economy
            .work_positions
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(points) = entries.get(&key) {
            return Arc::clone(points);
        }
        let mut inner = work_candidates(kind, target);
        let count = inner.len();
        inner.retain(|&at| self.interaction_static_clear(kind, at));
        let constrained = inner.len() != count;
        let mut outer = if kind.is_naval() {
            Vec::new()
        } else {
            outer_work_candidates(kind, target)
        };
        outer.retain(|&at| self.interaction_static_clear(kind, at));
        let mut contact_tiles: Vec<_> = inner
            .iter()
            .chain(&outer)
            .map(|at| at.tile_floor())
            .collect();
        contact_tiles.sort_unstable();
        contact_tiles.dedup();
        let points = Arc::new(Positions {
            inner,
            outer,
            contact_tiles,
            constrained,
        });
        // Bound derived memory even when successive jobs visit new map regions.
        if entries.len() == 4096 {
            entries.clear();
        }
        // Page failures can recover without a ground edit. Do not retain a
        // negative geometry answer if any tile used by its bodies is unknown.
        let known = (-2..=2).all(|dy| {
            (-2..=2).all(|dx| {
                self.unit_tile_passable(kind, TileCoord::new(target.x + dx, target.y + dy), &|| {
                    false
                })
                .is_ok()
            })
        });
        if known {
            entries.insert(key, Arc::clone(&points));
        }
        points
    }
}

fn work_candidates(kind: UnitKind, target: TileCoord) -> Vec<aoe_core::WorldPosition> {
    if kind.is_naval() {
        // Half-tile hull separation with quarter-tile shore clearance:
        // sample both legal lanes, instead of two outer hull corners.
        let mut points = Vec::new();
        for dy in -1..=1 {
            for dx in -1..=1 {
                for y in [256, 768] {
                    for x in [256, 768] {
                        let at = aoe_core::WorldPosition::new(
                            (target.x + dx) * 1024 + x,
                            (target.y + dy) * 1024 + y,
                        );
                        if within_work_reach(kind, at, target) {
                            points.push(at);
                        }
                    }
                }
            }
        }
        points
    } else {
        let rect = aoe_core::TileRect::from_xywh(target.x, target.y, 1, 1);
        perimeter_positions(rect, kind)
    }
}

fn outer_work_candidates(kind: UnitKind, target: TileCoord) -> Vec<aoe_core::WorldPosition> {
    let mut points = Vec::new();
    let reach = kind.collision_radius() + 104;
    let lo = aoe_core::WorldPosition::new(target.x * 1024 - reach, target.y * 1024 - reach);
    let hi =
        aoe_core::WorldPosition::new((target.x + 1) * 1024 + reach, (target.y + 1) * 1024 + reach);
    for n in 0..16 {
        let along = (hi.x - lo.x) * n / 16;
        for at in [
            aoe_core::WorldPosition::new(lo.x + along, lo.y),
            aoe_core::WorldPosition::new(lo.x + along, hi.y),
            aoe_core::WorldPosition::new(lo.x, lo.y + along),
            aoe_core::WorldPosition::new(hi.x, lo.y + along),
        ] {
            if within_work_reach(kind, at, target) {
                points.push(at);
            }
        }
    }
    points
}
