//! PR 6: speed matching, destination admission and parallel determinism.
use super::*;
use aoe_core::Formation;

#[test]
fn block_front_members_walk_on_the_first_tick_without_assembly() {
    let mut w = wide_world();
    let ids = army(&mut w, 60);
    let before = ids
        .iter()
        .map(|&id| w.unit(id).unwrap().position)
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(180, 43),
        false,
        false,
    )
    .unwrap();
    w.advance();
    for (&id, from) in ids.iter().zip(before) {
        assert!(
            w.unit(id).unwrap().position.x > from.x,
            "front member {id:?} waited to assemble"
        );
    }
}

#[test]
fn block_catch_up_is_exactly_one_and_a_half_pace_capped_by_own_speed() {
    use crate::game_world::formation::matched_speed;
    let fraction = 1_u64 << 32;
    assert_eq!(matched_speed((40, 1), (100, 1)), 60 * fraction);
    assert_eq!(matched_speed((40, 1), (45, 1)), 45 * fraction);
    assert_eq!(matched_speed((3, 8), (1, 1)), 9 * fraction / 16);
    assert_eq!(matched_speed((3, 4), (1, 1)), fraction);
}

#[test]
fn leaving_the_slowest_member_changes_pace_without_a_movement_credit_burst() {
    let mut w = wide_world();
    let fast = w
        .spawn_kind(PlayerId(0), UnitKind::Scout, center(40, 40))
        .unwrap();
    let slow = w
        .spawn_kind(PlayerId(0), UnitKind::Archer, center(40, 42))
        .unwrap();
    w.issue_formation_move(
        PlayerId(0),
        &[fast, slow],
        Formation::Line,
        center(180, 41),
        false,
        false,
    )
    .unwrap();
    for _ in 0..101 {
        w.advance();
    }
    w.issue_stop(PlayerId(0), slow).unwrap();
    let from = w.formation_anchor(fast).unwrap();
    let (numerator, denominator) = w.movement_speed(UnitKind::Scout, PlayerId(0));
    for _ in 0..denominator {
        w.advance();
    }
    let to = w.formation_anchor(fast).unwrap();
    assert_eq!(to.x - from.x, numerator as i32);
    assert_eq!(to.y, from.y);
    assert_eq!(w.formation(fast), Some(Formation::Line));
    assert_eq!(w.formation(slow), None);
}

#[test]
fn block_destination_slots_use_body_spacing_and_class_order() {
    let mut w = wide_world();
    let kinds = [
        UnitKind::Monk,
        UnitKind::Archer,
        UnitKind::Militia,
        UnitKind::Mangonel,
        UnitKind::Skirmisher,
        UnitKind::Scout,
    ];
    let ids = kinds
        .into_iter()
        .enumerate()
        .map(|(n, kind)| {
            w.spawn_kind(PlayerId(0), kind, center(40, 40 + n as i32 * 2))
                .unwrap()
        })
        .collect::<Vec<_>>();
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(100, 45),
        false,
        false,
    )
    .unwrap();
    let slot = |at: usize| w.formation_destination_slot(ids[at]).unwrap();
    assert!(slot(2).x >= slot(1).x && slot(5).x >= slot(4).x);
    assert!(slot(1).x > slot(0).x && slot(4).x > slot(3).x);
    for (n, &id) in ids.iter().enumerate() {
        let kind = w.unit(id).unwrap().kind;
        assert!(w.body_position_clear(kind, slot(n)));
        for (other, &other_id) in ids.iter().enumerate().skip(n + 1) {
            let radius =
                kind.collision_radius() + w.unit(other_id).unwrap().kind.collision_radius();
            let (a, b) = (slot(n), slot(other));
            assert!(
                i64::from(a.x - b.x).pow(2) + i64::from(a.y - b.y).pow(2)
                    >= i64::from(radius).pow(2)
            );
        }
    }
}

#[test]
#[cfg(not(target_arch = "wasm32"))]
fn block_parallel_proposals_replay_identically_on_one_and_four_threads() {
    let mut a = wide_world();
    let mut b = wide_world();
    let spawn = |w: &mut GameWorld| {
        (0..5)
            .map(|group| {
                (0..60)
                    .map(|n| {
                        w.spawn_kind(
                            PlayerId(0),
                            UnitKind::Militia,
                            center(40 + n % 10, 40 + group * 20 + n / 10),
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    let groups = spawn(&mut a);
    assert_eq!(groups, spawn(&mut b));
    for w in [&mut a, &mut b] {
        w.set_fog_disabled(true);
        for (n, ids) in groups.iter().enumerate() {
            w.issue_formation_move(
                PlayerId(0),
                ids,
                Formation::Line,
                center(140, 43 + n as i32 * 20),
                false,
                false,
            )
            .unwrap();
        }
    }
    let one = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build()
        .unwrap();
    let four = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    for _ in 0..2800 {
        one.install(|| a.advance());
        four.install(|| b.advance());
        assert_eq!(a.canonical_hash(), b.canonical_hash());
    }
    for id in groups.into_iter().flatten() {
        assert_eq!(
            a.unit(id).unwrap().position,
            a.formation_destination_slot(id).unwrap()
        );
        assert!(!a.unit(id).unwrap().moving);
    }
}

#[test]
fn blocked_member_routes_out_of_a_west_facing_pocket_and_rejoins() {
    for carrier in [false, true] {
        let mut w = wide_world();
        w.set_fog_disabled(true);
        let first = carrier.then(|| {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, center(40, 60))
                .unwrap()
        });
        let mut ids = army(&mut w, 20);
        let lagger = first.unwrap_or_else(|| {
            w.spawn_kind(PlayerId(0), UnitKind::Militia, center(40, 60))
                .unwrap()
        });
        ids.push(lagger);
        // The only exit is nine tiles west, away from the group's eastbound
        // route. Local forward steering cannot get this member onto that route.
        for area in [
            TileRect::from_xywh(32, 59, 10, 1),
            TileRect::from_xywh(32, 61, 10, 1),
            TileRect::from_xywh(41, 60, 1, 1),
        ] {
            super::super::group_baseline::forest(&mut w, area);
        }
        w.issue_formation_move(
            PlayerId(0),
            &ids,
            Formation::Line,
            center(100, 43),
            false,
            false,
        )
        .unwrap();
        let (mut detached, mut rejoined) = (false, false);
        for _ in 0..3500 {
            w.advance();
            assert!(w.formation(lagger).is_some(), "a detour keeps membership");
            if w.is_detached_member(w.lookup[&lagger]) {
                detached = true;
                if carrier {
                    assert_ne!(w.units[w.lookup[&lagger]].formation, Some(lagger));
                }
            } else if detached {
                rejoined = true;
            }
        }
        assert!(
            detached && rejoined,
            "carrier={carrier} detour={detached} rejoin={rejoined}"
        );
        for id in ids {
            assert_eq!(
                w.unit(id).unwrap().position,
                w.formation_destination_slot(id).unwrap(),
                "carrier={carrier} id={id:?}"
            );
            assert!(!w.unit(id).unwrap().moving);
        }
        assert!(w.route_scheduler_stats().metrics.orders >= 2);
        assert_eq!(w.route_accounting_error(), None);
        assert_eq!(w.active_mover_count(), 0);
    }
}

#[test]
fn bounded_group_sweeps_match_full_sweeps_with_large_bodies_and_overfull_cells() {
    for overfull in [false, true] {
        let mut w = wide_world();
        let mover = w
            .spawn_kind(PlayerId(0), UnitKind::Militia, center(40, 40))
            .unwrap();
        for (n, kind) in [UnitKind::Militia, UnitKind::Scout, UnitKind::Mangonel]
            .into_iter()
            .enumerate()
        {
            w.spawn_kind(PlayerId(1), kind, center(41 + n as i32, 40))
                .unwrap();
        }
        if overfull {
            for _ in 0..40 {
                w.spawn_kind(PlayerId(1), UnitKind::Militia, center(40, 41))
                    .unwrap();
            }
        }
        let index = w.lookup[&mover];
        for x in -8..=8 {
            for y in -8..=8 {
                let from = WorldPosition::new(center(40, 40).x + x * 96, center(40, 40).y + y * 96);
                for (dx, dy) in [(47, 0), (0, -47), (33, 33)] {
                    let to = WorldPosition::new(from.x + dx, from.y + dy);
                    assert_eq!(
                        w.collision_sweep_bounded(index, from, to, false, false, Some((512, 0))),
                        w.collision_sweep(index, from, to, false, false)
                    );
                }
            }
        }
    }
}

#[test]
fn group_sweep_certificate_survives_every_direction_of_a_bounded_neighbor_step() {
    let mut w = wide_world();
    let mover = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, center(40, 40))
        .unwrap();
    let other = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, center(41, 40))
        .unwrap();
    let (index, neighbor) = (w.lookup[&mover], w.lookup[&other]);
    let from = w.unit(mover).unwrap().position;
    let to = WorldPosition::new(from.x + 47, from.y);
    let (mut certified, mut uncertified) = (0, 0);
    let relocate = |w: &mut GameWorld, at| {
        let old = w.units[neighbor].state.position.tile_floor();
        w.units[neighbor].state.position = at;
        let unit = w.units[neighbor].state;
        w.track_unit_step(other, old, &unit);
    };
    for x in -8..=8 {
        for y in -8..=8 {
            let base = WorldPosition::new(from.x + x * 96, from.y + y * 96);
            relocate(&mut w, base);
            if !w.collision_sweep_bounded(index, from, to, false, false, Some((205, 64))) {
                uncertified += 1;
                continue;
            }
            certified += 1;
            for (dx, dy) in [
                (64, 0),
                (-64, 0),
                (0, 64),
                (0, -64),
                (45, 45),
                (-45, 45),
                (45, -45),
                (-45, -45),
            ] {
                relocate(&mut w, WorldPosition::new(base.x + dx, base.y + dy));
                assert!(w.collision_sweep(index, from, to, false, false));
            }
        }
    }
    assert!(certified > 0 && uncertified > 0);
}

#[test]
fn clearance_region_certificates_match_static_sweeps_near_a_forest() {
    use crate::game_world::movement_step::certificates::StaticArea;
    let mut w = wide_world();
    super::super::group_baseline::forest(&mut w, TileRect::from_xywh(45, 32, 1, 20));
    let origin = center(40, 40);
    let clearance = w.static_clearance(UnitKind::Militia, origin.tile_floor());
    let area = StaticArea::new(origin.tile_floor(), clearance).unwrap();
    let mut count = 0;
    for x in -10..=10 {
        for y in -10..=10 {
            let from = WorldPosition::new(origin.x + x * 512, origin.y + y * 512);
            if area.covers(from, UnitKind::Militia.collision_radius(), 513) {
                count += 1;
                for (dx, dy) in [
                    (512, 0),
                    (-512, 0),
                    (0, 512),
                    (0, -512),
                    (362, 362),
                    (-362, -362),
                ] {
                    assert!(w.body_step_clear(
                        UnitKind::Militia,
                        from,
                        WorldPosition::new(from.x + dx, from.y + dy)
                    ));
                }
            }
        }
    }
    assert!(count > 0 && count < 21 * 21);
    assert!(StaticArea::new(origin.tile_floor(), 0).is_none());
}
