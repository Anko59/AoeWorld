use super::Entry;
use crate::ship::git;
use nix::fcntl::{Flock, FlockArg};
use serde_json::{Value, json};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0., |d| d.as_secs_f64())
}

/// Observability must never change shipping's result, including on a full disk,
/// invalid checkout or busy journal. One append under a nonblocking file lock.
pub(crate) fn mark(root: &Path, stage: &str, event: &str, detail: Value) {
    let _ = append(root, stage, event, detail);
}

fn append(root: &Path, stage: &str, event: &str, mut detail: Value) -> super::Result<()> {
    static RUN: OnceLock<String> = OnceLock::new();
    let run = RUN.get_or_init(|| format!("{}-{}", std::process::id(), now()));
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let branch = git::branch(root)?;
    let head = git::git(root, &["rev-parse", "HEAD"])?;
    let path = Path::new(&common)
        .join("aoe-ship/timeline")
        .join(format!("{branch}.jsonl"));
    if !detail.is_object() {
        detail = json!({"value":detail});
    }
    detail["run"] = json!(run);
    let entry = Entry {
        t: now(),
        branch,
        head,
        stage: stage.into(),
        event: event.into(),
        detail,
    };
    let mut line = serde_json::to_vec(&entry)?;
    line.push(b'\n');
    fs::create_dir_all(path.parent().ok_or("timeline has no parent")?)?;
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut locked = Flock::lock(file, FlockArg::LockExclusiveNonblock).map_err(|(_, e)| e)?;
    locked.write_all(&line)?;
    Ok(())
}

/// A few coarse call sites cover normal returns and errors. Kills/panics leave
/// an unmatched start, which the report explicitly excludes from durations.
pub(crate) fn stage<T, E>(
    root: &Path,
    name: &str,
    mut detail: Value,
    work: impl FnOnce() -> std::result::Result<T, E>,
) -> std::result::Result<T, E> {
    mark(root, name, "start", detail.clone());
    let result = work();
    if !detail.is_object() {
        detail = json!({"value":detail});
    }
    detail["ok"] = json!(result.is_ok());
    mark(root, name, "end", detail);
    result
}
