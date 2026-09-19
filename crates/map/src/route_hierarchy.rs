//! Lazy navigation hierarchy for virtual maps.
//!
//! Coarse and intermediate cells guide a 32-tile fine chunk. The actual
//! crossing is chosen from passable boundary pairs at the moment it is needed,
//! so no map-scale portal or connectivity table is allocated.
use crate::{EdgePassability, MapChunkGenerator, ResourceOverlay};
use aoe_core::TileCoord;

const FINE_TILES: i32 = 32;
const INTERMEDIATE_TILES: i32 = 256;
const COARSE_TILES: i32 = 2_048;

pub(crate) fn portal_candidates(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    destination: TileCoord,
) -> Vec<TileCoord> {
    if same_cell(origin, destination, FINE_TILES) {
        return Vec::new();
    }
    let guidance = hierarchy_boundary(origin, destination);
    let mut candidates = boundary_portals(origin)
        .into_iter()
        .filter(|(from, to)| {
            passable(terrain, overlay, *from)
                && passable(terrain, overlay, *to)
                && matches!(terrain.edge_between(*from, *to), EdgePassability::Passable)
        })
        .map(|(_, to)| to)
        .collect::<Vec<_>>();
    candidates.sort_by_key(|tile| (distance(*tile, guidance), tile.y, tile.x));
    candidates
}

pub(crate) fn hierarchy_boundary(origin: TileCoord, destination: TileCoord) -> TileCoord {
    for span in [COARSE_TILES, INTERMEDIATE_TILES, FINE_TILES] {
        if !same_cell(origin, destination, span) {
            return boundary_toward(origin, destination, span);
        }
    }
    destination
}

fn boundary_toward(origin: TileCoord, destination: TileCoord, span: i32) -> TileCoord {
    let cell_x = origin.x.div_euclid(span);
    let cell_y = origin.y.div_euclid(span);
    let destination_x = destination.x.div_euclid(span);
    let destination_y = destination.y.div_euclid(span);
    TileCoord::new(
        boundary_axis(origin.x, cell_x, destination_x, span),
        boundary_axis(origin.y, cell_y, destination_y, span),
    )
}

fn boundary_axis(origin: i32, cell: i32, target_cell: i32, span: i32) -> i32 {
    match target_cell.cmp(&cell) {
        std::cmp::Ordering::Less => cell * span,
        std::cmp::Ordering::Equal => origin,
        std::cmp::Ordering::Greater => (cell + 1) * span - 1,
    }
}

fn boundary_portals(origin: TileCoord) -> Vec<(TileCoord, TileCoord)> {
    let left = origin.x.div_euclid(FINE_TILES) * FINE_TILES;
    let top = origin.y.div_euclid(FINE_TILES) * FINE_TILES;
    let mut portals = Vec::with_capacity((FINE_TILES * 4) as usize);
    for offset in 0..FINE_TILES {
        let x = left + offset;
        let y = top + offset;
        portals.extend([
            (TileCoord::new(x, top), TileCoord::new(x, top - 1)),
            (
                TileCoord::new(x, top + FINE_TILES - 1),
                TileCoord::new(x, top + FINE_TILES),
            ),
            (TileCoord::new(left, y), TileCoord::new(left - 1, y)),
            (
                TileCoord::new(left + FINE_TILES - 1, y),
                TileCoord::new(left + FINE_TILES, y),
            ),
        ]);
    }
    portals
}

fn passable(terrain: &MapChunkGenerator, overlay: &ResourceOverlay, tile: TileCoord) -> bool {
    terrain.tile_at(tile).is_some_and(|sample| {
        sample.passable
            && terrain
                .object_at(tile)
                .is_none_or(|node| !overlay.blocks(terrain, node.id))
    })
}

fn same_cell(left: TileCoord, right: TileCoord, span: i32) -> bool {
    left.x.div_euclid(span) == right.x.div_euclid(span)
        && left.y.div_euclid(span) == right.y.div_euclid(span)
}

fn distance(left: TileCoord, right: TileCoord) -> u64 {
    u64::from((left.x - right.x).unsigned_abs()) + u64::from((left.y - right.y).unsigned_abs())
}
