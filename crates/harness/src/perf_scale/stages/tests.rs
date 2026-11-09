use super::*;
use crate::perf_scale::callgrind::is;

/// movement calls propose (twice, once through formations) and apply_step;
/// propose is 100 Ir with 2 misses, formations 40 Ir with 1 miss.
const PROFILE: &str = "\
# callgrind format
version: 1
positions: line
events: Ir ILmr DLmr DLmw
desc: LL cache: 8388608 B, 64 B, 16-way associative

fl=(1) a.rs
fn=(1) <aoe_simulation::game_world::GameWorld>::advance_movement
10 5 0 0 0
cfn=(2) <aoe_simulation::game_world::GameWorld>::propose_steps_into
calls=1 20
10 100 0 2 0
cfn=(3) <aoe_simulation::game_world::GameWorld>::advance_formations
calls=1 30
+1 60 0 1 0
cfn=(4) <aoe_simulation::game_world::GameWorld>::apply_step
calls=1 40
+1 30 0 0 3

fn=(2)
20 100 0 2 0
fn=(3)
30 20 0 0 0
cfn=(2)
calls=1 20
+1 40 0 1 0
fn=(4)
40 30 0 0 3

totals: 195 0 3 3
";

#[test]
fn inclusive_cost_adds_the_calls_a_function_makes() {
    let profile = parse(PROFILE).expect("profile");
    assert_eq!(
        profile.inclusive("GameWorld>::advance_formations"),
        Some(vec![60, 0, 1, 0])
    );
    assert_eq!(
        profile.inclusive("GameWorld>::advance_movement"),
        Some(vec![195, 0, 3, 3])
    );
    assert_eq!(profile.inclusive("GameWorld>::nothing_ran_here"), None);
}

#[test]
fn a_call_edge_separates_one_caller_from_another() {
    let profile = parse(PROFILE).expect("profile");
    let from = |caller: &str| profile.edge(caller, "GameWorld>::propose_steps_into");
    assert_eq!(
        from("GameWorld>::advance_movement"),
        Some(vec![100, 0, 2, 0])
    );
    assert_eq!(
        from("GameWorld>::advance_formations"),
        Some(vec![40, 0, 1, 0])
    );
}

#[test]
fn closures_and_generics_do_not_match_their_parent() {
    assert!(is("<X as Y>::f::<u8>", "::f"));
    assert!(!is("<X as Y>::f::{closure#0}", "::f"));
}

#[test]
fn stages_report_misses_bytes_and_per_mover_costs() {
    let texts = [("stages_steady", "d".to_owned(), PROFILE.to_owned())];
    let report = report(&texts).expect("report");
    let stages = &report["scenarios"][0]["stages"];
    assert_eq!(stages["tick"]["ir"], 195);
    assert_eq!(stages["propose"]["ll_misses"], 2);
    assert_eq!(stages["group_propose"]["ir"], 40);
    // Misses are ILmr + DLmr + DLmw: movement has 0 + 3 + 3 ... of its own.
    assert_eq!(stages["movement"]["ll_misses"], 6);
    assert_eq!(stages["movement"]["dram_bytes"], 6 * 64);
    assert_eq!(stages["index_chunks"]["present"], false);
    let movers = f64::from(crate::perf_scale::groups_workload::DEFAULT_MOVERS);
    let per_mover = stages["movement"]["per_mover"]["ir"]
        .as_f64()
        .expect("number");
    assert!((per_mover - 195.0 / movers).abs() < 0.001);
}

#[test]
fn the_apply_stage_is_what_the_movement_phase_has_left() {
    let profile = parse(PROFILE).expect("profile");
    let found = costs(&profile).expect("costs");
    let ir = |name: &str| {
        found
            .iter()
            .find(|(stage, _)| *stage == name)
            .and_then(|(_, cost)| cost.as_ref())
            .map(|cost| cost[0])
    };
    // 195 - 100 (propose) - 60 (formations); apply_step is called by formations.
    assert_eq!(ir("apply"), Some(35));
}

#[test]
fn a_profile_without_cache_events_is_refused() {
    let text = PROFILE.replace("events: Ir ILmr DLmr DLmw", "events: Ir");
    let error = report(&[("stages_steady", "d".to_owned(), text)]).expect_err("no misses");
    assert!(error.contains("ILmr"), "{error}");
}
