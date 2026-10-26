use super::*;

#[test]
fn a_builder_starts_work_on_a_free_perimeter_spot_before_an_occupied_goal() {
    let (mut world, _, _) = village();
    let builder = villager_at(&mut world, 96, 100);
    let occupant = villager_at(&mut world, 99, 100);
    fixture::survey(&mut world, PlayerId(0), at(100, 100), 4);
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(100, 100),
            false,
        )
        .unwrap();
    for _ in 0..120 {
        world.advance();
        if world.unit(builder).unwrap().task == UnitTask::Building {
            assert!(world.building(house).unwrap().work_done > 0);
            assert!(world.movement_order(builder).is_none());
            assert_eq!(
                world.unit(occupant).unwrap().position.tile_floor(),
                at(99, 100)
            );
            return;
        }
    }
    panic!(
        "builder did not reach the available perimeter: {:?}",
        world.unit(builder)
    );
}

#[test]
fn a_starting_villager_promptly_resumes_a_house_behind_the_town_center() {
    for pause in [0, 8, 17, 26, 35, 44, 53, 62, 71] {
        resumes_house_after(pause);
    }
}

fn resumes_house_after(pause: u32) {
    let (mut world, _, center) = village();
    let footprint = world.building(center).unwrap().footprint;
    let builder = world
        .units()
        .find(|u| u.kind == UnitKind::Villager)
        .unwrap()
        .id;
    let house = world
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::House,
            at(footprint.min.x + 1, footprint.max.y),
            false,
        )
        .unwrap();
    advance(&mut world, pause);
    world.issue_stop(PlayerId(0), builder).unwrap();
    world
        .issue_construct(PlayerId(0), &[builder], house)
        .unwrap();
    // Allow the annex/sheep detour, with a second left for the browser HUD.
    for _ in 0..180 {
        world.advance();
        if world.unit(builder).unwrap().task == UnitTask::Building {
            return;
        }
    }
    panic!(
        "builder still approaching after nine seconds (paused at tick {pause}): {:?}",
        world.unit(builder)
    );
}
