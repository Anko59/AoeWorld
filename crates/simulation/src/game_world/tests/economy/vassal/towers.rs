//! Upgraded towers keep serving the Vassal's tower sites.
use super::*;

#[test]
fn an_upgraded_guard_tower_still_fills_a_vassal_tower_site() {
    let (mut w, v, _) = setup();
    w.insert_building(
        PlayerId(0),
        BuildingKind::GuardTower,
        TileRect::from_xywh(96, 85, 2, 2),
        true,
    )
    .unwrap();
    for i in 0..8 {
        w.spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(89 + i, 98)).unwrap(),
        )
        .unwrap();
    }
    advance(&mut w, 20_000);
    assert!(w.vassal_status(v).unwrap().developed);
    let towers = [
        BuildingKind::WatchTower,
        BuildingKind::GuardTower,
        BuildingKind::Keep,
    ]
    .into_iter()
    .map(|kind| w.player_buildings_of(PlayerId(0), kind).count())
    .sum::<usize>();
    assert_eq!(
        towers, 3,
        "the Guard Tower serves the castle, as a Watch Tower would"
    );
}
