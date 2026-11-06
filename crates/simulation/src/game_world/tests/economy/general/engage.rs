//! Share of a General's soldiers that fight when their army meets an enemy.
use super::*;
use crate::game_world::tests::combat::engage::{Probe, army};

fn run(name: &str, with_general: bool, enemy_marches: bool) -> Probe {
    let (mut w, _) = setup();
    w.set_fog_disabled(true);
    let ours = army(&mut w, 0, 190, 19, 60, 100);
    let theirs = army(&mut w, 1, 120, 12, 120, 100);
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
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/engage");
    std::fs::write(
        dir.join(format!("{name}-time.txt")),
        format!("advance total ms={}", spent.as_millis()),
    )
    .unwrap();
    probe
}

fn assert_engages(name: &str, general: bool, march: bool) {
    let p = run(name, general, march);
    let share = p.with_target * 100 / p.in_reach;
    assert!(share >= 80, "{name}: {share}% of units in reach fight");
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
