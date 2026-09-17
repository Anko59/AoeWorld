//! Pinned nightly mutation evidence for critical selection and comparison rules.
use crate::{perf::Verdict, process};
use serde::{Deserialize, Serialize};
use std::{error::Error, fs, path::Path, process::Command, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const TOOL_VERSION: &str = "27.1.0";
const FILTER: &str = "compare|classify|selection";
const EXCLUDE: &str = " in client$";
const OUTPUT: &str = "reports/mutation/campaign";
const FILES: [&str; 2] = ["crates/harness/src/perf.rs", "crates/harness/src/gates.rs"];

#[derive(Deserialize, Serialize)]
struct Outcomes {
    cargo_mutants_version: String,
    total_mutants: u64,
    caught: u64,
    missed: u64,
    timeout: u64,
    unviable: u64,
    success: u64,
    end_time: Option<String>,
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    tool: &'static str,
    tool_version: &'static str,
    files: [&'static str; 2],
    filter: &'static str,
    excluded_mutants: &'static str,
    mutant_set_hash: Option<String>,
    total_mutants: Option<u64>,
    caught: Option<u64>,
    missed: Option<u64>,
    timeout: Option<u64>,
    unviable: Option<u64>,
    verdict: Verdict,
    failure: Option<String>,
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {args:?} failed").into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn assess(outcomes: &Outcomes, command_succeeded: bool) -> (Verdict, Option<String>) {
    if outcomes.cargo_mutants_version != TOOL_VERSION
        || outcomes.end_time.is_none()
        || outcomes.total_mutants < 30
        || outcomes.caught
            + outcomes.missed
            + outcomes.timeout
            + outcomes.unviable
            + outcomes.success
            != outcomes.total_mutants
    {
        return (
            Verdict::Inconclusive,
            Some("mutation tool identity, completion, or sample count is invalid".into()),
        );
    }
    if outcomes.missed > 0 || outcomes.timeout > 0 {
        return (
            Verdict::Regression,
            Some(format!(
                "{} missed and {} timed-out critical mutations",
                outcomes.missed, outcomes.timeout
            )),
        );
    }
    if !command_succeeded {
        return (
            Verdict::Inconclusive,
            Some("mutation command failed despite complete outcomes".into()),
        );
    }
    (Verdict::Pass, None)
}

fn write_report(root: &Path, command_result: Result<()>) -> Result<Report> {
    let output = root.join(OUTPUT).join("mutants.out");
    let outcomes = fs::read(output.join("outcomes.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Outcomes>(&bytes).ok());
    let mut report = Report {
        version: 1,
        revision: git(&["rev-parse", "HEAD"])?,
        dirty: !git(&["status", "--porcelain"])?.is_empty(),
        tool: "cargo-mutants",
        tool_version: TOOL_VERSION,
        files: FILES,
        filter: FILTER,
        excluded_mutants: EXCLUDE,
        mutant_set_hash: fs::read(output.join("mutants.json"))
            .ok()
            .map(|bytes| blake3::hash(&bytes).to_hex().to_string()),
        total_mutants: None,
        caught: None,
        missed: None,
        timeout: None,
        unviable: None,
        verdict: Verdict::Inconclusive,
        failure: Some("mutation outcomes missing".into()),
    };
    if let Some(outcomes) = outcomes {
        report.total_mutants = Some(outcomes.total_mutants);
        report.caught = Some(outcomes.caught);
        report.missed = Some(outcomes.missed);
        report.timeout = Some(outcomes.timeout);
        report.unviable = Some(outcomes.unviable);
        (report.verdict, report.failure) = assess(&outcomes, command_result.is_ok());
    } else if let Err(error) = command_result {
        report.failure = Some(format!("mutation command failed: {error}"));
    }
    let directory = root.join("reports/mutation");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join("nightly.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(
        directory.join("nightly.md"),
        format!(
            "# Critical mutation campaign\n\nRevision: `{}`; dirty: `{}`; verdict: `{:?}`.\n\nCaught: {:?}; missed: {:?}; timed out: {:?}; unviable: {:?}; total: {:?}.\n",
            report.revision,
            report.dirty,
            report.verdict,
            report.caught,
            report.missed,
            report.timeout,
            report.unviable,
            report.total_mutants
        ),
    )?;
    Ok(report)
}

pub fn run() -> Result<()> {
    fs::create_dir_all("reports/mutation")?;
    let args = [
        "mutants",
        "--in-place",
        "--timeout",
        "120",
        "--output",
        OUTPUT,
        "--file",
        FILES[0],
        "--file",
        FILES[1],
        "--re",
        FILTER,
        "--exclude-re",
        EXCLUDE,
    ];
    let command = process::run("cargo", &args, Duration::from_secs(4_500))
        .map_err(|error| -> Box<dyn Error> { error.into() });
    let report = write_report(Path::new("."), command)?;
    println!(
        "mutation campaign: {:?}, {:?} caught, {:?} missed",
        report.verdict, report.caught, report.missed
    );
    if report.verdict != Verdict::Pass {
        return Err(report
            .failure
            .unwrap_or_else(|| "mutation campaign failed".into())
            .into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outcomes() -> Outcomes {
        Outcomes {
            cargo_mutants_version: TOOL_VERSION.into(),
            total_mutants: 42,
            caught: 38,
            missed: 0,
            timeout: 0,
            unviable: 4,
            success: 0,
            end_time: Some("completed".into()),
        }
    }

    #[test]
    fn incomplete_or_uncaught_mutations_cannot_pass() {
        assert_eq!(assess(&outcomes(), true).0, Verdict::Pass);
        assert_eq!(assess(&outcomes(), false).0, Verdict::Inconclusive);
        let mut sample = outcomes();
        sample.missed = 1;
        sample.caught -= 1;
        assert_eq!(assess(&sample, false).0, Verdict::Regression);
        sample.missed = 0;
        sample.timeout = 1;
        assert_eq!(assess(&sample, false).0, Verdict::Regression);
        sample.end_time = None;
        assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
        sample.end_time = Some("complete".into());
        sample.total_mutants = 1;
        assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
        sample.total_mutants = 42;
        sample.cargo_mutants_version = "wrong".into();
        assert_eq!(assess(&sample, true).0, Verdict::Inconclusive);
    }

    #[test]
    fn missing_outcomes_yield_inconclusive_report() {
        let temp = tempfile::tempdir().expect("report directory");
        let report = write_report(temp.path(), Err("missing tool".into())).expect("report");
        assert_eq!(report.verdict, Verdict::Inconclusive);
        assert!(temp.path().join("reports/mutation/nightly.json").is_file());
        assert!(temp.path().join("reports/mutation/nightly.md").is_file());
    }

    #[test]
    fn complete_outcomes_write_revision_bound_pass_and_regression_reports() {
        let temp = tempfile::tempdir().expect("report directory");
        let output = temp.path().join(OUTPUT).join("mutants.out");
        fs::create_dir_all(&output).expect("campaign directory");
        fs::write(output.join("mutants.json"), "[]").expect("mutant set");
        fs::write(
            output.join("outcomes.json"),
            serde_json::to_vec(&outcomes()).expect("outcomes JSON"),
        )
        .expect("outcomes");
        let report = write_report(temp.path(), Ok(())).expect("pass report");
        assert_eq!(report.verdict, Verdict::Pass);
        assert_eq!(report.total_mutants, Some(42));
        assert_eq!(report.revision.len(), 40);
        assert!(report.mutant_set_hash.is_some());
        let mut missed = outcomes();
        missed.missed = 1;
        missed.caught -= 1;
        fs::write(
            output.join("outcomes.json"),
            serde_json::to_vec(&missed).expect("outcomes JSON"),
        )
        .expect("outcomes");
        let report =
            write_report(temp.path(), Err("mutants failed".into())).expect("regression report");
        assert_eq!(report.verdict, Verdict::Regression);
    }
}
