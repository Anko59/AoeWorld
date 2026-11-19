//! Last sightings survive a saved match without revealing later enemy changes.
use super::*;
use crate::{BuildingKind, TileVisibility};
use aoe_core::{TileRect, WorldPosition};

#[test]
fn saved_fog_retains_only_what_each_player_last_saw() {
    let mut saved = world();
    let viewer = PlayerId(0);
    let enemy = PlayerId(1);
    let third = PlayerId(2);
    let site = TileCoord::new(68, 64);
    let lost_site = TileCoord::new(68, 68);
    let house = saved
        .insert_building(
            enemy,
            BuildingKind::House,
            TileRect::from_xywh(68, 64, 2, 2),
            true,
        )
        .unwrap();
    let destroyed = saved
        .insert_building(
            enemy,
            BuildingKind::House,
            TileRect::from_xywh(68, 68, 2, 2),
            true,
        )
        .unwrap();
    let observer = WorldPosition::from_tile_center(TileCoord::new(67, 66)).unwrap();
    let scout = saved.spawn_unit(viewer, observer).unwrap();
    let other_scout = saved
        .spawn_unit(
            third,
            WorldPosition::from_tile_center(TileCoord::new(12, 12)).unwrap(),
        )
        .unwrap();
    saved.advance();
    let old_hp = saved.remembered_building(viewer, house).unwrap().hp;
    assert!(saved.remembered_building(viewer, destroyed).is_some());
    saved.remove_unit(scout);
    saved.advance();
    saved.building_mut(house).unwrap().hp -= 100;
    saved.remove_building(destroyed);
    saved.advance();
    assert_eq!(saved.visibility(viewer, site), TileVisibility::Explored);
    assert_eq!(
        saved.visibility(viewer, lost_site),
        TileVisibility::Explored
    );

    let (mut restored, _) = roundtrip(&saved, world());
    assert_eq!(restored.visibility(viewer, site), TileVisibility::Explored);
    assert_eq!(
        restored.visibility(viewer, lost_site),
        TileVisibility::Explored
    );
    assert_eq!(
        restored.remembered_building(viewer, house).unwrap().hp,
        old_hp
    );
    assert!(restored.remembered_building(viewer, destroyed).is_some());
    assert!(restored.building(destroyed).is_none());
    assert!(restored.unit(other_scout).is_some());
    assert!(restored.remembered_building(third, house).is_none());
    assert!(restored.remembered_building(third, destroyed).is_none());

    assert!(restored.building(house).unwrap().hp < old_hp);
    restored.spawn_unit(viewer, observer).unwrap();
    restored.advance();
    assert_eq!(
        restored.remembered_building(viewer, house).unwrap().hp,
        restored.building(house).unwrap().hp
    );
    assert!(restored.remembered_building(viewer, destroyed).is_none());
}
