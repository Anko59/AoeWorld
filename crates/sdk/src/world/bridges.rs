//! Public bridge observations and one bounded, local AI siting heuristic.
use super::*;
use aoe_core::TileCoord;
use aoe_protocol::{BuildingKind, ConstructionState};

fn complete(b: &GameplayBuildingState) -> bool {
    b.kind == BuildingKind::Bridge && b.construction == ConstructionState::Complete
}

pub(super) fn update(
    overlay: &mut aoe_map::ResourceOverlay,
    old: Option<&GameplayBuildingState>,
    new: Option<&GameplayBuildingState>,
) {
    let old = old.filter(|b| complete(b)).map(|b| b.footprint);
    let new = new.filter(|b| complete(b)).map(|b| b.footprint);
    if old == new {
        return;
    }
    if let Some(rect) = old {
        overlay.remove_bridge(rect);
    }
    if let Some(rect) = new {
        let _ = overlay.add_bridge(rect);
    }
}

impl World {
    /// Try five cardinal spans beside a reachable bank near `origin`, toward
    /// `target`. Reads at most 210 exactly explored tiles; no new terrain fetch
    /// or global route search. The server remains the placement authority.
    pub fn find_bridge_site(
        &self,
        origin: TileCoord,
        target: TileCoord,
    ) -> Option<(TileCoord, TileCoord)> {
        if !self.in_bounds(origin) || !self.in_bounds(target) {
            return None;
        }
        let horizontal = origin.x.abs_diff(target.x) >= origin.y.abs_diff(target.y);
        let dx = if horizontal {
            (target.x - origin.x).signum()
        } else {
            0
        };
        let dy = if horizontal {
            0
        } else {
            (target.y - origin.y).signum()
        };
        if dx == 0 && dy == 0 {
            return None;
        }
        for offset in [0, -1, 1, -2, 2] {
            let bank = TileCoord::new(
                origin.x + if horizontal { 0 } else { offset },
                origin.y + if horizontal { offset } else { 0 },
            );
            if !self.fog.explored_exactly(bank) || self.catalog.passable(bank) != Some(true) {
                continue;
            }
            for step in 1..=41 {
                let tile = TileCoord::new(bank.x + dx * step, bank.y + dy * step);
                if !self.fog.explored_exactly(tile) {
                    break;
                }
                if self.catalog.water(tile) == Some(true) {
                    continue;
                }
                if step > 1 && self.catalog.passable(tile) == Some(true) {
                    let rect = aoe_simulation::GameWorld::bridge_footprint(bank, tile)?;
                    if self.buildings.values().all(|b| !overlap(b.footprint, rect))
                        && self.nodes().all(|n| {
                            !rect.contains(n.tile)
                                || self.amounts.get(&n.id).copied().unwrap_or(n.initial) == 0
                        })
                    {
                        return Some((bank, tile));
                    }
                }
                break;
            }
        }
        None
    }
}

fn overlap(a: aoe_core::TileRect, b: aoe_core::TileRect) -> bool {
    a.min.x < b.max.x && b.min.x < a.max.x && a.min.y < b.max.y && b.min.y < a.max.y
}
