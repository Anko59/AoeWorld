//! Setup only: all measured actors use ordinary movement and economy rules.
use super::position;
use crate::{BuildingKind, EconomyError, GameWorld, UnitKind};
use aoe_core::{EntityId, Formation, PlayerId, TileCoord, TileRect};
use std::error::Error;

fn place(
    world: &mut GameWorld,
    player: PlayerId,
    center: TileCoord,
) -> Result<EntityId, EconomyError> {
    let kind = BuildingKind::WatchTower;
    let rules = kind.rules();
    for dy in -24..=24 {
        for dx in -24..=24 {
            let rect = TileRect::from_xywh(center.x + dx, center.y + dy, rules.side, rules.depth);
            if world.validate_footprint(rect).is_ok()
                && super::super::placement::perimeter(rect)
                    .into_iter()
                    .any(|tile| world.tile_passable(tile))
            {
                return world.insert_building(player, kind, rect, true);
            }
        }
    }
    Err(EconomyError::InvalidFootprint)
}

pub(super) fn seed(
    world: &mut GameWorld,
    player: PlayerId,
    scout: EntityId,
    center: TileCoord,
    spots: &[TileCoord],
    barracks: EntityId,
) -> Result<(), Box<dyn Error>> {
    world.issue_auto_scout(player, scout, true)?;
    let tower = place(world, player, center)?;
    let footprint = world
        .building(tower)
        .ok_or(EconomyError::UnknownEntity)?
        .footprint;
    let shelter = super::super::placement::perimeter(footprint)
        .into_iter()
        .find(|&tile| world.tile_passable(tile))
        .ok_or(EconomyError::InvalidFootprint)?;
    let archer = world.spawn_kind(player, UnitKind::Archer, position(shelter)?)?;
    world.units[world.lookup[&archer]].state.hp = 1;
    world.issue_garrison(player, archer, tower)?;
    let hospital = spots
        .iter()
        .copied()
        .find(|&tile| world.tile_passable(tile))
        .ok_or(EconomyError::InvalidFootprint)?;
    world.spawn_kind(player, UnitKind::Monk, position(hospital)?)?;
    let knight = world.spawn_kind(player, UnitKind::Knight, position(hospital)?)?;
    world.units[world.lookup[&knight]].state.hp = 1;
    // Two ships per seat march in the generated southern sea. No land unit
    // can take their routes; naval collision, sight and wire state stay live.
    for (i, kind) in [UnitKind::Galley, UnitKind::TransportShip]
        .into_iter()
        .enumerate()
    {
        let tile = TileCoord::new(
            512 + i32::from(player.0 % 10) * 1536 + i as i32 * 8,
            15_900 + i32::from(player.0 / 10) * 16,
        );
        let id = world.spawn_kind(player, kind, position(tile)?)?;
        world.issue_move(id, position(TileCoord::new(tile.x + 192, tile.y))?)?;
    }
    let mut count = 6; // scout, archer, monk, knight, two ships
    if player.0.is_multiple_of(4) {
        let footprint = world
            .building(barracks)
            .ok_or(EconomyError::UnknownEntity)?
            .footprint;
        for tile in super::super::placement::perimeter(footprint) {
            if world.tile_passable(tile) {
                world.spawn_militia(player, position(tile)?)?;
                count += 1;
            }
        }
    }
    let mut army = Vec::new();
    for tile in spots.iter().copied().cycle().take(60 - count) {
        if world.tile_passable(tile) {
            army.push(world.spawn_militia(player, position(tile)?)?);
        }
    }
    // Keep exactly 140 population even if the tower claimed a former spawn.
    while army.len() + count < 60 {
        army.push(world.spawn_militia(player, position(hospital)?)?);
    }
    for (index, &id) in army.iter().enumerate().skip(12) {
        march(world, id, index)?;
    }
    let origin = world
        .unit(army[0])
        .ok_or(EconomyError::UnknownEntity)?
        .position
        .tile_floor();
    let mut formed = false;
    for (dx, dy) in [(192, 0), (0, 192), (-192, 0), (0, -192)] {
        if world
            .issue_formation_move(
                player,
                &army[..12],
                Formation::Line,
                position(TileCoord::new(origin.x + dx, origin.y + dy))?,
                false,
                false,
            )
            .is_ok()
        {
            formed = true;
            break;
        }
    }
    if !formed {
        return Err("late-game formation has no legal destination".into());
    }
    Ok(())
}

fn march(world: &mut GameWorld, id: EntityId, index: usize) -> Result<(), Box<dyn Error>> {
    let origin = world
        .unit(id)
        .ok_or(EconomyError::UnknownEntity)?
        .position
        .tile_floor();
    let directions = [
        (1, 0),
        (0, 1),
        (-1, 0),
        (0, -1),
        (1, 1),
        (-1, 1),
        (-1, -1),
        (1, -1),
    ];
    for distance in [192, 160, 224, 128] {
        for turn in 0..directions.len() {
            let (dx, dy) = directions[(index + turn) % directions.len()];
            let tile = TileCoord::new(
                origin.x + dx * distance,
                origin.y + dy * distance + index as i32 % 3,
            );
            if world.issue_move(id, position(tile)?).is_ok() {
                return Ok(());
            }
        }
    }
    Err(format!("late-game army has no legal march: {id:?}").into())
}
