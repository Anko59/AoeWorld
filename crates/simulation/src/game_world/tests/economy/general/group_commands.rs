use super::*;
use aoe_core::Formation;

#[test]
fn selected_army_marches_and_follows_queued_attack_move() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let run = |workers| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap()
                .install(queued_march_replay)
        };
        assert_eq!(run(1), run(4));
    }
    #[cfg(target_arch = "wasm32")]
    queued_march_replay();
}

fn queued_march_replay() -> Vec<[u8; 32]> {
    let (mut w, _) = setup();
    let first = at(127, 104);
    let second = at(153, 104);
    let a = soldier(&mut w, UnitKind::Militia, 101, 104);
    let b = soldier(&mut w, UnitKind::Archer, 103, 106);
    let g = soldier(&mut w, UnitKind::General, 104, 104);
    let selection = [a, g]; // The unselected archer also remains in the army.
    w.issue_player_formation_move(
        PlayerId(0),
        &selection,
        Formation::Line,
        first,
        false,
        false,
    )
    .unwrap();
    w.issue_player_formation_move(PlayerId(0), &selection, Formation::Line, second, true, true)
        .unwrap();
    assert_eq!(w.move_destinations(g), [first, second]);
    assert_eq!(w.general_controller(a), Some(g));
    assert_eq!(w.general_controller(b), Some(g));
    let mut reached_first = false;
    let mut checkpoints = Vec::new();
    for _ in 0..1800 {
        w.advance();
        if w.tick.0.is_multiple_of(10) {
            checkpoints.push(w.canonical_hash());
        }
        let x = w.unit(g).unwrap().position.tile_floor().x;
        reached_first |= (125..=129).contains(&x);
        if x >= 151 {
            break;
        }
    }
    assert!(reached_first);
    assert!(w.unit(g).unwrap().position.tile_floor().x >= 151);
    assert!(w.unit(a).unwrap().position.tile_floor().x > 130);
    assert!(w.unit(b).unwrap().position.tile_floor().x > 130);
    assert_eq!(w.general_status(g).unwrap().army_size, 2);
    checkpoints.push(w.canonical_hash());
    checkpoints
}

#[test]
fn selecting_a_general_and_soldier_focuses_the_army_on_the_clicked_enemy() {
    let (mut w, _) = setup();
    let a = soldier(&mut w, UnitKind::Militia, 103, 103);
    let b = soldier(&mut w, UnitKind::Archer, 104, 104);
    let g = soldier(&mut w, UnitKind::General, 103, 104);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 104))
        .unwrap();
    w.issue_player_formation_move(
        PlayerId(0),
        &[a, g],
        Formation::Line,
        at(132, 112),
        false,
        true,
    )
    .unwrap();
    w.issue_player_attack(PlayerId(0), &[a, g], enemy).unwrap();
    advance(&mut w, 12);
    for id in [a, b, g] {
        assert_eq!(w.unit(id).unwrap().attack_target, Some(enemy), "{id:?}");
    }
    assert_eq!(w.general_controller(a), Some(g));
    assert_eq!(w.general_controller(b), Some(g));
}

#[test]
fn implicit_army_members_count_toward_the_attack_selection_limit() {
    let (mut w, _) = setup();
    let a = soldier(&mut w, UnitKind::Militia, 103, 103);
    soldier(&mut w, UnitKind::Archer, 104, 104);
    let g = soldier(&mut w, UnitKind::General, 103, 104);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 104))
        .unwrap();
    let mut selected = vec![g];
    selected.extend(
        (10_000..)
            .map(EntityId)
            .take(aoe_core::MAX_FORMATION_MEMBERS - 2),
    );
    assert!(matches!(
        w.issue_player_attack(PlayerId(0), &selected, enemy),
        Err(crate::EconomyError::OrderTooLarge)
    ));
    assert!(w.unit(g).unwrap().attack_target.is_none());
    assert!(w.unit(a).unwrap().attack_target.is_none());
}

#[test]
fn a_group_attack_cancels_only_the_patrols_of_units_whose_attack_is_accepted() {
    let (mut w, _) = setup();
    let a = soldier(&mut w, UnitKind::Militia, 103, 103);
    let b = soldier(&mut w, UnitKind::Archer, 104, 104);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Knight, at(110, 104))
        .unwrap();
    for id in [a, b] {
        w.issue_patrol(PlayerId(0), id, at(132, 112)).unwrap();
        assert!(w.combat.patrols.contains_key(&id));
    }
    assert!(w.issue_player_attack(PlayerId(0), &[a, b], a).is_err());
    assert!(w.combat.patrols.contains_key(&a));
    assert!(w.combat.patrols.contains_key(&b));
    w.issue_player_attack(PlayerId(0), &[a, b], enemy).unwrap();
    for id in [a, b] {
        assert!(!w.combat.patrols.contains_key(&id));
        assert_eq!(w.unit(id).unwrap().attack_target, Some(enemy));
    }
}
