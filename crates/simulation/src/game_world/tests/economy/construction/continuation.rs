use super::super::{buildings::*, *};

fn at(x: i32, y: i32) -> TileCoord {
    TileCoord::new(x, y)
}

fn foundation(world: &mut GameWorld, player: PlayerId, x: i32, y: i32) -> EntityId {
    world
        .insert_building(
            player,
            BuildingKind::House,
            aoe_core::TileRect::new(at(x, y), at(x + 2, y + 2)),
            false,
        )
        .unwrap()
}

#[test]
fn shift_foundations_charge_each_click_and_build_in_order_before_nearer_sites() {
    fn setup() -> (GameWorld, EntityId, [EntityId; 3]) {
        let (mut world, _, _) = village();
        let builder = villager_at(&mut world, 99, 100);
        let builders = [builder, villager_at(&mut world, 99, 101)];
        fixture::survey(&mut world, PlayerId(0), at(110, 100), 20);
        let a = world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::House,
                at(100, 100),
                true,
            )
            .unwrap();
        let b = world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::House,
                at(116, 100),
                true,
            )
            .unwrap();
        let c = world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::House,
                at(108, 100),
                true,
            )
            .unwrap();
        assert_eq!(stock(&world).1, 125);
        for id in builders {
            assert_eq!(world.economy.builders[&id].building, a);
            assert_eq!(world.economy.builders[&id].queue, [b, c]);
        }
        (world, builder, [a, b, c])
    }
    let (mut world, builder, [a, b, c]) = setup();
    let (mut replay, _, _) = setup();
    let nearby = foundation(&mut world, PlayerId(0), 100, 104);
    foundation(&mut replay, PlayerId(0), 100, 104);
    for _ in 0..2500 {
        world.advance();
        replay.advance();
        assert_eq!(world.canonical_hash(), replay.canonical_hash());
        if !world.building(a).unwrap().is_complete() {
            assert_eq!(world.building(b).unwrap().work_done, 0);
        }
        if !world.building(b).unwrap().is_complete() {
            assert_eq!(world.building(c).unwrap().work_done, 0);
            assert_eq!(world.building(nearby).unwrap().work_done, 0);
        }
        if world.building(c).unwrap().is_complete() {
            return;
        }
    }
    panic!(
        "queued foundations did not complete; builder {:?}",
        world.unit(builder)
    );
}

#[test]
fn rejected_shift_cost_keeps_the_active_target_and_queue_unchanged() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    set_stock(&mut world, 0, 50, 0, 0);
    fixture::survey(&mut world, PlayerId(0), at(104, 100), 10);
    let a = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            true,
        )
        .unwrap();
    let b = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(104, 100),
            true,
        )
        .unwrap();
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(108, 100),
            true
        ),
        Err(EconomyError::InsufficientResources)
    );
    assert_eq!(world.canonical_hash(), before);
    assert_eq!(world.economy.builders[&builder].building, a);
    finish(&mut world, b, 1600);
}

#[test]
fn cancelled_shift_targets_are_skipped_and_move_clears_the_queue() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    fixture::survey(&mut world, PlayerId(0), at(106, 100), 10);
    let a = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            true,
        )
        .unwrap();
    let b = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(104, 100),
            true,
        )
        .unwrap();
    let c = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(108, 100),
            true,
        )
        .unwrap();
    world.issue_cancel_construction(PlayerId(0), b).unwrap();
    finish(&mut world, a, 600);
    assert_eq!(world.economy.builders[&builder].building, c);
    world
        .issue_move(
            builder,
            WorldPosition::from_tile_center(at(90, 90)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 1000);
    assert_eq!(world.building(c).unwrap().work_done, 0);
    assert!(!world.economy.builders.contains_key(&builder));
}

#[test]
fn builders_continue_to_nearest_own_unfinished_site_with_id_ties_and_ten_tile_limit() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    let a = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    let enemy = foundation(&mut world, PlayerId(1), 96, 100);
    let first = foundation(&mut world, PlayerId(0), 96, 103);
    let second = foundation(&mut world, PlayerId(0), 101, 103);
    let far = foundation(&mut world, PlayerId(0), 115, 100);
    finish(&mut world, a, 600);
    // Distance is measured to the footprint, from the builder's actual tile.
    assert_eq!(world.economy.builders[&builder].building, first);
    finish(&mut world, first, 1300);
    assert_eq!(world.economy.builders[&builder].building, second);
    finish(&mut world, second, 1300);
    assert!(!world.economy.builders.contains_key(&builder));
    assert_eq!(world.building(enemy).unwrap().work_done, 0);
    assert_eq!(world.building(far).unwrap().work_done, 0);
}

#[test]
fn queue_is_bounded_before_placing_or_charging_and_normal_build_replaces_it() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 99, 100);
    set_stock(&mut world, 0, 5000, 0, 0);
    fixture::survey(&mut world, PlayerId(0), at(115, 115), 40);
    for i in 0..33 {
        world
            .issue_build(
                PlayerId(0),
                &[builder],
                BuildingKind::House,
                at(100 + (i % 8) * 3, 100 + (i / 8) * 3),
                true,
            )
            .unwrap();
    }
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(130, 130),
            true
        ),
        Err(EconomyError::OrderTooLarge)
    );
    assert_eq!(world.canonical_hash(), before);
    let new = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(130, 130),
            false,
        )
        .unwrap();
    assert_eq!(world.economy.builders[&builder].building, new);
    assert!(world.economy.builders[&builder].queue.is_empty());
}

#[test]
fn builders_continue_to_every_castle_age_and_bridge_foundation() {
    for kind in [
        BuildingKind::Castle,
        BuildingKind::Monastery,
        BuildingKind::University,
        BuildingKind::SiegeWorkshop,
        BuildingKind::Bridge,
    ] {
        fn run(kind: BuildingKind) -> [u8; 32] {
            let (mut world, _, _) = village();
            let builder = villager_at(&mut world, 99, 100);
            let house = world
                .issue_build(
                    PlayerId(0),
                    &[builder],
                    BuildingKind::House,
                    at(100, 100),
                    false,
                )
                .unwrap();
            let rule = kind.rules();
            // Bridge rules describe one deck tile; its foundation includes
            // both banks, so use the ordinary variable-length footprint.
            let footprint = if kind == BuildingKind::Bridge {
                GameWorld::bridge_footprint(at(104, 100), at(108, 100)).unwrap()
            } else {
                aoe_core::TileRect::from_xywh(104, 100, rule.side, rule.depth)
            };
            let next = world
                .insert_building(PlayerId(0), kind, footprint, false)
                .unwrap();
            finish(&mut world, house, 600);
            assert_eq!(
                world.economy.builders.get(&builder).map(|t| t.building),
                Some(next),
                "{kind:?}"
            );
            advance(&mut world, 200);
            assert!(world.building(next).unwrap().work_done > 0, "{kind:?}");
            world.canonical_hash()
        }
        assert_eq!(run(kind), run(kind));
    }
}

#[test]
fn auto_continue_includes_ten_tiles_and_excludes_eleven() {
    for (x, continues) in [(109, true), (110, false)] {
        let (mut world, _, _) = village();
        let builder = villager_at(&mut world, 99, 100);
        let house = world
            .issue_build(
                PlayerId(0),
                &[builder],
                BuildingKind::House,
                at(100, 100),
                false,
            )
            .unwrap();
        let next = foundation(&mut world, PlayerId(0), x, 100);
        finish(&mut world, house, 600);
        assert_eq!(
            world.economy.builders.get(&builder).map(|t| t.building),
            continues.then_some(next)
        );
    }
}
