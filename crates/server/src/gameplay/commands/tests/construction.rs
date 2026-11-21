use super::*;

#[test]
fn a_full_world_reports_a_world_limit_for_explicit_and_shared_land_builds() {
    // The owner's report: build orders after Feudal Age answered "The order
    // queue is full" because the world's building capacity was reached.
    let (mut world, _) = aoe_simulation::GameWorld::default_with_village(Seed(1));
    let player = aoe_core::PlayerId(0);
    let villager = world
        .units()
        .find(|unit| unit.kind == aoe_simulation::UnitKind::Villager)
        .unwrap()
        .id;
    let origin = world
        .buildings()
        .next()
        .map(|building| {
            aoe_core::TileCoord::new(building.footprint.max.x + 3, building.footprint.min.y)
        })
        .unwrap();
    world.set_limits(aoe_simulation::WorldLimits {
        buildings: world.buildings().count(),
        ..world.limits()
    });
    let build = |builder_ids, kind| PendingAction::Build {
        builder_ids,
        kind,
        origin,
        queued: false,
    };
    assert_eq!(
        apply(
            &mut world,
            player,
            build(vec![villager], BuildingKind::House)
        ),
        CommandResult::RejectedWorldLimit
    );
    assert_eq!(
        apply(&mut world, player, build(Vec::new(), BuildingKind::House)),
        CommandResult::RejectedWorldLimit
    );
    assert_eq!(
        apply(
            &mut world,
            player,
            build(Vec::new(), BuildingKind::FishTrap)
        ),
        CommandResult::RejectedOrderTooLarge
    );
}
