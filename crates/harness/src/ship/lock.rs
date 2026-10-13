//! One `make ship` or `make duel` at a time per machine (docs/shipping.md).
//! The lock is an OS advisory lock (`flock`) on `<git common dir>/aoe-ship/ship.lock`,
//! which every worktree and checkout of the repository shares. The kernel drops it
//! when the holder exits however it exits, so a dead holder never leaves a stale
//! lock. The holder written into the file is for messages only.
//!
//! Waiters are served in arrival order. Each one creates a ticket in `ship.lock.queue/`
//! named by its arrival time in nanoseconds, its pid and a per-process counter (names sort
//! in arrival order), and holds an exclusive `flock` on that ticket while it waits and
//! while it runs. It may take the main lock only when no older ticket is still locked; an
//! older ticket whose `flock` can be taken belongs to a dead process and is removed.
//! Tickets are removed on release and on drop.
use super::{Result, git};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const DEFAULT_WAIT_MINUTES: u64 = 120;
const POLL: Duration = Duration::from_secs(1);
const REPORT_EVERY: Duration = Duration::from_secs(300);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Holder {
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) command: String,
    pub(crate) started: u64,
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
    _ticket: Ticket,
}

/// A place in the queue: the file exists, and is locked by us, until this is dropped.
#[derive(Debug)]
struct Ticket {
    dir: PathBuf,
    name: String,
    _flock: Flock<File>,
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl Ticket {
    /// Lock a hidden staging file, then rename it into the queue, so that no waiter can see
    /// a ticket before it is locked and mistake it for a dead process's.
    fn enter(lock_path: &Path, holder: &Holder) -> Result<Self> {
        let dir = queue_dir(lock_path);
        fs::create_dir_all(&dir)?;
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let staging = dir.join(format!(".enter-{pid}-{sequence}"));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&staging)?;
        let mut flock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|(_, errno)| format!("ship queue {}: {errno}", staging.display()))?;
        write_holder(&mut flock, holder)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!("{nanos:020}-{pid:010}-{sequence:06}");
        fs::rename(&staging, dir.join(&name))?;
        Ok(Self {
            dir,
            name,
            _flock: flock,
        })
    }

    /// The holders of the tickets older than ours that are still alive, oldest first.
    /// Tickets left by dead processes are removed on the way.
    fn ahead(&self) -> Result<Vec<Option<Holder>>> {
        let mut alive = Vec::new();
        for name in ticket_names(&self.dir)? {
            if name >= self.name {
                break;
            }
            let path = self.dir.join(&name);
            let Ok(file) = File::open(&path) else {
                continue;
            };
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(_dead) => {
                    let _ = fs::remove_file(&path);
                }
                Err((_, nix::errno::Errno::EWOULDBLOCK)) => alive.push(read_holder(&path)),
                Err((_, errno)) => {
                    return Err(format!("ship queue {}: {errno}", path.display()).into());
                }
            }
        }
        Ok(alive)
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.dir.join(&self.name));
    }
}

fn queue_dir(lock_path: &Path) -> PathBuf {
    let mut name = lock_path.as_os_str().to_owned();
    name.push(".queue");
    PathBuf::from(name)
}

/// Ticket file names in arrival order (staging files are not tickets yet).
fn ticket_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
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
    let holder = Holder {
        worktree: root.display().to_string(),
        branch: git::branch(root).unwrap_or_else(|_| "(detached)".into()),
        command: command.into(),
        started: now(),
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
    let ticket = Ticket::enter(path, holder)?;
    let mut announced = false;
    let mut last_position = 0;
    let mut last_report = started;
    loop {
        let ahead = ticket.ahead()?;
        if ahead.is_empty() {
            let file = open(path)?;
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(mut flock) => {
                    write_holder(&mut flock, holder)?;
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
        let current = ahead
            .into_iter()
            .next()
            .flatten()
            .or_else(|| read_holder(path))
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
        if !announced {
            say(&format!("ship: waiting for {current}"));
            say(&format!("ship: position {position} in the queue"));
            announced = true;
            last_position = position;
        } else if position != last_position {
            say(&format!("ship: position {position} in the queue"));
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
        Err((_, nix::errno::Errno::EWOULDBLOCK)) => Ok(Some(read_holder(path).unwrap_or(Holder {
            worktree: "unknown".into(),
            branch: "unknown".into(),
            command: "unknown".into(),
            started: 0,
        }))),
        Err((_, errno)) => Err(format!("ship lock {}: {errno}", path.display()).into()),
    }
}

/// Everyone queued now, oldest first; the first is normally the running ship.
/// Tickets of dead processes are skipped (waiters remove them). Never waits.
pub(crate) fn queue(path: &Path) -> Result<Vec<Holder>> {
    let dir = queue_dir(path);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut alive = Vec::new();
    for name in ticket_names(&dir)? {
        let ticket = dir.join(&name);
        let Ok(file) = File::open(&ticket) else {
            continue;
        };
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(_dead) => {}
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => {
                alive.push(read_holder(&ticket).unwrap_or_else(|| Holder {
                    worktree: "unknown".into(),
                    branch: "unknown".into(),
                    command: "unknown".into(),
                    started: 0,
                }))
            }
            Err((_, errno)) => {
                return Err(format!("ship queue {}: {errno}", ticket.display()).into());
            }
        }
    }
    Ok(alive)
}

pub(crate) fn print_status(root: &Path) -> Result<()> {
    let lock_path = path(root)?;
    let held = status(&lock_path)?;
    let mut waiting = queue(&lock_path)?;
    if held.is_some() && !waiting.is_empty() {
        waiting.remove(0);
    }
    match held {
        Some(holder) => println!(
            "ship lock: held by `{}` on {} (started {})",
            holder.command,
            holder.describe_place(),
            utc(holder.started)
        ),
        None => println!("ship lock: free"),
    }
    if !waiting.is_empty() {
        println!("ship queue (first come, first served):");
    }
    for (index, holder) in waiting.iter().enumerate() {
        println!(
            "  {}. `{}` on {} (queued {})",
            index + 1,
            holder.command,
            holder.describe_place(),
            utc(holder.started)
        );
    }
    Ok(())
}

impl Holder {
    fn describe_place(&self) -> String {
        format!("{} in {}", self.branch, self.worktree)
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

fn write_holder(file: &mut File, holder: &Holder) -> Result<()> {
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(&serde_json::to_vec(holder)?)?;
    file.sync_data()?;
    Ok(())
}

fn read_holder(path: &Path) -> Option<Holder> {
    let mut text = String::new();
    File::open(path).ok()?.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `2026-10-13 14:05:09Z` from Unix seconds (civil-from-days, proleptic Gregorian).
fn utc(seconds: u64) -> String {
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
