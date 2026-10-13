use super::{
    super::{
        describe::{Level, level_section},
        evidence::{Evidence, Verdict},
        tests::{fixture, run},
    },
    Change, Decision, Levels, current_floor, numstat_lines,
    stats::{Distribution, marker},
};
use std::{collections::BTreeSet, fs};

fn committed() -> Levels {
    Levels::parse(include_str!("../../../../../gates/levels.json")).unwrap()
}

fn change(lines: u64, suites: &[&str], files: &[&str]) -> Change {
    Change {
        lines,
        files: files.iter().map(|f| (*f).to_owned()).collect(),
        suites: suites
            .iter()
            .map(|s| (*s).to_owned())
            .collect::<BTreeSet<_>>(),
    }
}

#[test]
fn the_committed_config_is_strict_and_matches_the_registry() {
    let levels = committed();
    assert_eq!(levels.spec(Level::Low).video_max_seconds, None);
    assert_eq!(levels.spec(Level::Medium).video_max_seconds, Some(60));
    assert_eq!(levels.spec(Level::High).video_max_seconds, Some(120));
    assert_eq!(levels.spec(Level::Max).video_max_seconds, Some(300));
    assert!(!levels.spec(Level::Medium).narration_required);
    assert!(levels.spec(Level::High).narration_required);
    assert!(levels.spec(Level::Max).narration_required);
    assert_eq!(levels.lowest_with_video(), Level::Medium);
    let registry: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../gates/registry.json")).unwrap();
    let ids: Vec<&str> = registry["suites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    levels.check_suites(ids.iter().copied()).unwrap();
    let error = levels.check_suites(["static"].into_iter()).unwrap_err();
    assert!(error.contains("unknown suite"), "{error}");
}

#[test]
fn a_malformed_config_is_refused() {
    let good: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../gates/levels.json")).unwrap();
    let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut value = good.clone();
        edit(&mut value);
        Levels::parse(&value.to_string()).unwrap_err()
    };
    assert!(broken(&|v| v["extra"] = 1.into()).contains("extra"));
    assert!(broken(&|v| v["version"] = 2.into()).contains("version"));
    assert!(broken(&|v| v["stats_window"] = 0.into()).contains("stats_window"));
    assert!(broken(&|v| v["levels"]["low"]["target_percent"] = 50.into()).contains("100"));
    assert!(
        broken(&|v| {
            v["levels"].as_object_mut().unwrap().remove("max");
        })
        .contains("max")
    );
    assert!(broken(&|v| v["levels"]["low"]["what_max_lines"] = 0.into()).contains("line limits"));
    assert!(broken(&|v| v["levels"]["high"]["video_max_seconds"] = 0.into()).contains("positive"));
    assert!(broken(&|v| v["levels"]["low"]["metrics"] = " ".into()).contains("metrics"));
    assert!(broken(&|v| v["floor"][0]["reason"] = "".into()).contains("reason"));
    assert!(
        Levels::parse("not json")
            .unwrap_err()
            .contains("levels.json")
    );
}

#[test]
fn the_floor_follows_size_and_what_the_change_touches() {
    let levels = committed();
    let floor = |c: Change| levels.floor(&c);
    assert_eq!(
        floor(change(60, &["docs", "static"], &["docs/a.md"])).level,
        Level::Low
    );
    let size = floor(change(61, &["docs"], &["docs/a.md"]));
    assert_eq!(size.level, Level::Medium);
    assert!(size.reason.contains("smallest PRs"));
    let product = floor(change(
        10,
        &["gameplay", "native"],
        &["crates/core/src/a.rs"],
    ));
    assert_eq!(product.level, Level::Medium);
    assert!(product.reason.contains("behaviour"));
    // The harness is `everything`: small stays medium, large needs high.
    assert_eq!(
        floor(change(199, &["everything"], &[])).level,
        Level::Medium
    );
    let harness = floor(change(200, &["everything"], &["crates/harness/src/a.rs"]));
    assert_eq!(harness.level, Level::High);
    assert!(harness.reason.contains("harness"));
    assert_eq!(
        floor(change(300, &["map"], &["crates/map/src/a.rs"])).level,
        Level::High
    );
    // Protocol is matched by path: it only selects the gameplay suite.
    assert_eq!(
        floor(change(199, &["gameplay"], &["crates/protocol/src/a.rs"])).level,
        Level::Medium
    );
    assert_eq!(
        floor(change(200, &["gameplay"], &["crates/protocol/src/a.rs"])).level,
        Level::High
    );
    assert_eq!(
        floor(change(5000, &["gameplay"], &["crates/core/src/a.rs"])).level,
        Level::Medium
    );
}

#[test]
fn binary_files_count_as_large_changes() {
    assert_eq!(numstat_lines("3\t4\ta.rs\n-\t-\tlogo.png\n", 61), 7 + 122);
    assert_eq!(numstat_lines("", 61), 0);
}

#[test]
fn a_level_below_the_floor_is_refused_with_the_reason_and_the_level_to_use() {
    use super::super::describe::Template;
    let levels = committed();
    let text = "<!-- level: low -->\n## Why\nFixes #295\n## What\nx\n";
    let low = Template::parse(text, &levels).unwrap();
    let big = change(300, &["gameplay"], &["crates/core/src/a.rs"]);
    let error = low.check(&levels, &big, None).unwrap_err();
    assert!(error.contains("below the floor `medium`"), "{error}");
    assert!(error.contains("use medium or higher"), "{error}");
    let ok = low.check(&levels, &change(5, &[], &[]), None).unwrap();
    assert_eq!(
        ok,
        Decision {
            chosen: Level::Low,
            floor: Level::Low,
            reason: "docs, configuration or a small change".into(),
            metrics: "summary".into()
        }
    );
    let medium = Template::parse(&text.replace("low", "medium"), &levels).unwrap();
    let decision = medium.check(&levels, &big, Some((30, false))).unwrap();
    assert_eq!(
        (decision.chosen, decision.floor),
        (Level::Medium, Level::Medium)
    );
    assert_eq!(decision.metrics, "standard");
}

#[test]
fn limits_come_from_the_config() {
    use super::super::describe::Template;
    let mut value: serde_json::Value =
        serde_json::from_str(include_str!("../../../../../gates/levels.json")).unwrap();
    value["levels"]["medium"]["what_max_lines"] = 1.into();
    value["levels"]["medium"]["why_max_lines"] = 2.into();
    let strict = Levels::parse(&value.to_string()).unwrap();
    let two_what = "<!-- level: medium -->\n## Why\nFixes #295\n## What\na\nb\n";
    assert!(Template::parse(two_what, &committed()).is_ok());
    assert!(
        Template::parse(two_what, &strict)
            .unwrap_err()
            .contains("more than 1 lines")
    );
    let three_why = "<!-- level: medium -->\n## Why\nFixes #295\nmore\nmore\n## What\na\n";
    assert!(Template::parse(three_why, &committed()).is_ok());
    assert!(
        Template::parse(three_why, &strict)
            .unwrap_err()
            .contains("split")
    );
}

#[test]
fn the_floor_is_computed_from_a_real_checkout() {
    let (_temp, root) = fixture("true");
    let (floor, change) = current_floor(&root).unwrap();
    assert_eq!((floor.level, change.lines), (Level::Low, 2));
    assert_eq!(change.files, vec!["README.md".to_owned()]);
    fs::write(root.join("gates/extra.txt"), "x\ny\nz\nw\nv\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "gates"]);
    let (floor, change) = current_floor(&root).unwrap();
    assert_eq!(floor.level, Level::High, "{change:?}");
    assert!(change.suites.contains("everything"));
}

#[test]
fn markers_are_read_from_descriptions() {
    assert_eq!(
        marker("<!-- level: high -->\n# For humans"),
        Some(Level::High)
    );
    assert_eq!(
        marker("# stack note\n  <!-- level: low -->  \nbody"),
        Some(Level::Low)
    );
    assert_eq!(marker("<!-- level: huge -->"), None);
    assert_eq!(marker("no marker"), None);
}

#[test]
fn the_distribution_is_reported_against_the_target() {
    let levels = committed();
    let bodies = [
        "<!-- level: low -->",
        "<!-- level: medium -->",
        "<!-- level: medium -->",
        "<!-- level: high -->",
        "old description",
    ];
    let found = Distribution::from_bodies(bodies);
    assert_eq!(found.unmarked, 1);
    assert_eq!(found.counts[&Level::Medium], 2);
    let line = found.line(&levels);
    assert!(
        line.contains("last 5 merged PRs (4 with a level marker)"),
        "{line}"
    );
    assert!(line.contains("low 25% (target 20%)"), "{line}");
    assert!(line.contains("medium 50% (target 30%)"), "{line}");
    assert!(line.contains("max 0% (target 10%)"), "{line}");
    let table = found.table(&levels);
    assert!(table.contains("| high | 1 | 25% | 40% |"), "{table}");
    assert!(table.contains("| (no marker) | 1 |"), "{table}");
    let empty = Distribution::from_bodies([]);
    assert!(empty.line(&levels).contains("low 0% (target 20%)"));
}

#[test]
fn the_level_is_recorded_in_the_evidence_and_shown_for_ai() {
    let decision = Decision {
        chosen: Level::High,
        floor: Level::Medium,
        reason: "product code".into(),
        metrics: "full".into(),
    };
    let text = serde_json::json!({
        "version": 1, "cadence": "preflight", "head": "a", "tree": "b", "branch": "c",
        "base": "d", "base_branch": "dev", "merge_base": "e", "changed": [], "gates": [],
        "verdict": "PASS", "level": decision, "started": 0, "finished": 0
    });
    let evidence: Evidence = serde_json::from_value(text).unwrap();
    assert_eq!(evidence.verdict, Verdict::Pass);
    assert_eq!(evidence.level.as_ref(), Some(&decision));
    let template = super::super::describe::Template::parse(
        "<!-- level: high -->\n## Why\nFixes #295\n## What\nx\n",
        &committed(),
    )
    .unwrap();
    let section = level_section(&template, &evidence, "Level distribution: none");
    assert!(section.contains("Chosen: `high`"), "{section}");
    assert!(
        section.contains("Floor: `medium` (product code)"),
        "{section}"
    );
    assert!(section.contains("Metrics rows: `full`"), "{section}");
    assert!(section.contains("Level distribution: none"), "{section}");
}

#[test]
fn the_embedded_limits_are_the_committed_file() {
    assert_eq!(Level::Low.video_limit(), None);
    assert_eq!(Level::Max.video_limit(), Some(300));
    assert!(!Level::Medium.voiced() && Level::High.voiced());
}

#[test]
fn the_embedded_config_parses() {
    assert!(Levels::embedded().is_some());
}

#[test]
fn a_published_description_names_its_level_in_the_gate_evidence_heading() {
    // `make ship` publishes the level there; the body-file marker is dropped.
    assert_eq!(
        marker("## How\ntext\n## Gate evidence (`852eef2045d2`, level medium)\n| gate |"),
        Some(Level::Medium)
    );
    assert_eq!(
        marker("## Gate evidence (`852eef2045d2`, level huge)"),
        None
    );
    assert_eq!(marker("## Gate evidence (`852eef2045d2`)"), None);
}
