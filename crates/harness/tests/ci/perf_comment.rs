//! Exercise the real print command with scripted GitHub responses, without writes.
use super::{command, proof};
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

const GH: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$PERF_FIXTURE/calls"
case "$1 $2" in
  'api repos/o/r/actions/workflows/ci.yml/runs?'*)
    page=${2##*&page=}
    cat "$PERF_FIXTURE/page-$page.json" ;;
  'api repos/o/r/actions/runs/'*)
    id=${2#repos/o/r/actions/runs/}
    id=${id%%/*}
    cat "$PERF_FIXTURE/jobs-$id.json" ;;
  'run download')
    id=$3
    [ "$4 $5 $6 $7 $8" = '--repo o/r --name performance-report --dir' ] || exit 2
    [ -f "$PERF_FIXTURE/report-$id.json" ] || exit 1
    mkdir -p "$9"
    cp "$PERF_FIXTURE/report-$id.json" "$9/ci.json"
    cp "$PERF_FIXTURE/wasm-$id" "$9/optimized.wasm" ;;
  *) echo "unexpected gh request: $*" >&2; exit 2 ;;
esac
"#;

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache");
        fs::create_dir_all(&cache).unwrap();
        let fixture = Self(tempfile::tempdir_in(cache).unwrap());
        let root = fixture.0.path();
        for args in [
            vec!["init", "--quiet"],
            vec!["remote", "add", "origin", "https://github.com/o/r.git"],
        ] {
            assert!(
                Command::new("git")
                    .current_dir(root)
                    .args(args)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(root.join("gh"), GH).unwrap();
        fs::set_permissions(root.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        fixture.job(999, Some("success"));
        fixture.report(999);
        fixture
    }

    fn page(&self, page: usize, runs: &[(u64, &str)]) {
        let runs: Vec<_> = runs
            .iter()
            .map(|(id, conclusion)| {
                json!({
                    "id": id, "conclusion": conclusion, "event": "push", "head_branch": "dev"
                })
            })
            .collect();
        fs::write(
            self.0.path().join(format!("page-{page}.json")),
            json!({"workflow_runs": runs}).to_string(),
        )
        .unwrap();
    }

    fn job(&self, id: u64, conclusion: Option<&str>) {
        fs::write(
            self.0.path().join(format!("jobs-{id}.json")),
            json!({"jobs": [
                {"name": "static", "conclusion": "success"},
                {"name": "target-performance", "conclusion": conclusion}
            ]})
            .to_string(),
        )
        .unwrap();
    }

    fn report(&self, id: u64) {
        fs::write(
            self.0.path().join(format!("report-{id}.json")),
            json!({
                "version": 1, "revision": format!("revision-{id}"),
                "workloads": [{"scenario": "smoke", "tick_duration_ns": [id, id]}],
                "comparisons": [{"metric": "instructions::kernel", "observed": 7}]
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            self.0.path().join(format!("wasm-{id}")),
            vec![0; id as usize],
        )
        .unwrap();
    }

    fn run(&self) -> (String, Vec<String>) {
        let root = self.0.path();
        let output = proof(
            command(root, "perf-comment")
                .env(
                    "PATH",
                    format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
                )
                .env("PERF_FIXTURE", root)
                .env("TMPDIR", root)
                .env("AOE_RUN_ID", "999")
                .env("PERF_COMMENT_PRINT", "1")
                .env_remove("AOE_PR_NUMBER"),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let calls = fs::read_to_string(root.join("calls"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect();
        (String::from_utf8(output.stdout).unwrap(), calls)
    }
}

fn pages(calls: &[String]) -> Vec<&str> {
    calls
        .iter()
        .filter(|call| call.starts_with("api repos/o/r/actions/workflows/"))
        .map(String::as_str)
        .collect()
}

fn downloads(calls: &[String]) -> Vec<u64> {
    calls
        .iter()
        .filter_map(|call| {
            call.strip_prefix("run download ")?
                .split_whitespace()
                .next()?
                .parse()
                .ok()
        })
        .collect()
}

#[test]
fn perf_comment_uses_cancelled_and_failed_runs_with_passed_jobs_newest_first() {
    let f = Fixture::new();
    let first: Vec<_> = (111..=120)
        .rev()
        .map(|id| (id, if id >= 119 { "success" } else { "cancelled" }))
        .collect();
    f.page(1, &first);
    f.page(
        2,
        &[
            (110, "cancelled"),
            (109, "failure"),
            (108, "cancelled"),
            (107, "success"),
        ],
    );
    for (id, _) in &first {
        f.job(*id, Some("failure"));
        // Even an artifact cannot rescue a failed/skipped job.
        f.report(*id);
    }
    f.job(119, Some("skipped"));
    f.job(118, Some("success"));
    fs::remove_file(f.0.path().join("report-118.json")).unwrap();
    // Absent and still-running performance jobs cannot qualify either.
    fs::write(
        f.0.path().join("jobs-117.json"),
        r#"{"jobs":[{"name":"static","conclusion":"success"}]}"#,
    )
    .unwrap();
    f.job(116, None);
    for id in [115, 114, 110, 109, 108, 107] {
        f.job(id, Some("success"));
        f.report(id);
    }
    let (body, calls) = f.run();
    assert!(
        body.contains("115](https://github.com/o/r/actions/runs/115) (revision `revision-115`)")
    );
    assert!(body.contains("| Optimized WASM raw (bytes) | 115 | 999 |"));
    assert!(body.contains("latest `dev` run whose `target-performance` job passed"));
    assert!(body.contains("across its last 5 runs whose `target-performance` job passed"));
    assert!(body.contains("| Tick p50, smoke (ns) | 115 | 999 |"));
    assert_eq!(downloads(&calls), [999, 118, 115, 114, 110, 109, 108]);
    assert_eq!(
        pages(&calls),
        [
            "api repos/o/r/actions/workflows/ci.yml/runs?branch=dev&event=push&per_page=10&page=1",
            "api repos/o/r/actions/workflows/ci.yml/runs?branch=dev&event=push&per_page=10&page=2",
        ]
    );
    assert!(!calls.iter().any(|call| call.contains("runs/107/")));
    assert!(
        !calls
            .iter()
            .any(|call| call.contains("--method") || call.starts_with("pr "))
    );
}

#[test]
fn perf_comment_stops_at_noise_count_inside_the_first_page() {
    let f = Fixture::new();
    let runs: Vec<_> = (1..=10).rev().map(|id| (id, "cancelled")).collect();
    f.page(1, &runs);
    for (id, _) in runs {
        f.job(id, Some("success"));
        f.report(id);
    }
    let (body, calls) = f.run();
    assert!(body.contains("revision-10"));
    assert_eq!(downloads(&calls), [999, 10, 9, 8, 7, 6]);
    assert_eq!(pages(&calls).len(), 1);
    assert!(!calls.iter().any(|call| call.contains("runs/5/")));
}

#[test]
fn perf_comment_stops_after_a_short_page_without_an_artifact() {
    let f = Fixture::new();
    f.page(1, &[(1, "cancelled")]);
    f.job(1, Some("success"));
    let (body, calls) = f.run();
    assert_eq!(downloads(&calls), [999, 1]);
    assert!(body.contains("no recent `dev` run whose `target-performance` job passed uploaded"));
    assert!(body.contains("| Optimized WASM raw (bytes) | — | 999 |"));
    assert_eq!(pages(&calls).len(), 1);
}

#[test]
fn perf_comment_search_is_bounded_at_fifty_candidates() {
    let f = Fixture::new();
    for page in 1..=5 {
        let runs: Vec<_> = (0..10)
            .map(|offset| ((60 - page * 10 - offset) as u64, "cancelled"))
            .collect();
        f.page(page, &runs);
        for (id, _) in runs {
            f.job(id, Some("skipped"));
        }
    }
    let (body, calls) = f.run();
    assert!(body.contains("`dev` values are missing"));
    assert_eq!(pages(&calls).len(), 5);
    assert_eq!(
        calls
            .iter()
            .filter(|call| call.starts_with("api repos/o/r/actions/runs/"))
            .count(),
        51
    );
    assert_eq!(downloads(&calls), [999]);
}
