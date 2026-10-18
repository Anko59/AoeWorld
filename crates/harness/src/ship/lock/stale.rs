//! A ship lock whose holder a host suspend froze. The kernel releases a flock when its
//! holder exits, so a live but frozen holder keeps the lock; the waiter ends it, but only
//! when the machine suspended since the holder last recorded the boot-time gap and the
//! holder did not record the new gap in the meantime (`refresh_held`).
use super::{Holder, open};
use crate::{
    process::suspend::{self, Clock},
    slots::queue::{read_json, write_json},
};
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};

/// Lock files this process holds, so it can refresh them when it notices a suspend.
static HELD: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn held() -> MutexGuard<'static, Vec<PathBuf>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

pub(super) fn track(path: &Path) {
    held().push(path.to_path_buf());
}

pub(super) fn untrack(path: &Path) {
    held().retain(|held| held != path);
}

/// The boot-time gap in whole seconds, `None` where the clocks cannot be read.
pub(super) fn gap_now(clock: &Clock<'_>) -> Option<u64> {
    clock().ok().map(|reading| reading.gap().as_secs())
}

/// Called by the process that holds a ship lock when its supervised child was interrupted
/// by a suspend, before it retries or ends the child: record the gap it now sees in every
/// lock it holds, so that a recovering holder does not read as frozen. It reads the real
/// clock, not the one a caller injected, so a simulated suspend never reaches a lock.
pub(crate) fn refresh_held() {
    let paths = held().clone();
    for path in &paths {
        if let Err(error) = refresh_gap_at(path, &suspend::read) {
            eprintln!(
                "ship lock {}: could not record the suspend: {error}",
                path.display()
            );
        }
    }
}

/// Rewrite the gap in the lock file's record, when this process is the holder it names.
fn refresh_gap_at(path: &Path, clock: &Clock<'_>) -> super::Result<()> {
    let Some(mut holder) = read_json::<Holder>(path) else {
        return Ok(());
    };
    if holder.pid != std::process::id() {
        return Ok(());
    }
    holder.gap = gap_now(clock);
    write_json(&mut open(path)?, &holder)
}

/// Why `holder` should not keep the lock: the boot-time gap moved by more than the threshold
/// since it recorded it. A holder with no readable gap (an unreadable clock, or a record
/// being rewritten) is never stale.
pub(super) fn stale(holder: &Holder, clock: &Clock<'_>) -> Option<String> {
    let recorded = holder.gap?;
    let now = clock().ok()?.gap();
    suspend::moved(Duration::from_secs(recorded), now).then(|| {
        format!(
            "the machine was suspended while it held the lock and it did not recover \
             (boot-time gap {recorded}s when it last recorded it, now {}s)",
            now.as_secs()
        )
    })
}

/// Stop a holder that a suspend froze, so the kernel releases its lock. Only the exact
/// process that recorded the lock is signalled: its pid and start time must both match.
/// Returns whether it was signalled.
pub(super) fn end(holder: &Holder) -> bool {
    holder.pid != 0
        && holder.pid != std::process::id()
        && holder.pid_started.is_some()
        && process_started(holder.pid) == holder.pid_started
        && kill(Pid::from_raw(holder.pid as i32), Signal::SIGKILL).is_ok()
}

/// The start time of `pid` in clock ticks since boot (`/proc/<pid>/stat`, field 22).
pub(super) fn process_started(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name may contain spaces and parentheses; the fields follow its last `)`.
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(19)?.parse().ok()
}
