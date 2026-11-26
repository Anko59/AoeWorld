//! The record of flaky tests: a case that failed and then passed on an immediate retry.
//! It does not fail the run, but it is never silent: one JSON line per case in a file
//! CI uploads, and a table in the job summary. The nightly triage counts the lines.
use super::{Case, Result};
use serde::Serialize;
use std::{fs, io::Write, path::PathBuf};

/// One flaky case, as written to `<report>.flaky.jsonl`.
#[derive(Debug, PartialEq, Serialize)]
pub(super) struct Flake {
    /// The test as the e2e quarantine names it: spec path without `.spec.ts`, then its title.
    pub test: String,
    pub project: String,
    /// The first line of the error of the failed first attempt.
    pub error: String,
    /// Attempts it took, counting the one that passed: 2 for an unlisted case.
    pub attempts: u8,
    /// The tracking issue of a quarantined case; absent for the ordinary one retry.
    pub quarantine: Option<String>,
    pub shard: Option<String>,
    pub sha: Option<String>,
    pub run_id: Option<String>,
    pub run_attempt: Option<String>,
}

impl Flake {
    fn of(case: &Case, attempts: u8, quarantine: Option<&str>) -> Self {
        let env = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        Self {
            test: format!("{} {}", case.key, case.identity.titles.join(" ")),
            project: case.identity.project.clone(),
            error: case.error.clone(),
            attempts,
            quarantine: quarantine.map(str::to_owned),
            shard: env("AOE_E2E_SHARD"),
            sha: env("GITHUB_SHA"),
            run_id: env("GITHUB_RUN_ID"),
            run_attempt: env("GITHUB_RUN_ATTEMPT"),
        }
    }
}

/// Appends to its file as cases recover, so a later failure of another case keeps the
/// record. The file exists only when something flaked.
pub(super) struct Log {
    path: Option<PathBuf>,
    flakes: Vec<Flake>,
}

impl Log {
    pub(super) fn new(path: PathBuf) -> Result<Self> {
        match fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error.into()),
            _ => {}
        }
        Ok(Self {
            path: Some(path),
            flakes: Vec::new(),
        })
    }

    /// A log that writes no file, for tests that do not look at it.
    #[cfg(test)]
    pub(super) fn unsaved() -> Self {
        Self {
            path: None,
            flakes: Vec::new(),
        }
    }

    pub(super) fn record(
        &mut self,
        case: &Case,
        attempts: u8,
        quarantine: Option<&str>,
    ) -> Result<()> {
        let flake = Flake::of(case, attempts, quarantine);
        if let Some(path) = &self.path {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)?;
            serde_json::to_writer(&mut file, &flake)?;
            writeln!(file)?;
        }
        println!(
            "flaky: {} [{}] passed on attempt {attempts}: {}",
            flake.test, flake.project, flake.error
        );
        self.flakes.push(flake);
        Ok(())
    }

    /// The job summary section, or `None` when nothing flaked.
    pub(super) fn summary(&self) -> Option<String> {
        if self.flakes.is_empty() {
            return None;
        }
        let mut text = format!(
            "### Flaky e2e tests: {} failed, then passed on retry\n\n\
             These did not fail the job. The nightly triage opens an issue for a test flagged \
             three times in seven days.\n\n| test | project | attempts | first error |\n\
             | --- | --- | ---: | --- |\n",
            self.flakes.len()
        );
        for flake in &self.flakes {
            text.push_str(&format!(
                "| `{}` | {} | {} | {} |\n",
                flake.test,
                flake.project,
                flake.attempts,
                flake.error.replace('|', "\\|")
            ));
        }
        Some(text)
    }

    /// Prints the summary and appends it to the GitHub job summary when there is one.
    pub(super) fn publish(&self) {
        let Some(text) = self.summary() else { return };
        println!("{text}");
        let Some(path) = std::env::var_os("GITHUB_STEP_SUMMARY").filter(|v| !v.is_empty()) else {
            return;
        };
        let appended = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .and_then(|mut file| writeln!(file, "{text}"));
        if let Err(error) = appended {
            eprintln!("flaky: cannot append to the job summary: {error}");
        }
    }
}
