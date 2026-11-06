//! A short group order is direct only when no building stands on a straight line.
use super::*;
use aoe_core::{Formation, TileRect};

#[test]
fn a_building_between_the_group_and_its_goal_keeps_the_shared_route() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 4);
    w.insert_building(
        PlayerId(1),
        BuildingKind::House,
        TileRect::from_xywh(44, 38, 2, 4),
        true,
    )
    .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(50, 40),
        false,
        false,
    )
    .unwrap();
    let direct = w
        .formation_member_probe(ids[0])
        .is_some_and(|probe| probe.0);
    assert!(!direct, "a house on the straight lines: not a direct order");
}

/// Members steer to their own slots, not the centre: a wall just beside the
/// centre lines (all at y >= 40.5) but across the lines to the slots of row
/// y = 38 keeps the shared route.
#[test]
fn a_wall_across_the_lines_to_the_slots_keeps_the_shared_route() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 12);
    w.insert_building(
        PlayerId(1),
        BuildingKind::House,
        TileRect::from_xywh(52, 39, 4, 1),
        true,
    )
    .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(60, 40),
        false,
        false,
    )
    .unwrap();
    let slots = ids
        .iter()
        .map(|&id| w.formation_destination_slot(id).unwrap().tile_floor())
        .collect::<Vec<_>>();
    assert!(
        slots.iter().any(|slot| slot.y <= 38),
        "the fixture needs slots beyond the wall: {slots:?}"
    );
    let direct = w
        .formation_member_probe(ids[0])
        .is_some_and(|probe| probe.0);
    assert!(
        !direct,
        "a wall across slot lines: not a direct order {slots:?}"
    );
}
