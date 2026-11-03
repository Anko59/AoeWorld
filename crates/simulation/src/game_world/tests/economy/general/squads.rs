//! A march delegates to bounded, homogeneous ordinary movement groups.
use super::*;
use std::collections::BTreeMap;

#[test]
fn a_mixed_hundred_marches_as_eight_independent_typed_squads() {
    let (mut w, _) = setup();
    let mut soldiers = Vec::new();
    for (kind, count) in [
        (UnitKind::Militia, 40),
        (UnitKind::Archer, 30),
        (UnitKind::Knight, 20),
        (UnitKind::BatteringRam, 10),
    ] {
        for _ in 0..count {
            let n = soldiers.len() as i32;
            soldiers.push(soldier(&mut w, kind, 100 + n % 10, 100 + n / 10));
        }
    }
    let g = soldier(&mut w, UnitKind::General, 104, 112);
    w.issue_move(g, at(150, 110)).unwrap();
    decide(&mut w);
    let mut squads = BTreeMap::<EntityId, Vec<EntityId>>::new();
    for id in soldiers {
        squads
            .entry(
                w.units[w.lookup[&id]]
                    .formation
                    .expect("every soldier marches"),
            )
            .or_default()
            .push(id);
    }
    assert_eq!(squads.len(), 8, "40/20 + 30/15 + 20/10 + 10/5");
    for members in squads.values() {
        let kind = w.unit(members[0]).unwrap().kind;
        assert!(
            members.iter().all(|&id| w.unit(id).unwrap().kind == kind),
            "one sergeant per troop type"
        );
        let limit = match w.unit(members[0]).unwrap().kind {
            UnitKind::Militia => 20,
            UnitKind::Archer => 15,
            UnitKind::Knight => 10,
            UnitKind::BatteringRam => 5,
            kind => panic!("unexpected squad kind {kind:?}"),
        };
        assert_eq!(members.len(), limit);
    }
    assert!(
        w.general_work().orders <= 11,
        "ten squads and their General"
    );
    let before: Vec<_> = squads
        .values()
        .flatten()
        .map(|&id| (id, w.units[w.lookup[&id]].formation))
        .collect();
    decide(&mut w);
    assert!(
        before
            .iter()
            .all(|&(id, group)| w.units[w.lookup[&id]].formation == group)
    );
    assert_eq!(
        w.general_work().orders,
        0,
        "an unchanged march keeps routes"
    );
}

#[test]
fn ten_infantry_squadrons_adopt_two_hundred_soldiers() {
    let (mut w, _) = setup();
    for n in 0..220 {
        soldier(&mut w, UnitKind::Militia, 100 + n % 11, 100 + n / 11);
    }
    let g = soldier(&mut w, UnitKind::General, 105, 110);
    assert_eq!(w.general_status(g).unwrap().army_size, 200);
    let other = soldier(&mut w, UnitKind::General, 106, 110);
    assert_eq!(w.general_status(other).unwrap().army_size, 20);
}

#[test]
fn ten_siege_squadrons_adopt_fifty_soldiers() {
    let (mut w, _) = setup();
    for n in 0..60 {
        soldier(&mut w, UnitKind::BatteringRam, 100 + n % 10, 100 + n / 10);
    }
    let g = soldier(&mut w, UnitKind::General, 105, 108);
    assert_eq!(w.general_status(g).unwrap().army_size, 50);
    let other = soldier(&mut w, UnitKind::General, 106, 108);
    assert_eq!(w.general_status(other).unwrap().army_size, 10);
}

#[test]
fn a_sergeants_death_promotes_its_next_soldier_without_reforming_other_squads() {
    let (mut w, _) = setup();
    let ids: Vec<_> = (0..21)
        .map(|n| soldier(&mut w, UnitKind::Militia, 100 + n % 7, 100 + n / 7))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 104, 106);
    w.issue_move(g, at(150, 106)).unwrap();
    decide(&mut w);
    let other = w.units[w.lookup[&ids[20]]].formation;
    w.kill_unit(w.lookup[&ids[0]]);
    decide(&mut w);
    let squads = &w.economy.generals.armies[&g].squads;
    assert_eq!(squads.len(), 2);
    assert_eq!(squads[0].members[0], ids[1]);
    assert_eq!(squads[1].members, vec![ids[20]]);
    assert_eq!(w.units[w.lookup[&ids[20]]].formation, other);
    assert_eq!(
        w.general_work().orders,
        0,
        "unaffected routes survive promotion"
    );
    assert!(
        ids[1..]
            .iter()
            .all(|&id| w.general_controller(id) == Some(g))
    );
}
