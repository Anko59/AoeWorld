//! A crash-safe first-come, first-served ticket queue on OS advisory locks, shared by
//! the ship lock (one at a time) and the heavy slots (N at a time).
//!
//! A waiter creates a ticket in the queue directory named by its arrival time in
//! nanoseconds, its pid and a per-process counter (names sort in arrival order), and holds
//! an exclusive `flock` on that ticket while it waits and while it runs. The kernel drops
//! the lock when the process exits however it exits, so an older ticket whose `flock` can
//! be taken belongs to a dead process and is removed. Tickets are removed on drop.
//! The payload written into the ticket is for messages (and the slots' nonce) only.
use nix::fcntl::{Flock, FlockArg};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, Write},
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// A place in the queue: the file exists, and is locked by us, until this is dropped.
#[derive(Debug)]
pub(crate) struct Ticket<T> {
    dir: PathBuf,
    name: String,
    _flock: Flock<File>,
    _payload: PhantomData<T>,
}

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl<T: Serialize + DeserializeOwned> Ticket<T> {
    /// Lock a hidden staging file, then rename it into the queue, so that no waiter can see
    /// a ticket before it is locked and mistake it for a dead process's.
    pub(crate) fn enter(dir: &Path, payload: &T) -> Result<Self> {
        fs::create_dir_all(dir)?;
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let staging = dir.join(format!(".enter-{pid}-{sequence}"));
        let file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&staging)?;
        let mut flock = Flock::lock(file, FlockArg::LockExclusiveNonblock)
            .map_err(|(_, errno)| format!("queue {}: {errno}", staging.display()))?;
        write_json(&mut flock, payload)?;
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let name = format!("{nanos:020}-{pid:010}-{sequence:06}");
        fs::rename(&staging, dir.join(&name))?;
        Ok(Self {
            dir: dir.to_owned(),
            name,
            _flock: flock,
            _payload: PhantomData,
        })
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    /// The payloads of the tickets older than ours that are still alive, oldest first
    /// (`None` when unreadable). Tickets left by dead processes are removed on the way.
    pub(crate) fn ahead(&self) -> Result<Vec<Option<T>>> {
        self.ahead_ranked(0, |_| 0)
    }

    /// The payloads of the live tickets that go before ours, in queue order: those with a
    /// higher `rank` than `own`, then those of equal rank that are older. An unreadable
    /// payload ranks 0. Tickets left by dead processes are removed on the way.
    pub(crate) fn ahead_ranked(&self, own: u8, rank: impl Fn(&T) -> u8) -> Result<Vec<Option<T>>> {
        let mut ahead = Vec::new();
        for name in ticket_names(&self.dir)? {
            let path = self.dir.join(&name);
            let Ok(file) = File::open(&path) else {
                continue;
            };
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(_dead) => {
                    let _ = fs::remove_file(&path);
                }
                Err((_, nix::errno::Errno::EWOULDBLOCK)) => {
                    let payload = read_json::<T>(&path);
                    let theirs = payload.as_ref().map_or(0, &rank);
                    if theirs > own || (theirs == own && name < self.name) {
                        ahead.push(payload);
                    }
                }
                Err((_, errno)) => {
                    return Err(format!("queue {}: {errno}", path.display()).into());
                }
            }
        }
        Ok(ahead)
    }
}

impl<T> Drop for Ticket<T> {
    fn drop(&mut self) {
        let _ = fs::remove_file(self.dir.join(&self.name));
    }
}

/// Everyone queued now, oldest first (`None` when unreadable). Tickets of dead processes
/// are skipped (waiters remove them). Never waits.
pub(crate) fn alive<T: DeserializeOwned>(dir: &Path) -> Result<Vec<Option<T>>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut alive = Vec::new();
    for name in ticket_names(dir)? {
        let path = dir.join(&name);
        let Ok(file) = File::open(&path) else {
            continue;
        };
        match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
            Ok(_dead) => {}
            Err((_, nix::errno::Errno::EWOULDBLOCK)) => alive.push(read_json(&path)),
            Err((_, errno)) => return Err(format!("queue {}: {errno}", path.display()).into()),
        }
    }
    Ok(alive)
}

/// Whether the ticket `name` in `dir` exists and its owner is still alive.
pub(crate) fn is_alive(dir: &Path, name: &str) -> bool {
    if name.is_empty() || name.starts_with('.') || name.contains('/') {
        return false;
    }
    let Ok(file) = File::open(dir.join(name)) else {
        return false;
    };
    matches!(
        Flock::lock(file, FlockArg::LockExclusiveNonblock),
        Err((_, nix::errno::Errno::EWOULDBLOCK))
    )
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

pub(crate) fn write_json<V: Serialize>(file: &mut File, value: &V) -> Result<()> {
    file.set_len(0)?;
    file.rewind()?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_data()?;
    Ok(())
}

pub(crate) fn read_json<V: DeserializeOwned>(path: &Path) -> Option<V> {
    let mut text = String::new();
    File::open(path).ok()?.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}
