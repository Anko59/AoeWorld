//! One non-blocking observation of CI on the commit ship actually pushed.
use super::{Result, api};
use crate::ship::git;
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Runs {
    workflow_runs: Vec<Run>,
}

#[derive(Deserialize)]
struct Run {
    id: u64,
    head_sha: String,
    html_url: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct Jobs {
    jobs: Vec<Job>,
}

#[derive(Deserialize)]
struct Job {
    name: String,
    status: String,
    conclusion: Option<String>,
}

#[derive(Default)]
struct State {
    run: Option<Run>,
    passed: usize,
    running: usize,
    failed: Vec<String>,
}

fn failed(conclusion: Option<&str>) -> bool {
    conclusion.is_some_and(|c| !matches!(c, "success" | "neutral" | "skipped"))
}

fn read(root: &Path, head: &str) -> Result<State> {
    crate::timeline::stage(
        root,
        "ci-state",
        serde_json::json!({"pushed_head":head}),
        || {
            let repository = git::origin_repository(root)?;
            let path = format!(
                "repos/{repository}/actions/workflows/ci.yml/runs?head_sha={head}&per_page=100"
            );
            let runs: Runs = serde_json::from_value(api(root, &[&path])?)?;
            // Ignore obsolete runs of the same head after a reship/reopen; also
            // verify the returned head rather than trusting the API filter alone.
            let run = runs
                .workflow_runs
                .into_iter()
                .filter(|r| r.head_sha == head)
                .max_by_key(|r| r.id);
            let Some(run) = run else {
                return Ok(State::default());
            };
            let path = format!(
                "repos/{repository}/actions/runs/{}/jobs?filter=latest&per_page=100",
                run.id
            );
            let pages: Vec<Jobs> =
                serde_json::from_value(api(root, &["--paginate", "--slurp", &path])?)?;
            let mut state = State::default();
            for job in pages.into_iter().flat_map(|page| page.jobs) {
                if job.status != "completed" || job.conclusion.is_none() {
                    state.running += 1;
                } else if failed(job.conclusion.as_deref()) {
                    state.failed.push(job.name);
                } else {
                    state.passed += 1;
                }
            }
            if state.failed.is_empty() && failed(run.conclusion.as_deref()) {
                state.failed.push(format!(
                    "CI workflow ({})",
                    run.conclusion.as_deref().unwrap_or_default()
                ));
            }
            state.run = Some(run);
            Ok(state)
        },
    )
}

impl State {
    fn summary(&self) -> String {
        let run = match &self.run {
            Some(run) if self.passed == 0 && self.running == 0 && self.failed.is_empty() => {
                return format!("CI: queued (run {})", run.id);
            }
            Some(run) => format!("run {}", run.id),
            None => return "CI: not started yet".into(),
        };
        format!(
            "CI: {} passed, {} running, {} failed ({run})",
            self.passed,
            self.running,
            self.failed.len()
        )
    }

    fn failures(&self) -> String {
        match &self.run {
            Some(run) if !self.failed.is_empty() => format!(
                "Failed jobs: {}\n{}\nRetry failed jobs: gh run rerun {} --failed",
                self.failed.join(", "),
                run.html_url,
                run.id
            ),
            _ => String::new(),
        }
    }
}

/// Keep review failures as failures, and a completed CI failure cannot be a
/// successful ship. An API failure is explicit and does not claim CI passed.
pub(in crate::ship) fn outcome<T>(
    root: &Path,
    head: &str,
    grade: Option<u8>,
    result: Result<T>,
) -> Result<T> {
    let review = grade.map_or_else(|| "review unavailable".into(), |g| format!("review {g}/10"));
    let state = match read(root, head) {
        Ok(state) => state,
        Err(error) => {
            let line = format!("{review} · CI: unavailable ({error})");
            return Err(match result {
                Err(error) => format!("{}\n{line}", after_draft(&error.to_string())),
                Ok(_) => format!("ship: review passed and auto-merge is armed; could not read CI for {head}; check CI and reship\n{line}"),
            }.into());
        }
    };
    let line = format!("{review} · {}", state.summary());
    eprintln!("ship: {line}");
    let details = state.failures();
    match result {
        Err(error) => Err(format!("{}\n{line}\n{details}", after_draft(&error.to_string())).into()),
        Ok(_) if !state.failed.is_empty() => Err(format!(
            "ship: review passed and auto-merge is armed but CI failed; fix it and reship\n{line}\n{details}"
        ).into()),
        Ok(value) => Ok(value),
    }
}

fn after_draft(error: &str) -> String {
    error
        .replace("nothing was pushed", "the PR remains a draft")
        .replace("Nothing was pushed", "The PR remains a draft")
}

pub(in crate::ship) fn recorded_grade(root: &Path, branch: &str, head: &str) -> Option<u8> {
    crate::review::history(root, branch)
        .ok()?
        .into_iter()
        .filter(|r| r.head == head)
        .max_by_key(|r| r.finished)
        .map(|r| r.grade)
}
