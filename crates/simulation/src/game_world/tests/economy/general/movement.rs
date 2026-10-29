use super::*;

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
        decide(&mut w);
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
        assert!(across(slots[4]) < 0 && across(slots[5]) > 0, "flanks");
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
    for &(id, goal) in &goals[1..] {
        assert_eq!(w.formation_destination_slot(id), Some(goal));
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
