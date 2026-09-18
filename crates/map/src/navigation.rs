use crate::{GroundMaterial, MapChunkGenerator, ResourceOverlay};
use aoe_core::TileCoord;
use std::collections::{BTreeMap, BTreeSet};

pub const ORTHOGONAL_COST: u32 = 1_024;
pub const DIAGONAL_COST: u32 = 1_448;

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
    find_path_with(terrain, origin, destination, max_expansions, |tile| {
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
    find_path_with(terrain, origin, destination, max_expansions, |tile| {
        terrain.tile_at(tile).is_some_and(|sample| {
            sample.passable
                && terrain
                    .object_at(tile)
                    .is_none_or(|node| !overlay.blocks(terrain, node.id))
        })
    })
}

fn find_path_with<F>(
    terrain: &MapChunkGenerator,
    origin: TileCoord,
    destination: TileCoord,
    max_expansions: u32,
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
            return path(origin, destination, cost, parents);
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
            if !passable(next) || !crossable(terrain, tile, next) {
                continue;
            }
            if delta_x != 0
                && delta_y != 0
                && (!passable(TileCoord::new(tile.x + delta_x, tile.y))
                    || !passable(TileCoord::new(tile.x, tile.y + delta_y)))
            {
                continue;
            }
            let base = if delta_x == 0 || delta_y == 0 {
                ORTHOGONAL_COST
            } else {
                DIAGONAL_COST
            };
            let multiplier = matches!(
                terrain.tile_at(next).map(|sample| sample.material),
                Some(GroundMaterial::Mud)
            )
            .then_some(3_u32)
            .unwrap_or(2);
            result.push((next, base * multiplier / 2));
        }
    }
    result
}

fn walkable(terrain: &MapChunkGenerator, tile: TileCoord) -> bool {
    terrain.tile_at(tile).is_some_and(|sample| sample.passable) && terrain.object_at(tile).is_none()
}

fn crossable(terrain: &MapChunkGenerator, from: TileCoord, to: TileCoord) -> bool {
    let Some(from) = terrain.tile_at(from) else {
        return false;
    };
    let Some(to) = terrain.tile_at(to) else {
        return false;
    };
    (i32::from(from.game_height_level) - i32::from(to.game_height_level)).abs() <= 1
}

fn heuristic(from: TileCoord, to: TileCoord) -> u64 {
    let dx = u64::from((from.x - to.x).unsigned_abs());
    let dy = u64::from((from.y - to.y).unsigned_abs());
    let diagonal = dx.min(dy);
    diagonal * u64::from(DIAGONAL_COST) + (dx - diagonal) * u64::from(ORTHOGONAL_COST)
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
                    && crossable(&terrain(), origin, destination)
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
}
