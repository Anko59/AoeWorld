use crate::{EconomyError, GameWorld, TileVisibility, UnitKind};
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};

fn world() -> (GameWorld, aoe_core::EntityId) {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(64, 64)).unwrap();
    (w, id)
}
fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn auto_scout_selects_the_nearest_reachable_frontier_deterministically() {
    let run = || {
        let (mut w, id) = world();
        let mut frontier = Vec::new();
        for y in 48..80 {
            for x in 48..80 {
                let p = TileCoord::new(x, y);
                if w.visibility(PlayerId(0), p) != TileVisibility::Unexplored
                    && [(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                        w.visibility(PlayerId(0), TileCoord::new(x + dx, y + dy))
                            == TileVisibility::Unexplored
                    })
                {
                    let (dx, dy) = (x.abs_diff(64), y.abs_diff(64));
                    frontier.push((1024 * dx.max(dy) + 424 * dx.min(dy), y, x));
                }
            }
        }
        frontier.sort_unstable();
        w.issue_auto_scout(PlayerId(0), id, true).unwrap();
        for _ in 0..32 {
            if w.movement_order(id).is_some() {
                break;
            }
            w.advance();
        }
        let goal = w
            .movement_order(id)
            .expect("a reachable unexplored frontier")
            .target_tile;
        assert_eq!((goal.y, goal.x), (frontier[0].1, frontier[0].2));
        for _ in 0..400 {
            w.advance();
        }
        assert_ne!(w.unit(id).unwrap().position, at(64, 64));
        assert!(w.unit(id).unwrap().auto_scout);
        w.canonical_hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn auto_scout_rejects_foreign_and_unmounted_units_and_manual_orders_cancel_it() {
    let (mut w, id) = world();
    let worker = w.spawn_villager(PlayerId(0), at(62, 62)).unwrap();
    assert_eq!(
        w.issue_auto_scout(PlayerId(1), id, true),
        Err(EconomyError::NotOwner)
    );
    assert_eq!(
        w.issue_auto_scout(PlayerId(0), worker, true),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(w.unit(id).unwrap().kind, UnitKind::Scout);
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    w.issue_move(id, at(60, 60)).unwrap();
    assert!(!w.unit(id).unwrap().auto_scout);
    let order = w.movement_order(id);
    w.issue_auto_scout(PlayerId(0), id, false).unwrap();
    assert_eq!(
        w.movement_order(id),
        order,
        "an already-off toggle preserves manual movement"
    );
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    w.issue_stop(PlayerId(0), id).unwrap();
    assert!(!w.unit(id).unwrap().auto_scout);
    assert!(w.movement_order(id).is_none());
}

#[test]
fn queued_manual_move_takes_over_an_active_scout_route() {
    let (mut w, id) = world();
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    for _ in 0..32 {
        if w.movement_order(id).is_some() {
            break;
        }
        w.advance();
    }
    let first = w.movement_order(id).expect("active autonomous leg");
    let before = w.canonical_hash();
    assert!(w.issue_queued_move(id, WorldPosition::new(-10, 0)).is_err());
    assert_eq!(w.canonical_hash(), before, "refusal preserves scouting");
    let destination = at(60, 60);
    w.issue_queued_move(id, destination).unwrap();
    assert!(!w.unit(id).unwrap().auto_scout);
    assert_eq!(w.move_destinations(id), [first.destination, destination]);
    assert!(!w.economy.scouts.contains_key(&id));
    assert!(!w.economy.scout_searches.contains(&id));
    for _ in 0..400 {
        w.advance();
    }
    assert_eq!(w.unit(id).unwrap().position, destination);
    assert!(w.move_destinations(id).is_empty());
    assert!(w.movement_order(id).is_none());
}

#[test]
fn combat_and_death_cancel_auto_scout_and_release_its_job() {
    let (mut w, id) = world();
    let enemy = w.spawn_unit(PlayerId(1), at(64, 63)).unwrap();
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    w.issue_attack(PlayerId(0), id, enemy).unwrap();
    assert!(!w.unit(id).unwrap().auto_scout);
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    let index = w.lookup[&id];
    w.kill_unit(index);
    assert!(!w.unit(id).unwrap().auto_scout);
    assert!(!w.economy.scouts.contains_key(&id));
    assert!(!w.economy.scout_searches.contains(&id));
    assert!(w.issue_auto_scout(PlayerId(0), id, true).is_err());
}

#[test]
fn known_sealed_areas_are_not_auto_scout_targets() {
    let (mut w, id) = world();
    for y in 0..256 {
        w.economy.blocked_tiles.insert(TileCoord::new(68, y));
    }
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    for _ in 0..1200 {
        w.advance();
        assert!(w.unit(id).unwrap().position.tile_floor().x < 68);
        if let Some(order) = w.movement_order(id) {
            assert!(order.target_tile.x < 68);
        }
    }
}

#[test]
fn a_fully_explored_world_does_not_restart_hopeless_scout_routes() {
    let (mut w, id) = world();
    w.set_fog_disabled(true);
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    for _ in 0..100 {
        w.advance();
    }
    assert_eq!(w.unit(id).unwrap().position, at(64, 64));
    assert!(w.movement_order(id).is_none());
    w.issue_auto_scout(PlayerId(0), id, false).unwrap();
    assert!(!w.unit(id).unwrap().auto_scout);
}

#[test]
fn many_scouts_share_bounded_frontier_work_and_release_search_memory_on_stop() {
    let (mut w, first) = world();
    let mut scouts = vec![first];
    for _ in 0..32 {
        scouts.push(w.spawn_unit(PlayerId(0), at(64, 64)).unwrap());
    }
    crate::game_world::tests::economy::fixture::survey(
        &mut w,
        PlayerId(0),
        TileCoord::new(64, 64),
        64,
    );
    for &id in &scouts {
        w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    }
    for _ in 0..50 {
        w.advance();
        assert!(w.economy.scout_searches.len() <= super::ACTIVE_SEARCHES);
        assert_eq!(
            w.economy.scout_searches.len(),
            w.economy
                .scouts
                .values()
                .filter(|j| j.search.is_some())
                .count()
        );
        for job in w.economy.scouts.values() {
            if let Some(search) = &job.search {
                assert!(search.costs.len() <= 65_536);
            }
        }
    }
    assert!(
        !w.economy.scout_searches.is_empty(),
        "fixture exercises retained searches"
    );
    for id in scouts {
        w.issue_stop(PlayerId(0), id).unwrap();
    }
    assert!(w.economy.scouts.is_empty());
    assert!(w.economy.scout_searches.is_empty());
}

#[test]
fn scout_crosses_a_large_explored_square_to_reach_its_frontier() {
    let mut w = GameWorld::new(WorldConfig::new(1024, 1024, Seed(7)).unwrap()).unwrap();
    let id = w.spawn_unit(PlayerId(0), at(512, 512)).unwrap();
    let observer = w.allocate_entity_id().unwrap();
    for y in 256..768 {
        for x in 256..768 {
            w.update_observer(observer, Some((PlayerId(0), TileCoord::new(x, y), 0)));
        }
    }
    w.update_observer(observer, None);
    assert_eq!(
        w.visibility(PlayerId(0), TileCoord::new(512, 512)),
        TileVisibility::Visible
    );
    let revision = w.player_vision_revision(PlayerId(0));
    w.issue_auto_scout(PlayerId(0), id, true).unwrap();
    let mut peak_pending = 0;
    let mut peak_settled_chunks = 0;
    for _ in 0..5000 {
        w.advance();
        if let Some(search) = w
            .economy
            .scouts
            .get(&id)
            .and_then(|job| job.search.as_ref())
        {
            peak_pending = peak_pending.max(search.costs.len());
            peak_settled_chunks = peak_settled_chunks.max(search.settled.len());
        }
        if w.movement_order(id).is_some() {
            break;
        }
    }
    assert_eq!(w.player_vision_revision(PlayerId(0)), revision);
    let target = w
        .movement_order(id)
        .expect("search must reach beyond 65,536 explored tiles")
        .target_tile;
    assert!((target.x - 512).abs().max((target.y - 512).abs()) > 140);
    assert!(super::NEIGHBOURS[..4].iter().any(|(dx, dy)| {
        w.visibility(PlayerId(0), TileCoord::new(target.x + dx, target.y + dy))
            == TileVisibility::Unexplored
    }));
    assert!(w.unit(id).unwrap().auto_scout);
    assert!(peak_pending < 4096, "pending distances: {peak_pending}");
    assert!(
        peak_settled_chunks <= 256,
        "settled chunks: {peak_settled_chunks}"
    );
    eprintln!(
        "512-square search: {peak_pending} pending distances, {peak_settled_chunks} settled chunk bitsets"
    );
}
