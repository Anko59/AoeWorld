//! DE's Dark Age palisade gate uses a four by one footprint.
use super::buildings::{finish, set_stock, villager_at};
use super::*;

#[test]
fn palisade_gate_is_dark_age_buildable_and_opens_for_its_owner() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 1000, 1000, 0, 1000);
    let worker = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::PalisadeGate,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    let gate = world.building(id).unwrap();
    assert_eq!(
        (gate.footprint.width(), gate.footprint.height(), gate.max_hp),
        (4, 1, 240)
    );
    assert!(!world.tile_passable(TileCoord::new(101, 100)));
    finish(&mut world, id, 700);
    world.advance();
    assert!(
        world.tile_passable(TileCoord::new(101, 100)),
        "completed gate opens for its nearby builder"
    );
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(TileCoord::new(90, 100)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 400);
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "gate closes once the owner leaves"
    );
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(99, 100)).unwrap(),
        )
        .unwrap();
    world.advance();
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "enemy does not open it"
    );
    assert!(
        world
            .issue_move(
                enemy,
                WorldPosition::from_tile_center(TileCoord::new(101, 100)).unwrap()
            )
            .is_err()
    );
}
