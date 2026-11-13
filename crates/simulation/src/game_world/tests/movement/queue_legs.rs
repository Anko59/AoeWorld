use super::*;

fn at(x: i32, y: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap()
}

#[test]
fn queued_formation_leg_stays_behind_a_members_earlier_scalar_queue() {
    let mut world = world();
    let ids: Vec<_> = (0..3)
        .map(|n| {
            world
                .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 10 + n))
                .unwrap()
        })
        .collect();
    let form = aoe_core::Formation::Line;
    world
        .issue_formation_move(PlayerId(0), &ids, form, at(30, 10), false, false)
        .unwrap();
    // A non-leader member is individually queued to B before the group's C.
    world.issue_queued_move(ids[1], at(30, 30)).unwrap();
    world
        .issue_formation_move(PlayerId(0), &ids, form, at(10, 30), true, false)
        .unwrap();
    let destinations = world.move_destinations(ids[1]);
    let at_b = destinations.iter().position(|&d| d == at(30, 30));
    let at_c = destinations.iter().position(|&d| d == at(10, 30));
    assert!(
        at_b.is_some() && at_c.is_some() && at_b < at_c,
        "A, then B, then C for the member: {destinations:?}"
    );
}

#[test]
fn queued_attack_move_leg_keeps_the_units_earlier_move_and_queue() {
    let mut world = world();
    let unit = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(10, 10))
        .unwrap();
    world.issue_move(unit, at(30, 10)).unwrap();
    world.issue_queued_move(unit, at(30, 30)).unwrap();
    // Shift attack-move for the single selected unit, as the client sends it.
    world.issue_queued_leg(unit, at(10, 30), true).unwrap();
    assert_eq!(
        world.move_destinations(unit),
        [at(30, 10), at(30, 30), at(10, 30)],
        "A, then B, then attack-move C"
    );
    let snapshot = world.snapshot([0; 32]).unwrap();
    let mut restored = super::world();
    restored
        .restore_snapshot(&snapshot, [0; 32], &|| false)
        .unwrap();
    assert_eq!(
        restored.move_destinations(unit),
        world.move_destinations(unit)
    );
    for _ in 0..3000 {
        world.advance();
    }
    assert_eq!(world.unit(unit).unwrap().position, at(10, 30));
}
