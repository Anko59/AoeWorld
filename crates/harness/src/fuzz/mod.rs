//! Bounded parser fuzz campaigns with exact target and toolchain selection.
use crate::process;
use serde::Serialize;
use std::{error::Error, fs, path::Path, process::Command, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
mod seeds;
const TARGETS: [&str; 7] = [
    "drs",
    "slp",
    "palette",
    "manifest",
    "map_package",
    "environment_page",
    "map_chunk",
];

#[derive(Clone, Copy)]
pub enum Mode {
    Smoke,
    Nightly,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Nightly => "nightly",
        }
    }

    fn limit(self) -> &'static str {
        match self {
            Self::Smoke => "-runs=512",
            Self::Nightly => "-max_total_time=300",
        }
    }

    fn deadline(self) -> Duration {
        match self {
            Self::Smoke => Duration::from_secs(600),
            Self::Nightly => Duration::from_secs(900),
        }
    }
}

#[derive(Serialize)]
struct Report {
    version: u16,
    revision: String,
    dirty: bool,
    mode: &'static str,
    toolchain: &'static str,
    cargo_fuzz: &'static str,
    targets: [&'static str; 7],
    limit: &'static str,
    result: &'static str,
}

fn execute<F>(mode: Mode, mut command: F) -> Result<()>
where
    F: FnMut(&[&str], Duration) -> Result<()>,
{
    for target in TARGETS {
        command(
            &[
                "+nightly-2026-09-01",
                "fuzz",
                "run",
                target,
                "--",
                mode.limit(),
                "-max_len=1048576",
                "-timeout=5",
            ],
            mode.deadline(),
        )?;
    }
    Ok(())
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).output()?;
    if !output.status.success() {
        return Err("cannot read Git identity for fuzz report".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn write_report(root: &Path, mode: Mode, revision: String, dirty: bool) -> Result<()> {
    let report = Report {
        version: 1,
        revision,
        dirty,
        mode: mode.label(),
        toolchain: "nightly-2026-09-01",
        cargo_fuzz: "0.13.2",
        targets: TARGETS,
        limit: mode.limit(),
        result: "PASS",
    };
    let directory = root.join("reports/fuzz");
    fs::create_dir_all(&directory)?;
    fs::write(
        directory.join(format!("{}.json", mode.label())),
        serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(())
}

pub fn run(mode: Mode) -> Result<()> {
    let root = std::env::current_dir()?
        .parent()
        .ok_or("fuzz command must run in fuzz directory")?
        .canonicalize()?;
    if !Path::new("Cargo.toml").is_file() || !root.join("fuzz/fuzz_targets").is_dir() {
        return Err("fuzz command must run in fuzz directory".into());
    }
    seeds::prepare(&root)?;
    execute(mode, |args, deadline| {
        process::run("cargo", args, deadline).map_err(Into::into)
    })?;
    write_report(
        &root,
        mode,
        git(&["rev-parse", "HEAD"])?,
        !git(&["status", "--porcelain"])?.is_empty(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_parser_targets_are_bounded_and_failures_stop_the_campaign() {
        for mode in [Mode::Smoke, Mode::Nightly] {
            let mut targets = Vec::new();
            execute(mode, |args, deadline| {
                targets.push(args[3].to_owned());
                assert_eq!(args[0], "+nightly-2026-09-01");
                assert_eq!(args[5], mode.limit());
                assert!(args.contains(&"-max_len=1048576"));
                assert_eq!(deadline, mode.deadline());
                Ok(())
            })
            .expect("campaign");
            assert_eq!(targets, TARGETS);
        }
        let mut calls = 0;
        assert!(
            execute(Mode::Smoke, |_, _| {
                calls += 1;
                Err("injected parser crash".into())
            })
            .is_err()
        );
        assert_eq!(calls, 1);
    }

    #[test]
    fn reports_distinguish_bounded_smoke_and_nightly_campaigns() {
        let temp = tempfile::tempdir().expect("directory");
        for mode in [Mode::Smoke, Mode::Nightly] {
            write_report(temp.path(), mode, "revision".into(), true).expect("report");
            let path = temp
                .path()
                .join(format!("reports/fuzz/{}.json", mode.label()));
            let value: serde_json::Value =
                serde_json::from_slice(&fs::read(path).expect("report")).expect("JSON");
            assert_eq!(value["result"], "PASS");
            assert_eq!(value["dirty"], true);
            assert_eq!(
                value["targets"].as_array().expect("targets").len(),
                TARGETS.len()
            );
        }
    }

    #[test]
    fn fuzz_report_revision_comes_from_git_and_rejects_unknown_refs() {
        assert_eq!(git(&["rev-parse", "HEAD"]).expect("revision").len(), 40);
        assert!(git(&["rev-parse", "this-ref-does-not-exist"]).is_err());
    }
}
