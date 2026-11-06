//! A river between the castle and a Town Center is crossed by a planned
//! bridge, and a Vassal builds it.
use super::*;

use super::super::bridges::river_fixture::river_width;

fn river_village() -> (GameWorld, EntityId) {
    // The river runs along x = 30..34 over the whole map.
    let mut w = river_width(true, 30..34);
    w.place_town_center(PlayerId(0), TileCoord::new(40, 12))
        .unwrap();
    let p = w.economy.players.get_mut(&PlayerId(0)).unwrap();
    p.age = Age::Castle;
    p.stockpile = Stockpile {
        food: 10_000,
        wood: 10_000,
        gold: 10_000,
        stone: 10_000,
    };
    w.set_fog_disabled(true);
    let castle = w
        .insert_building(
            PlayerId(0),
            BuildingKind::Castle,
            TileRect::from_xywh(10, 14, 4, 4),
            true,
        )
        .unwrap();
    let v = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Vassal,
            WorldPosition::from_tile_center(TileCoord::new(9, 16)).unwrap(),
        )
        .unwrap();
    w.issue_garrison(PlayerId(0), v, castle).unwrap();
    w.prepare_navigation(&|| false).unwrap();
    advance(&mut w, 2);
    (w, v)
}

#[test]
fn vassal_roads_span_a_river_between_the_castle_and_the_town_center() {
    let (mut w, v) = river_village();
    let net = w.vassal_network(v);
    let bridges: Vec<_> = net.iter().filter(|n| n.0 == BuildingKind::Bridge).collect();
    eprintln!("roads {} bridges {bridges:?}", net.len() - bridges.len());
    assert_eq!(bridges.len(), 1, "one bridge over the river: {net:?}");
    let (_, from, to) = bridges[0];
    let to = to.unwrap();
    assert!(from.x <= 29 && to.x >= 34, "{from:?} {to:?}");
    assert!(net.iter().any(|n| n.0 == BuildingKind::Road && n.1.x > 34));
}

#[test]
fn a_vassal_builds_the_bridge_and_its_road_across_the_river() {
    let (mut w, v) = river_village();
    for i in 0..6 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(14 + i, 20)).unwrap(),
        )
        .unwrap();
    }
    let mut built = None;
    for t in 0..30_000 {
        w.advance();
        if built.is_none()
            && w.buildings()
                .any(|b| b.kind == BuildingKind::Bridge && b.is_complete())
        {
            built = Some(t);
        }
        if built.is_some() && t > built.unwrap() + 2_000 {
            break;
        }
    }
    eprintln!("bridge complete at {built:?}");
    assert!(built.is_some(), "{:?}", w.vassal_status(v));
}
