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
            (0..4).flat_map(move |x| terrain.chunk(x, y).expect("fixture chunk").resources)
        })
        // This regression removes a land obstacle, not fish in impassable water.
        .find(|node| {
            !matches!(
                node.object,
                crate::ObjectKind::ShoreFish | crate::ObjectKind::DeepFish
            ) && terrain.tile_at(node.tile).is_some_and(|tile| tile.passable)
        })
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
