use super::table::{describe, grouped};
use super::{Played, json, table};
use crate::game_score::Plan;
use crate::game_score::scenarios::{BATTLE, LATE, MARCH, NATURAL, Scenario};
use crate::game_score::tests::{hashes, minute, run, verdict, window};
use crate::game_score::windows::{Battle, Crossing};
use serde_json::{Value, json as value};
use std::path::PathBuf;

fn plan(scenario: &Scenario) -> Plan {
    Plan {
        package_directory: PathBuf::from("packages"),
        map_hash: "ce1c8d7e".repeat(8),
        scenario: scenario.id.to_owned(),
        seats: 100,
        ticks: scenario.ticks,
        seed: 1,
        late_percent: None,
        workers: 1,
        hash_every: scenario.hash_every,
        sample_every: scenario.sample_every,
        record_orders: true,
    }
}

/// A scenario as played at 1 and 4 workers, with the windows it names.
fn played(scenario: &'static Scenario, windows: &[(&str, u64, u64)], minutes: u64) -> Played {
    let build = |workers| {
        let mut one = run(workers, hashes(&[(0, "a"), (10, "b")]));
        one.scenario = scenario.id.to_owned();
        one.windows = windows
            .iter()
            .map(|&(id, from, to)| {
                let mut window = window(id, from, to);
                window.service.size.units = 3_209;
                window.service.arrival.orders_given = 41_000;
                window.service.arrival.orders_arrived = 12_345;
                window.service.arrival.arrived_permille = Some(913);
                window.service.economy.delivered_total = 6_789;
                window
            })
            .collect();
        one.series = (1..=minutes).map(minute).collect();
        one
    };
    let (first, second) = (build(1), build(4));
    let (verdict, _) = verdict(&first, &second);
    Played {
        scenario,
        plan: plan(scenario),
        runs: vec![first, second],
        verdict,
    }
}

fn both() -> Vec<Played> {
    vec![
        played(
            &NATURAL,
            &[
                ("early", 1_000, 3_000),
                ("middle", 8_000, 10_000),
                ("whole", 0, 10_000),
            ],
            17,
        ),
        played(&LATE, &[("late", 20, 60), ("whole", 0, 60)], 1),
    ]
}

#[test]
fn numbers_are_grouped_by_thousands() {
    for (number, text) in [
        (0, "0"),
        (999, "999"),
        (1_000, "1,000"),
        (3_209_000, "3,209,000"),
    ] {
        assert_eq!(grouped(number), text);
    }
}

#[test]
fn the_table_has_a_block_per_scenario_and_a_column_per_window() {
    let text = table("game score: 100 seats", &both());
    for words in [
        "natural: one fresh game",
        "late: the late-game recipe",
        "early",
        "middle",
        "whole",
        "1000-3000",
        "8000-10000",
        "20-60",
        "units alive at the end",
        "units / buildings produced",
        "villagers standing idle (share)",
        "movement orders given",
        "orders arrived (share of orders ended)",
        "order to arrival, p50 / p95 (ticks)",
        "order to first step, p50 / p95 (ticks)",
        "route searches started / answers from the cache",
        "route orders stuck / expired",
        "movers waiting for a route, end / peak",
        "wait for a route, p95 end / worst (ticks)",
        "Steward orders / unit visits",
        "General orders / unit visits",
        "Vassal / Rust AI orders",
        "hits landed",
        "first blow after contact (ticks, mean)",
        "kills / losses",
        "resources delivered",
        "seconds inside the ticks",
        "tick ms p50 / p95 / p99 / max",
        "peak memory (MiB)",
        "cost per arrival / 1,000 resources / hit",
        "resources delivered per game minute:",
        "determinism, 1 worker and 4 workers: the same hash every 10 ticks",
        "native seconds: 1 worker 4.5 s (start 1.5), 4 workers 4.5 s (start 1.5)",
    ] {
        assert!(text.contains(words), "missing {words:?} in\n{text}");
    }
    assert!(text.contains("12,345 (91%)"), "{text}");
    assert!(!text.contains("scoreboard"), "that word names another page");
    assert_eq!(
        text.matches("units alive at the end").count(),
        2,
        "one per scenario"
    );
}

#[test]
fn the_table_for_the_pull_request_set_is_short_enough_for_a_comment() {
    let text = table("game score: 100 seats, seed 1, map ce1c8d7e", &both());
    let lines = text.lines().count();
    assert!(lines <= 62, "{lines} lines:\n{text}");
    let widest = text.lines().map(str::len).max().unwrap_or(0);
    assert!(widest <= 110, "a line is {widest} characters wide");
}

#[test]
fn a_scenario_with_one_minute_has_no_series_lines() {
    let text = table("title", &both()[1..]);
    assert!(!text.contains("per game minute"), "{text}");
}

#[test]
fn a_difference_is_told_in_words_with_its_first_tick() {
    let mut played = played(&LATE, &[("late", 20, 60)], 1);
    played.runs[1].digests = hashes(&[(0, "a"), (10, "x")]);
    let (verdict, comparison) = verdict(&played.runs[0], &played.runs[1]);
    played.verdict = verdict;
    let text = table("title", &[played]);
    assert!(
        text.contains("DIFFERENT GAMES, first differing tick 10"),
        "{text}"
    );
    assert!(describe(&comparison, 10).contains("1 worker and 4 workers"));
}

#[test]
fn a_known_difference_of_a_scenario_is_in_the_table() {
    let scenario: &'static Scenario = Box::leak(Box::new(Scenario {
        note: "late differs between 1 and 4 workers: see the issue",
        fixed_workers: Some(4),
        ..LATE
    }));
    let text = table("title", &[played(scenario, &[("late", 20, 60)], 1)]);
    assert!(
        text.contains("note: late differs between 1 and 4 workers"),
        "{text}"
    );
}

#[test]
fn the_json_has_a_schema_a_header_and_scenarios_with_windows_series_and_determinism() {
    let played = both();
    let text = table("title", &played);
    let report = json(value!({ "revision": "abc" }), &played, &text);
    assert_eq!(report["schema"], 1);
    assert_eq!(report["header"]["revision"], "abc");
    let scenarios = report["scenarios"].as_array().expect("scenarios");
    let ids: Vec<_> = scenarios
        .iter()
        .map(|s| s["id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(ids, ["natural", "late"]);
    let natural = &scenarios[0];
    assert_eq!(natural["ticks"], 10_000);
    assert_eq!(natural["measured_at_workers"], 1);
    assert_eq!(natural["start"]["kind"], "fresh");
    assert_eq!(scenarios[1]["start"]["kind"], "late_game");
    assert_eq!(scenarios[1]["start"]["percent"], 100);
    let windows = natural["windows"].as_array().expect("windows");
    let names: Vec<_> = windows
        .iter()
        .map(|w| w["id"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(names, ["early", "middle", "whole"]);
    let early = &windows[0];
    assert_eq!(
        (early["from_tick"].as_u64(), early["to_tick"].as_u64()),
        (Some(1_000), Some(3_000))
    );
    for part in ["service", "cost", "efficiency"] {
        assert!(early[part].is_object(), "{part}");
    }
    assert_eq!(natural["series"].as_array().map(Vec::len), Some(17));
    assert_eq!(natural["determinism"]["holds"], true);
    assert_eq!(
        natural["determinism"]["runs"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(natural["determinism"]["runs"][1]["workers"], 4);
    assert_eq!(report["table"].as_str(), Some(text.as_str()));
}

#[test]
fn numbers_of_the_game_and_numbers_of_the_clock_live_in_separate_objects() {
    let played = both();
    let report = json(Value::Null, &played, "");
    let early = &report["scenarios"][0]["windows"][0];
    let service = early["service"].to_string();
    for clock in [
        "tick_seconds",
        "tick_ms",
        "peak_memory",
        "resident",
        "nanos",
        "instructions",
    ] {
        assert!(
            !service.contains(clock),
            "{clock} is in the deterministic numbers"
        );
    }
    assert!(early["cost"]["tick_seconds"].is_number());
    assert_eq!(early["cost"]["instructions"], Value::Null);
    for part in [
        "size",
        "arrival",
        "planning",
        "management",
        "fights",
        "economy",
    ] {
        assert!(early["service"][part].is_object(), "service.{part}");
    }
    let minute = &report["scenarios"][0]["series"][0];
    assert_eq!(minute["minute"], 1);
    assert!(minute["service"].is_object() && minute["cost"].is_object());
}

#[test]
fn the_table_says_which_basis_the_cost_per_service_is_computed_from() {
    let mut set = both();
    let text = table("title", &set);
    assert!(text.contains("hit (wall ms)"), "{text}");
    for window in &mut set[0].runs[0].windows {
        window.cost.instructions = Some(1_000_000);
        window.efficiency.basis = "instructions".to_owned();
    }
    let text = table("title", &set);
    assert!(text.contains("hit (instructions)"), "{text}");
}

#[test]
fn a_short_series_fits_on_the_line_of_its_label_and_a_long_one_wraps_under_it() {
    let mut set = both();
    for minute in &mut set[0].runs[0].series {
        minute.service.economy.delivered_total = 123_456;
    }
    let text = table("title", &set);
    let line = |start: &str| {
        text.lines()
            .position(|line| line.starts_with(start))
            .unwrap_or_else(|| panic!("no line starts with {start:?} in\n{text}"))
    };
    let lines: Vec<_> = text.lines().collect();
    let first = line("resources delivered per game minute:");
    assert!(
        lines[first].len() > "resources delivered per game minute:".len(),
        "17 minutes"
    );
    let short = table("title", &[played(&LATE, &[("late", 20, 60)], 2)]);
    assert!(
        short.contains("resources delivered per game minute: 0 0\n"),
        "two minutes fit on the label's line:\n{short}"
    );
    assert!(
        lines[first + 1].starts_with(' '),
        "the numbers go on under their label"
    );
    assert!(
        lines.iter().all(|line| line.len() <= 110),
        "a line is too long"
    );
}

fn staged(scenario: &'static Scenario, ticks: u64, battle: bool) -> Played {
    let mut one = played(scenario, &[("crossing", 0, ticks)], 4);
    for run in &mut one.runs {
        for window in &mut run.windows {
            window.service.choke = Some(Crossing {
                units_sent: 800,
                crossed: 800,
                crossed_per_minute_peak: 776,
                first_crossing_tick: Some(193),
                last_crossing_tick: Some(1_036),
                waiting_near_side: 0,
                standing_still_inside_ticks: 12,
                arrived_at_goal: 798,
                all_arrived: false,
            });
            window.service.battle = battle.then_some(Battle {
                first_blow_tick: Some(292),
                fought: 770,
                queued: 30,
                fight_ended_tick: None,
                alive_seat_0_permille: 187,
                alive_seat_1_permille: 95,
            });
        }
        for minute in &mut run.series {
            minute.service.choke = Some(Crossing {
                crossed: 100,
                ..Crossing::default()
            });
        }
    }
    one
}

#[test]
fn a_staged_scenario_shows_what_happened_at_its_choke_and_not_what_a_village_does() {
    let text = table("title", &[staged(&MARCH, 2_400, false)]);
    for wanted in [
        "march:",
        "units sent / across the line",
        "800 / 800",
        "most across the line in one game minute",
        "776",
        "first / last unit across (tick)",
        "193 / 1,036",
        "longest stand still inside the neck (ticks)",
        "798 of 800",
        "units across the line per game minute: 100 100 100 100",
    ] {
        assert!(text.contains(wanted), "{wanted:?} is not in\n{text}");
    }
    for village in [
        "villagers standing idle",
        "Steward orders",
        "Vassal / Rust AI orders",
        "resources delivered per game minute",
        "first blow (tick)",
    ] {
        assert!(!text.contains(village), "{village:?} is in\n{text}");
    }
    assert!(text.contains("hits landed"), "the fights rows stay");
}

#[test]
fn two_armies_add_the_rows_of_a_fight() {
    let text = table("title", &[staged(&BATTLE, 4_500, true)]);
    for wanted in [
        "first blow (tick)",
        "292",
        "units that fought / only queued behind",
        "770 / 30",
        "fight ended (tick)",
        "none",
        "alive at the end, seat 0 / seat 1",
        "19% / 10%",
    ] {
        assert!(text.contains(wanted), "{wanted:?} is not in\n{text}");
    }
}

#[test]
fn the_report_says_where_a_staged_scenario_is_staged() {
    let report = json(value!({}), &[staged(&MARCH, 2_400, false)], "table");
    let march = &report["scenarios"][0];
    assert_eq!(march["start"]["kind"], "staged");
    assert_eq!(march["start"]["units"], 800);
    assert_eq!(march["start"]["attack"], false);
    assert_eq!(march["start"]["armies"][0]["from"], "Near");
    let record = &march["start"]["choke_record"];
    assert_eq!(record["width_tiles"], 5);
    assert!(
        record["why"]
            .as_str()
            .is_some_and(|why| why.contains("62 steps")),
        "{record}"
    );
    let window = &march["windows"][0]["service"];
    assert_eq!(window["choke"]["crossed"], 800);
    assert!(window.get("battle").is_none(), "one army: no battle");
    let natural = json(
        value!({}),
        &[played(&NATURAL, &[("early", 1_000, 3_000)], 2)],
        "t",
    );
    assert!(
        natural["scenarios"][0]["windows"][0]["service"]
            .get("choke")
            .is_none()
    );
}
