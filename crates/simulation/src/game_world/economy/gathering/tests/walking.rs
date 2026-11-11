//! Moving gatherers wake on contact, failed orders, and job replacement.
use super::*;

fn returning(sleep: bool, start: TileCoord) -> (GameWorld, EntityId) {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
    w.set_work_sleep(sleep);
    let center = w
        .place_town_center(PlayerId(0), TileCoord::new(128, 128))
        .unwrap();
    let worker = w
        .spawn_villager(PlayerId(0), position(start).unwrap())
        .unwrap();
    let mut task = fresh_task();
    task.phase = GatherPhase::Returning;
    task.dropoff = Some(center);
    task.return_target = Some(TileCoord::new(127, 127));
    w.issue_task_move(worker, position(task.return_target.unwrap()).unwrap())
        .unwrap();
    let index = w.lookup[&worker];
    w.units[index].state.carried = Some(CarriedResource {
        kind: ResourceKind::Wood,
        amount: 10,
    });
    w.units[index].state.task = UnitTask::Returning;
    w.economy.gatherers.insert(worker, task);
    w.index_destination(worker);
    (w, worker)
}

#[test]
fn a_distant_returning_gatherer_is_not_visited_between_movement_events() {
    let (mut on, worker) = returning(true, TileCoord::new(20, 20));
    let (mut off, _) = returning(false, TileCoord::new(20, 20));
    for tick in 0..160 {
        assert_eq!(on.advance(), off.advance(), "changed walker at {tick}");
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "walking hash at {tick}"
        );
    }
    assert!(on.movement_order(worker).is_some());
    let cost = on.gather_work();
    assert!(
        cost.visits - cost.slept < 5,
        "walking visits must sleep: {cost:?}"
    );
    let oracle = off.gather_work();
    assert_eq!(oracle.visits - oracle.slept, 160);
}

#[test]
fn walking_sleep_delivers_on_the_same_contact_tick() {
    let (mut on, worker) = returning(true, TileCoord::new(116, 116));
    let (mut off, _) = returning(false, TileCoord::new(116, 116));
    let before = on.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    for tick in 0..700 {
        assert_eq!(on.advance(), off.advance(), "contact changes at {tick}");
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "contact hash at {tick}"
        );
    }
    assert_eq!(
        on.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        before + 10
    );
    assert!(on.unit(worker).unwrap().carried.is_none());
    let cost = on.gather_work();
    assert!(
        cost.visits - cost.slept < 12,
        "contact must wake only when needed: {cost:?}"
    );
}

#[test]
fn stopping_a_walking_gatherer_removes_its_sleep_subscription() {
    let (mut on, worker) = returning(true, TileCoord::new(20, 20));
    let (mut off, _) = returning(false, TileCoord::new(20, 20));
    for tick in 0..300 {
        for w in [&mut on, &mut off] {
            if tick == 90 {
                w.issue_stop(PlayerId(0), worker).unwrap();
            }
            w.advance();
        }
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "cancelled walker at {tick}"
        );
    }
    assert!(on.economy.gatherers.is_empty());
    assert!(on.economy.gatherers.dozing().is_empty());
}

#[test]
fn movement_termination_wakes_a_walker_without_a_changed_position() {
    let (mut on, worker) = returning(true, TileCoord::new(20, 20));
    let (mut off, _) = returning(false, TileCoord::new(20, 20));
    for tick in 0..240 {
        for w in [&mut on, &mut off] {
            if tick == 80 {
                let index = w.lookup[&worker];
                w.stop_movement(index);
                w.set_movement_error(index, Some(GameWorldError::Unreachable));
            }
            w.advance();
        }
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "terminated walker at {tick}"
        );
    }
    assert!(
        on.movement_order(worker).is_some(),
        "a carrier retries another exit"
    );
}

#[test]
fn work_sleep_can_be_disabled_while_a_walker_is_dozing() {
    let (mut on, _) = returning(true, TileCoord::new(20, 20));
    let (mut off, _) = returning(false, TileCoord::new(20, 20));
    let mut slept = 0;
    for tick in 0..160 {
        if tick == 60 {
            slept = on.gather_work().slept;
            on.set_work_sleep(false);
        }
        assert_eq!(on.advance(), off.advance(), "toggle changes at {tick}");
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "toggle hash at {tick}"
        );
    }
    assert!(slept > 50);
    assert_eq!(on.gather_work().slept, slept);
    assert!(on.economy.gatherers.dozing().is_empty());
}

#[test]
fn empty_gathering_drains_deferred_changes_before_a_new_job() {
    let (mut on, worker) = returning(true, TileCoord::new(116, 116));
    let (mut off, _) = returning(false, TileCoord::new(116, 116));
    let before = on.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    for world in [&mut on, &mut off] {
        let task = world.economy.gatherers.remove(&worker).unwrap();
        world.economy.late_changed.push(worker);
        let mut changed = vec![worker];
        world.advance_gathering(&mut |_| true, &mut changed);
        assert!(world.economy.late_changed.is_empty());
        assert_eq!(changed, [worker]);
        world.economy.gatherers.insert(worker, task);
    }
    for tick in 0..700 {
        assert_eq!(on.advance(), off.advance(), "new job changes at {tick}");
        assert_eq!(
            on.canonical_hash_hex(),
            off.canonical_hash_hex(),
            "new job hash at {tick}"
        );
    }
    assert_eq!(
        on.player_economy(PlayerId(0)).unwrap().stockpile.wood,
        before + 10
    );
}
