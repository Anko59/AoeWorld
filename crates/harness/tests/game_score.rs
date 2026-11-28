//! `aoe-harness game-score` end to end: real child processes at different
//! worker counts play small matches on a stored map package, and the runs
//! must agree on every hash and every order.
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest, Ratio};
use serde_json::Value;
use std::{path::Path, process::Command};

/// A small synthetic package in `directory`; its hash.
fn store_package(directory: &Path) -> String {
    let request = MapRequest {
        requested_side_meters: 1_000,
        compression: Ratio::new(1, 1).expect("ratio"),
        ..MapRequest::default()
    };
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new()).expect("package");
    let hash = package.content_hash_hex();
    std::fs::write(
        directory.join(format!("{hash}.json")),
        serde_json::to_vec(&package).expect("package json"),
    )
    .expect("package written");
    hash
}

/// Runs the game score on a fresh package in a fresh directory.
fn game_score(arguments: &[&str]) -> (std::process::Output, Value, tempfile::TempDir) {
    let directory = tempfile::tempdir().expect("directory");
    let packages = directory.path().join("packages");
    std::fs::create_dir(&packages).expect("package directory");
    store_package(&packages);
    let out = directory.path().join("score.json");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .arg("game-score")
        .args([
            "--package-directory",
            packages.to_str().expect("utf-8 path"),
        ])
        .args(["--seats", "3", "--hash-every", "5", "--sample-every", "20"])
        .args(["--out", out.to_str().expect("utf-8 path")])
        .args(arguments)
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    let report = std::fs::read_to_string(&out)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    (output, report, directory)
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn scenario<'a>(report: &'a Value, id: &str) -> &'a Value {
    report["scenarios"]
        .as_array()
        .and_then(|all| all.iter().find(|scenario| scenario["id"] == id))
        .unwrap_or_else(|| panic!("no scenario {id} in {report}"))
}

fn window<'a>(scenario: &'a Value, id: &str) -> &'a Value {
    scenario["windows"]
        .as_array()
        .and_then(|all| all.iter().find(|window| window["id"] == id))
        .unwrap_or_else(|| panic!("no window {id} in {scenario}"))
}

fn units_in(window: &Value) -> u64 {
    window["service"]["size"]["units"].as_u64().unwrap_or(0)
}

#[test]
fn game_score_natural_one_worker_and_three_workers_play_the_same_game() {
    let (output, report, _directory) = game_score(&[
        "--scenario",
        "natural",
        "--ticks",
        "150",
        "--workers",
        "1,3",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(report["schema"], 1);
    assert_eq!(report["scenarios"].as_array().map(Vec::len), Some(1));
    let natural = scenario(&report, "natural");
    assert_eq!(natural["determinism"]["holds"], true, "{report}");
    assert_eq!(natural["determinism"]["against"][0]["same_numbers"], true);
    let runs = natural["determinism"]["runs"].as_array().expect("runs");
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0]["final_hash"], runs[1]["final_hash"]);
    assert!(
        runs[0]["orders"].as_u64().is_some_and(|n| n > 0),
        "no orders"
    );
    let whole = window(natural, "whole");
    assert_eq!(whole["to_tick"], 150);
    assert!(units_in(whole) > 0);
    assert!(
        whole["service"]["arrival"]["orders_given"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "the units were given no movement order: {whole}"
    );
    assert!(
        whole["cost"]["tick_seconds"]
            .as_f64()
            .is_some_and(|seconds| seconds > 0.0)
    );
    assert_eq!(whole["cost"]["instructions"], Value::Null);
    assert_eq!(whole["efficiency"]["basis"], "wall_time");
    assert_eq!(natural["series"].as_array().map(Vec::len), Some(1));
    let table = report["table"].as_str().expect("the table");
    assert!(table.contains("natural:"), "{table}");
    assert!(table.contains("1 worker and 3 workers"), "{table}");
    assert!(stderr(&output).contains("units alive at the end"));
}

#[test]
fn game_score_late_plays_the_recipe_at_a_scale_and_names_its_window() {
    let (output, report, _directory) = game_score(&[
        "--scenario",
        "late",
        "--late-percent",
        "5",
        "--workers",
        "1,2",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let late = scenario(&report, "late");
    assert_eq!(late["determinism"]["holds"], true, "{report}");
    assert_eq!(late["start"]["kind"], "late_game");
    assert_eq!(late["start"]["percent"], 5);
    let named = window(late, "late");
    assert!(named["from_tick"].as_u64().is_some_and(|from| from > 0));
    assert_eq!(named["to_tick"], late["ticks"]);
    let (_, fresh, _directory) =
        game_score(&["--scenario", "natural", "--ticks", "60", "--workers", "1"]);
    let plain = units_in(window(scenario(&fresh, "natural"), "whole"));
    assert!(
        units_in(named) > plain,
        "the recipe left {} units against {plain} of a fresh start",
        units_in(named)
    );
}

#[test]
fn game_score_two_runs_at_one_worker_count_agree() {
    let (output, report, _directory) = game_score(&[
        "--scenario",
        "natural",
        "--ticks",
        "100",
        "--workers",
        "2,2",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        scenario(&report, "natural")["determinism"]["holds"],
        true,
        "{report}"
    );
}

#[test]
fn game_score_without_orders_hashes_are_still_compared() {
    let (output, report, _directory) = game_score(&[
        "--scenario",
        "natural",
        "--ticks",
        "100",
        "--workers",
        "1,2",
        "--no-orders",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let natural = scenario(&report, "natural");
    assert_eq!(natural["determinism"]["runs"][0]["orders"], Value::Null);
    assert_eq!(natural["determinism"]["holds"], true, "{report}");
}

#[test]
fn game_score_an_unknown_scenario_is_refused_by_name() {
    let (output, _, _directory) = game_score(&["--scenario", "siege"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("siege"), "{}", stderr(&output));
}

#[test]
fn game_score_a_staged_scenario_is_refused_on_a_map_it_is_not_staged_on() {
    for scenario in ["march", "battle"] {
        let (output, report, _directory) = game_score(&["--scenario", scenario, "--workers", "1"]);
        assert!(!output.status.success(), "{scenario} was played");
        let text = stderr(&output);
        assert!(
            text.contains(scenario) && text.contains("staged at a choke of map"),
            "{text}"
        );
        assert_eq!(report, Value::Null, "nothing was played");
    }
}

#[test]
fn game_score_the_pull_request_set_leaves_out_what_is_staged_on_another_map_and_says_so() {
    let (output, report, _directory) =
        game_score(&["--ticks", "40", "--late-percent", "5", "--workers", "1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stderr(&output);
    assert!(
        text.contains("march is left out") && text.contains("battle is left out"),
        "{text}"
    );
    assert!(
        !text.contains("natural is left out") && !text.contains("late is left out"),
        "the scenarios that are played are not said to be left out: {text}"
    );
    assert_eq!(report["scenarios"].as_array().map(Vec::len), Some(2));
}

#[test]
fn game_score_names_the_missing_package() {
    let directory = tempfile::tempdir().expect("directory");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .args(["game-score", "--package-directory"])
        .arg(directory.path())
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("no map package"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn game_score_parallel_runs_play_the_same_games_as_runs_one_after_the_other() {
    let flags = ["--ticks", "60", "--late-percent", "5", "--workers", "1,2"];
    let (one_by_one, sequential, _a) = game_score(&flags);
    let (at_once, parallel, _b) = game_score(&[&flags[..], &["--parallel"]].concat());
    assert!(one_by_one.status.success(), "{}", stderr(&one_by_one));
    assert!(at_once.status.success(), "{}", stderr(&at_once));
    let ids = |report: &Value| -> Vec<String> {
        report["scenarios"]
            .as_array()
            .expect("scenarios")
            .iter()
            .map(|scenario| scenario["id"].as_str().unwrap_or("").to_owned())
            .collect()
    };
    assert_eq!(
        ids(&parallel),
        ["natural", "late"],
        "the order of the report"
    );
    for id in ["natural", "late"] {
        let (a, b) = (scenario(&sequential, id), scenario(&parallel, id));
        assert_eq!(b["determinism"]["holds"], true, "{id}");
        let (before, after) = (
            a["windows"].as_array().expect("a"),
            b["windows"].as_array().expect("b"),
        );
        assert_eq!(before.len(), after.len());
        for (before, after) in before.iter().zip(after) {
            assert_eq!(
                before["service"], after["service"],
                "{id}: the numbers of the game do not move"
            );
        }
        assert_eq!(
            a["determinism"]["runs"][0]["final_hash"],
            b["determinism"]["runs"][0]["final_hash"]
        );
        assert_eq!(b["determinism"]["runs"].as_array().map(Vec::len), Some(2));
    }
}

#[test]
fn game_score_without_a_package_flag_plays_on_the_map_of_the_shared_home() {
    let directory = tempfile::tempdir().expect("directory");
    let home = directory.path().join("home");
    let packages = home.join("packages");
    std::fs::create_dir_all(&packages).expect("packages");
    store_package(&packages);
    let out = directory.path().join("score.json");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .args(["game-score", "--scenario", "natural", "--ticks", "40"])
        .args(["--seats", "3", "--workers", "1"])
        .args(["--out", out.to_str().expect("utf-8 path")])
        .env("AOE_GAME_SCORE_HOME", &home)
        // CI keeps the package in `.cache/game-map/packages` (the game-map workflow restores it
        // there); the shared home is the default everywhere else.
        .env_remove("GITHUB_ACTIONS")
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    assert!(output.status.success(), "{}", stderr(&output));
    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(&out).expect("report")).expect("json");
    assert_eq!(
        report["header"]["package_directory"],
        packages.to_str().expect("utf-8")
    );
}

#[test]
fn game_score_without_a_map_says_which_make_goal_makes_it() {
    let directory = tempfile::tempdir().expect("directory");
    let output = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .args(["game-score", "--scenario", "natural"])
        .env("AOE_GAME_SCORE_HOME", directory.path().join("empty"))
        .current_dir(directory.path())
        .output()
        .expect("the harness runs");
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("make game-map"),
        "{}",
        stderr(&output)
    );
}
