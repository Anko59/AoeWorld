use super::*;
use aoe_core::{Seed, WorldConfig, WorldPosition};

#[test]
fn relic_views_do_not_reveal_sites_outside_the_viewers_sight_or_subscription() {
    let mut world = GameWorld::new(WorldConfig::new(128, 128, Seed(13)).unwrap()).unwrap();
    let player = PlayerId(0);
    let scout = world
        .spawn_unit(
            player,
            WorldPosition::from_tile_center(TileCoord::new(64, 64)).unwrap(),
        )
        .unwrap();
    world.place_relics(&[scout], &|| false).unwrap();
    let whole = TileRect::from_xywh(0, 0, 128, 128);
    assert!(view(&world, player, 1, whole, markers(&world)).is_none());
    let (id, at) = world.ground_relics().next().unwrap();
    world.spawn_villager(player, at).unwrap();
    let known = view(&world, player, 1, whole, markers(&world)).unwrap();
    assert_eq!(known.ground, [GroundRelic { id, position: at }]);
    assert!(view(&world, PlayerId(1), 1, whole, markers(&world)).is_none());
    let other = TileRect::from_xywh(0, 0, 4, 4);
    assert!(view(&world, player, 2, other, markers(&world)).is_none());
}
