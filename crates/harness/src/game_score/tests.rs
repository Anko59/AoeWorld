use super::judge::{self, Comparison};
use super::measure::{Counters, Digest, Measurement};
use super::{Options, Plan, report, resolve_hash, worker_counts};
use clap::Parser;
use serde_json::json;
use std::path::PathBuf;

fn digest(tick: u64, hash: &str, units: u64) -> Digest {
    Digest {
        tick,
        hash: hash.to_owned(),
        units,
        buildings: 5,
    }
}

fn run(workers: usize, digests: Vec<Digest>) -> Measurement {
    Measurement {
        workers,
        ticks: 100,
        seats: 3,
        setup_seconds: 1.5,
        tick_seconds: 2.0,
        ticks_per_second: 50.0,
        slowest_tick: 2,
        tick_ms: json!({ "max_ms": 31.0 }),
        peak_memory_mib: 210,
        counters: Counters::default(),
        samples: Vec::new(),
        digests,
        order_count: None,
        order_log_hash: None,
        order_log_file: None,
    }
}

fn hashes(list: &[(u64, &str)]) -> Vec<Digest> {
    list.iter()
        .map(|(tick, hash)| digest(*tick, hash, 40))
        .collect()
}

#[test]
fn equal_hash_sequences_do_not_differ() {
    let base = hashes(&[(0, "a"), (10, "b"), (20, "c")]);
    assert_eq!(judge::first_difference(&base, &base.clone()), None);
}

#[test]
fn the_first_differing_tick_is_named_with_what_differs() {
    let base = hashes(&[(0, "a"), (10, "b"), (20, "c")]);
    let mut other = base.clone();
    other[1].hash = "x".to_owned();
    other[2].units = 41;
    let difference = judge::first_difference(&base, &other).expect("a difference");
    assert_eq!(difference.tick, 10);
    assert!(
        difference.what.contains("different world state"),
        "{}",
        difference.what
    );
    other[1].units = 39;
    let difference = judge::first_difference(&base, &other).expect("a difference");
    assert!(
        difference.what.contains("unit count 40 against 39"),
        "{}",
        difference.what
    );
}

#[test]
fn a_run_that_hashed_less_differs_where_it_stopped() {
    let long = hashes(&[(0, "a"), (10, "b"), (20, "c")]);
    let short = long[..2].to_vec();
    for (base, other) in [(&long, &short), (&short, &long)] {
        let difference = judge::first_difference(base, other).expect("a difference");
        assert_eq!(difference.tick, 20, "{difference:?}");
    }
}

#[test]
fn the_first_differing_order_is_named_with_its_tick() {
    let lines = |list: &[&str]| list.iter().map(|l| (*l).to_owned()).collect::<Vec<_>>();
    let base = lines(&[
        "5 1 1 Accepted move",
        "10 2 1 Accepted gather",
        "15 1 2 Accepted train",
    ]);
    assert_eq!(judge::first_order_difference(&base, &base.clone()), None);
    let mut other = base.clone();
    other[1] = "10 2 1 Accepted build".to_owned();
    let difference = judge::first_order_difference(&base, &other).expect("a difference");
    assert_eq!((difference.index, difference.tick), (1, Some(10)));
    let difference = judge::first_order_difference(&base, &base[..2]).expect("a short log");
    assert_eq!((difference.index, difference.tick), (2, Some(15)));
}

#[test]
fn the_game_is_the_same_when_hashes_and_orders_agree_and_counters_are_only_reported() {
    let base = run(1, hashes(&[(0, "a"), (10, "b")]));
    let mut other = run(4, hashes(&[(0, "a"), (10, "b")]));
    let same = judge::compare(&base, &other, (None, None));
    assert!(same.holds() && same.same_counters && same.same_orders.is_none());
    other.counters.route_searches = 7;
    let counters = judge::compare(&base, &other, (None, None));
    assert!(counters.holds(), "counters never decide the verdict");
    assert!(!counters.same_counters);
    let orders: Vec<String> = vec!["5 1 1 Accepted move".to_owned()];
    let other_orders: Vec<String> = vec!["5 1 1 Accepted stop".to_owned()];
    let apart = judge::compare(
        &base,
        &other,
        (Some(orders.as_slice()), Some(other_orders.as_slice())),
    );
    assert!(!apart.holds(), "different orders are different games");
    assert_eq!(apart.first_differing_tick, Some(5));
}

#[test]
fn the_table_names_every_number_in_plain_words_one_column_per_run() {
    let one = run(1, hashes(&[(0, "a"), (10, "b")]));
    let four = run(4, hashes(&[(0, "a"), (10, "x")]));
    let verdict = judge::Verdict {
        holds: false,
        against: vec![judge::compare(&one, &four, (None, None))],
        diagnosis: None,
    };
    let table = report::table("game score, early game", &[one, four], &verdict);
    for words in [
        "1 worker",
        "4 workers",
        "ticks per second",
        "slowest tick was tick number",
        "peak memory (MiB)",
        "units alive at the end",
        "villagers standing idle at the end",
        "movers waiting for a route, at the end",
        "wait for a route, 95th percentile (ticks), worst sample",
        "route orders given",
        "route searches started",
        "route orders answered when given",
        "route answers taken from the cache",
        "hits landed",
        "kills / losses",
        "resources delivered in the last game minute",
        "DIFFERENT GAMES, first differing tick 10",
    ] {
        assert!(table.contains(words), "missing {words:?} in\n{table}");
    }
    assert!(
        !table.contains("scoreboard"),
        "that word names another page"
    );
}

#[test]
fn a_comparison_of_equal_runs_reads_as_a_pass_in_words() {
    let base = run(1, hashes(&[(0, "a")]));
    let comparison: Comparison = judge::compare(&base, &run(4, hashes(&[(0, "a")])), (None, None));
    let verdict = judge::Verdict {
        holds: true,
        against: vec![comparison],
        diagnosis: None,
    };
    let table = report::table("title", &[base], &verdict);
    assert!(table.contains("1 worker and 4 workers: the same hash at every hashed tick"));
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    options: Options,
}

#[test]
fn a_childs_plan_comes_back_the_same_through_the_command_line() {
    let plan = Plan {
        package_directory: PathBuf::from("packages here"),
        map_hash: "ab".repeat(32),
        seats: 7,
        ticks: 123,
        seed: 9,
        workers: 3,
        hash_every: 4,
        sample_every: 0,
        record_orders: false,
    };
    let out = PathBuf::from("out/run.json");
    let args = plan.child_args(&out);
    assert_eq!(args[0], "game-score");
    let cli =
        Cli::try_parse_from(std::iter::once("test".to_owned()).chain(args.into_iter().skip(1)))
            .expect("the child's own arguments parse");
    assert_eq!(cli.options.child, Some(out));
    assert_eq!(cli.options.plan(&plan.map_hash, 3), plan);
}

#[test]
fn defaults_play_one_worker_against_this_machines_cores() {
    let cli = Cli::try_parse_from(["test"]).expect("the defaults parse");
    assert_eq!(cli.options.workers, vec![1, 0]);
    assert_eq!(
        worker_counts(&cli.options.workers, 6).expect("counts"),
        vec![1, 6]
    );
    assert_eq!(worker_counts(&[2, 2], 6).expect("counts"), vec![2, 2]);
    assert_eq!(worker_counts(&[0], 0).expect("counts"), vec![1]);
    assert!(worker_counts(&[], 6).is_err());
}

#[test]
fn the_only_package_of_a_directory_is_the_default_and_a_choice_is_never_guessed() {
    let directory = tempfile::tempdir().expect("directory");
    assert!(resolve_hash(directory.path(), None).is_err(), "no package");
    let (first, second) = ("ab".repeat(32), "cd".repeat(32));
    std::fs::write(directory.path().join(format!("{first}.json")), "{}").expect("package");
    std::fs::write(directory.path().join("notes.json"), "{}").expect("other file");
    assert_eq!(
        resolve_hash(directory.path(), None).expect("one package"),
        first
    );
    std::fs::write(directory.path().join(format!("{second}.json")), "{}").expect("package");
    let error = resolve_hash(directory.path(), None).expect_err("two packages");
    assert!(error.to_string().contains("--map-hash"), "{error}");
    assert_eq!(
        resolve_hash(directory.path(), Some(&second)).expect("named"),
        second
    );
}

#[test]
fn a_difference_is_replayed_at_every_tick_through_the_one_after_it_first_showed() {
    let cli = Cli::try_parse_from(["test", "--ticks", "900", "--no-orders"]).expect("parse");
    let base = run(1, hashes(&[(0, "a"), (40, "b")]));
    let other = run(4, hashes(&[(0, "a"), (40, "x")]));
    let failed = judge::compare(&base, &other, (None, None));
    let replay = super::replay_plan(&cli.options, "hash", &failed);
    assert_eq!(
        (replay.ticks, replay.hash_every, replay.workers),
        (41, 1, 1)
    );
    assert!(!replay.record_orders, "the replay keeps what the run kept");
    let same = judge::compare(&base, &base, (None, None));
    assert_eq!(super::replay_plan(&cli.options, "hash", &same).ticks, 900);
}

#[test]
fn an_order_logged_at_a_tick_is_replayed_through_the_call_that_applies_it() {
    // Orders logged at tick 37 are applied while the world goes from tick 37
    // to 38, so a replay of 37 ticks would never apply them.
    let cli = Cli::try_parse_from(["test", "--ticks", "900"]).expect("parse");
    let base = run(1, hashes(&[(0, "a"), (40, "b")]));
    let other = run(4, hashes(&[(0, "a"), (40, "x")]));
    let (a, b) = (
        vec!["37 1 1 Accepted move".to_owned()],
        vec!["37 1 1 Accepted stop".to_owned()],
    );
    let failed = judge::compare(&base, &other, (Some(a.as_slice()), Some(b.as_slice())));
    assert_eq!(failed.first_differing_tick, Some(37));
    assert_eq!(super::replay_plan(&cli.options, "hash", &failed).ticks, 38);
    let last = Cli::try_parse_from(["test", "--ticks", "37"]).expect("parse");
    assert_eq!(super::replay_plan(&last.options, "hash", &failed).ticks, 37);
}

#[test]
fn the_slowest_tick_is_counted_from_one() {
    assert_eq!(super::play::slowest_tick(&[1.0, 30.0, 2.0]), 2);
    assert_eq!(super::play::slowest_tick(&[9.0]), 1);
    assert_eq!(super::play::slowest_tick(&[]), 0);
}
