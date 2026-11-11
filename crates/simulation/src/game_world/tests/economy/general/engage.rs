//! Share of a General's soldiers that fight when their army meets an enemy.
use super::*;
use crate::game_world::tests::combat::engage::{Probe, army};

fn run(name: &str, with_general: bool, enemy_marches: bool) -> Probe {
    run_against(name, with_general, enemy_marches, 120)
}

fn run_against(name: &str, with_general: bool, enemy_marches: bool, enemies: i32) -> Probe {
    run_scene(name, with_general, enemy_marches, enemies, 0)
}

fn run_scene(
    name: &str,
    with_general: bool,
    enemy_marches: bool,
    enemies: i32,
    defender_offset: i32,
) -> Probe {
    let (mut w, _) = setup();
    w.set_fog_disabled(true);
    let timer = std::sync::Arc::new(RouteTimer::default());
    w.set_planning_probe(timer.clone());
    let ours = army(&mut w, 0, 190, 19, 60, 100);
    let theirs = army(&mut w, 1, enemies, 12, 120, 100 + defender_offset);
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
    let summary = probe.report(name, 20, mine.len());
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/regions");
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join(format!("{name}.txt")),
        format!(
            "{summary}with_target={} in_reach={}\nroute spans ns: {:?}\n",
            probe.with_target,
            probe.in_reach,
            timer.0.lock().unwrap()
        ),
    )
    .unwrap();
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
    // Fixed symmetric deployment perturbations, including the original scene.
    // Route admission can shift a wounded retreat by a tick and change the
    // battle trajectory; judge engagement across trajectories, not one outcome.
    let shares: Vec<_> = (-2..=2)
        .map(|offset| {
            let name = format!("general_vs_idle_offset_{offset}");
            let p = run_scene(&name, true, false, 120, offset);
            assert!(p.in_reach > 0, "{name}: the armies must meet");
            let share = p.with_target as f64 * 100.0 / p.in_reach as f64;
            println!("{name}: engagement={share:.6}%");
            assert!(share >= 80.0, "{name}: {share}% of units in reach fight");
            assert!(p.stuck_units(6, 20) <= 5, "{name}: units idle in reach");
            share
        })
        .collect();
    let mean = shares.iter().sum::<f64>() / shares.len() as f64;
    let summary = format!("shares={shares:?} mean={mean:.6}%\n");
    println!("general_vs_idle: {summary}");
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/regions/general_vs_idle_variants.txt"),
        summary,
    )
    .unwrap();
    assert!(mean >= 85.0, "mean engagement {mean}%: {shares:?}");
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

#[derive(Debug, Default)]
struct RouteTimer(std::sync::Arc<std::sync::Mutex<std::collections::BTreeMap<&'static str, u128>>>);
struct RouteSpan {
    timer: RouteTimer,
    name: &'static str,
    start: std::time::Instant,
}
impl aoe_map::NavigationSpan for RouteSpan {}
impl Drop for RouteSpan {
    fn drop(&mut self) {
        *self.timer.0.lock().unwrap().entry(self.name).or_default() +=
            self.start.elapsed().as_nanos();
    }
}
impl crate::PlanningProbe for RouteTimer {
    fn begin(&self) {}
    fn end(&self) {}
    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        Some(Box::new(RouteSpan {
            timer: RouteTimer(self.0.clone()),
            name,
            start: std::time::Instant::now(),
        }))
    }
}
