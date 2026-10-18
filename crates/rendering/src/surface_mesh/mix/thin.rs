//! Streams one or two tiles wide. Shore mixing gives a vertex the water share
//! of its four incident tiles, so a channel without a 2x2 block of water never
//! passes half coverage: it draws as faded necks between full pools. A vertex
//! that touches such a thin tile is raised to a water share of 0.63, which keeps
//! the channel solid along its line and widens it by about a sixth of a tile. Thickness is read from the source tiles, never from the displayed
//! vertices, so it cannot depend on zoom, camera or chunk order.
use super::*;

/// Water coverage of a Ford: a light, sandy-looking shallow, never opaque.
/// This is the water share whose shore coverage is about 0.85.
pub(super) const FORD_SHARE: u8 = 150;

fn water(material: u8) -> bool {
    matches!(material, 5 | 10 | 12)
}

/// Vertex `v` of the tile grid touches tiles `v - 1 ..= v` on each axis.
fn interior(wet: &crate::WorldKeyIndex, vertex: [i32; 2]) -> bool {
    [[-1, -1], [0, -1], [-1, 0], [0, 0]]
        .iter()
        .all(|d| wet.get([vertex[0] + d[0], vertex[1] + d[1]]).is_some())
}

/// Overlay share (of 255) of a vertex of a thin water tile: coverage 1.
pub(super) const THIN_SHARE: u8 = 160;

/// The displayed vertices of every thin water tile. Cells keep their source
/// materials, so a land tile beside a stream never turns into a pure water
/// quad; only the share (and, on junctions, the observations) are raised.
pub(super) fn thin_vertices(
    keys: &crate::WorldKeyIndex,
    count: usize,
    terrain: &[SceneTerrain],
) -> Vec<bool> {
    let mut thin = vec![false; count];
    let mut wet = crate::WorldKeyIndex::default();
    let mut tiles = Vec::new();
    for sample in terrain {
        if ground(sample).is_some_and(|cell| water(cell.0)) {
            let tile = tile_key(sample.position);
            if wet.get(tile).is_none() {
                wet.entry(tile);
                tiles.push(tile);
            }
        }
    }
    for tile in tiles {
        let corners = [[0, 0], [1, 0], [0, 1], [1, 1]].map(|d| [tile[0] + d[0], tile[1] + d[1]]);
        if corners.iter().any(|corner| interior(&wet, *corner)) {
            continue;
        }
        for corner in corners {
            if let Some(index) = keys.get(corner) {
                thin[index] = true;
            }
        }
    }
    thin
}

/// Make three of four cells water, converting land cells in slot order.
pub(super) fn fill(cells: &mut [(u8, u16); 4]) {
    if cells.iter().any(|cell| cell.0 == MISSING) {
        return;
    }
    let count = |cells: &[(u8, u16); 4]| cells.iter().filter(|cell| water(cell.0)).count();
    let Some(sheet) = cells
        .iter()
        .map(|cell| cell.0)
        .filter(|material| water(*material))
        .max_by_key(|material| rank(*material))
    else {
        return;
    };
    let mut wet = count(cells);
    for cell in cells.iter_mut() {
        if wet >= 3 {
            break;
        }
        if !water(cell.0) {
            *cell = (sheet, 0);
            wet += 1;
        }
    }
}
