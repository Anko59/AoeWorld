use super::*;

#[test]
fn hulk_line_launches_three_damaging_hooks_per_attack() {
    for kind in [UnitKind::Hulk, UnitKind::WarHulk, UnitKind::Carrack] {
        let mut w = coast();
        let (_, water) = bank(&w);
        let hulk = ship(&mut w, kind, water, 0);
        let target = ship(
            &mut w,
            UnitKind::Galley,
            TileCoord::new(water.x + 1, water.y),
            1,
        );
        w.issue_stance(PlayerId(1), &[target], aoe_core::CombatStance::NoAttack)
            .unwrap();
        w.issue_attack(PlayerId(0), hulk, target).unwrap();
        let before = w.unit(target).unwrap().hp;
        let mut impact = None;
        for _ in 0..40 {
            w.advance();
            let shots: Vec<_> = w.projectiles().filter(|p| p.attacker == hulk).collect();
            if !shots.is_empty() {
                assert_eq!(shots.len(), 3, "{kind:?}");
                assert!(shots.iter().all(|p| p.launched == w.tick()));
                assert!(shots.iter().any(|p| p.to != shots[0].to));
                impact = shots.iter().map(|p| p.impact).max();
                break;
            }
        }
        while w.tick() < impact.expect("the Hulk fires within range") {
            w.advance();
        }
        assert_eq!(before - w.unit(target).unwrap().hp, 12, "{kind:?}");
    }
}
