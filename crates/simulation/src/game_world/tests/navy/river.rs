//! PR 9: narrow-water traffic and fishing work, including a temporarily
//! blocked job. Terrain stays hard; compressed one-tile traffic never tunnels.
use super::*;
use aoe_map::ObjectKind;

fn record(name: &str, value: &impl std::fmt::Debug) {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/movement");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("water-{name}.txt")),
        format!("{value:?}\n"),
    )
    .unwrap();
}

/// Four fishing ships, two each side of one fish, in a channel `width` tiles
/// wide beside it. Returns (orders refused, ships with a catch, idle ships).
fn fish_river_fixture(width: i32) -> (GameWorld, aoe_map::ResourceNode, i32) {
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
    (w, *fish, river)
}

fn fish_in_river(width: i32) -> (usize, usize, usize) {
    let (mut w, fish, river) = fish_river_fixture(width);
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
    record("fishing", &(narrow, wide));
    assert_eq!(narrow, (0, 4, 0));
    assert_eq!(wide, (0, 4, 0));
}

#[test]
fn a_blocked_fishing_ship_keeps_its_job_until_the_channel_clears() {
    let (mut w, fish, river) = fish_river_fixture(1);
    let id = ship(
        &mut w,
        UnitKind::FishingShip,
        TileCoord::new(fish.tile.x - 6, river),
        0,
    );
    w.issue_gather(PlayerId(0), id, fish.id).unwrap();
    let blocker = ship(
        &mut w,
        UnitKind::Galley,
        TileCoord::new(fish.tile.x - 3, river),
        1,
    );
    w.issue_stance(PlayerId(1), &[blocker], aoe_core::CombatStance::NoAttack)
        .unwrap();
    advance(&mut w, 320);
    assert_eq!(w.gather_target(id), Some(fish.id));
    assert_ne!(w.unit(id).unwrap().task, UnitTask::Idle);
    w.remove_unit(blocker);
    advance(&mut w, 600);
    assert!(w.unit(id).unwrap().carried.is_some());
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
        assert!(ordered, "{kind:?} order refused");
        actors.push((id, w.movement_order(id).map(|o| o.destination)));
    }
    let mut done = 0;
    for tick in 1..=1600 {
        w.advance();
        for (n, &(id, _)) in actors.iter().enumerate() {
            let unit = w.unit(id).unwrap();
            assert!(w.body_position_clear(unit.kind, unit.position));
            for &(other, _) in &actors[n + 1..] {
                let at = w.unit(other).unwrap().position;
                let distance = i64::from(unit.position.x - at.x).pow(2)
                    + i64::from(unit.position.y - at.y).pow(2);
                let hard: i64 = if width == 1 { 512 } else { 1024 };
                assert!(distance >= hard * hard, "hull hard distance at tick {tick}");
            }
        }
        if done == 0
            && actors
                .iter()
                .all(|&(id, goal)| Some(w.unit(id).unwrap().position) == goal)
        {
            done = tick;
        }
    }
    let never = actors
        .iter()
        .filter(|&&(id, goal)| Some(w.unit(id).unwrap().position) != goal)
        .count();
    (done, never)
}

#[test]
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
    // Fourteen tiles take 223 ticks at fishing-ship speed. Allow at most
    // 77 further ticks for contact and routing; nobody may give up as arrived.
    record("boats", &r);
    assert!(
        r.iter()
            .all(|&(tick, never)| (1..=300).contains(&tick) && never == 0),
        "{r:?}"
    );
}
