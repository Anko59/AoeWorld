//! Targets come from ordinary offensive movement, rather than a General focus table.
use super::*;

#[test]
fn offensive_squad_engages_the_near_enemy_before_a_distant_siege_unit() {
    let (mut w, _) = setup();
    for n in 0..8 {
        soldier(&mut w, UnitKind::Archer, 100 + n % 4, 100 + n / 4);
    }
    let g = soldier(&mut w, UnitKind::General, 103, 103);
    let near = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(106, 100))
        .unwrap();
    let far = w
        .spawn_kind(PlayerId(1), UnitKind::BatteringRam, at(116, 100))
        .unwrap();
    w.issue_stance(
        PlayerId(1),
        &[near, far],
        aoe_core::CombatStance::StandGround,
    )
    .unwrap();
    w.issue_move(g, at(150, 103)).unwrap();
    for _ in 0..80 {
        w.advance();
        if w.unit(near).unwrap().hp < w.unit(near).unwrap().max_hp {
            assert_eq!(
                w.unit(far).unwrap().hp,
                w.unit(far).unwrap().max_hp,
                "the first hit is on the near contact, not the distant siege"
            );
            return;
        }
    }
    panic!("the near contact was never engaged");
}
