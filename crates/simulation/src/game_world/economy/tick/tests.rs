use super::*;
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};

#[test]
fn collect_changed_counts_input_ids_and_keeps_sorted_deduplicated_output() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let at = WorldPosition::from_tile_center(TileCoord::new(100, 100)).unwrap();
    let a = world.spawn_villager(PlayerId(0), at).unwrap();
    let b = world.spawn_villager(PlayerId(0), at).unwrap();
    let changed = world.collect_changed(vec![b, a, b, EntityId(100)]);
    assert_eq!(
        changed.iter().map(|unit| unit.id).collect::<Vec<_>>(),
        [a, b]
    );
    let count = |world: &GameWorld| {
        world
            .pass_counts()
            .into_iter()
            .find(|(name, _)| *name == "collect_changed")
            .unwrap()
            .1
    };
    assert_eq!(
        count(&world),
        crate::PassCount {
            calls: 1,
            visited: 4
        }
    );
    assert!(world.collect_changed(Vec::new()).is_empty());
    assert_eq!(
        count(&world),
        crate::PassCount {
            calls: 2,
            visited: 4
        }
    );
}
