//! Equal melee cohorts meeting head on: the quality-metrics `melee` case at
//! simulation scale. An attack-move must put most of the cohort into the fight
//! and deal damage, not walk through an equal enemy force untouched.
use super::*;
use aoe_core::Formation;

/// Forty militia per side, five wide, as the measurement cohorts stand.
fn cohort(w: &mut GameWorld, player: PlayerId, at: (i32, i32)) -> Vec<EntityId> {
    (0..40)
        .map(|n| {
            w.spawn_kind(
                player,
                UnitKind::Militia,
                center(at.0 + n % 5, at.1 + n / 5),
            )
            .unwrap()
        })
        .collect()
}

/// Dealt damage per member of `ids`, and damage they received, in quarter HP.
fn clash(w: &mut GameWorld, ids: &[EntityId], ticks: u64) -> (Vec<u64>, u64) {
    let mut dealt = vec![0_u64; ids.len()];
    let mut received = 0;
    w.set_damage_observation(true);
    for _ in 0..ticks {
        w.advance();
        for hit in w.take_damage_observations() {
            if let Some(n) = ids.iter().position(|&id| id == hit.attacker) {
                dealt[n] += u64::from(hit.amount_quarters);
            }
            if ids.contains(&hit.target) {
                received += u64::from(hit.amount_quarters);
            }
        }
    }
    (dealt, received)
}

#[test]
fn an_attack_move_cohort_fights_an_equal_melee_force_instead_of_marching_through_it() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let ids = cohort(&mut w, PlayerId(0), (40, 40));
    cohort(&mut w, PlayerId(1), (70, 40));
    // The objective sits past the enemy, as a player's attack-move does: the
    // order is a march, and the fight has to start on the way.
    w.issue_formation_move(
        PlayerId(0),
        &ids,
        Formation::Line,
        center(90, 44),
        false,
        true,
    )
    .unwrap();
    let (dealt, received) = clash(&mut w, &ids, 1_200);
    let engaged = dealt.iter().filter(|&&d| d > 0).count();
    let total: u64 = dealt.iter().sum();
    eprintln!("melee clash engaged={engaged}/40 dealt={total} received={received}");
    assert!(
        engaged >= 30,
        "a melee attack-move must put most of the cohort into the fight, not {engaged} of 40"
    );
    assert!(
        total >= received,
        "an equal melee cohort must not lose the exchange {total} / {received}"
    );
}

#[test]
fn a_lone_attack_move_militia_fights_the_enemy_it_walks_into() {
    let mut w = wide_world();
    w.set_fog_disabled(true);
    let id = w
        .spawn_kind(PlayerId(0), UnitKind::Militia, center(40, 40))
        .unwrap();
    w.spawn_kind(PlayerId(1), UnitKind::Militia, center(55, 40))
        .unwrap();
    w.issue_attack_move(id, center(70, 40)).unwrap();
    let (dealt, _) = clash(&mut w, &[id], 1_200);
    assert!(
        dealt[0] > 0,
        "a single unit's attack-move must fight what it meets"
    );
}
