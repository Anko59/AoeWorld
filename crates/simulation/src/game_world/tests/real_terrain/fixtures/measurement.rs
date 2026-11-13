//! Measurement setup only: keep terrain and obstacles, remove fixture actors.
use super::*;
pub fn reset_measurement_actors(world: &mut GameWorld) {
    let ids: Vec<_> = world.units().map(|u| u.id).collect();
    for id in ids {
        world.remove_unit(id);
    }
    world.config.tick_hz = 20;
    world.set_fog_disabled(true);
}
pub fn measurement_world(name: &str) -> GameWorld {
    let mut fixture = build(name);
    reset_measurement_actors(&mut fixture.world);
    let (x, y) = match name {
        "long" => (28, 95),
        "quality-march" | "long-bridges-40" => (28, 108),
        "army-pass" => (63, 150),
        _ => (28, 48),
    };
    // Enough genuine work positions for the forty-worker adoption cohort.
    trees(&mut fixture.world, TileRect::from_xywh(x, y, 2, 4));
    fixture.world
}
pub fn measurement_tower(world: &mut GameWorld, origin: TileCoord) {
    let kind = BuildingKind::WatchTower;
    let rules = kind.rules();
    world
        .insert_building(
            PlayerId(1),
            kind,
            TileRect::from_xywh(origin.x, origin.y, rules.side, rules.depth),
            true,
        )
        .unwrap();
}

/// Use the ordinary certified village kit, without creating setup actors.
pub fn measurement_starting_kit(
    world: &mut GameWorld,
    town: EntityId,
    root: TileCoord,
) -> Result<(), crate::EconomyError> {
    let footprint = world
        .building(town)
        .ok_or(crate::EconomyError::UnknownEntity)?
        .footprint;
    world.initialize_starting_kit(footprint, root)
}
