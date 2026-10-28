//! Temporary congestion preserves orders and bounds work on every tick.
use super::*;

fn boxed_mover(destination: WorldPosition) -> (GameWorld, EntityId) {
    boxed_mover_by(destination, PlayerId(0))
}

fn boxed_mover_by(destination: WorldPosition, ring: PlayerId) -> (GameWorld, EntityId) {
    let mut w = world();
    w.set_fog_disabled(true);
    let mover = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.issue_move(mover, destination).unwrap();
    // A later crowd closes every escape edge without overlapping the mover.
    for (dx, dy) in [
        (-1, -1),
        (0, -1),
        (1, -1),
        (-1, 0),
        (1, 0),
        (-1, 1),
        (0, 1),
        (1, 1),
    ] {
        w.spawn_villager(
            ring,
            WorldPosition::new(at(40, 30).x + dx * 640, at(40, 30).y + dy * 640),
        )
        .unwrap();
    }
    (w, mover)
}

#[test]
fn blocked_near_arrival_accepts_a_free_spot_and_stays_settled() {
    let mut w = world();
    let mover = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    w.spawn_villager(PlayerId(0), at(42, 30)).unwrap();
    w.issue_move(mover, at(42, 30)).unwrap();
    for _ in 0..160 {
        movement_tick(&mut w);
    }
    assert!(
        w.movement_order(mover).is_none(),
        "congested arrival never settled"
    );
    assert_eq!(w.units[w.lookup[&mover]].last_movement_error, None);
    assert_eq!(w.active_mover_count(), 0);
    let position = w.unit(mover).unwrap().position;
    assert!(distance(position, at(42, 30)) <= (3 * 1024_i64).pow(2));
    for _ in 0..80 {
        assert!(movement_tick(&mut w).is_empty());
        assert_eq!(w.unit(mover).unwrap().position, position);
    }
}

#[test]
fn a_future_destination_reservation_does_not_keep_a_free_arrival_moving() {
    let mut w = world();
    let mover = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    let later = w.spawn_villager(PlayerId(0), at(20, 30)).unwrap();
    w.issue_move(later, at(40, 30)).unwrap();
    for _ in 0..32 {
        movement_tick(&mut w);
    }
    assert!(w.movement_order(mover).is_none());
    assert_eq!(w.movement_failure(mover), None);
    assert_eq!(w.unit(mover).unwrap().position, at(40, 30));
    assert!(w.movement_order(later).is_some());
}

#[test]
fn a_full_worksite_waits_and_resumes_when_a_physical_work_spot_opens() {
    let mut w = super::super::far_orders::wide_world();
    w.set_fog_disabled(true);
    let node = aoe_map::ResourceNode::starting(TileCoord::new(40, 30), aoe_map::ResourceKind::Wood)
        .unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, node).unwrap();
    }
    // Work-slot admission refuses a site whose perimeter is already full, so
    // the crowd closes it after the approach was admitted.
    let worker = w.spawn_villager(PlayerId(0), at(35, 30)).unwrap();
    w.issue_gather(PlayerId(0), worker, node.id).unwrap();
    let blockers = crate::game_world::movement_step::slots::perimeter_positions(
        TileRect::from_xywh(40, 30, 1, 1),
        UnitKind::Villager,
    )
    .into_iter()
    .map(|p| w.spawn_villager(PlayerId(0), p).unwrap())
    .collect::<Vec<_>>();
    for _ in 0..160 {
        w.advance();
    }
    assert_eq!(w.gather_target(worker), Some(node.id));
    assert_eq!(w.movement_failure(worker), None);
    assert_ne!(w.unit(worker).unwrap().task, UnitTask::Gathering);
    for id in blockers {
        w.remove_unit(id);
    }
    for _ in 0..200 {
        w.advance();
    }
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Gathering);
    assert!(w.at_work_position(worker, node.tile));
}

#[test]
fn boxed_far_order_retains_its_goal_without_a_false_static_failure() {
    let (mut w, mover) = boxed_mover_by(at(70, 30), PlayerId(1));
    for _ in 0..160 {
        movement_tick(&mut w);
    }
    assert!(w.movement_order(mover).is_some());
    assert_eq!(w.movement_failure(mover), None);
    // The smaller body has room to yield inside a sealed foreign ring, but
    // cannot pass through it or misreport the original goal as unreachable.
    assert!(distance(w.unit(mover).unwrap().position, at(40, 30)) <= 640_i64.pow(2));
    assert_eq!(w.movement_order(mover).unwrap().destination, at(70, 30));
}

#[test]
fn a_sustained_friendly_gridlock_lets_the_mover_squeeze_through() {
    let (mut w, mover) = boxed_mover(at(70, 30));
    for _ in 0..160 {
        movement_tick(&mut w);
    }
    assert_eq!(w.movement_failure(mover), None);
    // Idle villagers of the same player do not seal a permanent box: after
    // sustained obstruction the mover shuffles through them toward its goal.
    let position = w.unit(mover).unwrap().position;
    assert!(position.x - at(40, 30).x >= 1024, "{position:?}");
    assert_eq!(
        w.movement_order(mover).map(|o| o.destination),
        Some(at(70, 30))
    );
}

#[test]
fn a_permanently_boxed_in_mover_gives_up_and_goes_idle() {
    use crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS;
    // A sealed ring of another player's bodies cannot be squeezed through.
    let (mut w, mover) = boxed_mover_by(at(70, 30), PlayerId(1));
    for _ in 0..GIVE_UP_AFTER_TICKS / 2 {
        movement_tick(&mut w);
    }
    assert!(w.movement_order(mover).is_some(), "still within the wait");
    for _ in 0..4 * u32::from(GIVE_UP_AFTER_TICKS) {
        movement_tick(&mut w);
    }
    assert!(w.movement_order(mover).is_none(), "gave up");
    assert_eq!(w.movement_failure(mover), Some(GameWorldError::Unreachable));
    assert_eq!(w.active_mover_count(), 0);
    assert_eq!(w.unit(mover).unwrap().task, UnitTask::Idle);
    assert!(!w.unit(mover).unwrap().moving);
}

#[test]
fn a_briefly_blocked_mover_arrives_when_the_crowd_clears_before_the_give_up() {
    use crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS;
    let (mut w, mover) = boxed_mover_by(at(70, 30), PlayerId(1));
    for _ in 0..GIVE_UP_AFTER_TICKS - 20 {
        movement_tick(&mut w);
    }
    assert!(w.movement_order(mover).is_some());
    assert_eq!(w.movement_failure(mover), None);
    let others = w
        .units()
        .filter(|u| u.id != mover)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    for id in others {
        w.remove_unit(id);
    }
    for _ in 0..1200 {
        movement_tick(&mut w);
    }
    assert_eq!(w.unit(mover).unwrap().position, at(70, 30));
    assert!(w.movement_order(mover).is_none());
    assert_eq!(w.movement_failure(mover), None);
}

#[test]
fn the_original_order_resumes_as_soon_as_temporary_traffic_clears() {
    let (mut w, mover) = boxed_mover(at(70, 30));
    for _ in 0..160 {
        movement_tick(&mut w);
    }
    assert_eq!(w.movement_failure(mover), None);
    let others = w
        .units()
        .filter(|u| u.id != mover)
        .map(|u| u.id)
        .collect::<Vec<_>>();
    for id in others {
        w.remove_unit(id);
    }
    assert!(w.movement_order(mover).is_some());
    for _ in 0..1200 {
        movement_tick(&mut w);
    }
    assert_eq!(w.unit(mover).unwrap().position, at(70, 30));
    assert!(w.movement_order(mover).is_none());
}

#[test]
fn sidestep_oscillation_cannot_replenish_the_obstruction_budget() {
    let (mut w, mover) = boxed_mover(at(70, 30));
    let index = w.lookup[&mover];
    let origin = w.unit(mover).unwrap().position;
    let hash = w.canonical_hash();
    w.record_unit_obstruction(index);
    assert_ne!(
        w.canonical_hash(),
        hash,
        "retry state affects future movement"
    );
    for n in 1..20 {
        w.tick.0 += 1;
        w.units[index].state.position.x = origin.x + if n % 2 == 0 { 256 } else { -256 };
        w.record_unit_obstruction(index);
        w.record_unit_obstruction(index); // one allowance per tick, including multi-segment steps
        w.observe_unit_progress(index);
        assert_eq!(w.units[index].obstruction.unwrap().blocked, n + 1);
    }
    w.units[index].state.position.x = origin.x + 640;
    w.observe_unit_progress(index);
    assert_eq!(w.units[index].obstruction.unwrap().blocked, 0);
    w.issue_stop(PlayerId(0), mover).unwrap();
    assert!(w.units[index].obstruction.is_none());
}

#[test]
fn movement_probe_observes_bounded_searches_without_changing_replay() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[derive(Debug, Default)]
    struct Probe(AtomicUsize, AtomicUsize);
    impl crate::PlanningProbe for Probe {
        fn begin(&self) {}
        fn end(&self) {}
        fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
            if name == "local_refinement" {
                self.0.fetch_add(1, Ordering::Relaxed);
            } else if name == "sidestep" {
                self.1.fetch_add(1, Ordering::Relaxed);
            }
            None
        }
    }
    let (mut measured, mover) = boxed_mover(at(70, 30));
    let (mut plain, _) = boxed_mover(at(70, 30));
    let probe = Arc::new(Probe::default());
    measured.set_planning_probe(probe.clone());
    let mut before = 0;
    for _ in 0..160 {
        assert_eq!(movement_tick(&mut measured), movement_tick(&mut plain));
        assert_eq!(measured.canonical_hash(), plain.canonical_hash());
        let now = probe.1.load(Ordering::Relaxed);
        // Measured: never more than one sidestep search on a tick.
        assert!(now - before <= 1, "sidesteps on one tick: {}", now - before);
        before = now;
    }
    assert!(measured.movement_order(mover).is_some());
    let searches = probe.0.load(Ordering::Relaxed);
    assert!(searches > 0 && searches <= 16, "searches: {searches}");
    let sidesteps = probe.1.load(Ordering::Relaxed);
    // Measured: 15 searches and 53 sidesteps (at most 1 per tick) in 160
    // ticks; the bounds sit just above, so a retry storm still fails.
    assert!(sidesteps <= 56, "sidesteps: {sidesteps}");
    // Repeated obstruction remains bounded on each tick, without cancelling
    // the command. Releasing the crowd lets the same command complete.
    for id in measured
        .units()
        .filter(|u| u.id != mover)
        .map(|u| u.id)
        .collect::<Vec<_>>()
    {
        measured.remove_unit(id);
    }
    for _ in 0..1200 {
        movement_tick(&mut measured);
    }
    assert!(measured.movement_order(mover).is_none());
    assert_eq!(measured.unit(mover).unwrap().position, at(70, 30));
}

#[test]
fn blocked_worker_accepts_a_free_spot_adjacent_to_its_resource() {
    let mut w = super::super::far_orders::wide_world();
    w.set_fog_disabled(true);
    let node = aoe_map::ResourceNode::starting(TileCoord::new(41, 31), aoe_map::ResourceKind::Wood)
        .unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, node).unwrap();
    }
    let worker = w.spawn_villager(PlayerId(0), at(40, 30)).unwrap();
    let reservation = w.spawn_villager(PlayerId(0), at(35, 30)).unwrap();
    w.issue_move(reservation, at(40, 30)).unwrap();
    w.issue_gather(PlayerId(0), worker, node.id).unwrap();
    w.issue_stop(PlayerId(0), reservation).unwrap();
    for (dx, dy) in [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1)] {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::new(at(40, 30).x + dx * 640, at(40, 30).y + dy * 640),
        )
        .unwrap();
    }
    for _ in 0..32 {
        movement_tick(&mut w);
    }
    assert!(
        w.movement_order(worker).is_none(),
        "worker keeps walking despite a legal work spot"
    );
    assert_eq!(w.units[w.lookup[&worker]].last_movement_error, None);
    assert_eq!(w.gather_target(worker), Some(node.id));
    w.advance();
    assert_eq!(w.unit(worker).unwrap().task, UnitTask::Gathering);
    assert!(!w.local_position_occupied(w.lookup[&worker], w.unit(worker).unwrap().position));
}

#[test]
fn default_village_hunter_clears_traffic_and_delivers_sheep() {
    let (mut w, _) = GameWorld::default_with_village(Seed(1));
    w.set_fog_disabled(true);
    let worker = w.units().find(|u| u.kind == UnitKind::Villager).unwrap().id;
    let sheep = w
        .animals()
        .find(|a| a.kind == crate::AnimalKind::Sheep)
        .unwrap()
        .id;
    w.issue_hunt(PlayerId(0), &[worker], sheep).unwrap();
    for _ in 0..10_000 {
        w.advance();
        if w.player_economy(PlayerId(0)).unwrap().stockpile.food == 300 {
            break;
        }
    }
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().stockpile.food,
        300,
        "worker={:?} order={:?} error={:?} hunt={:?} sheep={:?} obstruction={:?}",
        w.unit(worker),
        w.movement_order(worker),
        w.movement_failure(worker),
        w.hunting_target(worker),
        w.animal(sheep),
        w.units[w.lookup[&worker]].obstruction
    );
}

#[test]
fn a_future_work_reservation_does_not_veto_a_free_gather_arrival() {
    let mut w = super::super::far_orders::wide_world();
    let tile = TileCoord::new(40, 30);
    let node = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Wood).unwrap();
    if let Terrain::Map { generator, overlay } = &mut w.terrain {
        overlay.insert_starting_resource(generator, node).unwrap();
    }
    let contact = WorldPosition::new(tile.x * 1024 - 213, tile.y * 1024 + 512);
    let worker = w.spawn_villager(PlayerId(0), contact).unwrap();
    w.issue_gather(PlayerId(0), worker, node.id).unwrap();
    let later = w.spawn_villager(PlayerId(0), at(20, 30)).unwrap();
    w.issue_task_move(later, contact).unwrap();
    let index = w.lookup[&worker];
    assert!(w.destination_point_reserved(contact, worker));
    assert!(w.arrival_position_free(index));
    assert!(w.at_gather_arrival(index));
}
