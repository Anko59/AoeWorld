//! S4: re-ordering some members of a marching group.
use super::*;

#[test]
fn reordering_ten_of_sixty_units_leaves_the_other_fifty_marching() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = block(&mut w, PlayerId(0), (40, 40), 10, 60);
    let to = center(140, 43);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, false)
        .unwrap();
    for _ in 0..100 {
        w.advance();
    }
    let (away, rest) = (&ids[20..30], [&ids[..20], &ids[30..]].concat());
    for &id in away {
        w.issue_move(id, center(60, 90)).unwrap();
    }
    let still_grouped = rest.iter().filter(|&&id| w.formation(id).is_some()).count();
    let moving_now = rest
        .iter()
        .filter(|&&id| w.unit(id).unwrap().moving)
        .count();
    for _ in 0..4000 {
        w.advance();
    }
    let arrived = rest
        .iter()
        .filter(|&&id| tiles_between(w.unit(id).unwrap().position, to) <= 8)
        .count();
    let left_arrived = away
        .iter()
        .filter(|&&id| tiles_between(w.unit(id).unwrap().position, center(60, 90)) <= 1)
        .count();
    note(
        "reorder-10-of-60",
        &[
            ("rest_still_grouped", still_grouped as i64),
            ("rest_moving_after_order", moving_now as i64),
            ("rest_arrived", arrived as i64),
            ("reordered_arrived", left_arrived as i64),
        ],
    );
    // The ten re-ordered units reach their new goal.
    assert_eq!(left_arrived, 10);
    // S4, fixed by PR 1: the other fifty stay grouped, keep moving and all
    // arrive. (Before PR 1 the group was deleted and all fifty stopped on the
    // spot, none arriving.)
    assert_eq!((still_grouped, moving_now, arrived), (50, 50, 50));
}
