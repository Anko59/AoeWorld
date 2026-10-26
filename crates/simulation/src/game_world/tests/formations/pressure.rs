use super::*;
use aoe_core::Formation;

#[test]
fn formations_wide_staggered_wings_can_start_assembling_with_bounded_rays() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let origin = center(200, 200);
    let ids = (0..4096)
        .map(|_| {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, origin)
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
    assert!(w.formation_slot(ids[0]).unwrap().y < origin.y - 90 * 1024);
    for _ in 0..200 {
        w.advance();
    }
    assert!(
        w.unit(ids[0]).unwrap().position.y < origin.y - 1024,
        "an open slot beyond the ray allowance must still make local progress"
    );
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
    // The last ranks begin far outside the centroid's engagement window.
    let wing = w.unit(*ids.last().unwrap()).unwrap().position.tile_floor();
    w.spawn_unit(PlayerId(1), center(wing.x + 2, wing.y))
        .unwrap();
    for _ in 0..120 {
        w.advance();
        if w.formation(ids[0]).is_none() {
            return;
        }
    }
    panic!("contact at a distant wing must release the army into combat");
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
    w.spawn_unit(PlayerId(1), center(45, 45)).unwrap();
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
    assert!(
        ids.iter().all(|&id| w.formation(id).is_none()),
        "own units must not starve enemy discovery"
    );
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
            TileRect::from_xywh(48, 40, 2, 2),
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
    assert!(ids.iter().all(|&id| w.formation(id).is_none()));
    assert!(
        ids.iter()
            .any(|&id| w.unit(id).unwrap().attack_target == Some(building))
    );
}

#[test]
fn formations_stop_and_casualty_release_routes_and_preserve_refused_orders() {
    let mut w = wide_world();
    let ids = army(&mut w, 50);
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(1000, 40),
        false,
        false,
    )
    .unwrap();
    w.economy.blocked_tiles.insert(TileCoord::new(60, 60));
    let before = w.canonical_hash();
    assert!(w.issue_move(ids[10], center(60, 60)).is_err());
    assert_eq!(w.canonical_hash(), before);
    w.kill_unit(w.lookup[&ids[0]]);
    assert!(ids.iter().all(|&id| w.formation(id).is_none()));
    assert_eq!(w.route_accounting_error(), None);
    assert_eq!(w.route_scheduler_stats().planners, 0);
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
        assert!(
            after.formation_steering_work - before.formation_steering_work
                <= u64::from(crate::game_world::formation::STEERING_WORK_PER_TICK)
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
    for _ in 0..150 {
        w.advance();
    }
    assert_eq!(
        w.formation(ids[0]),
        Some(Formation::Line),
        "the first leg is an ordinary move"
    );
    for _ in 0..200 {
        w.advance();
    }
    assert!(ids.iter().all(|&id| w.formation(id).is_none()));
    assert!(w.unit(enemy).is_none_or(|u| u.hp < u.max_hp));
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
