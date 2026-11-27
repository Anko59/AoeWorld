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
