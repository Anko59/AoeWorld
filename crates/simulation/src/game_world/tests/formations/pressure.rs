use super::*;
use aoe_core::Formation;

#[test]
fn formations_wide_staggered_wings_move_with_body_spacing_and_no_rays() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let origin = center(200, 200);
    let ids = (0..4096)
        .map(|n| {
            // A valid radius-separated army; a 4,096-body perfect overlap has
            // no physical packing and does not isolate wide-block steering.
            w.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                WorldPosition::new(
                    origin.x + (n % 64 - 32) * 512,
                    origin.y + (n / 64 - 32) * 512,
                ),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Staggered,
        center(400, 200),
        false,
        false,
    )
    .unwrap();
    let wing = *ids
        .iter()
        .min_by_key(|&&id| w.formation_slot(id).unwrap().y)
        .unwrap();
    assert_eq!(w.formation_slot(wing).unwrap().y, 120_597);
    let from = w.unit(wing).unwrap().position;
    for _ in 0..200 {
        w.advance();
    }
    assert!(
        w.unit(wing).unwrap().position.y < from.y - 1024,
        "an open outer slot must make local progress: {:?}",
        w.unit(wing)
    );
    assert_eq!(w.route_scheduler_stats().metrics.formation_steering_work, 0);
}

#[test]
fn formations_attack_move_notices_contact_at_the_outer_wing() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 200);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Flank,
        center(180, 50),
        false,
        true,
    )
    .unwrap();
    // The last ranks begin far from the army's centre.
    let wing = w.unit(*ids.last().unwrap()).unwrap().position.tile_floor();
    let enemy = w
        .spawn_unit(PlayerId(1), center(wing.x + 2, wing.y))
        .unwrap();
    for _ in 0..120 {
        w.advance();
    }
    // Only the members that see the enemy themselves and find a free spot
    // around it leave the group to fight it (before #612's follow-up, 32 did:
    // anyone within a 13x13-tile window); the head of the army, far away,
    // keeps marching in formation.
    let fighters = ids
        .iter()
        .filter(|&&id| w.unit(id).unwrap().attack_target == Some(enemy))
        .count();
    let left = ids.iter().filter(|&&id| w.formation(id).is_none()).count();
    assert_eq!((fighters, left), (9, 9));
    assert_eq!(w.formation(ids[0]), Some(Formation::Flank));
}

#[test]
fn formations_attack_move_does_not_hide_an_enemy_behind_a_dense_own_army() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = (0..200)
        .map(|n| {
            w.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                center(40 + n % 10, 40 + (n / 10) % 10),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let enemy = w.spawn_unit(PlayerId(1), center(45, 45)).unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Box,
        center(180, 50),
        false,
        true,
    )
    .unwrap();
    for _ in 0..16 {
        w.advance();
    }
    // Enemy discovery is not starved by the own units around it: within 16
    // ticks 8 members found it and a free spot around it and left the group;
    // the other 192 still march. (Before each
    // member broke off for any enemy in a 13x13-tile window: 88 stood around
    // the one enemy, most of them unable to reach it.)
    let fighters = ids
        .iter()
        .filter(|&&id| w.unit(id).unwrap().attack_target == Some(enemy))
        .count();
    let left = ids.iter().filter(|&&id| w.formation(id).is_none()).count();
    assert_eq!((fighters, left), (8, 8));
}

#[test]
fn formations_attack_move_engages_visible_buildings() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 4);
    let building = w
        .insert_building(
            PlayerId(1),
            BuildingKind::House,
            TileRect::from_xywh(46, 40, 2, 2),
            true,
        )
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(80, 40),
        false,
        true,
    )
    .unwrap();
    for _ in 0..16 {
        w.advance();
    }
    // Three of the four see the house (militia sight is 4 tiles) and attack
    // it; the fourth keeps marching in the group.
    let fighters = ids
        .iter()
        .filter(|&&id| w.unit(id).unwrap().attack_target == Some(building))
        .count();
    let left = ids.iter().filter(|&&id| w.formation(id).is_none()).count();
    assert_eq!((fighters, left), (3, 3));
}

/// Whether `id` stands within `tiles` tiles of `to`.
fn near(w: &GameWorld, id: EntityId, to: WorldPosition, tiles: i32) -> bool {
    let p = w.unit(id).unwrap().position;
    let (x, y) = (i64::from(p.x - to.x), i64::from(p.y - to.y));
    x * x + y * y <= i64::from(tiles * 1024).pow(2)
}

fn march_sixty(w: &mut GameWorld) -> (Vec<EntityId>, WorldPosition) {
    w.set_fog_disabled(true);
    let ids = army(w, 60);
    let goal = center(140, 43);
    w.issue_formation_move(PlayerId(0), &ids, Formation::Line, goal, false, false)
        .unwrap();
    for _ in 0..100 {
        w.advance();
    }
    (ids, goal)
}

#[test]
fn formations_lose_only_the_units_that_are_reordered_or_killed() {
    let mut w = wide_world();
    let (ids, goal) = march_sixty(&mut w);
    // A refused order changes nothing.
    w.economy.blocked_tiles.insert(TileCoord::new(60, 10));
    let before = w.canonical_hash();
    assert!(w.issue_move(ids[30], center(60, 10)).is_err());
    assert_eq!(w.canonical_hash(), before);
    // Ten members take a new order and the first member, which carries the
    // group's route, dies.
    let away = center(60, 90);
    for &id in &ids[20..30] {
        w.issue_move(id, away).unwrap();
    }
    w.kill_unit(w.lookup[&ids[0]]);
    let rest = [&ids[1..20], &ids[30..]].concat();
    assert_eq!(rest.len(), 49);
    assert!(
        rest.iter()
            .all(|&id| w.formation(id) == Some(Formation::Line))
    );
    assert!(rest.iter().all(|&id| w.unit(id).unwrap().moving));
    for _ in 0..4000 {
        w.advance();
    }
    // All forty-nine reach the destination together, still in formation.
    assert!(rest.iter().all(|&id| near(&w, id, goal, 8)), "{rest:?}");
    assert!(rest.iter().all(|&id| !w.unit(id).unwrap().moving));
    assert!(rest.iter().all(|&id| w.formation(id).is_some()));
    assert!(ids[20..30].iter().all(|&id| near(&w, id, away, 2)));
    assert_eq!(w.route_accounting_error(), None);
    assert_eq!(w.route_scheduler_stats().planners, 0);
    assert_eq!(w.active_mover_count(), 0);
}

#[test]
fn formations_keep_marching_when_their_route_carrier_is_stopped() {
    let mut w = wide_world();
    let (ids, goal) = march_sixty(&mut w);
    w.issue_stop(PlayerId(0), ids[0]).unwrap();
    assert!(w.formation(ids[0]).is_none());
    assert!(!w.unit(ids[0]).unwrap().moving);
    for _ in 0..4000 {
        w.advance();
    }
    assert!(ids[1..].iter().all(|&id| near(&w, id, goal, 8)));
    assert!(ids[1..].iter().all(|&id| w.formation(id).is_some()));
    assert_eq!(w.route_accounting_error(), None);
    assert_eq!(w.active_mover_count(), 0);
}

#[test]
fn formations_change_shape_immediately_without_replacing_the_route_or_queue() {
    let mut w = wide_world();
    let ids = army(&mut w, 10);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(100, 40),
        false,
        false,
    )
    .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(100, 70),
        true,
        false,
    )
    .unwrap();
    let markers = w.move_destinations(ids[0]);
    let requests = w.route_scheduler_stats().metrics.orders;
    w.issue_formation(PlayerId(0), &ids, Formation::Box)
        .unwrap();
    assert_eq!(w.formation(ids[0]), Some(Formation::Box));
    assert_eq!(w.move_destinations(ids[1]), markers);
    assert_eq!(w.route_scheduler_stats().metrics.orders, requests);
    w.issue_stop(PlayerId(0), ids[0]).unwrap();
    w.issue_formation(PlayerId(0), &ids, Formation::Flank)
        .unwrap();
    for _ in 0..500 {
        w.advance();
    }
    assert_eq!(w.formation(ids[1]), Some(Formation::Flank));
    assert!(ids.iter().all(|&id| !w.unit(id).unwrap().moving));
}

#[test]
fn formations_thousands_of_members_share_one_search_with_bounded_local_work() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = (0..4000)
        .map(|n| {
            w.spawn_kind(
                PlayerId(0),
                UnitKind::Militia,
                center(200 + n % 80, 200 + n / 80),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let high_start = w.unit(*ids.last().unwrap()).unwrap().position;
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(1000, 225),
        false,
        false,
    )
    .unwrap();
    for _ in 0..80 {
        let before = w.route_scheduler_stats().metrics;
        w.advance();
        let after = w.route_scheduler_stats().metrics;
        assert_eq!(
            after.formation_steering_work - before.formation_steering_work,
            0
        );
        assert!(
            after.fair_work + after.lookahead_work - before.fair_work - before.lookahead_work
                <= u64::from(crate::MAX_ROUTE_WORK_PER_TICK)
        );
    }
    assert_eq!(w.route_scheduler_stats().metrics.searches_started, 1);
    assert_ne!(
        w.unit(*ids.last().unwrap()).unwrap().position,
        high_start,
        "high IDs must get steering work too"
    );
    assert_eq!(w.route_accounting_error(), None);
}

#[test]
fn formations_batch_sight_once_per_tick_and_match_sequential_visibility() {
    let mut w = wide_world();
    let mut shadow = wide_world();
    let ids = army(&mut w, 20);
    assert_eq!(ids, army(&mut shadow, 20));
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(90, 50),
        false,
        false,
    )
    .unwrap();
    for tick in 0..800 {
        let before = w.vision_batch_applications();
        w.advance();
        assert_eq!(
            w.vision_batch_applications() - before,
            1,
            "all movement observers must share the tick's batch, tick {tick}"
        );
        for &id in &ids {
            let position = w.unit(id).unwrap().position;
            shadow.units[shadow.lookup[&id]].state.position = position;
            shadow.update_unit_vision(id);
        }
        if tick % 8 == 0 {
            let rect = TileRect::from_xywh(30, 25, 70, 40);
            assert_eq!(
                w.visibility_tiles(PlayerId(0), rect),
                shadow.visibility_tiles(PlayerId(0), rect)
            );
        }
    }
}

#[test]
fn formations_crossing_tiles_do_no_idle_separation_index_work() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 200);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(180, 60),
        false,
        false,
    )
    .unwrap();
    let before = w.idle_spacing.adjacent_visits;
    for _ in 0..800 {
        w.advance();
        assert_eq!(
            w.idle_spacing.adjacent_visits, before,
            "formation movers must not insert/remove themselves in the idle index"
        );
    }
}

#[test]
fn formations_queued_attack_move_starts_combat_on_its_own_leg() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = army(&mut w, 4);
    let enemy = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(51, 40))
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(48, 40),
        false,
        false,
    )
    .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(80, 40),
        true,
        true,
    )
    .unwrap();
    let mut started_queued_leg = false;
    for _ in 0..150 {
        w.advance();
        // The queued leg may acquire contact and issue its attack orders in
        // this same tick, so the observable counter can jump past two.
        if w.route_scheduler_stats().metrics.orders >= 2 {
            started_queued_leg = true;
            break;
        }
        assert_eq!(
            w.formation(ids[0]),
            Some(Formation::Line),
            "the first leg is an ordinary move: orders={} units={:?}",
            w.route_scheduler_stats().metrics.orders,
            ids.iter().map(|&id| w.unit(id)).collect::<Vec<_>>()
        );
        let unit = w.unit(enemy).unwrap();
        assert_eq!(unit.hp, unit.max_hp, "no combat before the queued leg");
    }
    assert!(started_queued_leg, "anchor must finish the first leg");
    for _ in 0..200 {
        w.advance();
    }
    // The queued attack-move leg fought on its own leg and killed the enemy;
    // then every member marched on again toward its slot.
    assert!(w.unit(enemy).is_none_or(|u| u.hp == 0));
    assert!(
        ids.iter().all(|&id| w.formation(id).is_some()),
        "units={:?} attackers={:?} resuming={:?}",
        ids.iter().map(|&id| w.unit(id)).collect::<Vec<_>>(),
        w.combat.attackers,
        w.combat.resuming
    );
}

#[test]
fn formations_queued_combat_mode_changes_the_replay_state() {
    let mut a = wide_world();
    let mut b = wide_world();
    let ids = army(&mut a, 4);
    assert_eq!(ids, army(&mut b, 4));
    for (w, attack_move) in [(&mut a, false), (&mut b, true)] {
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            Formation::Line,
            center(48, 40),
            false,
            false,
        )
        .unwrap();
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            Formation::Line,
            center(80, 40),
            true,
            attack_move,
        )
        .unwrap();
    }
    assert_eq!(a.unit(ids[0]), b.unit(ids[0]));
    assert_ne!(a.canonical_hash(), b.canonical_hash());
}
