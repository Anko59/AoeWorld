//! A General's army orders lead the army; a "General only" move does not.
use super::*;

fn muster(w: &mut GameWorld) -> (Vec<EntityId>, EntityId) {
    let ids: Vec<_> = (0..12)
        .map(|n| soldier(w, UnitKind::Militia, 100 + n % 4, 100 + n / 4))
        .collect();
    let g = soldier(w, UnitKind::General, 102, 104);
    (ids, g)
}

fn player_move(w: &mut GameWorld, g: EntityId, to: WorldPosition) {
    w.issue_move(g, to).unwrap();
    w.general_player_order(PlayerId(0), &[g], Some(to));
}

fn walked(w: &GameWorld, id: EntityId, from: WorldPosition) -> i64 {
    let p = w.unit(id).unwrap().position;
    i64::from(p.x - from.x)
        .abs()
        .max(i64::from(p.y - from.y).abs())
        / 1024
}

#[test]
fn army_move_leads_the_army_but_general_only_move_leaves_it_in_place() {
    for solo in [false, true] {
        let (mut w, _) = setup();
        let (ids, g) = muster(&mut w);
        let starts: Vec<_> = ids.iter().map(|&id| w.unit(id).unwrap().position).collect();
        let origin = w.unit(g).unwrap().position;
        if solo {
            w.issue_general_move(PlayerId(0), g, at(150, 104)).unwrap();
        } else {
            w.issue_move(g, at(150, 104)).unwrap();
        }
        advance(&mut w, 500);
        assert!(
            // Led by his army's block the General is held to its pace.
            walked(&w, g, origin) > 15,
            "General walks (solo {solo}): {} tiles",
            walked(&w, g, origin)
        );
        let moved = ids
            .iter()
            .zip(&starts)
            .filter(|&(&id, &p)| walked(&w, id, p) > 10)
            .count();
        if solo {
            assert_eq!(moved, 0, "the army holds position");
            assert_eq!(w.general_status(g).unwrap().state, GeneralState::Idle);
        } else {
            assert_eq!(moved, ids.len(), "the army follows its General");
        }
    }
}

#[test]
fn general_only_move_ends_with_the_next_army_order() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    w.issue_general_move(PlayerId(0), g, at(120, 104)).unwrap();
    advance(&mut w, 100);
    let start = w.unit(ids[0]).unwrap().position;
    w.issue_move(g, at(150, 104)).unwrap();
    advance(&mut w, 300);
    assert!(walked(&w, ids[0], start) > 10, "army marches again");
}

#[test]
fn solo_hold_ignores_visible_threats_until_march_or_stop() {
    for threat in ["soldier", "overwhelming", "building", "wounded"] {
        for resume_with_stop in [false, true] {
            let (mut w, _) = setup();
            let (ids, g) = muster(&mut w);
            w.issue_general_move(PlayerId(0), g, at(140, 104)).unwrap();
            if threat == "building" {
                w.insert_building(
                    PlayerId(1),
                    BuildingKind::House,
                    aoe_core::TileRect::from_xywh(110, 104, 2, 2),
                    true,
                )
                .unwrap();
            } else {
                let count = if threat == "overwhelming" { 30 } else { 1 };
                for n in 0..count {
                    w.spawn_kind(PlayerId(1), UnitKind::Knight, at(110 + n % 4, 104))
                        .unwrap();
                }
            }
            if threat == "wounded" {
                w.units[w.lookup[&g]].state.hp = 1;
            }
            for _ in 0..8 {
                decide(&mut w);
                assert_eq!(
                    w.general_status(g).unwrap().state,
                    GeneralState::Idle,
                    "{threat}"
                );
                assert_eq!(w.general_work().orders, 0, "solo means no army orders");
                for &id in &ids {
                    let u = w.unit(id).unwrap();
                    assert!(!u.moving && !u.planning && u.attack_target.is_none());
                    assert!(w.formation(id).is_none());
                }
            }
            if resume_with_stop {
                w.issue_stop(PlayerId(0), g).unwrap();
            } else {
                w.issue_move(g, at(140, 104)).unwrap();
            }
            decide(&mut w);
            assert_ne!(w.general_status(g).unwrap().state, GeneralState::Idle);
            assert!(w.general_work().orders > 0);
        }
    }
}

#[test]
fn general_only_move_rejects_other_owners_and_other_units() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    assert!(w.issue_general_move(PlayerId(1), g, at(120, 104)).is_err());
    assert!(
        w.issue_general_move(PlayerId(0), ids[0], at(120, 104))
            .is_err()
    );
}

#[test]
fn general_walks_through_his_own_army_that_fills_a_corridor() {
    let (mut w, _) = setup();
    for x in 100..160 {
        w.economy.blocked_tiles.insert(TileCoord::new(x, 59));
        w.economy.blocked_tiles.insert(TileCoord::new(x, 61));
    }
    let g = soldier(&mut w, UnitKind::General, 101, 60);
    let wall: Vec<_> = (0..16)
        .map(|n| soldier(&mut w, UnitKind::Militia, 110 + n, 60))
        .collect();
    advance(&mut w, 5);
    w.issue_general_move(PlayerId(0), g, at(135, 60)).unwrap();
    advance(&mut w, 900);
    let p = w.unit(g).unwrap().position;
    assert!(p.x > 130 * 1024, "General crossed his army: {p:?}");
    assert!(wall.iter().all(|&id| w.unit(id).unwrap().hp > 0));
}

/// The owner could not rescue a stuck group: its General kept taking it back.
#[test]
fn a_general_never_overwrites_a_players_order_on_his_soldiers() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    assert_eq!(w.general_controller(ids[0]), Some(g));
    w.issue_move(g, at(170, 104)).unwrap();
    advance(&mut w, 40);
    let rescued = &ids[..4];
    let refuge = at(102, 140);
    w.issue_formation_move(
        PlayerId(0),
        rescued,
        aoe_core::Formation::Line,
        refuge,
        false,
        false,
    )
    .unwrap();
    w.general_player_order(PlayerId(0), rescued, Some(refuge));
    assert_eq!(w.general_controller(ids[0]), None);
    advance(&mut w, 800);
    for &id in rescued {
        let unit = w.unit(id).unwrap();
        assert!(
            unit.position.y > 130 * 1024,
            "{id:?} went where the player sent it: {:?}",
            unit.position
        );
        assert_eq!(w.general_controller(id), None);
    }
    assert!(w.general_status(g).unwrap().army_size < 12);
    assert!(
        w.unit(ids[5]).unwrap().position.x > 110 * 1024,
        "the rest march on"
    );
}

#[test]
fn a_general_takes_no_soldier_passing_under_a_players_order() {
    let (mut w, _) = setup();
    let g = soldier(&mut w, UnitKind::General, 102, 104);
    let passing = soldier(&mut w, UnitKind::Militia, 80, 104);
    w.issue_move(passing, at(125, 104)).unwrap();
    for _ in 0..1000 {
        advance(&mut w, 1);
        assert_eq!(w.general_controller(passing), None);
        if !w.unit(passing).unwrap().moving {
            break;
        }
    }
    assert_eq!(w.general_status(g).unwrap().army_size, 0);
}

#[test]
fn ordering_the_general_readmits_the_soldiers_who_finished_nearby() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    advance(&mut w, 40);
    assert_eq!(
        w.general_controller(ids[0]),
        None,
        "no census takes it back"
    );
    player_move(&mut w, g, at(150, 104));
    advance(&mut w, 40);
    assert_eq!(
        w.general_controller(ids[0]),
        None,
        "a soldier stopped moments ago stays out: the player meant it"
    );
    advance(&mut w, 300);
    player_move(&mut w, g, at(120, 104));
    advance(&mut w, 40);
    assert_eq!(
        w.general_controller(ids[0]),
        Some(g),
        "idle long enough, a fresh General order takes it back"
    );
}

#[test]
fn a_soldier_under_a_live_order_is_never_readmitted() {
    let (mut w, _) = setup();
    let (ids, g) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    w.issue_move(ids[0], at(101, 104)).unwrap();
    advance(&mut w, 400);
    w.issue_move(ids[0], at(101, 140)).unwrap();
    player_move(&mut w, g, at(150, 104));
    advance(&mut w, 20);
    assert_eq!(w.general_controller(ids[0]), None);
    assert!(w.unit(ids[0]).unwrap().moving);
}

#[test]
fn another_players_general_never_releases_a_detached_soldier() {
    let (mut w, _) = setup();
    let (ids, _) = muster(&mut w);
    advance(&mut w, 20);
    w.general_player_order(PlayerId(0), &ids[..1], None);
    advance(&mut w, 400);
    let rival = w
        .spawn_kind(PlayerId(1), UnitKind::General, at(103, 104))
        .unwrap();
    w.issue_move(rival, at(150, 104)).unwrap();
    w.general_player_order(PlayerId(1), &[rival], Some(at(150, 104)));
    assert!(
        w.economy.generals.manual.contains_key(&ids[0]),
        "player 1's order leaves player 0's detached soldier alone"
    );
}

#[test]
fn detached_soldiers_replay_alike_on_one_and_four_workers() {
    let run = |workers: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap()
            .install(|| {
                let (mut w, _) = setup();
                let (ids, g) = muster(&mut w);
                advance(&mut w, 20);
                w.general_player_order(PlayerId(0), &ids[..3], None);
                advance(&mut w, 350);
                player_move(&mut w, g, at(150, 104));
                advance(&mut w, 200);
                w.canonical_hash()
            })
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn an_unchanged_army_goal_retains_its_route_and_layout() {
    let (mut w, _) = setup();
    for x in 100..104 {
        soldier(&mut w, UnitKind::Militia, x, 110);
    }
    let g = soldier(&mut w, UnitKind::General, 104, 110);
    let goal = at(150, 112);
    w.general_march(g, goal);
    decide(&mut w);
    let before = w.canonical_hash();
    for same in [goal, at(151, 112)] {
        w.general_march(g, same);
        assert_eq!(
            w.canonical_hash(),
            before,
            "retain the army's route and layout"
        );
    }
    w.general_march(g, at(155, 112));
    assert_ne!(w.canonical_hash(), before);
}

#[test]
fn wounded_soldiers_keep_their_home_trip_without_new_general_orders() {
    let (mut w, _) = setup();
    let ids: Vec<_> = (0..8)
        .map(|n| soldier(&mut w, UnitKind::Knight, 100 + n, 110))
        .collect();
    let g = soldier(&mut w, UnitKind::General, 104, 112);
    w.units[w.lookup[&ids[0]]].state.hp = 1;
    w.spawn_kind(PlayerId(1), UnitKind::Militia, at(115, 110))
        .unwrap();
    w.set_fog_disabled(true);
    w.with_order_issuer(crate::OrderIssuer::General, decide);
    assert_eq!(w.general_status(g).unwrap().state, GeneralState::Fighting);
    assert!(w.unit(ids[0]).unwrap().moving);
    let before = w.order_demand()[crate::OrderIssuer::General as usize].orders;
    for _ in 0..10 {
        w.with_order_issuer(crate::OrderIssuer::General, decide);
    }
    assert_eq!(
        w.order_demand()[crate::OrderIssuer::General as usize].orders,
        before
    );
}
