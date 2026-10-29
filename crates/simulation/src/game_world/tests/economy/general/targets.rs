use super::*;

#[test]
fn focus_orders_siege_monks_leaders_archers_then_wounded_by_distance_and_id() {
    let (mut w, _) = setup();
    let archer = soldier(&mut w, UnitKind::Archer, 104, 100);
    let g = soldier(&mut w, UnitKind::General, 103, 100);
    // A large friendly army keeps each decision favourable.
    for n in 0..20 {
        soldier(&mut w, UnitKind::Knight, 100 + n % 5, 101 + n / 5);
    }
    // Keep the distant siege visible through the ordinary friendly observer index.
    soldier(&mut w, UnitKind::Scout, 114, 102);
    let kinds = [
        UnitKind::Mangonel,
        UnitKind::BatteringRam,
        UnitKind::Monk,
        UnitKind::General,
        UnitKind::Vassal,
        UnitKind::Archer,
        UnitKind::Crossbowman,
        UnitKind::Skirmisher,
        UnitKind::EliteSkirmisher,
        UnitKind::Militia,
        UnitKind::Militia,
    ];
    let enemies: Vec<_> = kinds
        .into_iter()
        .enumerate()
        .map(|(n, kind)| {
            // Lower categories are closer; class priority must still win.
            let x = if n == 0 { 114 } else { 113 - n as i32 / 2 };
            w.spawn_kind(PlayerId(1), kind, at(x, 100)).unwrap()
        })
        .collect();
    w.units[w.lookup[&enemies[9]]].state.hp = 1;
    let order = [1, 0, 2, 4, 3, 8, 6, 7, 5, 9, 10];
    // The test controls only the decision; no movement or combat tick changes distances.
    for n in order {
        // Put the archer in the micro window on each admission.
        w.economy.generals.armies.get_mut(&g).unwrap().micro_after = 0;
        decide(&mut w);
        assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
        assert_eq!(
            w.unit(archer).unwrap().attack_target,
            Some(enemies[n]),
            "expected {:?}",
            kinds[n]
        );
        w.kill_unit(w.lookup[&enemies[n]]);
    }
}
