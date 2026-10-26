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
    let mut castle = world.building(shelter).unwrap().clone();
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
