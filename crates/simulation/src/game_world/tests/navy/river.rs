//! S7: fishing ships in a river. Records today's behaviour; PR 9 of the
//! group-movement rewrite (docs/movement/group-movement-rewrite.md, section
//! 6) makes four ships work in a river one tile wide.
use super::*;
use aoe_map::ObjectKind;

/// Four fishing ships, two each side of one fish, in a channel `width` tiles
/// wide beside it. Returns (orders refused, ships with a catch, idle ships).
fn fish_in_river(width: i32) -> (usize, usize, usize) {
    let mut w = coast();
    w.set_fog_disabled(true);
    let crate::Terrain::Map { generator, .. } = w.terrain() else {
        panic!("the prepared coast has a map terrain")
    };
    let nodes = (0..5)
        .flat_map(|cy| (0..8).flat_map(move |cx| generator.chunk(cx, cy).unwrap().resources))
        .collect::<Vec<_>>();
    let water = |w: &GameWorld, x: i32, y: i32| {
        w.unit_tile_passable(UnitKind::FishingShip, TileCoord::new(x, y), &|| false)
            .unwrap()
    };
    // A fish with open water along the three rows below it, eight tiles each way.
    let fish = nodes
        .iter()
        .find(|n| {
            n.object == ObjectKind::DeepFish
                && n.tile.y > 8
                && (-8..=8).all(|dx| (1..=3).all(|dy| water(&w, n.tile.x + dx, n.tile.y + dy)))
        })
        .expect("a fish beside a straight reach of water");
    let river = fish.tile.y + 1;
    for x in fish.tile.x - 40..fish.tile.x + 40 {
        for y in river - 6..river + 6 {
            if !(river..river + width).contains(&y) && TileCoord::new(x, y) != fish.tile {
                w.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
    }
    let ships = [-6, -4, 4, 6].map(|dx| {
        let at = TileCoord::new(fish.tile.x + dx, river);
        ship(&mut w, UnitKind::FishingShip, at, 0)
    });
    let refused = ships
        .iter()
        .filter(|&&id| w.issue_gather(PlayerId(0), id, fish.id).is_err())
        .count();
    advance(&mut w, 2400);
    let unit = |id: EntityId| w.unit(id).unwrap();
    let caught = ships
        .iter()
        .filter(|&&id| unit(id).carried.is_some())
        .count();
    let idle = ships
        .iter()
        .filter(|&&id| unit(id).task == UnitTask::Idle)
        .count();
    (refused, caught, idle)
}

#[test]
fn four_fishing_ships_in_a_one_tile_river() {
    let narrow = fish_in_river(1);
    let wide = fish_in_river(3);
    eprintln!("group-baseline river-fishing: one_tile={narrow:?} three_tile={wide:?}");
    // S7: in a one-tile river every gather order is refused as unreachable
    // (the design blames the 512-subunit hull against the terrain check; not
    // yet confirmed), so none catches a fish and all four idle. PR 9 flips this: all four work. A three-tile channel
    // already admits two of the four.
    assert_eq!(narrow, (4, 0, 4));
    assert_eq!(wide, (2, 2, 2));
}
