use super::*;

#[test]
fn farm_staffing_preserves_the_lower_id_builders_unique_queue() {
    fn run() -> [u8; 32] {
        let (mut world, first, _, farm) = mill_and_farm();
        let helper = villager_at(&mut world, 106, 104);
        assert!(first < helper);
        world
            .issue_construct(PlayerId(0), &[first, helper], farm)
            .unwrap();
        fixture::survey(&mut world, PlayerId(0), at(110, 100), 3);
        let house = world
            .issue_build(
                PlayerId(0),
                &[first],
                BuildingKind::House,
                at(110, 100),
                true,
            )
            .unwrap();
        finish(&mut world, farm, 700);
        assert_eq!(
            world.economy.builders.get(&first).map(|job| job.building),
            Some(house),
            "the unique queued order must survive farm staffing"
        );
        finish(&mut world, house, 1400);
        advance(&mut world, 100);
        assert_eq!(world.gathering_kind(helper), Some(ResourceKind::Food));
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}

#[test]
fn queued_group_farms_leave_one_farmer_on_each_completed_farm() {
    fn run() -> [u8; 32] {
        let (mut world, first, _, farm) = mill_and_farm();
        let workers = [
            first,
            villager_at(&mut world, 106, 104),
            villager_at(&mut world, 107, 104),
        ];
        world.issue_construct(PlayerId(0), &workers, farm).unwrap();
        let mut farms = vec![farm];
        for x in [106, 109] {
            fixture::survey(&mut world, PlayerId(0), at(x + 1, 101), 3);
            farms.push(
                world
                    .issue_build(PlayerId(0), &workers, BuildingKind::Farm, at(x, 100), true)
                    .unwrap(),
            );
        }
        advance(&mut world, 2500);
        for farm in farms {
            let footprint = world.building(farm).unwrap().footprint;
            assert_eq!(
                workers
                    .iter()
                    .filter(
                        |id| world.unit(**id).is_some_and(|u| u.task == UnitTask::Farming
                            && footprint.contains(u.position.tile_floor()))
                    )
                    .count(),
                1,
                "farm {farm:?} must have one farmer"
            );
        }
        assert_eq!(
            workers
                .iter()
                .filter(|id| world.gathering_kind(**id) == Some(ResourceKind::Food))
                .count(),
            workers.len()
        );
        world.canonical_hash()
    }
    assert_eq!(run(), run(), "farm assignment replays deterministically");
}

#[test]
fn later_queued_group_placements_keep_the_farmer_already_left_behind() {
    let (mut world, first, _, farm) = mill_and_farm();
    let workers = [
        first,
        villager_at(&mut world, 106, 104),
        villager_at(&mut world, 107, 104),
    ];
    world.issue_construct(PlayerId(0), &workers, farm).unwrap();
    finish(&mut world, farm, 700);
    advance(&mut world, 100);
    let farmer = workers
        .iter()
        .copied()
        .find(|id| world.unit(*id).is_some_and(|u| u.task == UnitTask::Farming))
        .unwrap();
    fixture::survey(&mut world, PlayerId(0), at(107, 101), 3);
    world
        .issue_build(
            PlayerId(0),
            &workers,
            BuildingKind::Farm,
            at(106, 100),
            true,
        )
        .unwrap();
    assert_eq!(
        world.gathering_kind(farmer),
        Some(ResourceKind::Food),
        "queued placement must use the remaining helpers"
    );
}
