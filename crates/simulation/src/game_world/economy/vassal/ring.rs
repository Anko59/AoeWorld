//! The domain wall: a square ring at `RING` tiles around the castle, the
//! area of the Steward's circle. Only open land approaches are walled: water,
//! cliffs, forest and the map edge are already barriers. Each side carries a
//! gate along its own axis, at its middle and near both ends.
use super::plan::Site;
use super::*;
use crate::TileVisibility;

/// Half side of the wall square: its area (4 R^2) is close to the Steward
/// circle's (pi r^2) and its corners stay inside the domain radius.
pub(super) const RING: i32 = VASSAL_RADIUS * 7 / 10;
/// How far along a side a gate may move to find four open tiles.
const GATE_SHIFT: i32 = 12;

/// `true` when the tile belongs in the wall line: owned barrier already, or
/// open explored ground with no building.
fn open(w: &GameWorld, player: PlayerId, tile: TileCoord) -> bool {
    if tile.x <= 0
        || tile.y <= 0
        || tile.x >= w.config.width_tiles - 1
        || tile.y >= w.config.height_tiles - 1
    {
        return false;
    }
    if let Some(b) = w.building_at(tile).and_then(|id| w.building(id)) {
        return b.player == player && b.kind.is_barrier();
    }
    w.tile_passable(tile) && w.visibility(player, tile) != TileVisibility::Unexplored
}

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    age: crate::Age,
) -> Vec<Site> {
    let (wall, gate) = if age >= crate::Age::Castle {
        (BuildingKind::StoneWall, BuildingKind::StoneGate)
    } else {
        (BuildingKind::PalisadeWall, BuildingKind::PalisadeGate)
    };
    let mut out = Vec::new();
    // Sides as (start, step, vertical): north, south run along x; west, east along y.
    let (min, max) = (
        TileCoord::new(center.x - RING, center.y - RING),
        TileCoord::new(center.x + RING, center.y + RING),
    );
    let sides = [
        (min, (1, 0), false, 2 * RING + 1),
        (TileCoord::new(min.x, max.y), (1, 0), false, 2 * RING + 1),
        (TileCoord::new(min.x, min.y + 1), (0, 1), true, 2 * RING - 1),
        (TileCoord::new(max.x, min.y + 1), (0, 1), true, 2 * RING - 1),
    ];
    for (start, (dx, dy), vertical, len) in sides {
        let at = |i: i32| TileCoord::new(start.x + dx * i, start.y + dy * i);
        let line: Vec<bool> = (0..len).map(|i| open(w, player, at(i))).collect();
        if !line.iter().any(|o| *o) {
            continue;
        }
        let run = |i: i32| (i..i + 4).all(|j| j >= 0 && j < len && line[j as usize]);
        let mut gates: Vec<i32> = Vec::new();
        // Thirds of a long side, so traffic never walks far to a gate.
        for i in [len / 2 - 2, len / 6 - 2, len * 5 / 6 - 2] {
            let found = (0..=GATE_SHIFT)
                .flat_map(|s| [i - s, i + s])
                .find(|&g| run(g) && gates.iter().all(|&o| (o - g).abs() >= 4));
            if let Some(g) = found {
                gates.push(g);
            }
        }
        gates.sort_unstable();
        for &g in &gates {
            out.push((gate, at(g), vertical));
        }
        for i in 0..len {
            if line[i as usize] && !gates.iter().any(|&g| (g..g + 4).contains(&i)) {
                out.push((wall, at(i), false));
            }
        }
    }
    out
}
