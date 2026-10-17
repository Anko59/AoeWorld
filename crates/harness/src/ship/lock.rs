//! One `make ship`, `make duel` or `make red` at a time (docs/shipping.md).
//! The lock is an OS advisory lock (`flock`) on `<git common dir>/aoe-ship/ship.lock`,
//! which every worktree and checkout of the repository shares. The kernel drops it
//! when the holder exits however it exits, so a dead holder never leaves a stale
//! lock. The holder written into the file is for messages only.
//!
//! Waiters are served through the ticket queue in `ship.lock.queue/` (`slots::queue`,
//! shared with the heavy slots): a waiter may take the main lock only when no ticket ahead
//! of it is still locked. Ships back from review go ahead of plain waiters (`handoff`).
//! Tickets are removed on release and on drop.
use super::{Result, git};
mod handoff;
use crate::slots::queue::{self, Ticket, read_json, write_json};
pub(crate) use handoff::Handoff;
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_WAIT_MINUTES: u64 = 120;
const POLL: Duration = Duration::from_secs(1);
const REPORT_EVERY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) command: String,
    pub(crate) started: u64,
    /// Set for a ship that already held the lock and queues again after its review. It is
    /// admitted before every plain waiter (see [`Handoff::reacquire`]).
    #[serde(default)]
    pub(crate) returning: bool,
}

impl Holder {
    fn describe(&self) -> String {
        format!(
            "{} in {} (started {})",
            self.branch,
            self.worktree,
            utc(self.started)
        )
    }
}

/// Held until dropped; the lock is released with the file, and the ticket is removed.
#[derive(Debug)]
pub(crate) struct ShipLock {
    _flock: Flock<File>,
    _ticket: Ticket<Holder>,
}

fn queue_dir(lock_path: &Path) -> PathBuf {
    let mut name = lock_path.as_os_str().to_owned();
    name.push(".queue");
    PathBuf::from(name)
}

pub(crate) fn path(root: &Path) -> Result<PathBuf> {
    let common = git::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    Ok(PathBuf::from(common).join("aoe-ship/ship.lock"))
}

/// `SHIP_LOCK_WAIT_MINUTES`: how long to queue (default 120; 0 fails at once).
pub(crate) fn wait_from_env() -> Result<Duration> {
    match std::env::var("SHIP_LOCK_WAIT_MINUTES") {
        Ok(text) if !text.is_empty() => text
            .trim()
            .parse::<u64>()
            .map(|minutes| Duration::from_secs(minutes.saturating_mul(60)))
            .map_err(|_| {
                format!("SHIP_LOCK_WAIT_MINUTES={text}: use a whole number of minutes").into()
            }),
        _ => Ok(Duration::from_secs(DEFAULT_WAIT_MINUTES * 60)),
    }
}

/// Take the machine-wide lock for `command`, queueing for at most the environment's wait.
pub(crate) fn acquire(root: &Path, command: &str) -> Result<ShipLock> {
    acquire_as(root, command, false)
}

/// [`acquire`] for a ship that is `returning` from its review (see [`Handoff::reacquire`]).
fn acquire_as(root: &Path, command: &str, returning: bool) -> Result<ShipLock> {
    let holder = Holder {
        worktree: root.display().to_string(),
        branch: git::branch(root).unwrap_or_else(|_| "(detached)".into()),
        command: command.into(),
        started: now(),
        returning,
    };
    acquire_at(
        &path(root)?,
        &holder,
        wait_from_env()?,
        POLL,
        REPORT_EVERY,
        &mut |line| eprintln!("{line}"),
    )
}

pub(crate) fn acquire_at(
    path: &Path,
    holder: &Holder,
    wait: Duration,
    poll: Duration,
    report_every: Duration,
    say: &mut dyn FnMut(&str),
) -> Result<ShipLock> {
    let started = Instant::now();
    let ticket = Ticket::enter(&queue_dir(path), holder)?;
    let mut announced = false;
    let mut last_position = 0;
    let mut last_report = started;
    loop {
        let ahead = ticket.ahead_ranked(holder.rank(), Holder::rank)?;
        if ahead.is_empty() {
            let file = open(path)?;
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(mut flock) => {
                    write_json(&mut flock, holder)?;
                    if announced {
                        say("ship: the lock is free; starting");
                    }
                    return Ok(ShipLock {
                        _flock: flock,
                        _ticket: ticket,
                    });
                }
                Err((_, nix::errno::Errno::EWOULDBLOCK)) => {}
                Err((_, errno)) => {
                    return Err(format!("ship lock {}: {errno}", path.display()).into());
                }
            }
        }
        let position = ahead.len() + 1;
        // The lock file names the holder only while that ship's ticket is live (the file keeps
        // its last content after a release); otherwise name the first live ticket ahead.
        let live = queue(path)?;
        let current = read_json::<Holder>(path)
            .filter(|named| live.contains(named))
            .or_else(|| ahead.into_iter().next().flatten())
            .map_or("another ship".into(), |h| h.describe());
        if started.elapsed() >= wait {
            return Err(format!(
                "ship: waiting for {current} (position {position} in the queue), but \
                 SHIP_LOCK_WAIT_MINUTES ({}) ran out; run `make ship-status`, then try again \
                 when it finishes",
                wait.as_secs() / 60
            )
            .into());
        }
        let line = format!(
            "ship: position {position} in the queue{}",
            if holder.returning {
                " (returning from review)"
            } else {
                ""
            }
        );
        if !announced {
            say(&format!("ship: waiting for {current}"));
            say(&line);
            announced = true;
            last_position = position;
        } else if position != last_position {
            say(&line);
            last_position = position;
        } else if last_report.elapsed() >= report_every {
            say(&format!(
                "ship: still waiting for {current} ({} min so far, position {position} in the queue)",
                started.elapsed().as_secs() / 60
            ));
            last_report = Instant::now();
        }
        thread::sleep(poll.min(wait.saturating_sub(started.elapsed())));
    }
}

/// Who holds the lock now, or `None` when it is free. Never waits and never keeps it.
pub(crate) fn status(path: &Path) -> Result<Option<Holder>> {
    if !path.exists() {
        return Ok(None);
    }
    match Flock::lock(open(path)?, FlockArg::LockExclusiveNonblock) {
        Ok(_free) => Ok(None),
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => {
            Ok(Some(read_json(path).unwrap_or_else(unknown)))
        }
        Err((_, errno)) => Err(format!("ship lock {}: {errno}", path.display()).into()),
    }
}

/// Everyone queued now, oldest first; the first is normally the running ship.
/// Tickets of dead processes are skipped (waiters remove them). Never waits.
pub(crate) fn queue(path: &Path) -> Result<Vec<Holder>> {
    Ok(queue::alive::<Holder>(&queue_dir(path))?
        .into_iter()
        .map(|holder| holder.unwrap_or_else(unknown))
        .collect())
}

fn unknown() -> Holder {
    Holder {
        worktree: "unknown".into(),
        branch: "unknown".into(),
        command: "unknown".into(),
        started: 0,
        returning: false,
    }
}

pub(crate) fn print_status(root: &Path) -> Result<()> {
    let held = status(&path(root)?)?;
    match held {
        Some(holder) => println!(
            "ship lock: held by `{}` on {} (started {})",
            holder.command,
            holder.describe_place(),
            utc(holder.started)
        ),
        None => println!("ship lock: free"),
    }
    print_queue(root)
}

/// The queued ships in admission order; the running ship (the lock holder) is not listed.
fn print_queue(root: &Path) -> Result<()> {
    let lock_path = path(root)?;
    for line in queue_lines(status(&lock_path)?.as_ref(), queue(&lock_path)?) {
        println!("{line}");
    }
    Ok(())
}

/// The waiting ships in the order they will be admitted: returning ships first, then plain
/// waiters, each by arrival. The holder is removed by identity, not by position, because a
/// returning ship can hold the lock with a newer ticket than the plain waiters.
pub(crate) fn queue_lines(held: Option<&Holder>, mut waiting: Vec<Holder>) -> Vec<String> {
    if let Some(index) = held.and_then(|holder| waiting.iter().position(|w| w == holder)) {
        waiting.remove(index);
    }
    waiting.sort_by_key(|holder| std::cmp::Reverse(holder.rank()));
    let mut lines = Vec::new();
    if !waiting.is_empty() {
        lines.push(
            "ship queue (admission order: returning from review first, then arrival):".into(),
        );
    }
    for (index, holder) in waiting.iter().enumerate() {
        lines.push(format!(
            "  {}. `{}` on {} (queued {})",
            index + 1,
            holder.command,
            holder.describe_place(),
            utc(holder.started)
        ));
    }
    lines
}

impl Holder {
    pub(crate) fn describe_place(&self) -> String {
        let place = format!("{} in {}", self.branch, self.worktree);
        if self.returning {
            format!("{place} (returning from review)")
        } else {
            place
        }
    }

    /// Higher goes first: a returning ship before every plain waiter.
    fn rank(&self) -> u8 {
        u8::from(self.returning)
    }
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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
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
