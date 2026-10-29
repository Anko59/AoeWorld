//! S5-S6 and S7: attack-move into a small enemy group, and a melee crowd.
use super::*;

#[test]
fn melee_strike_census_keeps_simultaneous_hits_on_one_victim() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let attackers = [49, 51].map(|x| {
        w.spawn_kind(PlayerId(0), UnitKind::Militia, center(x, 50))
            .unwrap()
    });
    let foe = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(50, 50))
        .unwrap();
    for id in attackers {
        w.issue_attack(PlayerId(0), id, foe).unwrap();
    }
    w.advance();
    assert_eq!(
        new_melee_hits(&w, &attackers, &mut Default::default()),
        attackers
    );
    assert_eq!(new_hits(&w, &[foe], &mut Default::default()).len(), 1);
}

#[test]
fn attack_move_kills_ten_enemies_then_resumes_to_the_slots() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = block(&mut w, PlayerId(0), (40, 40), 10, 60);
    let foes = block(&mut w, PlayerId(1), (90, 40), 5, 10);
    let to = center(140, 43);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, to, false, true)
        .unwrap();
    // Each unit's slot at the destination, before the march starts.
    let slots = ids
        .iter()
        .map(|&id| (id, w.formation_destination_slot(id).unwrap()))
        .collect::<Vec<_>>();
    let mut cleared = None;
    let mut strikers = std::collections::BTreeSet::new();
    let mut seen = Default::default();
    for tick in 1..=4000_u32 {
        w.advance();
        strikers.extend(new_melee_hits(&w, &ids, &mut seen));
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
    let apart = |&&(id, slot): &&(EntityId, WorldPosition)| {
        w.unit(id)
            .filter(|u| u.hp > 0)
            .map(|u| tiles_between(u.position, slot))
    };
    let at_slot = |tiles: i64| {
        slots
            .iter()
            .filter_map(|s| apart(&s))
            .filter(|&d| d <= tiles)
            .count()
    };
    let (within_1, within_2) = (at_slot(1), at_slot(2));
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
    note(
        "attack-move-10",
        &[
            ("cleared_tick", cleared.map_or(-1, i64::from)),
            ("distinct_strikers", strikers.len() as i64),
            ("survivors", survivors as i64),
            ("foes_alive", foes_alive as i64),
            ("own_idle", own_idle as i64),
            ("at_destination", at_destination as i64),
            ("still_moving", still_moving as i64),
            ("slot_within_1_tile", within_1 as i64),
            ("slot_within_2_tiles", within_2 as i64),
        ],
    );
    // PR 10 first-contact probes and bounded repeats change clearing to tick 1174.
    assert_eq!((cleared, foes_alive), (Some(1174), 0));
    assert_eq!((survivors, own_idle, still_moving), (60, 60, 0));
    // Keep all 60 arrivals and the original 37-striker assertion.
    assert_eq!((at_destination, strikers.len()), (60, 37));
    assert!(
        within_1 * 100 >= survivors * 95,
        "{within_1} of {survivors}"
    );
    assert_eq!((within_1, within_2), (60, 60));
}

#[test]
fn forty_against_forty_melee_strike_count() {
    let (hits, alive) = melee_clash(10);
    // PR 10 first-contact probes and bounded repeats change participation and survivors.
    assert_eq!(hits, (Some(145), 50, 80));
    assert_eq!(alive, (0, 7));
}

#[test]
fn forty_against_forty_fronts_strike_within_five_seconds() {
    let (hits, _) = melee_clash(4);
    assert!(hits.1 >= 72, "{hits:?}: 90% must strike within 5 s");
    assert!(
        hits.1 >= 79,
        "{hits:?}: retain PR 10 deployed-front participation"
    );
    assert_eq!(hits.2, 80);
}

fn melee_clash(columns: usize) -> ((Option<u32>, usize, usize), (usize, usize)) {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let west = block(&mut w, PlayerId(0), (40, 40), columns, 40);
    let east = block(&mut w, PlayerId(1), (60, 40), columns, 40);
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
        let hits = new_melee_hits(&w, &everyone, &mut seen);
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
        if columns == 10 {
            "melee-40v40"
        } else {
            "melee-40v40-front"
        },
        &[
            ("first_strike_tick", first_strike.map_or(-1, i64::from)),
            ("distinct_strikers_within_5s", within_5s as i64),
            ("distinct_strikers_total", strikers.len() as i64),
            ("west_alive", alive(&west) as i64),
            ("east_alive", alive(&east) as i64),
        ],
    );
    let result = (
        (first_strike, within_5s, strikers.len()),
        (alive(&west), alive(&east)),
    );
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/movement");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("melee-{columns}.txt")),
        format!("{result:?}\n"),
    )
    .unwrap();
    result
}

#[test]
fn a_ready_melee_unit_retargets_a_dead_contact_without_waiting_for_its_scan_phase() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let actor = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, center(50, 50))
        .unwrap();
    let stale = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(51, 50))
        .unwrap();
    let contact = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(49, 50))
        .unwrap();
    w.issue_stance(PlayerId(1), &[stale, contact], CombatStance::NoAttack)
        .unwrap();
    w.begin_autonomous_attack(actor, stale, center(50, 50));
    let dead = w.lookup[&stale];
    w.units[dead].state.hp = 0;
    let hp = w.unit(contact).unwrap().hp;
    assert!(!(w.tick.0 + 1 + u64::from(actor.0)).is_multiple_of(4));
    w.advance();
    assert_eq!(w.unit(actor).unwrap().attack_target, Some(contact));
    assert!(w.unit(contact).unwrap().hp < hp);
}

#[test]
fn a_ready_melee_unit_takes_live_contact_before_chasing_a_distant_target() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let actor = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, center(50, 50))
        .unwrap();
    let distant = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(60, 50))
        .unwrap();
    let contact = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(49, 50))
        .unwrap();
    w.issue_stance(PlayerId(1), &[distant, contact], CombatStance::NoAttack)
        .unwrap();
    w.begin_autonomous_attack(actor, distant, center(50, 50));
    let hp = w.unit(contact).unwrap().hp;
    assert!(!(w.tick.0 + 1 + u64::from(actor.0)).is_multiple_of(4));
    w.advance();
    assert_eq!(w.unit(actor).unwrap().attack_target, Some(contact));
    assert!(w.unit(contact).unwrap().hp < hp);
}
