use super::*;
use aoe_map::ResourceKind;

#[test]
fn resource_memory_changes_only_while_the_player_can_see_the_node() {
    let (package, pages, water) = economy::fixture::flat_fixture(false);
    let generator = package
        .generator_with_environment(pages.clone(), water.clone(), Vec::new(), Vec::new())
        .unwrap();
    let mut w =
        GameWorld::from_prepared_map(package, pages, water, Vec::new(), Vec::new()).unwrap();
    let mut found = None;
    'search: for cy in 0..8 {
        for cx in 0..8 {
            for node in generator.chunk(cx, cy).unwrap().resources {
                if node.kind != ResourceKind::Wood {
                    continue;
                }
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let tile = TileCoord::new(node.tile.x + dx, node.tile.y + dy);
                    if let Ok(at) = WorldPosition::from_tile_center(tile)
                        && let Ok(id) = w.spawn_villager(PlayerId(0), at)
                    {
                        found = Some((node, id, at));
                        break 'search;
                    }
                }
            }
        }
    }
    let (node, id, at) = found.expect("resource with a passable adjacent tile");
    w.deplete_resource(node.id, 1).unwrap();
    let observed = node.initial_amount - 1;
    assert_eq!(
        w.known_resource_amounts(PlayerId(0)),
        vec![(node.id, observed)]
    );
    w.kill_unit(w.lookup[&id]);
    w.deplete_resource(node.id, 1).unwrap();
    assert_eq!(
        w.known_resource_amounts(PlayerId(0)),
        vec![(node.id, observed)]
    );
    w.spawn_villager(PlayerId(0), at).unwrap();
    assert_eq!(
        w.known_resource_amounts(PlayerId(0)),
        vec![(node.id, observed - 1)]
    );
}
