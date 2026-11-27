use super::*;

#[test]
fn barracks_accepts_only_militia_and_requires_completion() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 500, 500, 0, 0);
    assert_eq!(
        trainable_units(BuildingKind::TownCenter),
        &[UnitKind::Villager, UnitKind::Steward]
    );
    assert_eq!(
        trainable_units(BuildingKind::Barracks),
        &[
            UnitKind::Militia,
            UnitKind::Spearman,
            UnitKind::ManAtArms,
            UnitKind::LongSwordsman,
            UnitKind::Pikeman,
            UnitKind::TwoHandedSwordsman,
            UnitKind::Champion,
            UnitKind::Halberdier
        ]
    );
    let worker = villager_at(&mut world, 99, 100);
    let barracks = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Barracks,
            at(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(
        world.issue_train(PlayerId(0), barracks, UnitKind::Villager),
        Err(EconomyError::UnderConstruction)
    );
    finish(&mut world, barracks, 1200);
    assert!(world.building(barracks).unwrap().training_queue.is_empty());
    assert_eq!(
        world.issue_train(PlayerId(0), barracks, UnitKind::Villager),
        Err(EconomyError::WrongUnitKind)
    );
}

#[test]
fn an_outpost_has_a_large_line_of_sight_and_costs_wood_and_stone() {
    let (mut world, _, _) = village();
    let worker = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Outpost,
            at(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(stock(&world), (200, 175, 100, 195));
    let outpost = world.building(id).unwrap();
    assert_eq!((outpost.los, outpost.max_hp), (20, 500));
    assert_eq!(
        world
            .building(world.buildings().next().unwrap().id)
            .unwrap()
            .los,
        8
    );
    // 15 s = 300 ticks for one builder.
    assert_eq!(finish(&mut world, id, 400), 300);
    assert_eq!(world.building(id).unwrap().hp, 500);
    assert_eq!(
        world.visibility(PlayerId(0), at(120, 100)),
        crate::TileVisibility::Visible
    );
    assert_ne!(
        world.visibility(PlayerId(0), at(121, 100)),
        crate::TileVisibility::Visible
    );
}

#[test]
fn identical_build_orders_replay_to_the_same_hash() {
    fn run() -> [u8; 32] {
        let (mut world, _, _) = village();
        let a = villager_at(&mut world, 99, 100);
        let b = villager_at(&mut world, 99, 101);
        world
            .issue_build(
                PlayerId(0),
                &[a, b],
                BuildingKind::House,
                at(100, 100),
                false,
            )
            .unwrap();
        fixture::survey(&mut world, PlayerId(0), at(112, 100), 2);
        world
            .issue_build_wall(
                PlayerId(0),
                &[b],
                BuildingKind::PalisadeWall,
                at(110, 100),
                at(114, 100),
                false,
            )
            .unwrap();
        advance(&mut world, 900);
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}

#[test]
fn outpost_sight_in_castle_and_imperial_with_town_research() {
    use crate::Tech;

    let player = PlayerId(0);
    for late_age in [Tech::CastleAge, Tech::ImperialAge] {
        for (watch, patrol, radius) in [(false, false, 20), (true, false, 24), (true, true, 28)] {
            let (mut world, _, _) = village();
            world.complete_research(player, Tech::FeudalAge, &mut Vec::new());
            if watch {
                world.complete_research(player, Tech::TownWatch, &mut Vec::new());
            }
            world.complete_research(player, Tech::CastleAge, &mut Vec::new());
            if patrol {
                world.complete_research(player, Tech::TownPatrol, &mut Vec::new());
            }
            if late_age == Tech::ImperialAge {
                world.complete_research(player, late_age, &mut Vec::new());
            }
            let worker = villager_at(&mut world, 99, 100);
            let id = world
                .issue_build(
                    player,
                    &[worker],
                    BuildingKind::Outpost,
                    at(100, 100),
                    false,
                )
                .unwrap();
            assert_eq!(
                world.building(id).unwrap().los,
                radius,
                "{late_age:?}, Town Watch={watch}, Town Patrol={patrol}"
            );
            finish(&mut world, id, 400);
            let edge = at(100 + i32::from(radius), 100);
            assert_eq!(
                world.visibility(player, edge),
                crate::TileVisibility::Visible
            );
            assert_ne!(
                world.visibility(player, at(edge.x + 1, edge.y)),
                crate::TileVisibility::Visible
            );

            let mut snapshot = world.snapshot([0; 32]).unwrap();
            // Restore derives outpost sight from the owner's research.
            snapshot
                .buildings
                .iter_mut()
                .find(|building| building.id == id)
                .unwrap()
                .los = 12;
            let mut restored = crate::game_world::tests::world();
            restored
                .restore_snapshot(&snapshot, [0; 32], &|| false)
                .unwrap();
            assert_eq!(restored.building(id).unwrap().los, radius);
            assert_eq!(
                restored.visibility(player, edge),
                crate::TileVisibility::Visible
            );
        }
    }
}
