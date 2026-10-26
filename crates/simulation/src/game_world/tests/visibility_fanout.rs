use super::*;
use crate::TileVisibility;
use aoe_core::TileRect;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn visibility_tiles_agree_with_per_tile_visibility() {
    let mut w = world();
    let walker = w.spawn_villager(PlayerId(0), at(20, 20)).unwrap();
    w.spawn_unit(PlayerId(0), at(70, 30)).unwrap();
    w.spawn_unit(PlayerId(1), at(60, 60)).unwrap();
    w.issue_move(walker, at(45, 20)).unwrap();
    for _ in 0..300 {
        w.advance();
    }
    // A rectangle that spans several 32 x 32 chunks and starts off-grid.
    for rect in [
        TileRect::from_xywh(5, 5, 90, 50),
        TileRect::from_xywh(0, 0, 33, 1),
        TileRect::from_xywh(31, 31, 2, 2),
        TileRect::from_xywh(200, 200, 56, 56),
    ] {
        for player in [PlayerId(0), PlayerId(1), PlayerId(9)] {
            let tiles = w.visibility_tiles(player, rect);
            let expected = (rect.min.y..rect.max.y)
                .flat_map(|y| (rect.min.x..rect.max.x).map(move |x| TileCoord::new(x, y)))
                .map(|tile| match w.visibility(player, tile) {
                    TileVisibility::Unexplored => 0,
                    TileVisibility::Explored => 1,
                    TileVisibility::Visible => 2,
                })
                .collect::<Vec<u8>>();
            assert_eq!(tiles, expected, "{player:?} {rect:?}");
        }
    }
    assert!(
        w.visibility_tiles(PlayerId(0), TileRect::from_xywh(5, 5, 90, 50))
            .contains(&2)
    );
}

#[test]
fn remembered_building_changes_are_taken_once_and_follow_memory() {
    let mut w = world();
    let scout = w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    let hall = w
        .place_town_center(PlayerId(1), TileCoord::new(65, 65))
        .unwrap();
    w.advance();
    let first = w.take_remembered_building_changes();
    assert!(
        first[&PlayerId(0)].contains(&hall),
        "seen for the first time"
    );
    assert!(
        first[&PlayerId(1)].contains(&hall),
        "the owner remembers it too"
    );
    assert!(w.take_remembered_building_changes().is_empty());
    w.advance();
    assert!(
        w.take_remembered_building_changes().is_empty(),
        "an unchanged building is not reported again"
    );
    w.economy.buildings.get_mut(&hall).unwrap().hp -= 100;
    w.advance();
    let hit = w.take_remembered_building_changes();
    assert_eq!(
        hit.keys().copied().collect::<Vec<_>>(),
        [PlayerId(0), PlayerId(1)]
    );
    assert_eq!(
        w.remembered_building(PlayerId(0), hall).map(|b| b.hp),
        w.building(hall).map(|b| b.hp)
    );
    // Out of sight the memory, and so the report, stays still.
    w.issue_move(scout, at(20, 20)).unwrap();
    for _ in 0..700 {
        w.advance();
    }
    let _ = w.take_remembered_building_changes();
    w.economy.buildings.get_mut(&hall).unwrap().hp -= 100;
    w.advance();
    let unseen = w.take_remembered_building_changes();
    assert!(!unseen.contains_key(&PlayerId(0)));
    assert!(unseen[&PlayerId(1)].contains(&hall));
    assert_ne!(
        w.remembered_building(PlayerId(0), hall).map(|b| b.hp),
        w.building(hall).map(|b| b.hp)
    );
}

#[test]
fn chunk_candidates_cover_every_unit_a_query_returns() {
    let mut w = world();
    for n in 0..40 {
        let _ = w.spawn_unit(PlayerId(n % 3), at(10 + 5 * n as i32, 20 + 3 * n as i32));
    }
    let rect = TileRect::from_xywh(30, 25, 70, 60);
    let mut candidates = Vec::new();
    w.collect_chunk_unit_ids(rect, &mut candidates);
    let (inside, _) = w.query(rect);
    assert!(!inside.is_empty());
    assert!(inside.iter().all(|unit| candidates.contains(&unit.id)));
    let mut none = Vec::new();
    w.collect_chunk_unit_ids(TileRect::from_xywh(-5, 0, 10, 10), &mut none);
    assert!(none.is_empty(), "an invalid rectangle selects nothing");
    assert_eq!(w.entity_id_bound(), 40);
}
