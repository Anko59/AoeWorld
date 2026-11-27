use super::*;
use crate::{
    gates::registry::{Cadence, Registry},
    native::affected::Selection,
};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

mod local;
mod table;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn registry() -> Registry {
    Registry::load(&root()).expect("registry")
}

fn rules() -> Rules {
    Rules::load(&root(), &registry()).expect("ci rules")
}

/// The Rust sources of `dir`, except the tests of the selection itself: they name the
/// probe paths of the table, which would make the harness "read" every one of them.
fn sources(dir: &Path, out: &mut String) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for path in entries {
        let name = path.to_string_lossy();
        if name.contains("gates/select/tests") || name.ends_with("gates/tests.rs") {
            continue;
        }
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n != "target") {
                sources(&path, out);
            }
        } else if path.extension().is_some_and(|e| e == "rs")
            && let Ok(text) = std::fs::read_to_string(&path)
        {
            out.push_str(&text);
        }
    }
}

/// The real workspace graph, with sources, loaded once: every test measures real crates.
fn graph() -> &'static Graph {
    static GRAPH: OnceLock<Graph> = OnceLock::new();
    GRAPH.get_or_init(|| {
        let mut graph = Graph::load(&root(), &rules(), false).expect("crate graph");
        for package in &mut graph.packages {
            sources(&root().join(&package.dir), &mut package.text);
        }
        graph
    })
}

fn paths(items: &[&str]) -> Vec<String> {
    items.iter().map(|p| (*p).to_owned()).collect()
}

fn decide_for(cadence: Cadence, items: &[&str]) -> Decision {
    let registry = registry();
    let eligible = eligible(&registry, cadence);
    decide(&rules(), graph(), &registry, &eligible, &paths(items))
}

fn pull_request(items: &[&str]) -> Decision {
    decide_for(Cadence::Pr, items)
}

fn running(decision: &Decision) -> BTreeSet<&str> {
    decision
        .jobs
        .iter()
        .filter(|(_, runs)| **runs)
        .map(|(job, _)| job.as_str())
        .collect()
}

#[test]
fn a_path_no_rule_covers_selects_everything() {
    for path in ["unlisted/file.txt", "a-new-top-level-file"] {
        let decision = pull_request(&[path]);
        assert!(
            matches!(decision.unit.selection, Selection::All { .. }),
            "{path}"
        );
        assert!(
            decision.jobs["e2e"] && decision.jobs["fuzz-smoke"],
            "{path}"
        );
        assert!(
            decision.reasons["browser"].contains("no ci rule covers"),
            "{path}"
        );
    }
}

#[test]
fn no_changed_path_selects_everything_the_cadence_runs() {
    let decision = pull_request(&[]);
    assert!(matches!(decision.unit.selection, Selection::All { .. }));
    assert_eq!(
        running(&decision),
        BTreeSet::from([
            "static",
            "browser",
            "e2e",
            "target-performance",
            "fuzz-smoke"
        ])
    );
    assert!(decision.reasons["e2e"].starts_with("runs because everything is selected"));
}

#[test]
fn post_merge_jobs_skip_pull_requests_and_run_on_pushes() {
    let post_merge = ["native-coverage", "qualification", "exhaustive"];
    let docs = pull_request(&["docs/new-guide.md"]);
    let push = decide_for(Cadence::Ci, &[]);
    for job in post_merge {
        assert!(!docs.jobs[job], "{job} must not run on a docs pull request");
        assert!(docs.reasons[job].contains("after merge only"), "{job}");
        assert!(push.jobs[job], "{job} must run on a push to dev");
    }
    assert!(
        push.jobs.values().all(|runs| *runs),
        "a push runs every job"
    );
    assert!(matches!(push.unit.selection, Selection::All { .. }));
    let mixed_pull_request = pull_request(&["Cargo.lock"]);
    for job in post_merge {
        assert!(!mixed_pull_request.jobs[job], "{job}");
    }
}

#[test]
fn every_job_the_registry_runs_on_pull_requests_has_a_rule_or_is_static() {
    let registry = registry();
    let rules = rules();
    for job in eligible(&registry, Cadence::Pr) {
        assert!(
            job == "static" || rules.jobs.contains_key(&job),
            "{job} runs on pull requests but no ci rule narrows it"
        );
    }
}

#[test]
fn every_tracked_path_has_a_rule() {
    let output = Command::new("git")
        .current_dir(root())
        .args(["ls-files", "-z"])
        .output()
        .expect("git");
    assert!(output.status.success(), "git ls-files failed");
    let rules = rules();
    let graph = graph();
    let mut unknown = BTreeSet::new();
    for path in String::from_utf8(output.stdout).expect("utf-8").split('\0') {
        if !path.is_empty() && rules.classify(path, |p| graph.owner(p)) == rules::Class::Unknown {
            unknown.insert(path.split('/').next().unwrap_or(path).to_owned());
        }
    }
    assert!(
        unknown.is_empty(),
        "no gates/ci-rules.json rule covers these top-level paths: {unknown:?}"
    );
}

#[test]
fn the_static_job_maps_each_gate_to_one_leg() {
    let rules = rules();
    for gate in &registry().jobs["static"] {
        assert!(rules.part_of(gate).is_some(), "{gate}");
    }
    assert_eq!(rules.part_of("lint"), Some("checks"));
    assert_eq!(rules.part_of("test-unit"), Some("unit"));
    assert_eq!(rules.part_of("docs-check"), Some("light"));
}

#[test]
fn rules_reject_unknown_jobs_unmapped_gates_and_bad_globs() {
    let registry = registry();
    let text = std::fs::read_to_string(root().join("gates/ci-rules.json")).expect("rules");
    let parse = |text: &str| Rules::parse(text.as_bytes(), Some(&registry));
    parse(&text).expect("committed rules");
    let renamed = text.replace("\"fuzz-smoke\": {", "\"fuzz-smoker\": {");
    assert!(parse(&renamed).is_err(), "a job the registry lacks");
    let unmapped = text.replace("\"test-unit\", ", "");
    assert!(parse(&unmapped).is_err(), "a static gate no leg runs");
    let bad_glob = text.replace("\"deny.toml\"", "\"deny[1].toml\"");
    assert!(parse(&bad_glob).is_err(), "an invalid glob");
    assert!(parse(&text.replace("\"version\": 1", "\"version\": 2")).is_err());
    assert!(
        parse(&text.replace(
            "\"effect\": \"outside-workspace\"",
            "\"effect\": \"static\""
        ))
        .is_err()
    );
}

#[test]
fn the_scope_the_select_job_hands_over_survives_the_environment() {
    let decision = pull_request(&["crates/client/src/lib.rs"]);
    let json = serde_json::to_string(&decision.unit).expect("json");
    let env = |all: &'static str| {
        let json = json.clone();
        move |name: &str| match name {
            "AOE_UNIT_SCOPE" => Some(json.clone()),
            "AOE_TEST_ALL" => Some(all.to_owned()),
            _ => None,
        }
    };
    assert_eq!(
        scope_from_env(env("")).expect("scope"),
        Some(decision.unit.clone())
    );
    assert_eq!(
        scope_from_env(env("0")).expect("scope"),
        Some(decision.unit.clone())
    );
    assert_eq!(scope_from_env(env("1")).expect("forced"), None);
    assert_eq!(scope_from_env(|_| None).expect("no scope"), None);
    let broken = |name: &str| (name == "AOE_UNIT_SCOPE").then(|| "{\"all\":1}".to_owned());
    assert!(
        scope_from_env(broken).is_err(),
        "a bad scope must not fall back to less testing"
    );
}

#[test]
fn builds_follow_normal_edges_and_tests_add_the_subjects_dev_dependencies() {
    let package = |name: &str, deps: &str| {
        let deps: Vec<String> = deps
            .split_whitespace()
            .map(|d| match d.strip_prefix("dev:") {
                Some(dev) => format!(r#"{{"name":"{dev}","kind":"dev"}}"#),
                None => format!(r#"{{"name":"{d}","kind":null}}"#),
            })
            .collect();
        format!(
            r#"{{"name":"{name}","manifest_path":"/w/crates/{name}/Cargo.toml","dependencies":[{}]}}"#,
            deps.join(",")
        )
    };
    let json = format!(
        r#"{{"workspace_root":"/w","packages":[{},{},{},{}]}}"#,
        package("app", "lib dev:fixture"),
        package("lib", "base"),
        package("base", ""),
        package("fixture", "dev:other"),
    );
    let graph = Graph::from_metadata(&json, &[]).expect("graph");
    let build = graph.closure("app", false);
    assert_eq!(
        build,
        BTreeSet::from(["app", "lib", "base"].map(str::to_owned))
    );
    let tests = graph.closure("app", true);
    assert!(tests.contains("fixture"));
    assert!(
        !tests.contains("other"),
        "a dev-dependency's own dev-dependencies are not built"
    );
}
