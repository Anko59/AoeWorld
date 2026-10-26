use super::*;

#[test]
fn barracks_accepts_only_militia_and_requires_completion() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 500, 500, 0, 0);
    assert_eq!(
        trainable_units(BuildingKind::TownCenter),
        &[UnitKind::Villager]
    );
    assert_eq!(
        trainable_units(BuildingKind::Barracks),
        &[
            UnitKind::Militia,
            UnitKind::Spearman,
            UnitKind::ManAtArms,
            UnitKind::LongSwordsman,
            UnitKind::Pikeman
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
    assert_eq!((outpost.los, outpost.max_hp), (10, 500));
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
            .issue_build_wall(PlayerId(0), &[b], at(110, 100), at(114, 100), false)
            .unwrap();
        advance(&mut world, 900);
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}
