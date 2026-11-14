use super::*;

#[test]
fn gameplay_guide_describes_offensive_formation_marching() {
    let guide = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/gameplay/general.md"
    ));
    assert!(guide.contains("A march is an attack-move formation"));
    assert!(!guide.contains("A march is a plain formation move"));
}

#[test]
fn adoption_waits_for_a_march_order_before_forming_at_the_castle_exit() {
    let (mut w, _) = setup();
    let ids: Vec<_> = [UnitKind::Militia, UnitKind::Archer, UnitKind::BatteringRam]
        .into_iter()
        .enumerate()
        .map(|(n, kind)| soldier(&mut w, kind, 100 + n as i32 * 2, 100))
        .collect();
    let positions: Vec<_> = ids.iter().map(|&id| w.unit(id).unwrap().position).collect();
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    advance(&mut w, 20);
    for (&id, position) in ids.iter().zip(positions) {
        assert_eq!(w.general_controller(id), Some(g));
        assert_eq!(w.formation(id), None, "adoption is not a march order");
        assert_eq!(w.unit(id).unwrap().position, position);
    }
    w.issue_move(g, at(145, 103)).unwrap();
    advance(&mut w, 10);
    assert!(ids.iter().all(|&id| w.formation(id).is_some()));
}

#[test]
fn role_destinations_rotate_and_keep_hard_bodies_separate() {
    for (dx, dy) in [(50, 0), (0, 50), (-50, 0), (35, -35)] {
        let (mut w, _) = setup();
        let kinds = [
            UnitKind::Archer,
            UnitKind::Militia,
            UnitKind::BatteringRam,
            UnitKind::Scout,
            UnitKind::Knight,
            UnitKind::Knight,
        ];
        let ids: Vec<_> = kinds
            .into_iter()
            .enumerate()
            .map(|(n, kind)| soldier(&mut w, kind, 110 + n as i32 * 2, 110))
            .collect();
        let g = soldier(&mut w, UnitKind::General, 116, 112);
        let center = at(116 + dx, 110 + dy);
        let positions: Vec<_> = ids
            .iter()
            .chain([&g])
            .map(|&id| w.unit(id).unwrap().position)
            .collect();
        let heading = WorldPosition::new(
            center.x - positions.iter().map(|p| p.x).sum::<i32>() / positions.len() as i32,
            center.y - positions.iter().map(|p| p.y).sum::<i32>() / positions.len() as i32,
        );
        w.issue_move(g, center).unwrap();
        advance(&mut w, 3000);
        let slots: Vec<_> = ids
            .iter()
            .map(|&id| w.formation_destination_slot(id).unwrap())
            .collect();
        let along = |p: WorldPosition| {
            i64::from(p.x - center.x) * i64::from(heading.x)
                + i64::from(p.y - center.y) * i64::from(heading.y)
        };
        let across = |p: WorldPosition| {
            i64::from(p.y - center.y) * i64::from(heading.x)
                - i64::from(p.x - center.x) * i64::from(heading.y)
        };
        assert!(along(slots[3]) > along(slots[1]), "scouts ahead");
        assert!(along(slots[1]) > along(slots[0]), "infantry front");
        assert!(along(slots[0]) > along(slots[2]), "siege rear");
        let flank = 3 * 1024 * (i64::from(heading.x).pow(2) + i64::from(heading.y).pow(2)).isqrt();
        assert!(
            across(slots[4]).abs() > flank && across(slots[5]).abs() > flank,
            "cavalry squad on the flank"
        );
        assert_eq!(
            across(slots[4]).signum(),
            across(slots[5]).signum(),
            "one squad stays on one flank"
        );
        assert_eq!(
            w.units[w.lookup[&ids[4]]].formation,
            w.units[w.lookup[&ids[5]]].formation
        );
        let mut all = slots;
        all.push(w.formation_destination_slot(g).unwrap());
        for (n, &a) in all.iter().enumerate() {
            let a_id = ids.iter().copied().chain([g]).nth(n).unwrap();
            for (m, &b) in all.iter().enumerate().skip(n + 1) {
                let b_id = ids.iter().copied().chain([g]).nth(m).unwrap();
                let gap = w.unit(a_id).unwrap().kind.collision_radius()
                    + w.unit(b_id).unwrap().kind.collision_radius()
                    + 8;
                assert!(distance_squared(a, b) >= i64::from(gap).pow(2));
            }
        }
    }
}

#[test]
fn role_block_reaches_its_validated_destinations_after_a_partial_stop() {
    let (mut w, _) = setup();
    let ids: Vec<_> = [
        UnitKind::Archer,
        UnitKind::Militia,
        UnitKind::BatteringRam,
        UnitKind::Scout,
        UnitKind::Knight,
    ]
    .into_iter()
    .enumerate()
    .map(|(n, kind)| soldier(&mut w, kind, 106 + n as i32 * 2, 106))
    .collect();
    let g = soldier(&mut w, UnitKind::General, 110, 108);
    w.issue_move(g, at(150, 108)).unwrap();
    decide(&mut w);
    let goals: Vec<_> = ids
        .iter()
        .chain([&g])
        .map(|&id| (id, w.formation_destination_slot(id).unwrap()))
        .collect();
    advance(&mut w, 150);
    // Stop only one member through the ordinary movement API. Its neighbours
    // retain their destination slots and the shared route carrier changes.
    w.issue_stop(PlayerId(0), ids[0]).unwrap();
    advance(&mut w, 2800);
    for &(id, _) in &goals[1..] {
        let goal = w
            .formation_destination_slot(id)
            .expect("validated final slot");
        if id != g {
            let army = &w.economy.generals.armies[&g];
            assert!(
                army.squads
                    .iter()
                    .find(|s| s.members.contains(&id))
                    .unwrap()
                    .final_leg,
                "an intermediate destination cannot satisfy this test"
            );
        }
        assert!(distance_squared(goal, at(150, 108)) < (32_i64 * 1024).pow(2));
        assert_eq!(w.unit(id).unwrap().position, goal, "member {id:?}");
        assert!(!w.unit(id).unwrap().moving);
    }
}

fn distance_squared(a: WorldPosition, b: WorldPosition) -> i64 {
    i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)
}

#[test]
fn showcase_trains_claims_marches_and_defeats_the_enemy_group() {
    let (mut w, _) = GameWorld::general_demo(aoe_core::Seed(1)).unwrap();
    let castle = w
        .buildings()
        .find(|b| b.kind == BuildingKind::Castle)
        .unwrap()
        .id;
    assert!(w.units().all(|u| u.kind != UnitKind::General));
    w.issue_train(PlayerId(0), castle, UnitKind::General)
        .unwrap();
    advance(&mut w, 300);
    let g = w.units().find(|u| u.kind == UnitKind::General).unwrap().id;
    assert_eq!(w.general_castle(g), Some(castle));
    // Thirteen staged soldiers plus the village's ordinary starting Scout.
    assert_eq!(w.general_status(g).unwrap().army_size, 14);
    w.issue_move(g, at(166, 132)).unwrap();
    let mut fought = false;
    for _ in 0..800 {
        w.advance();
        fought |= w.general_status(g).unwrap().state == GeneralState::Fighting;
    }
    assert!(fought);
    assert!(w.unit(g).unwrap().hp > 0);
    let survivors: Vec<_> = w
        .units()
        .filter(|u| u.player != PlayerId(0) && u.hp > 0)
        .collect();
    assert!(
        survivors.is_empty(),
        "survivors {survivors:?}; General {:?} {:?}",
        w.unit(g),
        w.general_status(g)
    );
}

#[test]
fn a_squad_whose_route_wait_expires_is_ordered_again_by_its_general() {
    let (mut w, _) = setup();
    let ids: Vec<_> = (0..3)
        .map(|n| soldier(&mut w, UnitKind::Militia, 100, 100 + n))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    w.issue_move(g, at(145, 103)).unwrap();
    decide(&mut w);
    let carrier = w.units[w.lookup[&ids[0]]].formation.expect("squad group");
    assert!(ids.iter().all(|&id| w.formation(id).is_some()));
    let index = w.lookup[&carrier];
    w.end_waiting_route(index, carrier, false, &mut Vec::new());
    for &id in &ids {
        assert!(w.formation(id).is_none(), "no group steers {id:?}");
        assert_eq!(w.unit(id).unwrap().task, crate::UnitTask::Idle);
        assert!(w.movement_failure(id).is_some(), "failure is visible");
    }
    assert!(!w.formations.contains_key(&carrier));
    decide(&mut w);
    assert!(
        ids.iter().all(|&id| w.formation(id).is_some()),
        "the General re-issues the squad's march"
    );
}

#[test]
fn army_membership_does_not_change_a_players_selection_layout() {
    let run = |detach: bool| {
        let (mut w, _) = setup();
        let mut ids: Vec<_> = [
            UnitKind::Militia,
            UnitKind::Archer,
            UnitKind::Knight,
            UnitKind::BatteringRam,
        ]
        .into_iter()
        .enumerate()
        .map(|(n, kind)| soldier(&mut w, kind, 100 + n as i32 * 2, 100))
        .collect();
        let g = soldier(&mut w, UnitKind::General, 104, 104);
        if detach {
            for &id in &ids {
                w.forget_general(id);
            }
        }
        ids.push(g);
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            aoe_core::Formation::Line,
            at(145, 110),
            false,
            true,
        )
        .unwrap();
        ids.iter()
            .map(|&id| w.formation_destination_slot(id).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        run(false),
        run(true),
        "the shared layout has no army-ownership hook"
    );
}
