//! Actual pinned-scanner restoration probes, never the >=30 critical campaign.
use super::{FILES, Result, assess, execution, io, outcomes};
use crate::{
    gates::scopes::{Kind, Snapshot},
    perf::Verdict,
    process,
};
use serde::Serialize;
use std::{fs, path::Path, time::Duration};

const TOKEN: &str = "AOE-SNAPSHOT-MUTATION-COMPAT-v1";
const ENVIRONMENT: &[(&str, &str)] = &[
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_ATTR_NOSYSTEM", "1"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_NO_REPLACE_OBJECTS", "1"),
    ("GIT_CONFIG_COUNT", "1"),
    ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
    ("GIT_CONFIG_VALUE_0", "false"),
    ("CARGO_NET_OFFLINE", "true"),
];

#[derive(Debug, Serialize)]
struct Leg {
    control: &'static str,
    total_mutants: u64,
    caught: u64,
    missed: u64,
    command_exit_code: Option<i32>,
    before_content_witness_digest: String,
    after_content_witness_digest: String,
    nightly_floor_assessment: Verdict,
}
#[derive(Serialize)]
struct Observation {
    schema: u16,
    assessment: &'static str,
    authoritative: bool,
    root_cause: &'static str,
    compiled_cache_provenance: &'static str,
    critical_campaign: &'static str,
    strong: Leg,
    weak: Leg,
}

fn fixture(root: &Path, strong: bool) -> Result<()> {
    for file in FILES {
        fs::create_dir_all(root.join(file).parent().ok_or("fixture parent missing")?)?;
    }
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname=\"aoe-harness\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\npath=\"{}\"\n[workspace]\n",
            FILES[0]
        ),
    )?;
    fs::write(
        root.join(FILES[0]),
        "pub mod gates;\npub fn compare(left:u32,right:u32)->bool { left == right }\n#[cfg(test)] mod tests;\n",
    )?;
    fs::write(root.join(FILES[1]), "pub mod registry;\n")?;
    fs::write(
        root.join(FILES[2]),
        "// Fixed-profile restoration fixture, no extra mutation functions.\n",
    )?;
    let tests = if strong {
        format!(
            "#[test] fn all_pairs() {{ for left in 0..4 {{ for right in 0..4 {{ assert_eq!(super::compare(left,right),left==right,\"{TOKEN}\"); }} }} }}\n"
        )
    } else {
        "#[test] fn deliberately_weak_temporary_control() { std::hint::black_box(1); }\n".to_owned()
    };
    fs::write(root.join("crates/harness/src/tests.rs"), tests)?;
    // Canonicalize through actual Cargo BEFORE anchoring the source commit.
    process::run_in(
        root,
        "cargo",
        &["generate-lockfile", "--offline"],
        ENVIRONMENT,
        Duration::from_secs(600),
    )?;
    for args in [
        vec!["init", "-q"],
        vec!["add", "Cargo.toml", "Cargo.lock", "crates"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixed mutation restoration control",
        ],
    ] {
        process::run_in(root, "git", &args, ENVIRONMENT, Duration::from_secs(30))?;
    }
    Ok(())
}
fn revision(root: &Path) -> Result<String> {
    let captured = process::capture_in(
        root,
        "git",
        &["rev-parse", "--verify", "HEAD^{commit}"],
        ENVIRONMENT,
        Duration::from_secs(30),
        &process::Cancellation::default(),
    );
    if !matches!(captured.exit, process::CaptureExit::Success) || captured.truncated {
        return Err("fixture commit resolution unavailable".into());
    }
    let revision = std::str::from_utf8(&captured.stdout)?.trim().to_owned();
    if !matches!(revision.len(), 40 | 64)
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("fixture full commit identity invalid".into());
    }
    Ok(revision)
}
fn counts_valid(
    total: u64,
    caught: u64,
    missed: u64,
    timeout: u64,
    unviable: u64,
    success: u64,
    strong: bool,
) -> bool {
    total == 3
        && timeout == 0
        && unviable == 0
        && success == 0
        && if strong {
            caught == 3 && missed == 0
        } else {
            caught == 0 && missed == 3
        }
}
fn leg(strong: bool) -> Result<Leg> {
    let source = tempfile::Builder::new()
        .prefix("aoe-mutation-compat-source-")
        .tempdir()?;
    fixture(source.path(), strong)?;
    let tracked = [
        "Cargo.toml",
        "Cargo.lock",
        FILES[0],
        FILES[1],
        FILES[2],
        "crates/harness/src/tests.rs",
    ];
    let original = tracked
        .iter()
        .map(|path| fs::read(source.path().join(path)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let snapshot =
        Snapshot::prepare_independent(source.path(), Kind::Commit(revision(source.path())?))?;
    let executed = execution::execute(source.path(), &snapshot)?;
    // Endpoint failure and actual tool failure are distinct, not one replacing the other.
    executed.verify(&snapshot)?;
    let mut pair = io::Pair::open(executed.artifact_directory())?;
    let observed = outcomes::parse(&pair.outcomes_bytes, &pair.inventory_bytes)?;
    if !counts_valid(
        observed.total_mutants,
        observed.caught,
        observed.missed,
        observed.timeout,
        observed.unviable,
        observed.success,
        strong,
    ) {
        return Err("actual compatibility mutant counts differ from fixed control".into());
    }
    let command_exit_code = match &executed.command {
        Some(Ok(())) if strong => None,
        Some(Err(process::ProcessError::Exit {
            code: Some(code), ..
        })) if !strong && *code != 0 => Some(*code),
        _ => return Err("actual compatibility command outcome differs from control".into()),
    };
    let (nightly_floor_assessment, _) = assess(&observed, strong);
    if nightly_floor_assessment != Verdict::Inconclusive {
        return Err("three compatibility mutants must not meet the nightly floor".into());
    }
    pair.recheck()?;
    executed.verify(&snapshot)?;
    let after = executed
        .after
        .as_ref()
        .ok_or("compatibility after witness missing")?;
    if executed.before != *after {
        return Err("compatibility source witness changed".into());
    }
    for (path, bytes) in tracked.iter().zip(original) {
        if fs::read(source.path().join(path))? != bytes {
            return Err("original fixture working bytes changed".into());
        }
    }
    pair.recheck()?;
    executed.verify(&snapshot)?;
    Ok(Leg {
        control: if strong {
            "CAUGHT_MUTANTS"
        } else {
            "MISSED_MUTANTS_WITH_ACTUAL_FAILED_COMMAND"
        },
        total_mutants: observed.total_mutants,
        caught: observed.caught,
        missed: observed.missed,
        command_exit_code,
        before_content_witness_digest: executed.before.digest.clone(),
        after_content_witness_digest: after.digest.clone(),
        nightly_floor_assessment,
    })
}

pub(super) fn run() -> Result<()> {
    let observation = Observation {
        schema: 1,
        assessment: "ACTUAL_SNAPSHOT_SCANNER_RESTORATION_OBSERVED_NON_AUTHORITATIVE",
        authoritative: false,
        root_cause: "ROOT_CAUSE_NOT_ASSESSED",
        compiled_cache_provenance: "UNAVAILABLE",
        critical_campaign: "NOT_RUN_BY_THIS_COMPATIBILITY_COMMAND",
        strong: leg(true)?,
        weak: leg(false)?,
    };
    println!("{}", serde_json::to_string_pretty(&observation)?);
    Ok(())
}

#[cfg(test)]
mod tests;
