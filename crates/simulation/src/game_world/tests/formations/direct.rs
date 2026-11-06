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
