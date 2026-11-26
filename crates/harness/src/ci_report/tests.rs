//! Comment parsing, daily medians, redefinition detection, the 2% moves, the
//! reliability counts, the incremental fetch and the rendered page. Fixtures in
//! `tests/fixtures` are real CI comments (old and new table formats); every
//! GitHub call goes through a scripted seam.
mod comments;
mod fetch;
mod history;
mod page;
mod reliability;
mod series;
mod timing;

pub(super) fn fixture(name: &str) -> String {
    let path = format!(
        "{}/src/ci_report/tests/fixtures/{name}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"))
}

use super::actions::{ActionJob, ActionRun};
use super::comments::{DevSide, JobTime, Observation};
use super::history::{Accepted, Version};
use super::series::Commit;
use super::{BundleLimit, Data, Sources, assemble};
use std::collections::BTreeMap;

fn counts(rows: &[(&str, u64)]) -> BTreeMap<String, u64> {
    rows.iter()
        .map(|(row, count)| ((*row).to_owned(), *count))
        .collect()
}

fn micro(position: usize, late: u64, accepted: Option<u64>) -> Version {
    let rows = [
        ("simulation::late_game", late),
        ("simulation::late_game_main", late * 9 / 10),
        ("simulation::smoke_ticks", 50),
        ("simulation::advance", 20),
    ];
    Version {
        position,
        at: format!("2026-11-{:02}T12:00:00Z", position + 1),
        pr: Some(500 + position as u64),
        title: format!("change {position}"),
        instructions: counts(&rows),
        reference: counts(&rows),
        accepted: accepted
            .map(|dev| {
                let entry = Accepted {
                    dev,
                    reason: "caps removed; accepted".to_owned(),
                };
                BTreeMap::from([("simulation::smoke_ticks".to_owned(), entry)])
            })
            .unwrap_or_default(),
    }
}

fn action_run(id: u64, day: u32, conclusion: &str, browser: &str) -> ActionRun {
    let job = |name: &str, conclusion: &str| ActionJob {
        name: name.to_owned(),
        conclusion: Some(conclusion.to_owned()),
        step: (conclusion == "failure").then(|| "Run tests".to_owned()),
    };
    ActionRun {
        id,
        attempt: 1,
        conclusion: Some(conclusion.to_owned()),
        created_at: format!("2026-11-{day:02}T09:00:00Z"),
        jobs: vec![job("browser", browser), job("static (unit)", "success")],
        first_attempt: None,
    }
}

/// A small, complete report: six measured commits, a late-game redefinition
/// at commit 4, an accepted increase, three days of CI runs and three Actions runs.
pub(super) fn series_fixture() -> Data {
    let commits: Vec<Commit> = (0..8)
        .map(|i| Commit {
            sha: format!("{i:040x}"),
            at: format!("2026-11-{:02}T12:00:00Z", i + 1),
            subject: format!("change {i} (#{})", 500 + i),
        })
        .collect();
    let late = [100, 101, 102, 400, 404, 408];
    let mut observations: Vec<Observation> = (1..=6)
        .map(|i: usize| Observation {
            pr: 900,
            at: format!("2026-11-{:02}T13:00:00Z", i + 1),
            run: None,
            dev: Some(DevSide {
                rev: format!("{i:040x}"),
                rows: counts(&[
                    ("simulation::late_game", late[i - 1]),
                    ("simulation::late_game_main", late[i - 1] * 9 / 10),
                    ("simulation::smoke_ticks", 50 + i as u64),
                    ("simulation::advance", 20),
                    ("wasm_gzip", 380 + i as u64 * 10),
                    ("wasm_raw", 900),
                ]),
            }),
            jobs: Vec::new(),
        })
        .collect();
    for (run, day, minutes) in [(1, 5, 20), (2, 5, 30), (3, 6, 25)] {
        let job = |name: &str, minutes: u64| JobTime {
            name: name.to_owned(),
            seconds: minutes * 60,
            failed: false,
        };
        observations.push(Observation {
            pr: 901,
            at: format!("2026-11-{day:02}T15:00:00Z"),
            run: Some(run),
            dev: None,
            jobs: vec![
                job("browser", minutes),
                job("e2e (1)", minutes + 5),
                job("select", 2),
            ],
        });
    }
    assemble(Sources {
        repository: "o/r".to_owned(),
        now: "2026-11-08T08:00:00Z".to_owned(),
        commits,
        comment_versions: observations.len(),
        observations,
        micro: vec![micro(0, 100, None), micro(4, 400, Some(52))],
        bundle: vec![BundleLimit {
            position: 0,
            baseline: 400,
            limit: crate::perf::regression_floor(400) - 1,
        }],
        actions: Some(vec![
            action_run(1, 3, "success", "success"),
            action_run(2, 5, "failure", "failure"),
            action_run(3, 6, "success", "success"),
        ]),
        warnings: vec!["origin/dev was not refreshed: offline".to_owned()],
    })
}

#[test]
fn the_fixture_report_is_assembled_from_its_sources() {
    let data = series_fixture();
    assert_eq!(data.dev.len(), 6);
    assert_eq!(
        data.events.len(),
        2,
        "one redefinition and one accepted increase: {:?}",
        data.events
    );
    assert_eq!(data.ci_runs.len(), 3);
    assert_eq!(data.ci_days.len(), 2);
    assert!(data.moves.iter().any(|m| m.event.is_some()));
    assert_eq!(data.reliability.as_ref().unwrap().runs, 3);
    assert_eq!(data.inputs.measured_commits, 6);
    let json = serde_json::to_string(&data).expect("data.json serializes");
    assert!(json.contains("\"headline\""));
}
