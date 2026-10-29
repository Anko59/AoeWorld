//! Static resource perimeters are derived once per node/kind and ground change.
//! Dynamic reservations and body clearance are still checked for each worker.
use crate::game_world::movement_step::slots::{perimeter_positions, within_work_reach};
use crate::{GameWorld, UnitKind};
use aoe_core::{TileCoord, WorldPosition};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub(super) struct Positions {
    pub inner: Vec<WorldPosition>,
    pub outer: Vec<WorldPosition>,
    pub constrained: bool,
}

#[derive(Debug)]
pub(super) struct Reachable {
    pub tiles: Vec<TileCoord>,
    bounds: aoe_core::TileRect,
}

#[derive(Debug, Default)]
pub(crate) struct WorkPositions {
    entries: Mutex<BTreeMap<(u8, TileCoord), Arc<Positions>>>,
    reached: Mutex<BTreeMap<(u8, TileCoord), Arc<Reachable>>>,
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
        self.reached
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
        self.reached
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, reached| {
                reached.bounds.max.x < area.min.x
                    || reached.bounds.max.y < area.min.y
                    || reached.bounds.min.x > area.max.x
                    || reached.bounds.min.y > area.max.y
            });
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
    pub(super) fn replacement_reach(&self, kind: UnitKind, work: TileCoord) -> Arc<Reachable> {
        let key = (kind as u8, work);
        let mut entries = self
            .economy
            .work_positions
            .reached
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(reached) = entries.get(&key) {
            return Arc::clone(reached);
        }
        let mut steps = self.economy.work_positions.steps();
        let mut known = true;
        let mut seen = HashSet::<
            u64,
            std::hash::BuildHasherDefault<crate::game_world::movement_step::CellHasher>,
        >::default();
        seen.insert(crate::game_world::movement_step::tile_key(work));
        let mut queue = vec![work];
        let mut cursor = 0;
        while cursor < queue.len() {
            let tile = queue[cursor];
            cursor += 1;
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
                let next = TileCoord::new(tile.x + dx, tile.y + dy);
                let key = crate::game_world::movement_step::tile_key(next);
                if seen.contains(&key) || seen.len() >= 1024 {
                    continue;
                }
                match steps.probe(self, kind, tile, next) {
                    Ok(true) => {
                        seen.insert(key);
                        queue.push(next);
                    }
                    Ok(false) => {}
                    Err(_) => known = false,
                }
            }
        }
        let mut seen = queue;
        seen.sort_unstable();
        let (mut min, mut max) = (work, work);
        for tile in &seen {
            min.x = min.x.min(tile.x);
            min.y = min.y.min(tile.y);
            max.x = max.x.max(tile.x);
            max.y = max.y.max(tile.y);
        }
        let max = TileCoord::new(max.x + 1, max.y + 1);
        let reached = Arc::new(Reachable {
            tiles: seen,
            bounds: aoe_core::TileRect::new(min, max),
        });
        // At most 4096 exact 1024-tile proofs (32 MiB of tile coordinates).
        if entries.len() == 4096 {
            entries.clear();
        }
        if known {
            entries.insert(key, Arc::clone(&reached));
        }
        reached
    }

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
        let points = Arc::new(Positions {
            inner,
            outer,
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
