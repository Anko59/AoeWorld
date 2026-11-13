use super::*;

#[test]
fn volley_scene_has_full_infantry_shelters_and_live_dense_targets_at_real_time() {
    let (mut w, _) = GameWorld::volley_demo(aoe_core::Seed(7)).unwrap();
    assert_eq!(w.config().tick_hz, 20);
    let shelters: Vec<_> = w
        .buildings()
        .filter(|b| matches!(b.kind, BuildingKind::Castle | BuildingKind::TownCenter))
        .map(|b| (b.id, b.kind))
        .collect();
    assert_eq!(shelters.len(), 2);
    for &(id, kind) in &shelters {
        let b = w.building(id).unwrap();
        assert_eq!(b.garrison.len(), usize::from(kind.garrison_capacity()));
        assert!(b.garrison.iter().all(|g| g.unit.kind == UnitKind::Militia));
    }
    assert_eq!(w.units().filter(|u| u.player == PlayerId(1)).count(), 96);
    w.advance();
    for (id, kind) in shelters {
        let arrows: Vec<_> = w.projectiles().filter(|p| p.attacker == id).collect();
        assert_eq!(
            arrows.len(),
            if kind == BuildingKind::Castle { 21 } else { 11 }
        );
        assert!(arrows.iter().all(|p| p.impact.0 - p.launched.0 >= 10));
        let launches: std::collections::BTreeSet<_> =
            arrows.iter().map(|p| (p.from.x, p.from.y)).collect();
        assert_eq!(launches.len(), arrows.len());
    }
}
