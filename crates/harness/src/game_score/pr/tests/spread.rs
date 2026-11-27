use super::*;
use crate::game_score::{
    pr::fixtures::{measured, set, standard},
    scenarios,
};
use serde_json::json;

fn row(path: &str, base: Option<f64>, branch: Option<f64>) -> Row {
    Row {
        path: path.to_owned(),
        base,
        branch,
    }
}

/// `standard()` at as many seeds as `values` has, the share of orders that arrived in the early
/// window of `natural` being each value.
fn seeds(values: &[u64]) -> Vec<Value> {
    values
        .iter()
        .map(|value| {
            let mut report = standard();
            set(
                &mut report,
                "natural",
                "early",
                "service.arrival.arrived_permille",
                json!(value),
            );
            report
        })
        .collect()
}

#[test]
fn the_spread_is_the_median_absolute_deviation_over_the_median_and_one_wild_seed_does_not_move_it()
{
    let entry = entry_of(vec![100.0, 102.0, 98.0, 101.0, 99.0, 100.0, 103.0, 300.0]);
    assert_eq!(entry.median, 100.5);
    assert_eq!(entry.mad_percent, 1.5);
    assert_eq!((entry.min, entry.max), (98.0, 300.0));
}

#[test]
fn a_median_of_zero_has_no_relative_spread() {
    let entry = entry_of(vec![0.0, 0.0, 0.0, 2.0]);
    assert_eq!(
        (entry.median, entry.mad_percent, entry.max),
        (0.0, 0.0, 2.0)
    );
}

fn only(path: &str, entry: Entry) -> Spread {
    Spread {
        measured: measured(),
        scenarios: BTreeMap::from([(
            "late".to_owned(),
            BTreeMap::from([(
                "late".to_owned(),
                BTreeMap::from([(path.to_owned(), entry)]),
            )]),
        )]),
    }
}

#[test]
fn the_tolerance_is_twice_the_spread_never_under_five_percent_and_never_under_the_range_or_the_floor()
 {
    let wide = Entry {
        median: 100.0,
        mad_percent: 4.0,
        min: 70.0,
        max: 100.0,
    };
    let spread = only("planning.stuck", wide);
    assert_eq!(
        spread.tolerance("late", "late", "planning.stuck"),
        Tolerance {
            percent: 8.0,
            min_move: 30.0
        }
    );
    let steady = Entry {
        mad_percent: 1.0,
        min: 99.0,
        max: 101.0,
        ..wide
    };
    assert_eq!(
        only("planning.stuck", steady).tolerance("late", "late", "planning.stuck"),
        Tolerance {
            percent: 5.0,
            min_move: 10.0
        },
        "five percent at least, and the floor of a count"
    );
    assert_eq!(
        only("arrival.order_to_first_step_ticks.p95", steady).tolerance(
            "late",
            "late",
            "arrival.order_to_first_step_ticks.p95"
        ),
        Tolerance {
            percent: 5.0,
            min_move: 3.0
        },
        "a few ticks are the floor of a delay"
    );
    assert_eq!(
        spread.tolerance("late", "late", "planning.expired"),
        Tolerance {
            percent: UNMEASURED_PERCENT,
            min_move: 10.0
        },
        "a number nobody measured"
    );
    assert_eq!(
        spread
            .tolerance("natural", "early", "planning.stuck")
            .percent,
        UNMEASURED_PERCENT,
        "a scenario nobody measured"
    );
}

#[test]
fn a_move_is_beyond_the_tolerance_only_in_relative_and_in_absolute_terms() {
    let tolerance = Tolerance {
        percent: 5.0,
        min_move: 10.0,
    };
    let beyond = |base, branch| tolerance.beyond(&row("planning.stuck", base, branch));
    assert!(!beyond(Some(100.0), Some(104.0)), "4% of 100");
    assert!(!beyond(Some(100.0), Some(106.0)), "6%, but 6 units");
    assert!(beyond(Some(100.0), Some(120.0)));
    assert!(beyond(Some(100.0), Some(80.0)), "downwards too");
    assert!(!beyond(Some(1000.0), Some(1030.0)), "30 units, but 3%");
    assert!(
        beyond(Some(0.0), Some(11.0)),
        "from zero the absolute move decides"
    );
    assert!(
        !beyond(Some(0.0), Some(10.0)),
        "strictly more than the floor"
    );
    assert!(!beyond(None, Some(500.0)) && !beyond(Some(500.0), None));
}

#[test]
fn every_judged_number_of_every_named_window_is_measured_and_the_whole_run_is_not() {
    let spread =
        measure(&seeds(&[970, 972, 960, 975, 968, 971, 969]), measured()).expect("a spread");
    let early = &spread.scenarios["natural"]["early"];
    assert_eq!(early.len(), judged().count());
    let arrived = early["arrival.arrived_permille"];
    assert_eq!(arrived.median, 970.0);
    assert_eq!(arrived.mad_percent, 0.2);
    assert_eq!((arrived.min, arrived.max), (960.0, 975.0));
    assert!(!spread.scenarios["natural"].contains_key("whole"));
    assert!(spread.scenarios["late"].contains_key("late"));
    assert!(
        early.keys().all(|path| !path.starts_with("efficiency.")),
        "a cost per unit is judged exactly, never on a spread"
    );
}

#[test]
fn fewer_than_three_seeds_say_nothing_about_a_spread() {
    assert!(measure(&seeds(&[970, 972]), measured()).is_err());
}

#[test]
fn the_table_gives_each_number_its_median_spread_range_and_tolerance() {
    let spread =
        measure(&seeds(&[970, 972, 960, 975, 968, 971, 969]), measured()).expect("a spread");
    let table = table(&spread);
    assert!(
        table.contains("| natural | early | share of orders arrived | 97.0% | 0.2% | 96.0% to 97.5% | 5% and 1.5% |"),
        "{table}"
    );
    assert!(table.starts_with("| scenario | window | number | median |"));
}

#[test]
fn a_spread_comes_back_from_its_file() {
    let spread =
        measure(&seeds(&[970, 972, 960, 975, 968, 971, 969]), measured()).expect("a spread");
    let text = serde_json::to_string_pretty(&spread).expect("json");
    assert_eq!(Spread::parse(&text).expect("back"), spread);
}

#[test]
fn the_command_reads_the_reports_of_each_seed_and_writes_the_spread() {
    let directory = tempfile::tempdir().expect("directory");
    for (seed, report) in seeds(&[970, 972, 960, 975]).iter().enumerate() {
        std::fs::write(
            directory.path().join(format!("seed-{}.json", seed + 1)),
            serde_json::to_string(report).expect("json"),
        )
        .expect("report");
    }
    std::fs::write(directory.path().join("seed-x.json"), "not a seed").expect("ignored");
    std::fs::write(directory.path().join("notes.txt"), "ignored").expect("ignored");
    let out = directory.path().join("spread.json");
    run(Options {
        reports: directory.path().to_owned(),
        commit: "c".repeat(40),
        date: "2026-11-27".to_owned(),
        out: out.clone(),
    })
    .expect("measured");
    let spread = Spread::parse(&std::fs::read_to_string(&out).expect("file")).expect("spread");
    assert_eq!(spread.measured.seeds, [1, 2, 3, 4]);
    assert_eq!(spread.measured.commit, "c".repeat(40));
    assert_eq!(spread.measured.workers, 1);
    assert_eq!(
        spread.scenarios["natural"]["early"]["arrival.arrived_permille"].median,
        971.0
    );
}

/// The numbers the verdict reads are those of the scenarios in this code: a scenario, a window or a
/// judged number added since the spread was measured makes this fail until it is measured again
/// (`make game-score-spread`, docs/scale/game-score.md).
#[test]
fn the_committed_spread_covers_every_judged_number_of_every_window_of_every_scenario() {
    let spread = Spread::committed().expect("the committed spread parses");
    for scenario in scenarios::ALL {
        for span in scenario.spans {
            for path in judged() {
                assert!(
                    spread.entry(scenario.id, span.id, path).is_some(),
                    "{}/{} {path} is not in scenarios/spread.json: measure it again",
                    scenario.id,
                    span.id
                );
            }
        }
    }
}

#[test]
fn the_committed_spread_says_what_it_was_measured_on_and_when() {
    let measured = &Spread::committed().expect("parses").measured;
    assert_eq!(measured.commit.len(), 40, "{}", measured.commit);
    assert!(measured.commit.bytes().all(|b| b.is_ascii_hexdigit()));
    let date: Vec<&str> = measured.date.split('-').collect();
    assert_eq!(date.len(), 3, "{}", measured.date);
    assert!(date[0].len() == 4 && date[1].len() == 2 && date[2].len() == 2);
    assert!(measured.seeds.len() >= 8, "{:?}", measured.seeds);
    assert!(!measured.measure.is_empty());
}

#[test]
fn a_spread_that_is_not_json_is_an_error_with_its_file_name() {
    assert!(Spread::parse("{").is_err());
}
