//! S3 and S6, fixed by PR 3: every member walks to its own slot and stops
//! only there, and members that have arrived fight while others still walk.
use super::*;

/// Trees on the eight tiles around `(x, y)`: a unit standing there is sealed in.
fn seal(w: &mut GameWorld, (x, y): (i32, i32)) {
    forest(w, TileRect::from_xywh(x - 1, y - 1, 3, 1));
    forest(w, TileRect::from_xywh(x - 1, y + 1, 3, 1));
    forest(w, TileRect::from_xywh(x - 1, y, 1, 1));
    forest(w, TileRect::from_xywh(x + 1, y, 1, 1));
}

#[test]
fn a_sealed_member_does_not_hold_back_the_others_who_fight_on_arrival() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let mut ids = block(&mut w, PlayerId(0), (40, 40), 10, 30);
    // The thirty-first militia is walled in on all sides and cannot move.
    let sealed_at = (40, 60);
    seal(&mut w, sealed_at);
    let sealed = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            center(sealed_at.0, sealed_at.1),
        )
        .unwrap();
    ids.push(sealed);
    let to = center(100, 43);
    // One enemy waits two tiles beyond the front of the destination line. It
    // never attacks, so only our units open the fight.
    let foe = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(103, 43))
        .unwrap();
    w.issue_stance(PlayerId(1), &[foe], CombatStance::NoAttack)
        .unwrap();
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, false)
        .unwrap();
    let healthy = &ids[..30];
    let mut arrived_at: Vec<Option<u32>> = vec![None; ids.len()];
    let mut seen = Default::default();
    let mut first_hit = None;
    let mut anchor_arrival = None;
    for tick in 1..=4000_u32 {
        w.advance();
        assert_eq!(
            w.unit(sealed).unwrap().position,
            center(sealed_at.0, sealed_at.1),
            "tick {tick}"
        );
        if anchor_arrival.is_none() && w.formation_anchor(ids[0]) == Some(to) {
            anchor_arrival = Some(tick);
        }
        if first_hit.is_none() && !new_hits(&w, &[foe], &mut seen).is_empty() {
            first_hit = Some(tick);
        }
        for (n, &id) in ids.iter().enumerate() {
            if arrived_at[n].is_none() && !w.unit(id).unwrap().moving {
                arrived_at[n] = Some(tick);
            }
        }
    }
    let arrivals = |range: &[Option<u32>]| range.iter().flatten().copied().collect::<Vec<_>>();
    let healthy_arrivals = arrivals(&arrived_at[..30]);
    let (first, last) = (
        healthy_arrivals.iter().min().copied(),
        healthy_arrivals.iter().max().copied(),
    );
    let sealed_unit = w.unit(sealed).unwrap();
    let stuck_arrival = arrived_at[30];
    let foe_hp = w.unit(foe).map_or(0, |u| u.hp);
    note(
        "sealed-member",
        &[
            ("healthy_arrived", healthy_arrivals.len() as i64),
            ("first_arrival", first.map_or(-1, i64::from)),
            ("last_arrival", last.map_or(-1, i64::from)),
            ("first_hit", first_hit.map_or(-1, i64::from)),
            ("sealed_arrival", stuck_arrival.map_or(-1, i64::from)),
            ("foe_hp", i64::from(foe_hp)),
        ],
    );
    // The sealed unit never moves; it stops where it stands once the anchor
    // has arrived and it has stalled for the give-up wait.
    assert_eq!(sealed_unit.position, center(sealed_at.0, sealed_at.1));
    assert_eq!(
        sealed_unit.position.tile_floor(),
        TileCoord::new(sealed_at.0, sealed_at.1)
    );
    assert!(w.body_position_clear(sealed_unit.kind, sealed_unit.position));
    assert_eq!(stuck_arrival, Some(1674));
    assert_eq!(
        stuck_arrival,
        anchor_arrival.map(|tick| tick
            + u32::from(crate::game_world::movement_step::settling::GIVE_UP_AFTER_TICKS))
    );
    assert!(!sealed_unit.moving && !sealed_unit.planning && sealed_unit.task == UnitTask::Idle);
    // Every other unit is at the destination: at its slot, or fighting beside it.
    assert!(
        healthy
            .iter()
            .all(|&id| tiles_between(w.unit(id).unwrap().position, to) <= 8)
    );
    // The faster cohesion floor settles the healthy tail by tick 1503.
    assert_eq!((first, last), (Some(1434), Some(1503)));
    // PR 10 permits legal blows on approach before the healthy tail settles.
    assert_eq!(
        (first_hit, healthy_arrivals.len(), foe_hp),
        (Some(1462), 30, 0)
    );
    assert!(first_hit < last && first_hit < stuck_arrival);
}
