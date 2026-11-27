use super::judge::{self, Comparison};
use super::measure::{Digest, Measurement};
use super::scenarios::{self, LATE, NATURAL};
use super::windows::{Cost, Efficiency, Minute, Service, WindowReport};
use super::{Options, Plan, counts_for, resolve_hash, worker_counts};
use clap::Parser;
use std::path::PathBuf;

pub(super) fn digest(tick: u64, hash: &str, units: u64) -> Digest {
    Digest {
        tick,
        hash: hash.to_owned(),
        units,
        buildings: 5,
    }
}

pub(super) fn hashes(list: &[(u64, &str)]) -> Vec<Digest> {
    list.iter()
        .map(|(tick, hash)| digest(*tick, hash, 40))
        .collect()
}

pub(super) fn window(id: &str, from: u64, to: u64) -> WindowReport {
    let service = Service::default();
    let cost = Cost {
        tick_seconds: 2.0,
        ..Cost::default()
    };
    WindowReport {
        id: id.to_owned(),
        from_tick: from,
        to_tick: to,
        service,
        cost,
        efficiency: Efficiency::default(),
    }
}

pub(super) fn minute(index: u64) -> Minute {
    Minute {
        minute: index,
        from_tick: (index - 1) * 600,
        to_tick: index * 600,
        service: Service::default(),
        cost: Cost::default(),
    }
}

/// A run of the `natural` scenario at `workers`, with `digests`.
pub(super) fn run(workers: usize, digests: Vec<Digest>) -> Measurement {
    Measurement {
        scenario: "natural".to_owned(),
        workers,
        ticks: 100,
        seats: 3,
        setup_seconds: 1.5,
        tick_seconds: 2.0,
        native_seconds: 4.5,
        ticks_per_second: 50.0,
        slowest_tick: 2,
        peak_memory_mib: 210,
        windows: vec![window("whole", 0, 100)],
        series: vec![minute(1)],
        digests,
        order_count: None,
        order_log_hash: None,
        order_log_file: None,
    }
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
fn the_game_is_the_same_when_hashes_and_orders_agree_and_numbers_are_only_reported() {
    let base = run(1, hashes(&[(0, "a"), (10, "b")]));
    let mut other = run(4, hashes(&[(0, "a"), (10, "b")]));
    let same = judge::compare(&base, &other, (None, None));
    assert!(same.holds() && same.same_numbers && same.same_orders.is_none());
    other.windows[0].service.planning.route_searches = 7;
    let numbers = judge::compare(&base, &other, (None, None));
    assert!(numbers.holds(), "numbers never decide the verdict");
    assert!(!numbers.same_numbers);
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
fn wall_time_and_memory_are_not_numbers_of_the_game() {
    let base = run(1, hashes(&[(0, "a")]));
    let mut slower = run(4, hashes(&[(0, "a")]));
    slower.windows[0].cost.tick_seconds = 9.0;
    slower.windows[0].cost.peak_memory_mib = 9_000;
    slower.series[0].cost.tick_seconds = 9.0;
    assert!(judge::compare(&base, &slower, (None, None)).same_numbers);
}

pub(super) fn verdict(base: &Measurement, other: &Measurement) -> (judge::Verdict, Comparison) {
    let comparison = judge::compare(base, other, (None, None));
    (
        judge::Verdict {
            holds: comparison.holds(),
            against: vec![comparison.clone()],
            diagnosis: None,
        },
        comparison,
    )
}

#[derive(Parser)]
struct Cli {
    #[command(flatten)]
    options: Options,
}

fn plan(scenario: &str) -> Plan {
    Plan {
        package_directory: PathBuf::from("packages here"),
        map_hash: "ab".repeat(32),
        scenario: scenario.to_owned(),
        seats: 7,
        ticks: 123,
        seed: 9,
        late_percent: Some(25),
        workers: 3,
        hash_every: 4,
        sample_every: 5,
        record_orders: false,
    }
}

#[test]
fn a_childs_plan_comes_back_the_same_through_the_command_line() {
    for scenario in ["natural", "late"] {
        let plan = plan(scenario);
        let out = PathBuf::from("out/run.json");
        let args = plan.child_args(&out);
        assert_eq!(args[0], "game-score");
        let cli =
            Cli::try_parse_from(std::iter::once("test".to_owned()).chain(args.into_iter().skip(1)))
                .expect("the child's own arguments parse");
        assert_eq!(cli.options.child, Some(out));
        let found = scenarios::find(scenario).expect("scenario");
        assert_eq!(cli.options.plan(found, &plan.map_hash, 3), plan);
    }
}

#[test]
fn a_plan_without_flags_takes_everything_from_its_scenario() {
    let cli = Cli::try_parse_from(["test"]).expect("the defaults parse");
    assert_eq!(cli.options.scenario, None);
    let natural = cli.options.plan(&NATURAL, "hash", 1);
    assert_eq!(
        (
            natural.ticks,
            natural.hash_every,
            natural.sample_every,
            natural.late_percent
        ),
        (
            NATURAL.ticks,
            NATURAL.hash_every,
            NATURAL.sample_every,
            None
        )
    );
    let late = cli.options.plan(&LATE, "hash", 1);
    assert_eq!(late.ticks, LATE.ticks);
    let flags = Cli::try_parse_from([
        "test",
        "--scenario",
        "late",
        "--ticks",
        "99",
        "--late-percent",
        "5",
        "--hash-every",
        "3",
    ])
    .expect("flags parse");
    let plan = flags.options.plan(&LATE, "hash", 1);
    assert_eq!(
        (plan.ticks, plan.late_percent, plan.hash_every),
        (99, Some(5), 3)
    );
    assert_eq!(plan.sample_every, LATE.sample_every);
}

#[test]
fn no_scenario_flag_plays_the_pull_request_set_and_a_flag_plays_one() {
    let anatolia = scenarios::MARCH.map().expect("staged on a map");
    let all = Cli::try_parse_from(["test"]).expect("parse");
    let ids = |options: &Options, map: &str| -> Vec<&'static str> {
        options
            .scenarios(map)
            .expect("scenarios")
            .iter()
            .map(|s| s.id)
            .collect()
    };
    assert_eq!(
        ids(&all.options, anatolia),
        ["natural", "late", "march", "battle"]
    );
    assert_eq!(ids(&all.options, "another"), ["natural", "late"]);
    let one = Cli::try_parse_from(["test", "--scenario", "late"]).expect("parse");
    assert_eq!(ids(&one.options, anatolia), ["late"]);
    let staged = Cli::try_parse_from(["test", "--scenario", "battle"]).expect("parse");
    assert_eq!(ids(&staged.options, anatolia), ["battle"]);
    let elsewhere = staged
        .options
        .scenarios("another")
        .expect_err("not its map");
    assert!(elsewhere.to_string().contains("battle"), "{elsewhere}");
    let wrong = Cli::try_parse_from(["test", "--scenario", "siege"]).expect("parse");
    let error = wrong
        .options
        .scenarios(anatolia)
        .expect_err("no such scenario");
    assert!(error.to_string().contains("siege"), "{error}");
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
fn a_scenario_that_is_not_the_same_game_at_every_count_is_played_twice_at_its_own() {
    assert_eq!(counts_for(&NATURAL, &[1, 4]), vec![1, 4]);
    let fixed = scenarios::Scenario {
        fixed_workers: Some(2),
        ..scenarios::LATE
    };
    assert_eq!(counts_for(&fixed, &[1, 4]), vec![2, 2]);
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
    let plan = Plan {
        ticks: 900,
        record_orders: false,
        workers: 1,
        ..plan("natural")
    };
    let base = run(1, hashes(&[(0, "a"), (40, "b")]));
    let other = run(4, hashes(&[(0, "a"), (40, "x")]));
    let failed = judge::compare(&base, &other, (None, None));
    let replay = plan.replay(&failed);
    assert_eq!(
        (replay.ticks, replay.hash_every, replay.workers),
        (41, 1, 1)
    );
    assert!(!replay.record_orders, "the replay keeps what the run kept");
    assert_eq!(replay.scenario, plan.scenario);
    let same = judge::compare(&base, &base, (None, None));
    assert_eq!(plan.replay(&same).ticks, 900);
}

#[test]
fn an_order_logged_at_a_tick_is_replayed_through_the_call_that_applies_it() {
    // Orders logged at tick 37 are applied while the world goes from tick 37
    // to 38, so a replay of 37 ticks would never apply them.
    let plan = Plan {
        ticks: 900,
        ..plan("natural")
    };
    let base = run(1, hashes(&[(0, "a"), (40, "b")]));
    let other = run(4, hashes(&[(0, "a"), (40, "x")]));
    let (a, b) = (
        vec!["37 1 1 Accepted move".to_owned()],
        vec!["37 1 1 Accepted stop".to_owned()],
    );
    let failed = judge::compare(&base, &other, (Some(a.as_slice()), Some(b.as_slice())));
    assert_eq!(failed.first_differing_tick, Some(37));
    assert_eq!(plan.replay(&failed).ticks, 38);
    let last = Plan {
        ticks: 37,
        ..plan.clone()
    };
    assert_eq!(last.replay(&failed).ticks, 37);
}

#[test]
fn the_slowest_tick_is_counted_from_one() {
    assert_eq!(super::play::slowest_tick(&[1.0, 30.0, 2.0]), 2);
    assert_eq!(super::play::slowest_tick(&[9.0]), 1);
    assert_eq!(super::play::slowest_tick(&[]), 0);
}
