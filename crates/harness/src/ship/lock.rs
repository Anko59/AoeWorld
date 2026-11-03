//! Immediate ship serialization shared by every worktree of the repository.
//! Metadata is diagnostic. The kernel releases a dead holder's lock.
use super::{Result, git};
mod handoff;
pub(crate) use handoff::Handoff;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::{Seek, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) command: String,
    pub(crate) started: u64,
}
impl Holder {
    pub(crate) fn describe_place(&self) -> String {
        format!("{} in {}", self.branch, self.worktree)
    }
    fn describe(&self) -> String {
        format!(
            "`{}` on {} (started {})",
            self.command,
            self.describe_place(),
            utc(self.started)
        )
    }
}

#[derive(Debug)]
struct Inner {
    file: File,
    path: PathBuf,
}
static HELD: Mutex<Vec<Weak<Inner>>> = Mutex::new(Vec::new());

impl Drop for Inner {
    fn drop(&mut self) {
        // A normal handoff must release even if a finished Git child's helper retained
        // a duplicate. SIGKILL instead leaves an active mutator holding serialization.
        let _ = self.file.unlock();
    }
}
#[derive(Debug)]
pub(crate) struct ShipLock(Arc<Inner>);
impl ShipLock {
    pub(crate) fn inherit_to(&self, command: &mut Command) -> std::io::Result<()> {
        crate::jobs::lock::inherit_file(command, self.0.file.try_clone()?);
        Ok(())
    }
}

/// Only a mutation in this same repository inherits the live ship descriptor.
pub(crate) fn prepare_command(root: &Path, command: &mut Command) -> Result<()> {
    let path = path(root)?;
    let owned = {
        let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
        held.retain(|entry| entry.strong_count() > 0);
        held.iter()
            .filter_map(Weak::upgrade)
            .find(|entry| entry.path == path)
    };
    if let Some(inner) = owned {
        ShipLock(inner).inherit_to(command)?;
    }
    Ok(())
}

pub(crate) fn path(root: &Path) -> Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/ship.lock"))
}

pub(crate) fn acquire(root: &Path, command: &str) -> Result<ShipLock> {
    acquire_stage(root, command, "lock-acquire")
}
fn acquire_stage(root: &Path, command: &str, stage: &str) -> Result<ShipLock> {
    crate::timeline::stage(root, stage, serde_json::json!({"command":command}), || {
        acquire_at(
            &path(root)?,
            &Holder {
                worktree: root.display().to_string(),
                branch: git::branch(root).unwrap_or_else(|_| "(detached)".into()),
                command: command.into(),
                started: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs()),
            },
        )
    })
}

pub(crate) fn acquire_at(path: &Path, holder: &Holder) -> Result<ShipLock> {
    let mut file = open(path)?;
    match file.try_lock() {
        Ok(()) => {
            file.set_len(0)?;
            file.rewind()?;
            serde_json::to_writer(&mut file, holder)?;
            file.flush()?;
            let inner = Arc::new(Inner {
                file,
                path: path.into(),
            });
            let mut held = HELD.lock().unwrap_or_else(|e| e.into_inner());
            held.retain(|entry| entry.strong_count() > 0);
            held.push(Arc::downgrade(&inner));
            Ok(ShipLock(inner))
        }
        Err(TryLockError::WouldBlock) => {
            Err(Box::new(crate::jobs::lock::Busy(record(path).describe())))
        }
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}

/// Never waits or keeps a lock; stale metadata cannot make a free lock busy.
pub(crate) fn status(path: &Path) -> Result<Option<Holder>> {
    if !path.exists() {
        return Ok(None);
    }
    match open(path)?.try_lock() {
        Ok(()) => Ok(None),
        Err(TryLockError::WouldBlock) => Ok(Some(record(path))),
        Err(TryLockError::Error(error)) => Err(error.into()),
    }
}
fn record(path: &Path) -> Holder {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_else(|| Holder {
            worktree: "unknown (metadata being recorded)".into(),
            branch: "unknown".into(),
            command: "unknown".into(),
            started: 0,
        })
}
pub(crate) fn print_status(root: &Path) -> Result<()> {
    match status(&path(root)?)? {
        Some(holder) => println!("ship lock: held by {}", holder.describe()),
        None => println!("ship lock: free"),
    }
    Ok(())
}
fn open(path: &Path) -> Result<File> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?)
}

/// `2026-10-13 14:05:09Z` from Unix seconds (civil-from-days, proleptic Gregorian).
pub(crate) fn utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let rest = seconds % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}
