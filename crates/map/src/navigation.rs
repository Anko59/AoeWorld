use crate::{EdgePassability, GroundMaterial, MapChunkGenerator, ResourceOverlay};
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

pub const ORTHOGONAL_COST: u32 = 1_024;
pub const DIAGONAL_COST: u32 = 1_448;
/// A long order is detailed in these bounded tile-scale segments. The caller
/// requests the next segment again after reaching its endpoint.
pub const MAX_ROUTE_SEGMENT_TILES: u32 = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Path {
    pub tiles: Vec<TileCoord>,
    pub cost: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MovementOutcome {
    Path(Path),
    InvalidDestination,
    Unreachable,
    BudgetExceeded,
}

pub fn find_path(
    terrain: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
) -> MovementOutcome {
    find_path_with(terrain, origin, destination, max_expansions, None, |tile| {
        walkable(terrain, tile)
    })
}

pub fn find_path_with_overlay(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
) -> MovementOutcome {
    find_path_with(terrain, origin, destination, max_expansions, None, |tile| {
        terrain.tile_at(tile).is_some_and(|sample| {
            sample.passable
                && terrain
                    .object_at(tile)
                    .is_none_or(|node| !overlay.blocks(terrain, node.id))
        })
    })
}

/// Finds either a complete route or the next deterministic fine-scale segment
/// of a longer order. It preserves the same passability, diagonal, resource,
/// and expansion rules as [`find_path_with_overlay`].
pub fn find_path_segment_with_overlay(
    terrain: &MapChunkGenerator,
    overlay: &ResourceOverlay,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
) -> MovementOutcome {
    find_path_with(
        terrain,
        origin,
        destination,
        max_expansions,
        Some(MAX_ROUTE_SEGMENT_TILES),
        |tile| {
            terrain.tile_at(tile).is_some_and(|sample| {
                sample.passable
                    && terrain
                        .object_at(tile)
                        .is_none_or(|node| !overlay.blocks(terrain, node.id))
            })
        },
    )
}

fn find_path_with<F>(
    terrain: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
    segment_tiles: Option<u32>,
    passable: F,
) -> MovementOutcome
where
    F: Fn(TileCoord) -> bool,
{
    if !passable(origin) || !passable(destination) {
        return MovementOutcome::InvalidDestination;
    }
    if origin == destination {
        return MovementOutcome::Path(Path {
            tiles: vec![origin],
            cost: 0,
        });
    }
    let mut open = BTreeSet::new();
    let mut g_scores = BTreeMap::new();
    let mut parents = BTreeMap::new();
    open.insert(key(heuristic(origin, destination), 0, origin));
    g_scores.insert(origin, 0_u64);
    let mut expansions = 0;
    while let Some(current) = open.pop_first() {
        let tile = TileCoord::new(current.2, current.3);
        let Some(cost) = g_scores.get(&tile).copied() else {
            continue;
        };
        if current.1 != cost {
            continue;
        }
        if tile == destination {
            return segment_path(
                terrain,
                path(origin, destination, cost, parents),
                segment_tiles,
            );
        }
        if expansions >= max_expansions {
            return MovementOutcome::BudgetExceeded;
        }
        expansions += 1;
        for (neighbor, step_cost) in neighbors(terrain, tile, &passable) {
            let next_cost = cost + u64::from(step_cost);
            if g_scores
                .get(&neighbor)
                .is_some_and(|known| *known <= next_cost)
            {
                continue;
            }
            g_scores.insert(neighbor, next_cost);
            parents.insert(neighbor, tile);
            open.insert(key(
                next_cost + heuristic(neighbor, destination),
                next_cost,
                neighbor,
            ));
        }
    }
    MovementOutcome::Unreachable
}

fn path(
    origin: TileCoord,
    destination: TileCoord,
    cost: u64,
    parents: BTreeMap<TileCoord, TileCoord>,
) -> MovementOutcome {
    let mut tiles = vec![destination];
    let mut current = destination;
    while current != origin {
        let Some(parent) = parents.get(&current).copied() else {
            return MovementOutcome::Unreachable;
        };
        current = parent;
        tiles.push(current);
    }
    tiles.reverse();
    MovementOutcome::Path(Path { tiles, cost })
}

fn segment_path(
    terrain: &MapChunkGenerator,
    outcome: MovementOutcome,
    segment_tiles: Option<u32>,
) -> MovementOutcome {
    let (mut path, limit) = match (outcome, segment_tiles) {
        (MovementOutcome::Path(path), Some(limit)) => (path, limit),
        (outcome, _) => return outcome,
    };
    let keep = usize::try_from(limit)
        .unwrap_or(usize::MAX)
        .saturating_add(1);
    if path.tiles.len() <= keep {
        return MovementOutcome::Path(path);
    }
    path.tiles.truncate(keep);
    path.cost = path
        .tiles
        .windows(2)
        .map(|pair| movement_cost(terrain, pair[0], pair[1]))
        .map(u64::from)
        .sum();
    MovementOutcome::Path(path)
}

fn neighbors<F>(terrain: &MapChunkGenerator, tile: TileCoord, passable: &F) -> Vec<(TileCoord, u32)>
where
    F: Fn(TileCoord) -> bool,
{
    let mut result = Vec::with_capacity(8);
    for delta_y in -1..=1 {
        for delta_x in -1..=1 {
            if delta_x == 0 && delta_y == 0 {
                continue;
            }
            let next = TileCoord::new(tile.x + delta_x, tile.y + delta_y);
            if !passable(next)
                || !matches!(terrain.edge_between(tile, next), EdgePassability::Passable)
            {
                continue;
            }
            if delta_x != 0
                && delta_y != 0
                && !diagonal_clear(tile, delta_x, delta_y, passable, |from, to| {
                    terrain.edge_between(from, to)
                })
            {
                continue;
            }
            result.push((next, movement_cost(terrain, tile, next)));
        }
    }
    result
}

fn movement_cost(terrain: &MapChunkGenerator, from: TileCoord, to: TileCoord) -> u32 {
    let base = if from.x == to.x || from.y == to.y {
        ORTHOGONAL_COST
    } else {
        DIAGONAL_COST
    };
    let multiplier = matches!(
        terrain.tile_at(to).map(|sample| sample.material),
        Some(GroundMaterial::Mud)
    )
    .then_some(3_u32)
    .unwrap_or(2);
    base * multiplier / 2
}

fn diagonal_clear<F, E>(
    tile: TileCoord,
    delta_x: i32,
    delta_y: i32,
    passable: &F,
    edge_between: E,
) -> bool
where
    F: Fn(TileCoord) -> bool,
    E: Fn(TileCoord, TileCoord) -> EdgePassability,
{
    let horizontal = TileCoord::new(tile.x + delta_x, tile.y);
    let vertical = TileCoord::new(tile.x, tile.y + delta_y);
    passable(horizontal)
        && passable(vertical)
        && matches!(edge_between(tile, horizontal), EdgePassability::Passable)
        && matches!(edge_between(tile, vertical), EdgePassability::Passable)
}

fn walkable(terrain: &MapChunkGenerator, tile: TileCoord) -> bool {
    terrain.tile_at(tile).is_some_and(|sample| sample.passable) && terrain.object_at(tile).is_none()
}

fn heuristic(from: TileCoord, to: TileCoord) -> u64 {
    let dx = u64::from((from.x - to.x).unsigned_abs());
    let dy = u64::from((from.y - to.y).unsigned_abs());
    let diagonal = dx.min(dy);
    diagonal * u64::from(DIAGONAL_COST) + (dx - diagonal) * u64::from(ORTHOGONAL_COST)
}

#[cfg(test)]
fn chebyshev_distance(from: TileCoord, to: TileCoord) -> u32 {
    (from.x - to.x)
        .unsigned_abs()
        .max((from.y - to.y).unsigned_abs())
}

fn key(total: u64, cost: u64, tile: TileCoord) -> (u64, u64, i32, i32) {
    (total, cost, tile.y, tile.x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn terrain() -> MapChunkGenerator {
        MapChunkGenerator::new([0; 32], 0, 64)
    }

    fn neighboring_land() -> (TileCoord, TileCoord) {
        for y in 1..63 {
            for x in 1..62 {
                let origin = TileCoord::new(x, y);
                let destination = TileCoord::new(x + 1, y);
                if walkable(&terrain(), origin)
                    && walkable(&terrain(), destination)
                    && matches!(
                        terrain().edge_between(origin, destination),
                        EdgePassability::Passable
                    )
                {
                    return (origin, destination);
                }
            }
        }
        panic!("test terrain has no neighboring passable tiles");
    }

    fn resource() -> (MapChunkGenerator, crate::ResourceNode) {
        let terrain = MapChunkGenerator::new([3; 32], 1, 128);
        let node = (0..4)
            .flat_map(|y| {
                let terrain = terrain.clone();
                (0..4).flat_map(move |x| terrain.chunk(x, y).resources)
            })
            .next()
            .expect("test terrain resource");
        (terrain, node)
    }

    fn flat_terrain() -> MapChunkGenerator {
        let level_zero = crate::ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![0; 4],
        };
        let overview = crate::ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        };
        let environment = crate::PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_000,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            elevation: crate::FieldPyramid {
                levels: vec![
                    crate::PyramidLevel {
                        samples_per_axis: 2,
                        ordered_page_root: crate::ordered_page_root(std::slice::from_ref(
                            &level_zero,
                        ))
                        .expect("level-zero root"),
                    },
                    crate::PyramidLevel {
                        samples_per_axis: 1,
                        ordered_page_root: crate::ordered_page_root(std::slice::from_ref(
                            &overview,
                        ))
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
                crate::Ratio::new(1, 1).expect("compression"),
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
    fn route_is_deterministic_and_bounded() {
        let (origin, destination) = neighboring_land();
        let first = find_path(&terrain(), origin, destination, 4_096);
        let second = find_path(&terrain(), origin, destination, 4_096);
        assert_eq!(first, second);
        assert!(matches!(
            find_path(&terrain(), origin, destination, 0),
            MovementOutcome::BudgetExceeded
        ));
    }

    #[test]
    fn blocked_or_outside_destinations_are_rejected() {
        assert_eq!(
            find_path(
                &terrain(),
                TileCoord::new(2, 2),
                TileCoord::new(-1, 2),
                4_096
            ),
            MovementOutcome::InvalidDestination
        );
    }

    #[test]
    fn depleted_resources_become_route_destinations() {
        let (terrain, node) = resource();
        let mut overlay = ResourceOverlay::default();
        assert_eq!(
            find_path_with_overlay(&terrain, &overlay, node.tile, node.tile, 4_096),
            MovementOutcome::InvalidDestination
        );
        overlay
            .deplete(&terrain, node.id, node.initial_amount)
            .expect("resource");
        assert!(matches!(
            find_path_with_overlay(&terrain, &overlay, node.tile, node.tile, 4_096),
            MovementOutcome::Path(_)
        ));
    }

    #[test]
    fn diagonal_moves_require_clear_cardinal_edges() {
        let origin = TileCoord::new(4, 4);
        assert!(!diagonal_clear(origin, 1, 1, &|_| true, |from, to| {
            if from == origin && to == TileCoord::new(5, 4) {
                EdgePassability::Blocked
            } else {
                EdgePassability::Passable
            }
        },));
        assert!(diagonal_clear(origin, 1, 1, &|_| true, |_, _| {
            EdgePassability::Passable
        },));
    }

    #[test]
    fn long_routes_return_a_bounded_deterministic_segment() {
        let terrain = flat_terrain();
        let overlay = cleared_overlay(&terrain);
        let mut fixture = None;
        'origins: for y in 1..127 {
            for x in 1..95 {
                let origin = TileCoord::new(x, y);
                let destination = TileCoord::new(x + 33, y);
                if !matches!(
                    find_path_with_overlay(&terrain, &overlay, origin, destination, 4_096),
                    MovementOutcome::Path(_)
                ) {
                    continue;
                }
                let first =
                    find_path_segment_with_overlay(&terrain, &overlay, origin, destination, 4_096);
                if let MovementOutcome::Path(path) = first
                    && path.tiles.len() > 1
                    && path.tiles.last().is_some_and(|tile| *tile != destination)
                {
                    fixture = Some((origin, destination, path));
                    break 'origins;
                }
            }
        }
        let (origin, destination, first) = fixture.expect("long generated route");
        let second = find_path_segment_with_overlay(&terrain, &overlay, origin, destination, 4_096);
        assert_eq!(MovementOutcome::Path(first.clone()), second);
        assert_eq!(first.tiles.first(), Some(&origin));
        assert_eq!(
            first
                .tiles
                .last()
                .map(|tile| chebyshev_distance(origin, *tile)),
            Some(MAX_ROUTE_SEGMENT_TILES)
        );
    }
}
