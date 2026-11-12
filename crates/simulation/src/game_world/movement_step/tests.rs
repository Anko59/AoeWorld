use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig};

#[test]
fn a_motion_commit_without_displacement_wakes_idle_acquisition() {
    let build = |sleep| {
        let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(1)).unwrap()).unwrap();
        w.set_work_sleep(sleep);
        let at = WorldPosition::from_tile_center(TileCoord::new(100, 100)).unwrap();
        let id = w
            .spawn_kind(PlayerId(0), crate::UnitKind::Militia, at)
            .unwrap();
        w.advance_combat(&mut Vec::new());
        (w, id)
    };
    let (mut on, id) = build(true);
    let (mut off, _) = build(false);
    assert_eq!(on.canonical_hash(), off.canonical_hash());
    for w in [&mut on, &mut off] {
        let index = w.lookup[&id];
        let mut state = w.units[index].state;
        state.moving = true;
        state.task = UnitTask::Moving;
        let proposal = FastStep {
            index,
            state,
            order: None,
            pops: 0,
            keeps_moving: true,
            bucket: w.units[index].bucket,
            vision_stale: false,
            member: None,
        };
        assert!(w.apply_step_full(proposal, &mut Vec::new(), &mut Vec::new()));
        w.advance_combat(&mut Vec::new());
    }
    assert_eq!(on.canonical_hash(), off.canonical_hash());
}

#[test]
fn group_neighbor_radius_bound_grows_at_spawn_and_stays_conservative_after_removal() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let center = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, center(20, 20))
        .unwrap();
    let mut tables = Tables::default();
    tables.refresh(&world, true);
    assert_eq!(tables.neighbor_radius, [205, 0]);
    let large = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Mangonel, center(30, 20))
        .unwrap();
    tables.refresh(&world, true);
    assert_eq!(tables.neighbor_radius, [461, 0]);
    world.remove_unit(large);
    tables.refresh(&world, true);
    assert_eq!(tables.neighbor_radius, [461, 0]);
}

#[test]
fn a_snapshot_with_overfull_cells_cannot_certify_an_ordered_group_step() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let center = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let mover = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, center(20, 20))
        .unwrap();
    let mut crowd = Vec::new();
    for _ in 0..33 {
        crowd.push(
            world
                .spawn_kind(PlayerId(1), crate::UnitKind::Militia, center(100, 100))
                .unwrap(),
        );
    }
    world
        .issue_formation_move(
            PlayerId(0),
            &[mover],
            aoe_core::Formation::Line,
            center(80, 20),
            false,
            false,
        )
        .unwrap();
    world.advance();
    let mut tables = Tables::default();
    tables.refresh(&world, true);
    let index = world.lookup[&mover];
    let proposal = world.propose_group_step(index, &tables).unwrap();
    assert!(world.collisions.has_overfull_cells());
    assert!(!proposal.member.unwrap().certified);
    // A preceding ordered write can remove overflow. The old proposal must
    // still recheck: its snapshot sampled only 32 members of the overfull cell.
    world.remove_unit(crowd.pop().unwrap());
    assert!(!world.collisions.has_overfull_cells());
    assert!(!proposal.member.unwrap().certified);
    tables.refresh(&world, true);
    let proposal = world
        .propose_group_step(world.lookup[&mover], &tables)
        .unwrap();
    assert!(proposal.member.unwrap().certified);
}

#[test]
fn movement_reuses_proposals_and_dense_tables_between_ticks() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    world
        .place_town_center(aoe_core::PlayerId(0), TileCoord::new(5, 5))
        .unwrap();
    for n in 0..64 {
        let at = WorldPosition::from_tile_center(TileCoord::new(20 + n % 8, 20 + n / 8)).unwrap();
        let id = world.spawn_unit(PlayerId(0), at).unwrap();
        world
            .issue_move(id, WorldPosition::new(at.x + 64 * 1024, at.y))
            .unwrap();
    }
    world.advance();
    let proposals = world.movement_scratch.proposals.as_ptr();
    let economies = world.movement_scratch.tables.economies.as_ptr();
    assert!(world.movement_scratch.tables.economies.capacity() > 0);
    let spare = world.movement_scratch.movers.as_ptr();
    world.advance();
    world.advance();
    assert_eq!(world.active_mover_count(), 64);
    assert_eq!(world.movement_scratch.proposals.as_ptr(), proposals);
    assert_eq!(world.movement_scratch.tables.economies.as_ptr(), economies);
    assert_eq!(world.movement_scratch.movers.as_ptr(), spare);
}

#[test]
fn an_idle_population_does_not_build_movement_tables() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 0..1_000 {
        world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(10 + n % 100, 10 + n / 100))
                    .unwrap(),
            )
            .unwrap();
    }
    world.advance();
    assert_eq!(world.movement_scratch.proposals.capacity(), 0);
    assert_eq!(world.movement_scratch.tables.economies.capacity(), 0);
}

#[test]
fn late_game_proposals_keep_rotation_order_among_idle_crowds() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    for n in 0..1_000 {
        world
            .spawn_unit(
                PlayerId(0),
                WorldPosition::from_tile_center(TileCoord::new(100 + n % 100, 100 + n / 100))
                    .unwrap(),
            )
            .unwrap();
    }
    let mut movers = Vec::new();
    for n in 0..64 {
        let at = WorldPosition::from_tile_center(TileCoord::new(20 + n % 8, 20 + n / 8)).unwrap();
        let id = world.spawn_unit(PlayerId(0), at).unwrap();
        world
            .issue_move(id, WorldPosition::new(at.x + 64 * 1024, at.y))
            .unwrap();
        movers.push(id);
    }
    let mut scratch = Scratch::default();
    world.propose_steps_into(&movers, 7, &mut scratch);
    assert_eq!(scratch.jobs.len(), 64);
    for turn in 0..64 {
        assert_eq!(
            scratch.take_step(turn).unwrap().state.id,
            movers[(7 + turn) % 64]
        );
    }
}

#[test]
fn a_waiting_formation_does_not_reverse_an_available_passing_lane() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let from = WorldPosition::from_tile_center(TileCoord::new(50, 50)).unwrap();
    let mover = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, from)
        .unwrap();
    // The enemy body vetoes a direct/diagonal step, while either lateral
    // lane stays open. Friendly squeeze cannot change that fixed geometry.
    world
        .spawn_kind(
            PlayerId(1),
            crate::UnitKind::Militia,
            WorldPosition::new(from.x + 430, from.y),
        )
        .unwrap();
    let goal = WorldPosition::new(from.x + 40 * 1024, from.y);
    world
        .issue_formation_move(
            PlayerId(0),
            &[mover],
            aoe_core::Formation::Line,
            goal,
            false,
            false,
        )
        .unwrap();
    let index = world.lookup[&mover];
    let proposed = WorldPosition::new(from.x + 50, from.y);
    world.record_obstruction_towards(index, goal);
    let first = world.avoid_unit_step(index, proposed);
    assert!(
        first.y > from.y,
        "an available passing lane is required: {first:?}"
    );
    // The actor can remain queued while other traffic crosses. Waiting must
    // not itself change a safe choice into a step back across that traffic.
    for tick in 1..=60 {
        world.tick.0 = tick;
        world.record_obstruction_towards(index, goal);
    }
    let later = world.avoid_unit_step(index, proposed);
    assert_eq!(
        later, first,
        "unchanged traffic must retain the passing lane"
    );
}

fn static_pressure_proposal() -> (GameWorld, usize, FastStep) {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let from = WorldPosition::from_tile_center(TileCoord::new(50, 50)).unwrap();
    let mover = world
        .spawn_kind(PlayerId(0), crate::UnitKind::Militia, from)
        .unwrap();
    world
        .spawn_kind(
            PlayerId(1),
            crate::UnitKind::Militia,
            WorldPosition::new(from.x + 430, from.y),
        )
        .unwrap();
    world
        .insert_building(
            PlayerId(0),
            crate::BuildingKind::Barracks,
            aoe_core::TileRect::from_xywh(49, 51, 3, 3),
            true,
        )
        .unwrap();
    world
        .issue_formation_move(
            PlayerId(0),
            &[mover],
            aoe_core::Formation::Line,
            WorldPosition::new(from.x + 40 * 1024, from.y),
            false,
            false,
        )
        .unwrap();
    world.advance_formations(&mut Vec::new(), &mut Scratch::default());
    let mut tables = Tables::default();
    tables.refresh(&world, true);
    let index = world.lookup[&mover];
    let proposal = world.propose_group_step(index, &tables).unwrap();
    assert_eq!(
        proposal.state.position,
        world.unit(mover).unwrap().position,
        "the body, not terrain, must obstruct the primary proposal"
    );
    assert!(!proposal.member.unwrap().obstructed);
    (world, index, proposal)
}

#[test]
fn collision_adjustment_reuses_the_proposals_static_clearance_proof() {
    let (mut world, index, mut proposal) = static_pressure_proposal();
    let (mut reference, reference_index, reference_proposal) = static_pressure_proposal();
    let motion = proposal.member.unwrap();
    let from = world.units[index].state.position;
    let proposed = crate::game_world::formation::group_step(from, motion.target, motion.budget);
    let before = reference
        .route_scheduler_stats()
        .metrics
        .group_collision_work;
    reference
        .record_obstruction_towards(reference_index, reference_proposal.member.unwrap().target);
    let expected = reference.avoid_unit_step(reference_index, proposed);
    let necessary = reference
        .route_scheduler_stats()
        .metrics
        .group_collision_work
        - before;
    let before = world.route_scheduler_stats().metrics.group_collision_work;
    world.adjust_group_step(&mut proposal, motion);
    let actual = world.route_scheduler_stats().metrics.group_collision_work - before;
    assert_eq!(proposal.state.position, expected);
    assert!(world.local_unit_step_clear(index, from, expected));
    assert!(world.body_step_clear(crate::UnitKind::Militia, from, expected));
    assert!(
        actual <= necessary,
        "a proved static segment needs only live avoidance: {actual} queries vs {necessary}"
    );
}

#[test]
fn a_moved_actor_invalidates_the_cached_static_proof() {
    let (mut world, index, mut proposal) = static_pressure_proposal();
    let old = world.units[index].state.position;
    let from = WorldPosition::new(old.x, 54 * 1024 + 206);
    let id = world.units[index].state.id;
    world.units[index].state.position = from;
    let state = world.units[index].state;
    world.track_unit_step(id, old.tile_floor(), &state);
    // A body at the old endpoint forces live collision adjustment after the
    // actor has moved to a legal point just outside the other building side.
    world
        .spawn_kind(PlayerId(1), crate::UnitKind::Militia, old)
        .unwrap();
    let motion = proposal.member.unwrap();
    let primary = crate::game_world::formation::group_step(from, motion.target, motion.budget);
    assert!(world.body_position_clear(crate::UnitKind::Militia, from));
    assert!(!world.body_step_clear(crate::UnitKind::Militia, from, primary));
    world.adjust_group_step(&mut proposal, motion);
    assert!(
        proposal.member.unwrap().obstructed,
        "the old proof cannot certify the new start"
    );
    assert!(world.body_step_clear(crate::UnitKind::Militia, from, proposal.state.position));
}

#[test]
fn a_blocked_scout_escapes_a_fractional_parking_wedge_without_new_overlap() {
    use crate::UnitKind;
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(0)).unwrap()).unwrap();
    let from = WorldPosition::new(44556, 44504);
    let id = w.spawn_kind(PlayerId(0), UnitKind::Scout, from).unwrap();
    let others: Vec<_> = [
        (UnitKind::Archer, WorldPosition::new(44680, 44704)),
        (UnitKind::Villager, WorldPosition::new(44616, 44299)),
        (UnitKind::Scout, WorldPosition::new(44386, 44271)),
    ]
    .into_iter()
    .map(|(kind, at)| w.spawn_kind(PlayerId(0), kind, at).unwrap())
    .collect();
    let index = w.lookup[&id];
    let goal = WorldPosition::new(60000, 60000);
    w.issue_move(id, goal).unwrap();
    for _ in 0..40 {
        w.tick.0 += 1;
        w.record_obstruction_towards(index, goal);
    }
    let next = w.avoid_unit_step(index, WorldPosition::new(from.x + 54, from.y + 55));
    assert_ne!(next, from, "there is a body-clear southwest escape");
    let distance = |a: WorldPosition, b: WorldPosition| {
        i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)
    };
    assert!(
        distance(from, next) <= 78 * 78,
        "only this tick's movement credit"
    );
    for other in others {
        let other = w.unit(other).unwrap();
        let hard = collision::hard_distance(w.unit(id).as_ref().unwrap(), &other);
        let before = distance(from, other.position);
        assert!(
            distance(next, other.position) >= before.min(hard * hard),
            "no new penetration: {other:?}"
        );
    }
}
