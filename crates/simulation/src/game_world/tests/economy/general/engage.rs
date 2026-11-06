//! Share of a General's soldiers that fight when their army meets an enemy.
use super::*;
use crate::game_world::tests::combat::engage::{Probe, army};

fn run(name: &str, with_general: bool, enemy_marches: bool) -> Probe {
    run_against(name, with_general, enemy_marches, 120)
}

fn run_against(name: &str, with_general: bool, enemy_marches: bool, enemies: i32) -> Probe {
    let (mut w, _) = setup();
    w.set_fog_disabled(true);
    let ours = army(&mut w, 0, 190, 19, 60, 100);
    let theirs = army(&mut w, 1, enemies, 12, 120, 100);
    let mut mine = ours.clone();
    if with_general {
        let g = soldier(&mut w, UnitKind::General, 70, 112);
        mine.push(g);
        w.issue_move(g, at(125, 106)).unwrap();
    } else {
        w.issue_formation_move(
            PlayerId(0),
            &ours,
            aoe_core::Formation::Line,
            at(125, 106),
            false,
            true,
        )
        .unwrap();
    }
    if enemy_marches {
        w.issue_formation_move(
            PlayerId(1),
            &theirs,
            aoe_core::Formation::Line,
            at(65, 106),
            false,
            true,
        )
        .unwrap();
    }
    let mut probe = Probe::default();
    let mut spent = std::time::Duration::ZERO;
    for _ in 0..1_800 {
        let start = std::time::Instant::now();
        w.advance();
        spent += start.elapsed();
        probe.sample(&w, &mine);
    }
    probe.report(name, 20, mine.len());
    println!("{name}: advance total ms={}", spent.as_millis());
    probe
}

fn assert_engages(name: &str, general: bool, march: bool) {
    let p = run(name, general, march);
    check(name, &p);
}

fn check(name: &str, p: &Probe) {
    let share = p.with_target * 100 / p.in_reach;
    assert!(share >= 85, "{name}: {share}% of units in reach fight");
    assert!(p.stuck_units(6, 20) <= 5, "{name}: units idle in reach");
}

#[test]
fn a_general_army_engages_idle_defenders() {
    assert_engages("general_vs_idle", true, false);
}

#[test]
fn a_general_army_engages_a_marching_enemy() {
    assert_engages("general_vs_march", true, true);
}

#[test]
fn an_army_without_a_general_engages_idle_defenders() {
    assert_engages("nogeneral_vs_idle", false, false);
}

#[test]
fn a_retreating_soldier_fights_an_enemy_in_its_reach_and_one_that_hits_it() {
    for hit_first in [false, true] {
        let (mut w, _) = setup();
        let ids: Vec<_> = (0..12)
            .map(|n| soldier(&mut w, UnitKind::Knight, 100 + n % 6, 100 + n / 6))
            .collect();
        let g = soldier(&mut w, UnitKind::General, 104, 104);
        w.units[w.lookup[&g]].state.hp = 1;
        w.spawn_kind(PlayerId(1), UnitKind::Militia, at(112, 104))
            .unwrap();
        decide(&mut w);
        assert_eq!(w.general_status(g).unwrap().state, GeneralState::Retreating);
        w.advance();
        assert!(w.unit(ids[0]).unwrap().moving, "the retreat is under way");
        let p = w.unit(ids[0]).unwrap().position;
        let enemy = w
            .spawn_kind(
                PlayerId(1),
                UnitKind::Militia,
                at(p.tile_floor().x + 1, p.tile_floor().y),
            )
            .unwrap();
        if hit_first {
            w.units[w.lookup[&enemy]].state.stance = aoe_core::CombatStance::NoAttack;
            let mut changed = Vec::new();
            w.retaliate(w.lookup[&ids[0]], enemy, &mut changed);
            assert_eq!(w.unit(ids[0]).unwrap().attack_target, Some(enemy));
        } else {
            for _ in 0..40 {
                w.advance();
            }
            assert!(
                w.unit(ids[0]).unwrap().attack_target == Some(enemy)
                    || w.unit(enemy).is_none_or(|e| e.hp == 0),
                "a retreating knight fights the enemy beside it"
            );
        }
    }
}

/// Outnumbered, the army still fights what reaches it while it falls back.
#[test]
fn an_outnumbered_general_army_still_fights_what_reaches_it() {
    let p = run_against("general_outnumbered", true, false, 300);
    assert!(p.in_reach > 0);
    let share = p.with_target * 100 / p.in_reach;
    assert!(share >= 25, "{share}% of units in reach fight");
}
