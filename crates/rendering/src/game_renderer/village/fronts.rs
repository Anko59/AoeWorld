//! Standing actors stay in front of the flat sprites of the buildings they
//! stand before and of the corpses lying beside them.
use super::*;
use std::collections::HashMap;

/// Cell side, in tiles, of the lookup that keeps the scan off distant buildings.
const CELL: f64 = 8.0;
/// A building sprite has one depth, centre plus `BUILDING_CLEARANCE`, while an
/// actor's depth is its own position plus a smaller clearance. An actor in front
/// of the footprint but within that difference of its centre line would lose the
/// comparison and vanish behind the wall.
const REACH: f64 = super::super::ground::BUILDING_CLEARANCE;
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
    pub(super) fn new(art: &GameArt, units: &[SceneUnit]) -> Self {
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
            let Some(side) = unit.kind.building_side() else {
                continue;
            };
            if unit.visual.ghost()
                || unit.kind.rubble()
                || unit
                    .kind
                    .building_slot()
                    .is_some_and(|slot| slot == 5 || (65..=70).contains(&slot))
                || unit.kind == SceneUnitKind::Farm
            {
                continue;
            }
            let id = unit.kind.building_id();
            let footprint = art
                .compositions
                .iter()
                .find(|b| Some(b.unit_id) == id)
                .map_or([side; 2], |b| b.footprint.map(f64::from));
            let lower = [0, 1].map(|axis| unit.position[axis] - footprint[axis] / 2.0);
            let upper = [0, 1].map(|axis| unit.position[axis] + footprint[axis] / 2.0);
            let entry = Footprint {
                lower,
                upper,
                elevation: f64::from(unit.elevation_meters),
                depth: object_depth(WorldObject::Unit(*unit)),
                beside: false,
            };
            fronts.insert(entry);
        }
        fronts
    }

    fn insert(&mut self, entry: Footprint) {
        let (x0, x1) = (cell(entry.lower[0]), cell(entry.upper[0] + REACH));
        let (y0, y1) = (cell(entry.lower[1]), cell(entry.upper[1] + REACH));
        for x in x0..=x1 {
            for y in y0..=y1 {
                self.cells.entry([x, y]).or_default().push(entry);
            }
        }
    }

    /// The least depth an actor at `position` needs to stay in front of every
    /// building whose footprint it stands beside or before, if any.
    pub(super) fn floor(&self, position: [f64; 2], elevation: f64) -> Option<f64> {
        let list = self.cells.get(&[cell(position[0]), cell(position[1])])?;
        let mut floor: Option<f64> = None;
        for b in list {
            let in_front = if b.beside {
                (0..2)
                    .all(|axis| position[axis] >= b.lower[axis] && position[axis] <= b.upper[axis])
            } else {
                (position[0] >= b.upper[0] && position[1] >= b.lower[1])
                    || (position[1] >= b.upper[1] && position[0] >= b.lower[0])
            };
            if in_front && (elevation - b.elevation).abs() <= LEVEL_TOLERANCE {
                floor = Some(floor.map_or(b.depth, |depth| depth.max(b.depth)));
            }
        }
        floor.map(|depth| depth + MARGIN)
    }
}
