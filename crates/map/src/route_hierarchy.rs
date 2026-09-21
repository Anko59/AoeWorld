//! Lazy navigation hierarchy for virtual maps.
//!
//! Coarse and intermediate cells guide a 32-tile fine chunk. The actual
//! crossing is chosen from passable boundary pairs at the moment it is needed,
//! so no map-scale portal or connectivity table is allocated.
use crate::{EdgePassability, EnvironmentPageError, MapChunkGenerator, ResourceOverlay};
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

pub(crate) fn portal_candidates_checked(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    destination: TileCoord,
    cancelled: &dyn Fn() -> bool,
) -> Result<Vec<TileCoord>, EnvironmentPageError> {
    if same_cell(origin, destination, FINE_TILES) {
        return Ok(Vec::new());
    }
    let guidance = hierarchy_boundary(origin, destination);
    let mut candidates = Vec::new();
    for (from, to) in boundary_portals(origin) {
        let from_sample = terrain
            .tile_at_with_cancel(from, cancelled)?
            .filter(|sample| sample.passable);
        let to_sample = terrain
            .tile_at_with_cancel(to, cancelled)?
            .filter(|sample| sample.passable);
        if from_sample.is_none() || to_sample.is_none() {
            continue;
        }
        let from_object = terrain.object_at_with_cancel(from, cancelled)?;
        let to_object = terrain.object_at_with_cancel(to, cancelled)?;
        if from_object.is_some_and(|node| overlay.blocks_node(node))
            || to_object.is_some_and(|node| overlay.blocks_node(node))
            || !matches!(
                terrain.edge_between_with_cancel(from, to, cancelled)?,
                EdgePassability::Passable
            )
        {
            continue;
        }
        candidates.push(to);
    }
    candidates.sort_by_key(|tile| (distance(*tile, guidance), tile.y, tile.x));
    Ok(candidates)
}

pub(crate) fn hierarchy_boundary(origin: TileCoord, destination: TileCoord) -> TileCoord {
    for span in [COARSE_TILES, INTERMEDIATE_TILES, FINE_TILES] {
        if !same_cell(origin, destination, span) {
            return boundary_toward(origin, destination, span);
        }
    }
    destination
}

pub(crate) fn same_intermediate_region(origin: TileCoord, destination: TileCoord) -> bool {
    same_cell(origin, destination, INTERMEDIATE_TILES)
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
    terrain.tile_at(tile).is_some_and(|sample| sample.passable)
        && terrain
            .object_at_with_cancel(tile, &|| false)
            .is_ok_and(|object| object.is_none_or(|node| !overlay.blocks_node(node)))
}

fn same_cell(left: TileCoord, right: TileCoord, span: i32) -> bool {
    left.x.div_euclid(span) == right.x.div_euclid(span)
        && left.y.div_euclid(span) == right.y.div_euclid(span)
}

fn distance(left: TileCoord, right: TileCoord) -> u64 {
    u64::from((left.x - right.x).unsigned_abs()) + u64::from((left.y - right.y).unsigned_abs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, MovementOutcome,
        PreparedEnvironment, PyramidLevel, Ratio, find_path_segment_with_overlay,
        find_path_with_overlay, ordered_page_root,
    };

    fn flat_terrain() -> MapChunkGenerator {
        let level_zero = ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![0; 4],
        };
        let overview = ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        };
        let environment = PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid {
                levels: vec![
                    PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&level_zero))
                            .expect("level-zero root"),
                    },
                    PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                            .expect("overview root"),
                    },
                ],
            },
            water: None,
            vegetation: None,
            historical_land_use: None,
        };
        MapChunkGenerator::new([0; 32], 0, 128)
            .with_prepared_elevation(
                Ratio::new(1, 1).expect("compression"),
                &environment,
                vec![level_zero, overview],
            )
            .expect("flat terrain")
    }

    fn cleared_overlay(terrain: &MapChunkGenerator) -> ResourceOverlay {
        let mut overlay = ResourceOverlay::default();
        for y in 0..128 {
            for x in 0..128 {
                if let Some(node) = terrain.object_at(TileCoord::new(x, y)) {
                    overlay
                        .deplete(terrain, node.id, node.initial_amount)
                        .expect("resource");
                }
            }
        }
        overlay
    }

    #[test]
    fn long_route_keeps_a_deterministic_bounded_segment() {
        let terrain = flat_terrain();
        let overlay = cleared_overlay(&terrain);
        let fixture = (1..127).find_map(|y| {
            (1..95).find_map(|x| {
                let origin = TileCoord::new(x, y);
                let destination = TileCoord::new(x + 33, y);
                matches!(
                    find_path_with_overlay(&terrain, &overlay, origin, destination, 4_096),
                    MovementOutcome::Path(_)
                )
                .then(|| {
                    find_path_segment_with_overlay(&terrain, &overlay, origin, destination, 4_096)
                })
                .and_then(|outcome| match outcome {
                    MovementOutcome::Path(path) if path.tiles.len() > 1 => (path.tiles.last()
                        != Some(&destination))
                    .then_some((origin, destination, path)),
                    _ => None,
                })
            })
        });
        let (origin, destination, first) = fixture.expect("long generated route");
        assert_eq!(
            find_path_segment_with_overlay(&terrain, &overlay, origin, destination, 4_096),
            MovementOutcome::Path(first.clone())
        );
        assert_eq!(first.tiles.first(), Some(&origin));
        assert!(first.tiles.last().is_some_and(|tile| {
            (origin.x - tile.x)
                .unsigned_abs()
                .max((origin.y - tile.y).unsigned_abs())
                <= 32
        }));
    }
}
