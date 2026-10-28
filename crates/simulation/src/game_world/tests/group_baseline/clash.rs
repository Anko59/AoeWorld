//! S5-S6 and S7: attack-move into a small enemy group, and a melee crowd.
use super::*;

#[test]
fn attack_move_into_ten_enemies_does_not_resume_the_march() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = block(&mut w, PlayerId(0), (40, 40), 10, 60);
    let foes = block(&mut w, PlayerId(1), (90, 40), 5, 10);
    let to = center(140, 43);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, true)
        .unwrap();
    let mut cleared = None;
    let mut strikers = std::collections::BTreeSet::new();
    let mut seen = Default::default();
    for tick in 1..=4000_u32 {
        w.advance();
        strikers.extend(new_hits(&w, &foes, &mut seen));
        if cleared.is_none() && foes.iter().all(|&f| w.unit(f).is_none_or(|u| u.hp == 0)) {
            cleared = Some(tick);
        }
    }
    let survivors = ids
        .iter()
        .filter(|&&id| w.unit(id).is_some_and(|u| u.hp > 0))
        .count();
    let at_destination = ids
        .iter()
        .filter(|&&id| {
            w.unit(id)
                .is_some_and(|u| u.hp > 0 && tiles_between(u.position, to) <= 8)
        })
        .count();
    let still_moving = ids
        .iter()
        .filter(|&&id| w.unit(id).is_some_and(|u| u.moving))
        .count();
    let foes_alive = foes
        .iter()
        .filter(|&&f| w.unit(f).is_some_and(|u| u.hp > 0))
        .count();
    let own_idle = ids
        .iter()
        .filter(|&&f| w.unit(f).is_some_and(|u| u.task == UnitTask::Idle))
        .count();
    let centroid_x = ids
        .iter()
        .filter_map(|&f| w.unit(f))
        .map(|u| i64::from(u.position.x) / 1024)
        .sum::<i64>()
        / 60;
    note(
        "attack-move-10",
        &[
            ("cleared_tick", cleared.map_or(-1, i64::from)),
            ("distinct_strikers", strikers.len() as i64),
            ("survivors", survivors as i64),
            ("foes_alive", foes_alive as i64),
            ("own_idle", own_idle as i64),
            ("centroid_x", centroid_x),
            ("at_destination", at_destination as i64),
            ("still_moving", still_moving as i64),
        ],
    );
    // S5: the group dissolves on contact and never resumes its march. After
    // 4,000 ticks every survivor stands idle near x 75 of 140; none is at the
    // destination. Two of the ten enemies are still alive.
    // PR 4 flips this: all ten die, and at least 95 % of survivors reach their
    // slots.
    assert_eq!((cleared, foes_alive), (None, 2));
    assert_eq!((survivors, own_idle, centroid_x), (55, 55, 75));
    assert_eq!((at_destination, still_moving, strikers.len()), (0, 0, 15));
}

#[test]
fn forty_against_forty_melee_strike_count() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let west = block(&mut w, PlayerId(0), (40, 40), 10, 40);
    let east = block(&mut w, PlayerId(1), (60, 40), 10, 40);
    w.issue_formation_move(
        PlayerId(0),
        &west,
        Formation::Line,
        center(64, 43),
        false,
        true,
    )
    .unwrap();
    w.issue_formation_move(
        PlayerId(1),
        &east,
        Formation::Line,
        center(44, 43),
        false,
        true,
    )
    .unwrap();
    let everyone = [west.clone(), east.clone()].concat();
    let mut seen = Default::default();
    let mut first_strike = None;
    let mut strikers = std::collections::BTreeSet::new();
    let mut within_5s = 0;
    for tick in 1..=1200_u32 {
        w.advance();
        let hits = new_hits(&w, &everyone, &mut seen);
        if !hits.is_empty() {
            first_strike.get_or_insert(tick);
        }
        strikers.extend(hits);
        if first_strike.is_some_and(|first| tick == first + 100) {
            within_5s = strikers.len();
        }
    }
    let alive = |ids: &[EntityId]| {
        ids.iter()
            .filter(|&&id| w.unit(id).is_some_and(|u| u.hp > 0))
            .count()
    };
    note(
        "melee-40v40",
        &[
            ("first_strike_tick", first_strike.map_or(-1, i64::from)),
            ("distinct_strikers_within_5s", within_5s as i64),
            ("distinct_strikers_total", strikers.len() as i64),
            ("west_alive", alive(&west) as i64),
            ("east_alive", alive(&east) as i64),
        ],
    );
    // S7: five seconds after the first blow only 12 of the 80 units have been
    // seen to strike (a lower bound: two blows on one victim in a tick count
    // once). PR 10 flips this: at least 90 % of the melee units strike within
    // 5 s.
    assert_eq!(
        (first_strike, within_5s, strikers.len()),
        (Some(304), 12, 36)
    );
    assert_eq!((alive(&west), alive(&east)), (29, 26));
}
