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

/// Ships sail a channel `width` tiles wide: each `(kind, start, goal)` is an x
/// offset along it and a row inside it. Returns the tick everyone arrived (0:
/// never) and how many never did.
fn channel(width: i32, ships: &[(UnitKind, i32, i32, i32, i32, u16)]) -> (u32, usize) {
    let mut w = coast();
    w.set_fog_disabled(true);
    let (_, water) = bank(&w);
    let base = TileCoord::new(water.x + 2, water.y + 4);
    for x in base.x - 2..base.x + 20 {
        for y in 0..w.config().height_tiles {
            if !(base.y..base.y + width).contains(&y) {
                w.economy.blocked_tiles.insert(TileCoord::new(x, y));
            }
        }
    }
    let mut actors = Vec::new();
    for &(kind, from_x, from_y, to_x, to_y, owner) in ships {
        let id = ship(
            &mut w,
            kind,
            TileCoord::new(base.x + from_x, base.y + from_y),
            owner,
        );
        let goal = at(TileCoord::new(base.x + to_x, base.y + to_y));
        let ordered = w.issue_move(id, goal).is_ok();
        actors.push((id, w.movement_order(id).map(|o| o.destination), ordered));
    }
    let mut done = 0;
    for tick in 1..=1600 {
        w.advance();
        if done == 0
            && actors
                .iter()
                .all(|&(id, goal, _)| Some(w.unit(id).unwrap().position) == goal)
        {
            done = tick;
        }
    }
    let never = actors
        .iter()
        .filter(|&&(id, goal, _)| Some(w.unit(id).unwrap().position) != goal)
        .count();
    (done, never)
}

#[test]
#[ignore = "recording, PR 9/10"]
fn boats_in_narrow_rivers() {
    let g = UnitKind::Galley;
    let f = UnitKind::FishingShip;
    let r = [
        // One ship alone in a one-tile river.
        channel(1, &[(f, 0, 0, 14, 0, 0)]),
        // A galley following a fishing ship in a one-tile river.
        channel(1, &[(f, 4, 0, 16, 0, 0), (g, 0, 0, 14, 0, 0)]),
        // Two galleys passing head-on in a two-tile river.
        channel(2, &[(g, 0, 0, 14, 0, 0), (g, 14, 1, 0, 1, 0)]),
        // Two galleys head-on along the same row of a two-tile river.
        channel(2, &[(g, 0, 0, 14, 0, 0), (g, 14, 0, 0, 0, 0)]),
        // A fishing ship and a galley head-on along one row of a two-tile river.
        channel(2, &[(f, 0, 0, 14, 0, 0), (g, 14, 0, 0, 0, 0)]),
        // Two fishing ships head-on in a one-tile river.
        channel(1, &[(f, 0, 0, 14, 0, 0), (f, 14, 0, 0, 0, 0)]),
        // The same two fishing ships belonging to different players.
        channel(1, &[(f, 0, 0, 14, 0, 0), (f, 14, 0, 0, 0, 1)]),
        // Two enemy galleys head-on along one row of a two-tile river.
        channel(2, &[(g, 0, 0, 14, 0, 0), (g, 14, 0, 0, 0, 1)]),
    ];
    panic!("record {r:?}");
}
