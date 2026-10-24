//! Soft unit collision must preserve passage while ending persistent stacks.
use super::*;

mod following_tests;
mod performance;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
fn run(w: &mut GameWorld, ticks: usize) {
    for _ in 0..ticks {
        w.advance();
    }
}
fn distance(a: WorldPosition, b: WorldPosition) -> i64 {
    let (x, y) = (
        i64::from(a.x) - i64::from(b.x),
        i64::from(a.y) - i64::from(b.y),
    );
    x * x + y * y
}
fn group(w: &mut GameWorld, stacked: bool) -> Vec<EntityId> {
    (0..10)
        .map(|i| {
            w.spawn_villager(PlayerId(0), at(30, if stacked { 30 } else { 25 + i }))
                .unwrap()
        })
        .collect()
}

#[test]
fn ten_villagers_share_a_click_but_arrive_on_at_least_eight_tiles() {
    let mut w = world();
    let ids = group(&mut w, false);
    for &id in &ids {
        w.issue_move(id, at(50, 30)).unwrap();
    }
    run(&mut w, 1000);
    let tiles = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position.tile_floor())
        .collect::<BTreeSet<_>>();
    assert!(tiles.len() >= 8, "destinations: {tiles:?}");
    assert!(ids.iter().all(|&id| w.movement_order(id).is_none()));
    assert!(
        tiles
            .iter()
            .all(|t| t.x.abs_diff(50) <= 3 && t.y.abs_diff(30) <= 3)
    );
}

#[test]
fn two_units_cross_head_on_and_both_reach_their_destinations() {
    let mut w = world();
    let a = w.spawn_villager(PlayerId(0), at(30, 30)).unwrap();
    let b = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.issue_move(a, at(40, 30)).unwrap();
    w.issue_move(b, at(30, 30)).unwrap();
    run(&mut w, 400);
    assert_eq!(w.unit(a).unwrap().position, at(40, 30));
    assert_eq!(w.unit(b).unwrap().position, at(30, 30));
}

#[test]
fn same_velocity_movers_break_a_stack_and_keep_minimum_spacing() {
    let mut w = world();
    let ids = group(&mut w, true);
    for &id in &ids {
        w.issue_move(id, at(100, 30)).unwrap();
    }
    run(&mut w, 350);
    assert!(ids.iter().all(|&id| w.unit(id).unwrap().moving));
    for tick in 0..40 {
        w.advance();
        for (i, &a) in ids.iter().enumerate() {
            for &b in &ids[i + 1..] {
                assert!(
                    distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position)
                        >= 512_i64.pow(2),
                    "merged {a:?}/{b:?} at {tick}"
                );
            }
        }
    }
}

#[test]
fn ten_idle_overlapping_units_separate_gently_within_two_hundred_ticks() {
    let mut w = world();
    let ids = group(&mut w, true);
    let first = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    w.advance();
    for (&id, start) in ids.iter().zip(first) {
        assert!(distance(w.unit(id).unwrap().position, start) <= 64_i64.pow(2));
    }
    run(&mut w, 199);
    for (i, &a) in ids.iter().enumerate() {
        for &b in &ids[i + 1..] {
            assert!(
                distance(w.unit(a).unwrap().position, w.unit(b).unwrap().position)
                    >= 640_i64.pow(2),
                "idle overlap {a:?}/{b:?}"
            );
        }
    }
}

#[test]
fn spacing_replay_is_identical_across_one_and_four_workers() {
    let replay = || {
        let mut w = world();
        let ids = group(&mut w, true);
        run(&mut w, 200);
        for &id in &ids {
            w.issue_move(id, at(60, 30)).unwrap();
        }
        (0..900)
            .map(|_| {
                w.advance();
                w.canonical_hash()
            })
            .collect::<Vec<_>>()
    };
    let expected = replay();
    std::thread::scope(|scope| {
        let workers = (0..4).map(|_| scope.spawn(replay)).collect::<Vec<_>>();
        for worker in workers {
            assert_eq!(expected, worker.join().unwrap());
        }
    });
}

#[test]
fn twenty_thousand_stacked_units_have_a_fixed_neighbour_inspection_bound() {
    let mut w = world();
    w.set_fog_disabled(true);
    let mut ids = Vec::new();
    for player in 0..100 {
        for _ in 0..200 {
            ids.push(w.spawn_villager(PlayerId(player), at(30, 30)).unwrap());
        }
    }
    assert_eq!(w.unit_count(), 20_000);
    for salt in [0, 1, 31, 19_998, 19_999, 20_000, u64::MAX] {
        let neighbours = w.spacing_neighbours(at(30, 30), salt).collect::<Vec<_>>();
        assert!(!neighbours.is_empty());
        assert!(neighbours.len() <= 128, "four cells times 32 inspections");
        let start = (salt % ids.len() as u64) as usize;
        let expected = (0..32)
            .map(|n| ids[(start + n) % ids.len()])
            .collect::<Vec<_>>();
        assert_eq!(
            neighbours, expected,
            "same cyclic window and insertion order"
        );
    }
    // Swap-removing a unit must repair the displaced unit's mirror slot.
    w.remove_unit(ids[0]);
    let moving = *ids.last().unwrap();
    w.issue_move(moving, at(40, 30)).unwrap();
    run(&mut w, 100);
    let position = w.unit(moving).unwrap().position;
    assert!(position.x > 32 * FIXED_SUBUNITS_PER_TILE);
    assert!(w.spacing_neighbours(position, 0).any(|id| id == moving));
}

#[test]
fn neighbour_queries_preserve_cell_order_and_filter_dead_and_distant_units() {
    let mut w = world();
    w.set_fog_disabled(true);
    let high_high = w.spawn_villager(PlayerId(0), at(32, 32)).unwrap();
    let low_low = w.spawn_villager(PlayerId(0), at(31, 31)).unwrap();
    let high_low = w.spawn_villager(PlayerId(0), at(32, 31)).unwrap();
    let low_high = w.spawn_villager(PlayerId(0), at(31, 32)).unwrap();
    w.spawn_villager(PlayerId(0), at(30, 30)).unwrap();
    let dead = w.spawn_villager(PlayerId(0), at(31, 32)).unwrap();
    w.kill_unit(w.lookup[&dead]);
    for salt in [0, 1, 7, u64::MAX] {
        assert_eq!(
            w.spacing_neighbours(WorldPosition::new(32 * 1024, 32 * 1024), salt)
                .collect::<Vec<_>>(),
            [low_low, high_low, low_high, high_high]
        );
    }
}

#[test]
fn neighbour_filter_keeps_the_strict_one_tile_axis_boundary() {
    let mut w = world();
    w.set_fog_disabled(true);
    let origin = at(32, 32);
    let mut expected = BTreeSet::new();
    for (dx, dy, included) in [
        (1023, 0, true),
        (1024, 0, false),
        (-1023, 0, true),
        (-1024, 0, false),
        (0, 1023, true),
        (0, 1024, false),
        (0, -1023, true),
        (0, -1024, false),
        (1023, 1023, true),
    ] {
        let id = w
            .spawn_villager(
                PlayerId(0),
                WorldPosition::new(origin.x + dx, origin.y + dy),
            )
            .unwrap();
        if included {
            expected.insert(id);
        }
    }
    assert_eq!(
        w.spacing_neighbours(origin, u64::MAX)
            .collect::<BTreeSet<_>>(),
        expected
    );
}

#[test]
fn destination_slots_stay_on_the_clicked_side_of_a_wall() {
    let mut w = world();
    for x in 0..256 {
        w.economy.blocked_tiles.insert(TileCoord::new(x, 29));
    }
    for y in 35..45 {
        let id = w.spawn_villager(PlayerId(0), at(40, y)).unwrap();
        w.issue_move(id, at(40, 30)).unwrap();
        assert!(w.movement_order(id).unwrap().target_tile.y >= 30);
    }
}
