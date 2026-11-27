//! Time under a name the live stats do not know is time nobody sees: in one log
//! `steward_economy` hid 6.2 of the 10.9 seconds of the first 20 ticks. Every
//! recorded name is shown or left out on purpose, and a tick phase that is
//! neither still counts, in `other`.
use super::*;
use crate::GameplayService;
use aoe_core::{PlayerId, Seed};
use aoe_simulation::{
    BuildingKind, GameWorld, ISSUER_SPANS, PLANNING_SPANS, PlanningProbe, STEWARD_SPANS, UnitKind,
};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The one list of names the live stats leave out on purpose, with the
/// replay-only `ISSUER_SPANS` and `STEWARD_SPANS`. Each runs inside a shown
/// phase, so showing it in `other` would count its time twice. These four
/// time single units' queries, for the benchmarks.
const IGNORED: [&str; 4] = [
    "arrival_search",
    "avoidance_query",
    "body_sweep",
    "settling",
];

/// Simulation calls whose span name is computed, by path below its `src` and
/// argument. Their names come from the `steward_span` calls, from
/// `ISSUER_SPANS` and from `PLANNING_SPANS`.
const COMPUTED: [&str; 3] = [
    "game_world/economy/steward/rules.rs: name",
    "game_world/orders/diagnostics.rs: ISSUER_SPANS[issuer as usize]",
    "game_world/route_scheduler/batch.rs: name",
];

fn ignored() -> BTreeSet<&'static str> {
    IGNORED
        .into_iter()
        .chain(ISSUER_SPANS)
        .chain(STEWARD_SPANS)
        .collect()
}

/// Product sources below `directory`; test files record no live span.
fn sources(directory: &Path, found: &mut Vec<(PathBuf, String)>) {
    for entry in std::fs::read_dir(directory).expect("a source directory") {
        let path = entry.expect("a source entry").path();
        let name = path.file_name().expect("a name");
        let name = name.to_string_lossy().into_owned();
        if path.is_dir() {
            if name != "tests" {
                sources(&path, found);
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            let source = std::fs::read_to_string(&path).expect("a source file");
            found.push((path, source));
        }
    }
}

fn literal(text: &str) -> Option<String> {
    Some(text.strip_prefix('"')?.strip_suffix('"')?.to_owned())
}

/// The first argument, as written, of every `call` in `source`.
fn arguments(source: &str, call: &str) -> Vec<String> {
    source
        .split(call)
        .skip(1)
        .filter_map(|rest| Some(rest.split([',', ')']).next()?.trim().to_owned()))
        .collect()
}

/// The names of the one-line match arms `prefix…Variant => "name",`.
fn arms(source: &str, prefix: &str) -> Vec<String> {
    source
        .split(prefix)
        .skip(1)
        .filter_map(|rest| rest.lines().next()?.split_once(" => "))
        .filter_map(|(_, name)| literal(name.trim_end_matches(',')))
        .collect()
}

/// Every name written where the server or the simulation records one, and
/// the simulation's calls with a computed name.
fn recorded() -> (BTreeSet<String>, BTreeSet<String>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let (mut names, mut computed) = (BTreeSet::new(), BTreeSet::new());
    let mut server = Vec::new();
    sources(&root.join("src"), &mut server);
    for (_, source) in &server {
        for call in [".phase(", ".span("] {
            names.extend(arguments(source, call).iter().filter_map(|a| literal(a)));
        }
        // The server names what `advance_with_probe` reports.
        names.extend(arms(source, "TickPhase::"));
    }
    let below = root.join("../simulation/src");
    let mut simulation = Vec::new();
    sources(&below, &mut simulation);
    for (path, source) in &simulation {
        let file = path.strip_prefix(&below).expect("a path below src");
        for argument in arguments(source, ".movement_span(") {
            match literal(&argument) {
                Some(name) => names.insert(name),
                None => computed.insert(format!("{}: {argument}", file.display())),
            };
        }
        let steward = arguments(source, ".steward_span(");
        names.extend(steward.iter().filter_map(|a| literal(a)));
    }
    let navigation = root.join("../map/src/terrain/walk_cache/probe.rs");
    let navigation = std::fs::read_to_string(navigation).expect("NavigationPhase::name");
    names.extend(arms(&navigation, "Self::"));
    (names, computed)
}

#[test]
fn every_recorded_name_is_shown_by_the_live_stats_or_left_out_on_purpose() {
    let (names, computed) = recorded();
    for known in [
        "sessions",
        "steward_economy",
        "route_cache",
        "propose",
        "steward_walk",
        "terrain",
    ] {
        assert!(names.contains(known), "the scan lost {known}: {names:?}");
    }
    let unread: Vec<_> = computed
        .iter()
        .filter(|call| !COMPUTED.contains(&call.as_str()))
        .collect();
    assert!(
        unread.is_empty(),
        "name these spans at the call or say where their names come from: {unread:?}"
    );
    let shown = BTreeSet::from(phases::NAMES);
    let ignored = ignored();
    assert_eq!(shown.len(), phases::NAMES.len(), "a name is listed twice");
    assert!(
        shown.is_disjoint(&ignored),
        "a name is both shown and left out"
    );
    let missing: BTreeSet<_> = names
        .iter()
        .map(String::as_str)
        .chain(PLANNING_SPANS)
        .filter(|name| !shown.contains(name) && !ignored.contains(name))
        .collect();
    assert!(
        missing.is_empty(),
        "recorded, but neither in NAMES nor left out on purpose: {missing:?}"
    );
}

/// Guards the sum: a nested span put in `other` would be counted twice.
#[test]
fn spans_left_out_on_purpose_open_no_clock() {
    let probe = TickProbe::default();
    for name in ignored() {
        assert!(probe.movement_span(name).is_none(), "{name}");
    }
}

#[tokio::test]
async fn ticks_with_a_steward_spend_time_in_its_economy_and_none_in_other() {
    let owner = PlayerId(0);
    let (mut world, primary) = GameWorld::steward_territory_demo(Seed(7)).unwrap();
    let steward = world
        .units()
        .find(|unit| unit.kind == UnitKind::Steward)
        .unwrap()
        .id;
    let town_center = world
        .buildings()
        .find(|b| {
            b.player == owner && b.kind == BuildingKind::TownCenter && b.footprint.min.x == 100
        })
        .unwrap()
        .id;
    world.issue_garrison(owner, steward, town_center).unwrap();
    for _ in 0..80 {
        world.advance();
    }
    assert!(
        world.steward_status(steward).is_some(),
        "the Steward runs its Town Center"
    );
    let service = GameplayService::from_scale_world_slots(world, [(owner, primary, true)]);
    // Several visits of the Steward: the demo runs at 5 ticks a second.
    for _ in 0..20 {
        service.tick().await;
    }
    let parts = service.take_tick_phase_micros();
    assert!(parts["steward_economy"] > 0, "{parts:?}");
    assert_eq!(parts["other"], 0, "{parts:?}");
}

/// Top-level names only: a listed name nested in another overlaps it.
#[test]
fn unknown_names_are_summed_into_other_so_the_parts_add_up_to_the_tick() {
    let probe = TickProbe::default();
    for (name, micros) in [
        ("movement", 30),
        ("not_listed", 5),
        ("steward_economy", 11),
        ("not_listed_either", 7),
    ] {
        probe.phase(name, Duration::from_micros(micros));
    }
    let stats = TickStats::default();
    for tick in 1..=2 {
        stats.record(
            (0, tick),
            Duration::from_micros(53),
            Duration::ZERO,
            Duration::from_millis(50),
            &probe,
            12,
        );
    }
    // What `GET /match/stats` answers.
    let page = serde_json::to_value(stats.page(0)).unwrap();
    let sum = |row: usize| {
        page["ticks"][row][8]
            .as_object()
            .unwrap()
            .values()
            .filter_map(serde_json::Value::as_u64)
            .sum::<u64>()
    };
    assert_eq!(page["ticks"][0][8]["other"], 12, "both unknown names");
    assert_eq!(sum(0), 53, "the parts add up to the tick");
    assert_eq!(page["ticks"][1][8]["other"], 0, "drained between ticks");
    assert_eq!(sum(1), 0);
    let summary = stats.summary(Duration::from_millis(50));
    assert_eq!(summary["phases"]["other"]["max_us"], 12);
}

#[test]
fn the_slow_tick_line_shows_other_added_up_before_rounding() {
    let probe = TickProbe::default();
    probe.phase("not_listed", Duration::from_nanos(600));
    probe.phase("not_listed_either", Duration::from_nanos(600));
    // The `phases_us = ?phases` field of "slow gameplay tick".
    let line = format!("{:?}", probe.phases.drain());
    assert_eq!(line, r#"{"other": 1}"#);
}
