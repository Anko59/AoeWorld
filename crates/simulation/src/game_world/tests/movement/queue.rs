use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn queued_player_moves_visit_each_destination_and_replay_identically() {
    let mut first = world();
    let mut second = world();
    let a = first.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let b = second.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let points = [at(14, 10), at(14, 14), at(10, 14), at(10, 10)];
    for (world, id) in [(&mut first, a), (&mut second, b)] {
        world.issue_move(id, points[0]).unwrap();
        for p in &points[1..] {
            world.issue_queued_move(id, *p).unwrap();
        }
        assert_eq!(world.move_destinations(id), points);
    }
    let mut visited = 0;
    for _ in 0..300 {
        first.advance();
        second.advance();
        assert_eq!(first.canonical_hash(), second.canonical_hash());
        if visited < points.len() && first.unit(a).unwrap().position == points[visited] {
            visited += 1;
        }
    }
    assert_eq!(visited, points.len());
    assert!(first.move_destinations(a).is_empty());
    assert_eq!(first.active_mover_count(), 0);
}

#[test]
fn queued_moves_are_bounded_refused_atomically_and_cancelled_by_replacement_or_stop() {
    let mut world = world();
    let id = world.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    world.issue_queued_move(id, at(14, 10)).unwrap();
    for n in 1..aoe_core::MAX_MOVE_DESTINATIONS {
        world.issue_queued_move(id, at(14 + n as i32, 10)).unwrap();
    }
    let before = world.canonical_hash();
    assert!(world.issue_queued_move(id, at(50, 10)).is_err());
    assert_eq!(world.canonical_hash(), before);
    assert!(
        world
            .issue_queued_move(id, WorldPosition::new(-10, 0))
            .is_err()
    );
    assert_eq!(world.canonical_hash(), before);
    world.issue_move(id, at(10, 14)).unwrap();
    assert!(world.move_destinations(id).is_empty());
    world.issue_queued_move(id, at(20, 14)).unwrap();
    world.issue_stop(PlayerId(0), id).unwrap();
    assert!(world.move_destinations(id).is_empty());
    world.advance();
    assert!(!world.unit(id).unwrap().moving);
}

#[test]
fn pending_destinations_participate_in_the_canonical_hash() {
    let mut first = world();
    let mut second = world();
    let a = first.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    let b = second.spawn_unit(PlayerId(0), at(10, 10)).unwrap();
    first.issue_move(a, at(14, 10)).unwrap();
    second.issue_move(b, at(14, 10)).unwrap();
    first.issue_queued_move(a, at(14, 14)).unwrap();
    second.issue_queued_move(b, at(14, 15)).unwrap();
    assert_eq!(first.unit(a), second.unit(b));
    assert_ne!(first.canonical_hash(), second.canonical_hash());
}

#[test]
fn fixed_speed_has_equal_distance_for_cardinal_and_diagonal_orders() {
    let mut cardinal = world();
    let mut diagonal = world();
    let start = WorldPosition::new(10 * FIXED_SUBUNITS_PER_TILE, 10 * FIXED_SUBUNITS_PER_TILE);
    let a = cardinal.spawn_unit(PlayerId(0), start).unwrap();
    let b = diagonal.spawn_unit(PlayerId(0), start).unwrap();
    cardinal
        .issue_move(a, WorldPosition::new(start.x + 5_000, start.y))
        .unwrap();
    diagonal
        .issue_move(b, WorldPosition::new(start.x + 5_000, start.y + 5_000))
        .unwrap();
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 0);
    for _ in 0..8 {
        cardinal.advance();
        diagonal.advance();
    }
    assert_eq!(cardinal.movement_order(a).unwrap().travelled, 1_024);
    assert_eq!(diagonal.movement_order(b).unwrap().travelled, 1_024);
}

#[test]
fn exact_arrival_and_mid_move_redirection_are_deterministic() {
    let mut first = world();
    let mut second = world();
    let start = WorldPosition::new(20_000, 20_000);
    let a = first.spawn_unit(PlayerId(0), start).unwrap();
    let b = second.spawn_unit(PlayerId(0), start).unwrap();
    let target = WorldPosition::new(23_000, 24_000);
    first.issue_move(a, target).unwrap();
    second.issue_move(b, target).unwrap();
    for _ in 0..100 {
        first.advance();
        second.advance();
    }
    assert_eq!(
        first.unit(a),
        second.unit(b).map(|unit| GameUnit { id: a, ..unit })
    );
    assert_eq!(
        first.unit(a).unwrap().position,
        WorldConfig::default().snap_ground_position(target)
    );
    assert!(!first.unit(a).unwrap().moving);
    first
        .issue_move(a, WorldPosition::new(25_000, 24_000))
        .unwrap();
    first.advance();
    assert!(first.unit(a).unwrap().moving);
}
