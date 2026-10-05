//! Fixed scanner against ONE retained immutable projection; no worker authority.
use super::{EXCLUDE, FILES, FILTER, Result, storage::Storage};
use crate::{
    gates::scopes::{ContentWitness, Snapshot},
    process::{self, ProcessError},
};
use std::{
    error::Error,
    path::{Path, PathBuf},
    time::Duration,
};
const GIT_ENV: [(&str, &str); 8] = [
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_ATTR_NOSYSTEM", "1"),
    ("GIT_OPTIONAL_LOCKS", "0"),
    ("GIT_NO_REPLACE_OBJECTS", "1"),
    ("GIT_CONFIG_COUNT", "1"),
    ("GIT_CONFIG_KEY_0", "core.fsmonitor"),
    ("GIT_CONFIG_VALUE_0", "false"),
];
pub(super) struct Execution {
    pub(super) command: Option<std::result::Result<(), ProcessError>>,
    pub(super) before: ContentWitness,
    pub(super) after: Option<ContentWitness>,
    pub(super) endpoint_error: Option<Box<dyn Error>>,
    artifact_directory: PathBuf,
    storage: Storage,
}
impl Execution {
    pub(super) fn artifact_directory(&self) -> &Path {
        &self.artifact_directory
    }
    pub(super) fn verify(&self, snapshot: &Snapshot) -> Result<()> {
        self.storage.verify()?;
        if self.endpoint_error.is_some() || self.after.as_ref() != Some(&self.before) {
            return Err("mutation immutable endpoints unavailable or differ".into());
        }
        if snapshot.content_witness()? != self.before {
            return Err("mutation immutable subject changed after execution".into());
        }
        self.storage.verify()
    }
}
pub(super) fn scanner_args(raw: &Path) -> Result<Vec<String>> {
    let output = raw.to_str().ok_or("mutation raw path is not UTF-8")?;
    let mut args: Vec<String> = [
        "mutants",
        "--in-place",
        "--timeout",
        "120",
        "--output",
        output,
        "--re",
        FILTER,
        "--exclude-re",
        EXCLUDE,
        "--cargo-arg=--locked",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for file in FILES {
        args.extend(["--file".into(), file.into()]);
    }
    Ok(args)
}
pub(super) fn execute(source: &Path, snapshot: &Snapshot) -> Result<Execution> {
    execute_with(source, snapshot, |root, args, environment| {
        process::run_in(root, "cargo", args, environment, Duration::from_secs(4500))
    })
}
// Private deterministic test seam. Production and canaries always use actual fixed run_in.
fn execute_with(
    source: &Path,
    snapshot: &Snapshot,
    operation: impl FnOnce(&Path, &[&str], &[(&str, &str)]) -> std::result::Result<(), ProcessError>,
) -> Result<Execution> {
    if !snapshot.identity.isolated_inputs {
        return Err("mutation requires Commit or intentional Index".into());
    }
    let storage = Storage::new(source, snapshot)?;
    let before = snapshot.content_witness()?;
    let arguments = scanner_args(storage.raw())?;
    let args: Vec<&str> = arguments.iter().map(String::as_str).collect();
    let target = storage
        .target()
        .to_str()
        .ok_or("mutation target path is not UTF-8")?;
    let mut environment = GIT_ENV.to_vec();
    environment.extend([("CARGO_TARGET_DIR", target), ("CARGO_NET_OFFLINE", "true")]);
    let mut command = None;
    // NEVER return the process error from the closure: run_checked postconditions
    // can otherwise replace it. The original typed result remains separately held.
    let mut endpoint_error = snapshot
        .run_checked(|root| {
            storage.verify()?;
            command = Some(operation(root, &args, &environment));
            Ok(())
        })
        .err();
    let after = match snapshot.content_witness() {
        Ok(witness) => {
            if witness != before && endpoint_error.is_none() {
                endpoint_error = Some("mutation subject differs after scanner".into());
            }
            Some(witness)
        }
        Err(error) => {
            if endpoint_error.is_none() {
                endpoint_error = Some(error);
            }
            None
        }
    };
    if let Err(error) = storage.verify()
        && endpoint_error.is_none()
    {
        endpoint_error = Some(error);
    }
    Ok(Execution {
        command,
        before,
        after,
        endpoint_error,
        artifact_directory: storage.raw().join("mutants.out"),
        storage,
    })
}
#[cfg(test)]
mod tests;
