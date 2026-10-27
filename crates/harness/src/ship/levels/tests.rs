mod checkout;

use super::{
    super::{
        describe::{Level, level_section},
        evidence::{Evidence, Verdict},
        tests::{fixture, run},
    },
    Change, Decision, Levels, Part, RowSet, current_floor, numstat_parts,
    stats::{Distribution, marker},
};
use crate::gates::registry::Registry;
use std::{collections::BTreeSet, fs};

fn committed() -> Levels {
    Levels::parse(include_str!("../../../../../gates/levels.json")).unwrap()
}

fn change(lines: u64, suites: &[&str], files: &[&str]) -> Change {
    let suites: BTreeSet<String> = suites.iter().map(|s| (*s).to_owned()).collect();
    Change {
        lines,
        parts: vec![Part {
            path: files.first().copied().unwrap_or_default().to_owned(),
            lines,
            suites,
            behaviour: true,
        }],
        ..Change::default()
    }
}

#[test]
fn browser_test_tooling_is_not_visible_but_web_code_is() {
    for path in [
        "browser/quarantine-reporter.ts",
        "browser/tests/play.spec.ts",
        "browser/playwright.config.ts",
        "browser/server.ts",
        "browser/qa-worker.mjs",
    ] {
        assert!(!change(1, &[], &[path]).visible(), "{path}");
    }
    assert!(change(1, &[], &["web/game.ts"]).visible());
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
    assert!(
        broken(&|v| v["levels"]["low"]["metrics"] = "everything".into())
            .contains("unknown variant `everything`")
    );
    assert!(broken(&|v| v["floor"][0]["reason"] = "".into()).contains("reason"));
    // A rule that counts its own lines needs the subject of "N of M changed lines".
    let counted = 3;
    assert_eq!(good["floor"][counted]["min_changed_lines"], 200);
    assert!(
        broken(&|v| {
            v["floor"][counted]
                .as_object_mut()
                .unwrap()
                .remove("subject");
        })
        .contains("needs a subject")
    );
    assert!(broken(&|v| v["floor"][counted]["subject"] = "  ".into()).contains("needs a subject"));
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

/// The change of `files` (`lines` on the first file, as one count) classified by `registry`.
fn from_files(levels: &Levels, registry: &Registry, lines: u64, files: Vec<String>) -> Change {
    let counted = files
        .into_iter()
        .enumerate()
        .map(|(index, file)| (file, if index == 0 { lines } else { 0 }))
        .collect();
    Change::from_parts(levels, registry, counted)
}

fn real(lines: u64, files: &[&str]) -> Change {
    let registry = Registry::parse(include_bytes!("../../../../../gates/registry.json")).unwrap();
    let files = files.iter().map(|f| (*f).to_owned()).collect();
    from_files(&committed(), &registry, lines, files)
}

#[test]
fn a_test_only_change_may_be_low_but_a_large_one_still_may_not() {
    let levels = committed();
    let floor = |lines, files: &[&str]| levels.floor(&real(lines, files)).level;
    // The two real cases that `make ship` refused at `low`.
    let probe = "crates/rendering/src/surface_mesh/tests/junction.rs";
    let stub = "crates/harness/src/ship/issues/tests/stub.rs";
    assert_eq!(floor(14, &[probe]), Level::Low);
    assert_eq!(floor(7, &[stub]), Level::Low);
    assert_eq!(
        floor(14, &["crates/rendering/src/surface_mesh/junction.rs"]),
        Level::Medium
    );
    // Product code next to a test still needs medium; size still counts.
    assert_eq!(floor(14, &[probe, "crates/core/src/a.rs"]), Level::Medium);
    assert_eq!(floor(61, &[probe]), Level::Medium);
    assert_eq!(floor(200, &[stub]), Level::High);
}

#[test]
fn docs_and_configuration_edits_may_be_low_but_harness_code_may_not() {
    let levels = committed();
    let floor = |lines, files: &[&str]| levels.floor(&real(lines, files)).level;
    for path in [
        "docs/shipping.md",
        "AGENTS.md",
        "crates/harness/AGENTS.md",
        "CLAUDE.md",
        "skills/shipping/SKILL.md",
        "make/ship.mk",
        ".github/workflows/ci.yml",
        "gates/levels.json",
    ] {
        assert_eq!(floor(10, &[path]), Level::Low, "{path}");
    }
    assert_eq!(
        floor(10, &["crates/harness/src/ship/mod.rs"]),
        Level::Medium
    );
    assert_eq!(floor(10, &[".claude/settings.json"]), Level::Medium);
    assert_eq!(floor(200, &["gates/levels.json"]), Level::High);
    assert_eq!(floor(200, &["make/ship.mk"]), Level::High);
}

#[test]
fn binary_files_count_as_large_changes_and_removed_lines_are_free() {
    // `git diff --numstat -z`: each record ends with a NUL.
    assert_eq!(
        numstat_parts("3\t4\ta.rs\0-\t-\tlogo.png\u{0}0\t900\tgone.rs\0", 61),
        vec![
            ("a.rs".to_owned(), 3),
            ("logo.png".to_owned(), 61),
            ("gone.rs".to_owned(), 0)
        ]
    );
    assert!(numstat_parts("", 61).is_empty());
}

#[test]
fn numstat_paths_are_raw_even_when_git_would_quote_them() {
    // Without `-z` Git prints this path as `"gates/\303\251 \"q\".txt"`.
    assert_eq!(
        numstat_parts("250\t0\tgates/é \"q\".txt\0", 61),
        vec![("gates/é \"q\".txt".to_owned(), 250)]
    );
}

/// A change of `(path, lines)` pairs, classified like a real one.
fn counted(files: &[(&str, u64)]) -> Change {
    let registry = Registry::parse(include_bytes!("../../../../../gates/registry.json")).unwrap();
    let files = files
        .iter()
        .map(|(path, lines)| ((*path).to_owned(), *lines))
        .collect();
    Change::from_parts(&committed(), &registry, files)
}

#[test]
fn only_the_lines_in_the_sensitive_paths_count_against_high() {
    let levels = committed();
    let big = counted(&[
        ("crates/harness/src/dev.rs", 18),
        ("crates/client/src/a.rs", 900),
    ]);
    let floor = levels.floor(&big);
    assert_eq!(floor.level, Level::Medium);
    assert_eq!(
        floor.reason,
        "harness, gates, protocol or map/geodata lines: 18 of 918 changed lines (need 200 for high); product code that changes behaviour needs at least medium"
    );
    let harness = counted(&[
        ("crates/harness/src/dev.rs", 200),
        ("crates/client/src/a.rs", 10),
    ]);
    let floor = levels.floor(&harness);
    assert_eq!(floor.level, Level::High);
    assert_eq!(
        floor.reason,
        "harness, gates, protocol or map/geodata lines: 200 of 210 changed lines (need 200 for high)"
    );
    // Lines in several sensitive paths add up; one file counts once.
    let both = counted(&[("crates/harness/src/a.rs", 150), ("gates/x.json", 150)]);
    assert_eq!(levels.floor(&both).level, Level::High);
    assert_eq!(
        levels
            .floor(&counted(&[("crates/harness/src/a.rs", 199)]))
            .level,
        Level::Medium
    );
    // A baseline moves a gate: its own reason, not the behaviour one.
    let baseline = counted(&[
        ("baselines/perf/wasm.json", 8),
        ("docs/performance.md", 5),
        ("crates/harness/src/ship/issues/tests/stub.rs", 1),
    ]);
    let floor = levels.floor(&baseline);
    assert_eq!(floor.level, Level::Medium);
    // A baseline is in `everything`, so the high rule counts its 8 lines too.
    assert_eq!(
        floor.reason,
        "harness, gates, protocol or map/geodata lines: 9 of 14 changed lines (need 200 for high); a changed baseline moves a gate: at least medium"
    );
    let alone = counted(&[("baselines/perf/wasm.json", 8)]);
    assert_eq!(
        levels.floor(&alone).reason,
        "a changed baseline moves a gate: at least medium"
    );
}

#[test]
fn the_floor_of_a_dirty_tree_is_the_floor_after_the_commit() {
    let (_temp, root) = fixture("true");
    let levels = Levels::load(&root).unwrap();
    let clean = Change::collect_working(&root, &levels, "dev", false).unwrap();
    assert_eq!((levels.floor(&clean).level, clean.lines), (Level::Low, 1));
    // A new, untracked gate file of 200 lines is in the change before its commit.
    fs::write(root.join("gates/new.txt"), "x\n".repeat(200)).unwrap();
    let dirty = Change::collect_working(&root, &levels, "dev", false).unwrap();
    assert_eq!(dirty.lines, 201);
    assert_eq!(levels.floor(&dirty).level, Level::High, "{dirty:?}");
    // The committed view does not see it until it is committed.
    let committed_only = Change::collect(&root, &levels, "dev", false).unwrap();
    assert_eq!(committed_only.lines, 1);
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "harness"]);
    let after = Change::collect(&root, &levels, "dev", false).unwrap();
    assert_eq!(levels.floor(&after), levels.floor(&dirty));
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
            metrics: RowSet::Summary
        }
    );
    let medium = Template::parse(&text.replace("low", "medium"), &levels).unwrap();
    let decision = medium.check(&levels, &big, Some((30, false))).unwrap();
    assert_eq!(
        (decision.chosen, decision.floor),
        (Level::Medium, Level::Medium)
    );
    assert_eq!(decision.metrics, RowSet::Standard);
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
    assert_eq!((floor.level, change.lines), (Level::Low, 1));
    let paths: Vec<&str> = change.parts.iter().map(|part| part.path.as_str()).collect();
    assert_eq!(paths, ["README.md"]);
    fs::write(root.join("gates/extra.txt"), "x\ny\nz\nw\nv\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "gates"]);
    let (floor, change) = current_floor(&root).unwrap();
    assert_eq!(floor.level, Level::High, "{change:?}");
    assert!(
        change
            .parts
            .iter()
            .any(|part| part.suites.contains("everything"))
    );
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
        metrics: RowSet::Full,
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
