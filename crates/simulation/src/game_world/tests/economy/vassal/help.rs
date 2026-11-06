//! A Vassal under fire from a building calls its soldiers and Generals.
use super::*;

#[test]
fn vassal_calls_for_help_when_an_enemy_tower_attacks_its_territory() {
    let (mut w, v, _) = setup();
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let soldier = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(106, 92))
        .unwrap();
    w.issue_stance(PlayerId(0), &[soldier], aoe_core::CombatStance::StandGround)
        .unwrap();
    let general = w
        .spawn_kind(PlayerId(0), UnitKind::General, at(190, 92))
        .unwrap();
    let tower = w
        .insert_building(
            PlayerId(1),
            BuildingKind::WatchTower,
            TileRect::from_xywh(110, 92, 1, 1),
            true,
        )
        .unwrap();
    w.issue_attack(PlayerId(1), tower, soldier).unwrap();
    advance(&mut w, 40);
    assert!(w.unit(soldier).unwrap().last_hit.is_some());
    assert!(w.vassal_status(v).unwrap().alert);
    assert_eq!(w.vassal_status(v).unwrap().responding, 1);
    assert!(w.economy.generals.armies[&general].destination.is_some());
}
