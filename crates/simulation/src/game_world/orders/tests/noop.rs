use super::*;
use aoe_core::{Formation, PlayerId, Seed, TileCoord, WorldConfig};

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}
fn world() -> GameWorld {
    GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap()
}

#[test]
fn a_near_goal_reissue_keeps_progress_and_queued_destinations() {
    let mut w = world();
    let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    w.issue_move(id, at(100, 20)).unwrap();
    w.issue_queued_move(id, at(140, 20)).unwrap();
    for _ in 0..8 {
        w.advance();
    }
    let before = w.canonical_hash();
    let order = w.movement_order(id);
    let requests = w.order_demand()[OrderIssuer::Automation as usize].route_requests;
    for goal in [at(100, 20), at(101, 20)] {
        assert_eq!(w.issue_move(id, goal), Ok(true));
        assert_eq!(w.movement_order(id), order);
        assert_eq!(w.canonical_hash(), before);
    }
    assert_eq!(
        w.order_demand()[OrderIssuer::Automation as usize].route_requests,
        requests
    );
    w.issue_move(id, at(103, 20)).unwrap();
    assert_eq!(w.movement_order(id).unwrap().destination, at(103, 20));
}

#[test]
fn a_near_formation_goal_keeps_the_shared_route_and_slots() {
    let mut w = world();
    let ids: Vec<_> = (0..4)
        .map(|x| {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, at(20 + x * 2, 20))
                .unwrap()
        })
        .collect();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(100, 20),
        false,
        false,
    )
    .unwrap();
    for _ in 0..20 {
        w.advance();
    }
    let before = w.canonical_hash();
    let requests = w.order_demand()[OrderIssuer::Automation as usize].route_requests;
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(101, 20),
        false,
        false,
    )
    .unwrap();
    assert_eq!(w.canonical_hash(), before);
    assert_eq!(
        w.order_demand()[OrderIssuer::Automation as usize].route_requests,
        requests
    );
    // Attack mode is meaningful even when the destination is unchanged.
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, at(100, 20), false, true)
        .unwrap();
    assert_ne!(w.canonical_hash(), before);
    // Repeated queued waypoints remain deliberate orders.
    let before_queue = w.canonical_hash();
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, at(100, 20), true, true)
        .unwrap();
    assert_ne!(w.canonical_hash(), before_queue);
}

#[test]
fn idle_units_still_take_a_new_goal_within_one_tile() {
    let mut w = world();
    let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    assert_eq!(w.issue_move(id, at(21, 20)), Ok(true));
    for _ in 0..80 {
        w.advance();
    }
    assert!(w.movement_order(id).is_none());
    assert_eq!(w.issue_move(id, at(22, 20)), Ok(true));
}

#[test]
fn nearby_interaction_contacts_remain_distinct_but_exact_repeats_keep_progress() {
    let mut w = world();
    let id = w.spawn_unit(PlayerId(0), at(20, 20)).unwrap();
    w.issue_task_move(id, at(100, 20)).unwrap();
    for _ in 0..8 {
        w.advance();
    }
    let before = w.canonical_hash();
    w.issue_task_move(id, at(100, 20)).unwrap();
    assert_eq!(w.canonical_hash(), before);
    w.issue_task_move(id, at(101, 20)).unwrap();
    assert_eq!(w.movement_order(id).unwrap().destination, at(101, 20));
}

#[test]
fn repeated_goal_orders_replay_with_one_and_four_workers() {
    let run = |workers| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let mut w = world();
                let trips: Vec<_> = (0..128)
                    .map(|i| {
                        let id = w
                            .spawn_kind(
                                PlayerId(0),
                                UnitKind::Militia,
                                at(20 + (i % 16) * 3, 20 + (i / 16) * 3),
                            )
                            .unwrap();
                        let goal = at(180 + (i % 16) * 3, 110 + (i / 16) * 3);
                        w.issue_move(id, goal).unwrap();
                        (id, goal)
                    })
                    .collect();
                let mut hashes = Vec::new();
                for tick in 0..60 {
                    if tick % 10 == 0 {
                        for &(id, goal) in &trips {
                            w.issue_move(id, goal).unwrap();
                        }
                    }
                    w.advance();
                    hashes.push(w.canonical_hash());
                }
                hashes
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn a_single_unit_override_still_leaves_a_formation_with_the_same_goal() {
    let mut w = world();
    let ids: Vec<_> = (0..4)
        .map(|x| {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, at(20 + x * 2, 20))
                .unwrap()
        })
        .collect();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        at(100, 20),
        false,
        false,
    )
    .unwrap();
    w.issue_move(ids[0], at(100, 20)).unwrap();
    assert!(
        w.formation(ids[0]).is_none(),
        "an individual override changes membership"
    );
    assert!(w.movement_order(ids[0]).is_some());
}
