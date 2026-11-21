use super::*;
use aoe_core::{Seed, TileCoord, TileRect, WorldConfig};

#[test]
fn army_centroid_castle_knight_uses_shelter_center_instead_of_storage_sentinel() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let shelter = world
        .place_town_center(PlayerId(0), TileCoord::new(100, 100))
        .unwrap();
    let soldier = world
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(99, 102)).unwrap(),
        )
        .unwrap();
    world.issue_garrison(PlayerId(0), soldier, shelter).unwrap();
    for _ in 0..100 {
        world.advance();
        if world.unit(soldier).is_none() {
            break;
        }
    }
    // Reuse the actual off-map storage record for a Castle's sheltered Knight.
    // The server has no scenario mutation API; the summary consumes these public records.
    let mut castle = world.building(shelter).unwrap().into_owned();
    assert_eq!(castle.garrison.len(), 1);
    castle.kind = BuildingKind::Castle;
    castle.footprint = TileRect::from_xywh(100, 100, 4, 4);
    castle.garrison[0].unit.kind = UnitKind::Knight;
    assert_eq!(castle.garrison[0].unit.position, WorldPosition::new(0, 0));
    let (unit, at) = occupants(&castle).next().unwrap();
    assert_eq!(unit.kind, UnitKind::Knight);
    assert_eq!(
        at,
        WorldPosition::new(102 * FIXED_SUBUNITS_PER_TILE, 102 * FIXED_SUBUNITS_PER_TILE)
    );
}

#[test]
fn development_counts_distinguish_idle_workers_and_marching_armies() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let owner = PlayerId(0);
    world
        .place_town_center(owner, TileCoord::new(100, 100))
        .unwrap();
    for (kind, tile) in [
        (UnitKind::Villager, TileCoord::new(110, 100)),
        (UnitKind::Steward, TileCoord::new(111, 100)),
        (UnitKind::Militia, TileCoord::new(112, 100)),
    ] {
        let id = world
            .spawn_kind(owner, kind, WorldPosition::from_tile_center(tile).unwrap())
            .unwrap();
        if kind != UnitKind::Villager {
            world
                .issue_move(
                    id,
                    WorldPosition::from_tile_center(TileCoord::new(120, 100)).unwrap(),
                )
                .unwrap();
        }
    }
    world.advance();
    let state = match_state(&world, &[]);
    let player = &state["players"][0];
    assert_eq!(player["completed_town_centers"], 1);
    assert_eq!(player["completed_castles"], 0);
    assert_eq!(player["units"]["Steward"], 1);
    assert_eq!(player["idle_villagers"], 1);
    assert_eq!(player["army"]["marching"], 1);
}

#[test]
fn a_unit_carrying_resources_contributes_to_gathering_activity() {
    let mut world = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
    let worker = world
        .spawn_kind(
            PlayerId(0),
            UnitKind::Villager,
            WorldPosition::from_tile_center(TileCoord::new(110, 100)).unwrap(),
        )
        .unwrap();
    let mut unit = world.unit(worker).unwrap();
    let gathering = {
        unit.task = UnitTask::Gathering;
        unit_activity(&unit)
    };
    unit.task = UnitTask::Carrying;
    assert_eq!(unit_activity(&unit), gathering);
}
