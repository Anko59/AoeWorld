use crate::{GameWorld, UnitKind};
use aoe_core::{PlayerId, Seed, TileCoord, WorldConfig, WorldPosition};
fn duel(observe: bool) -> (GameWorld, aoe_core::EntityId, aoe_core::EntityId) {
    let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(9)).unwrap()).unwrap();
    let at = |x| WorldPosition::from_tile_center(TileCoord::new(x, 100)).unwrap();
    let a = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(100))
        .unwrap();
    let b = w
        .spawn_kind(PlayerId(1), UnitKind::Militia, at(101))
        .unwrap();
    w.set_damage_observation(observe);
    w.issue_attack(PlayerId(0), a, b).unwrap();
    (w, a, b)
}
#[test]
fn quality_damage_observation_is_opt_in_drainable_and_does_not_change_gameplay() {
    let (mut observed, a, b) = duel(true);
    let (mut plain, _, _) = duel(false);
    let mut dealt = 0;
    for _ in 0..180 {
        observed.advance();
        plain.advance();
        assert_eq!(observed.canonical_hash(), plain.canonical_hash());
        for hit in observed.take_damage_observations() {
            assert!(hit.amount_quarters > 0);
            if hit.attacker == a && hit.target == b {
                dealt += hit.amount_quarters;
            }
        }
        assert!(observed.take_damage_observations().is_empty());
        assert!(plain.take_damage_observations().is_empty());
    }
    assert!(dealt > 0);
}

#[test]
fn quality_impacts_include_every_hit_and_cap_overkill_at_actual_health() {
    let (mut w, a, victim) = duel(true);
    let second = w
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(101, 99)).unwrap(),
        )
        .unwrap();
    w.issue_attack(PlayerId(0), second, victim).unwrap();
    let mut shooters = std::collections::BTreeSet::new();
    for _ in 0..80 {
        let index = w.lookup[&victim];
        let before = u32::from(w.units[index].state.hp) * 4
            + u32::from(w.combat.hp_quarters[victim.0 as usize]);
        w.advance();
        let index = w.lookup[&victim];
        let after = u32::from(w.units[index].state.hp) * 4
            + u32::from(w.combat.hp_quarters[victim.0 as usize]);
        let hits = w.take_damage_observations();
        let damage: u32 = hits
            .iter()
            .filter(|hit| hit.target == victim)
            .map(|hit| {
                shooters.insert(hit.attacker);
                hit.amount_quarters
            })
            .sum();
        assert_eq!(before - after, damage);
        if after == 0 {
            break;
        }
    }
    assert_eq!(shooters, std::collections::BTreeSet::from([a, second]));
}
