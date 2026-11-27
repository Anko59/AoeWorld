use aoe_sdk::{
    Age, BuildingKind, EntityId, GroundRelic, Order, RelicAction, RelicView, TileCoord, UnitKind,
    WorldPosition,
};

use crate::tests::fake::{FakeWorld, building, unit_of};
use crate::tests::play::{eco, play_fresh, with_age};

fn relic(id: u32, x: i32, y: i32) -> GroundRelic {
    GroundRelic {
        id: EntityId(id),
        position: WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap(),
    }
}

#[test]
fn the_rust_seat_sends_its_idle_monk_to_the_nearest_visible_relic() {
    let mut world = FakeWorld::new(
        vec![unit_of(13, UnitKind::Monk, (50, 50))],
        vec![
            building(1, BuildingKind::TownCenter, (45, 45)),
            building(2, BuildingKind::Monastery, (52, 52)),
        ],
    );
    world.economy = Some(with_age(eco(0, 0, 0, 1, 10), Age::Castle, &[]));
    world.relics = Some(RelicView {
        ground: vec![relic(20, 57, 50), relic(21, 53, 51), relic(22, 210, 230)],
        ..RelicView::default()
    });
    let (_, orders) = play_fresh(&world);
    assert!(orders.orders.contains(&Order::Relic(RelicAction::PickUp {
        monk: EntityId(13),
        relic: EntityId(21)
    })));
    world.buildings.pop();
    let (_, orders) = play_fresh(&world);
    assert!(!orders.orders.iter().any(|o| matches!(o, Order::Relic(_))));
}

#[test]
fn the_rust_seat_delivers_a_carrier_to_its_nearest_completed_monastery() {
    let mut wrong_site = building(2, BuildingKind::Monastery, (50, 53));
    wrong_site.complete = false;
    let mut world = FakeWorld::new(
        vec![unit_of(13, UnitKind::Monk, (50, 50))],
        vec![
            building(1, BuildingKind::TownCenter, (45, 45)),
            wrong_site,
            building(3, BuildingKind::Monastery, (60, 52)),
        ],
    );
    world.economy = Some(with_age(eco(0, 0, 0, 1, 10), Age::Castle, &[]));
    world.relics = Some(RelicView {
        carriers: vec![EntityId(13)],
        ..RelicView::default()
    });
    let (_, orders) = play_fresh(&world);
    assert!(orders.orders.contains(&Order::Garrison {
        unit: EntityId(13),
        building: EntityId(3)
    }));
}

#[test]
fn the_build_order_wants_one_monastery_for_a_nearby_relic_in_castle_age() {
    use crate::tests::fake::with_task;
    use aoe_sdk::{ResourceKind, UnitTask};

    let mut builder = with_task(
        unit_of(13, UnitKind::Villager, (48, 50)),
        UnitTask::Gathering,
    );
    builder.gathering = Some(ResourceKind::Wood);
    builder.carrying = Some((ResourceKind::Wood, 5));
    let mut world = FakeWorld::new(
        vec![builder],
        vec![
            building(1, BuildingKind::TownCenter, (44, 44)),
            building(2, BuildingKind::Barracks, (37, 44)),
            building(3, BuildingKind::ArcheryRange, (40, 39)),
            building(4, BuildingKind::Blacksmith, (49, 39)),
            building(5, BuildingKind::Stable, (43, 52)),
        ],
    );
    let plans_monastery = |world: &FakeWorld| {
        play_fresh(world).1.orders.iter().any(|order| {
            matches!(
                order,
                Order::Build {
                    kind: BuildingKind::Monastery,
                    ..
                }
            )
        })
    };
    world.relics = Some(RelicView {
        ground: vec![relic(20, 60, 53)],
        ..RelicView::default()
    });
    world.economy = Some(with_age(eco(0, 400, 0, 1, 10), Age::Feudal, &[]));
    assert!(!plans_monastery(&world));

    world.economy = Some(with_age(eco(0, 120, 0, 1, 10), Age::Castle, &[]));
    assert!(!plans_monastery(&world));

    world.economy = Some(with_age(eco(0, 400, 0, 1, 10), Age::Castle, &[]));
    assert!(plans_monastery(&world));

    world.relics.as_mut().unwrap().ground = vec![relic(20, 210, 230)];
    assert!(!plans_monastery(&world));
}

#[test]
fn a_relic_carrier_uses_the_nearest_monastery_on_its_side_of_a_river() {
    let mut world = FakeWorld::new(
        vec![unit_of(13, UnitKind::Monk, (48, 50))],
        vec![
            building(1, BuildingKind::TownCenter, (34, 45)),
            building(2, BuildingKind::Monastery, (52, 50)),
            building(3, BuildingKind::Monastery, (48, 68)),
            building(4, BuildingKind::Monastery, (30, 65)),
        ],
    );
    world.economy = Some(with_age(eco(0, 0, 0, 1, 10), Age::Castle, &[]));
    world.relics = Some(RelicView {
        carriers: vec![EntityId(13)],
        ..RelicView::default()
    });
    // Water runs from edge to edge at x = 50.
    world.route_fn = Some(Box::new(|from, to| {
        (from.0 < 50 && to.0 < 50).then(|| vec![from, to])
    }));
    let (_, orders) = play_fresh(&world);
    let deliveries = orders
        .orders
        .iter()
        .filter_map(|order| match order {
            Order::Garrison { unit, building } if *unit == EntityId(13) => Some(*building),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(deliveries, [EntityId(3)]);
}
