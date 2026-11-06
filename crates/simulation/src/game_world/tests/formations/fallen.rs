//! An army order names the units as the player selected them. In a 99-AI
//! match four whole probe groups (39 of 40 up to 399 of 400) stood idle all
//! run: one member had died before the order arrived, and the whole order was
//! refused as `UnknownEntity`. The fallen are left out; the living march.
use super::*;
use aoe_core::Formation;

#[test]
fn an_army_order_naming_a_fallen_member_moves_the_living() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 12);
    let fallen = ids[0];
    w.remove_unit(fallen);
    let dying = ids[5];
    let at = w.lookup[&dying];
    w.units[at].state.hp = 0;
    let start = ids
        .iter()
        .map(|&id| w.unit(id).map(|u| u.position))
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(70, 40),
        false,
        false,
    )
    .expect("the living members take the order");
    assert_eq!(
        w.route_metrics.attribution.group_members_left_out, 2,
        "both fallen members are counted"
    );
    for _ in 0..40 {
        w.advance();
    }
    for (n, &id) in ids.iter().enumerate().skip(1) {
        if id == dying {
            continue;
        }
        let unit = w.unit(id).unwrap();
        let from = start[n].unwrap();
        assert_eq!(unit.task, UnitTask::Moving, "{id:?} stands idle");
        assert!(unit.position.x > from.x + 512, "{id:?} did not move");
    }
}

#[test]
fn an_army_order_of_only_fallen_members_is_refused() {
    let mut w = wide_world();
    let ids = army(&mut w, 2);
    for &id in &ids {
        w.remove_unit(id);
    }
    assert_eq!(
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            Formation::Line,
            center(70, 40),
            false,
            false
        ),
        Err(EconomyError::UnknownEntity)
    );
}
