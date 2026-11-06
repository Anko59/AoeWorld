//! The domain wall: a square ring at `RING` tiles around the castle, the
//! area of the Steward's circle. Only open land approaches are walled: water,
//! cliffs, forest and the map edge are already barriers. Each side carries a
//! gate along its own axis, at its middle and near both ends. The side facing
//! the threat is built first (tier 1), the others by decreasing exposure; only
//! the two most exposed sides are stone, the rest palisade (cheap and quick).
use super::plan::{Site, TIER_ROAD};
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

/// Build tier of the wall side with this exposure rank (0 = most exposed).
fn side_tier(rank: usize) -> u8 {
    [1, TIER_ROAD + 2, TIER_ROAD + 4, TIER_ROAD + 5][rank.min(3)]
}

/// Which side (0 north, 1 south, 2 west, 3 east) a point from the castle
/// lies toward.
pub(super) fn side_of(center: TileCoord, at: TileCoord) -> usize {
    let (dx, dy) = (at.x - center.x, at.y - center.y);
    if dx.abs() > dy.abs() {
        2 + usize::from(dx > 0)
    } else {
        usize::from(dy > 0)
    }
}

pub(super) fn sites(
    w: &GameWorld,
    center: TileCoord,
    player: PlayerId,
    age: crate::Age,
    toward: (i32, i32),
) -> Vec<Site> {
    // Exposure of each side: how far its outward normal points at the threat.
    let normals = [(0, -1), (0, 1), (-1, 0), (1, 0)];
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by_key(|&i| (-(normals[i].0 * toward.0 + normals[i].1 * toward.1), i));
    let rank_of = |side: usize| order.iter().position(|&i| i == side).unwrap_or(3);
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
    for (side, (start, (dx, dy), vertical, len)) in sides.into_iter().enumerate() {
        let rank = rank_of(side);
        let tier = side_tier(rank);
        let (wall, gate) = if age >= crate::Age::Castle && rank < 2 {
            (BuildingKind::StoneWall, BuildingKind::StoneGate)
        } else {
            (BuildingKind::PalisadeWall, BuildingKind::PalisadeGate)
        };
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
            out.push(Site {
                vertical,
                ..Site::new(gate, at(g), tier)
            });
        }
        for i in 0..len {
            if line[i as usize] && !gates.iter().any(|&g| (g..g + 4).contains(&i)) {
                out.push(Site::new(wall, at(i), tier));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wall_side_facing_the_threat_is_built_first_and_in_stone() {
        let config = aoe_core::WorldConfig::new(256, 256, aoe_core::Seed(7)).unwrap();
        let mut w = GameWorld::new(config).unwrap();
        w.set_fog_disabled(true);
        let center = TileCoord::new(92, 92);
        // A raider to the east: its side is tier 1 stone, the far side is
        // the last tier and palisade.
        let toward = (108, 2);
        let sites = sites(&w, center, PlayerId(0), crate::Age::Castle, toward);
        let on_side = |s: &Site, x: i32| {
            s.tile.x == x && s.tile.y > center.y - RING && s.tile.y < center.y + RING
        };
        let east = |s: &Site| on_side(s, center.x + RING);
        assert!(sites.iter().filter(|s| east(s)).all(|s| s.tier == 1));
        assert!(
            sites
                .iter()
                .filter(|s| east(s))
                .all(|s| matches!(s.kind, BuildingKind::StoneWall | BuildingKind::StoneGate))
        );
        let west: Vec<_> = sites
            .iter()
            .filter(|s| on_side(s, center.x - RING))
            .collect();
        assert!(!west.is_empty());
        assert!(west.iter().all(|s| s.tier > 5));
        assert!(west.iter().all(|s| matches!(
            s.kind,
            BuildingKind::PalisadeWall | BuildingKind::PalisadeGate
        )));
        assert_eq!(side_of(center, TileCoord::new(200, 95)), 3);
        assert_eq!(side_of(center, TileCoord::new(95, 10)), 0);
    }
}
