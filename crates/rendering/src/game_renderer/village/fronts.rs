//! Resolve overlaps between standing actors and nearby footprint-sorted sprites.
use super::*;
use bounds::{
    ROCK_HALF_EXTENT, bridge_deck, building_depth, building_half_extent, footprint_depth,
};
use std::collections::HashMap;

/// Cell side, in tiles, of the lookup that keeps the scan off distant buildings.
const CELL: f64 = 8.0;
/// Allow sprite overlap around the footprint while keeping queries local.
const REACH: f64 = 3.0;
/// Half side, in tiles, of the box a corpse covers.
const CORPSE_REACH: f64 = 1.0;
/// Tie-break above the building so the actor wins the equal-depth comparison.
const MARGIN: f64 = 0.01;
/// Levels apart beyond which a footprint cannot be the thing an actor stands by.
const LEVEL_TOLERANCE: f64 = 0.5;

#[derive(Clone, Copy)]
struct Footprint {
    lower: [f64; 2],
    upper: [f64; 2],
    elevation: f64,
    depth: f64,
    /// A corpse covers every actor standing within its box, not just before it.
    beside: bool,
}

#[derive(Default)]
pub(super) struct Fronts {
    cells: HashMap<[i32; 2], Vec<Footprint>>,
}

fn cell(value: f64) -> i32 {
    (value / CELL).floor() as i32
}

impl Fronts {
    pub(super) fn new(art: &GameArt, units: &[SceneUnit], resources: &[SceneResource]) -> Self {
        let mut fronts = Self::default();
        for unit in units {
            if unit.kind.is_corpse() {
                let elevation = f64::from(unit.elevation_meters);
                let base = surface_depth([unit.position[0], unit.position[1], elevation]);
                fronts.insert(Footprint {
                    lower: unit.position.map(|v| v - CORPSE_REACH),
                    upper: unit.position.map(|v| v + CORPSE_REACH),
                    elevation,
                    depth: base + super::bounds::CORPSE_CEILING,
                    beside: true,
                });
                continue;
            }
            if unit.kind.building_side().is_none()
                || unit.visual.ghost()
                || bridge_deck(*unit)
                || unit.kind.building_id() == Some(50)
            {
                continue;
            }
            let id = unit.kind.oriented_building_id(unit.facing);
            let art = art.compositions.iter().find(|b| Some(b.unit_id) == id);
            fronts.insert_footprint(
                unit.position,
                f64::from(unit.elevation_meters),
                building_half_extent(*unit, art),
                building_depth(*unit, art),
            );
        }
        for &rock in resources.iter().filter(|r| r.kind == SceneResource::ROCK) {
            if !environment::present(art, rock) && scenery::frame(art, rock).is_none() {
                continue;
            }
            fronts.insert_footprint(
                rock.position,
                rock.elevation_meters,
                ROCK_HALF_EXTENT,
                footprint_depth(rock.position, rock.elevation_meters, ROCK_HALF_EXTENT),
            );
        }
        fronts
    }

    fn insert_footprint(&mut self, center: [f64; 2], elevation: f64, half: [f64; 2], depth: f64) {
        self.insert(Footprint {
            lower: [0, 1].map(|axis| center[axis] - half[axis]),
            upper: [0, 1].map(|axis| center[axis] + half[axis]),
            elevation,
            depth,
            beside: false,
        });
    }

    fn insert(&mut self, entry: Footprint) {
        let reach = if entry.beside { 0.0 } else { REACH };
        let (x0, x1) = (cell(entry.lower[0] - reach), cell(entry.upper[0] + reach));
        let (y0, y1) = (cell(entry.lower[1] - reach), cell(entry.upper[1] + reach));
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.cells.entry([x, y]).or_default().push(entry);
            }
        }
    }

    /// Choose actor order at the ground contact point. Both the near face
    /// and the projected width come from the footprint.
    pub(super) fn actor_depth(&self, position: [f64; 2], elevation: f64, depth: f64) -> f64 {
        let Some(list) = self.cells.get(&[cell(position[0]), cell(position[1])]) else {
            return depth;
        };
        let mut floor = f64::NEG_INFINITY;
        let mut ceiling = f64::INFINITY;
        let lateral = position[0] - position[1];
        let ground_depth = surface_depth([position[0], position[1], elevation]);
        for b in list {
            if (elevation - b.elevation).abs() > LEVEL_TOLERANCE
                || (0..2).any(|axis| {
                    position[axis] < b.lower[axis] - REACH || position[axis] > b.upper[axis] + REACH
                })
            {
                continue;
            }
            if b.beside {
                if (0..2)
                    .all(|axis| position[axis] >= b.lower[axis] && position[axis] <= b.upper[axis])
                {
                    floor = floor.max(b.depth + MARGIN);
                }
                continue;
            }
            let in_front = (position[0] >= b.upper[0] && position[1] >= b.lower[1])
                || (position[1] >= b.upper[1] && position[0] >= b.lower[0]);
            if in_front {
                floor = floor.max(b.depth + MARGIN);
            } else if (b.lower[0] - b.upper[1]..=b.upper[0] - b.lower[1]).contains(&lateral)
                && ground_depth < b.depth - MARGIN
            {
                ceiling = ceiling.min(b.depth - MARGIN);
            }
        }
        depth.max(floor).min(ceiling)
    }
}
