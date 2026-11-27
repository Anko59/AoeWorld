//! The merge base, built and played in one persistent scratch worktree of the shared home: a
//! detached checkout that moves to each merge base in turn, so its `target/` keeps the compiled
//! crates and the next build recompiles only what the change touched. It is not a worktree
//! anyone works in: `make clean-stale` may remove it, and the next miss makes it again.
//!
//! This code runs in the Make container, where only the caller's checkout and the common Git
//! directory are mounted: Git there sees every other worktree's folder as missing. So it never
//! asks Git to prune, repair or clean worktrees as a whole. It touches one worktree, `<home>/base`
//! and nothing else, and it refuses to touch any other path.
use super::super::BoxError;
use super::home;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc::{self, RecvTimeoutError, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime},
};

fn git(directory: &Path, args: &[&str]) -> Result<String, BoxError> {
    let output = Command::new("git")
        .current_dir(directory)
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn common_directory(directory: &Path) -> Option<PathBuf> {
    let path = git(
        directory,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()?;
    fs::canonicalize(path).ok()
}

/// `checkout` is the scratch worktree and nothing else: exactly `<home>/base`, in a home that is
/// not itself a checkout, and neither the checkout `make` runs in nor a folder that holds it.
/// Every command that moves, cleans or removes anything goes through this first.
fn ours(repo: &Path, home: &Path, checkout: &Path) -> Result<(), BoxError> {
    let canonical = |path: &Path| fs::canonicalize(path).unwrap_or_else(|_| path.to_owned());
    if checkout != home::base_checkout(home) {
        return Err(format!(
            "{} is not the scratch worktree {}",
            checkout.display(),
            home::base_checkout(home).display()
        )
        .into());
    }
    if home.join(".git").exists() || canonical(repo).starts_with(canonical(checkout)) {
        return Err(format!(
            "{} is a checkout, or holds the one this runs in: it is not a place for the scratch worktree",
            home.display()
        )
        .into());
    }
    Ok(())
}

/// One merge base at a time: the scratch worktree is shared by every checkout of the repository,
/// and a second ship that moves, cleans or builds it under the first would store the wrong
/// numbers for everyone. The lock is a directory of the shared home, made atomically (`mkdir`
/// answers the same in every container, whatever the file sharing does with `flock`), and its
/// holder touches a heartbeat file while it lives: a lock whose heartbeat stopped belongs to a
/// dead process and is taken over.
const BEAT: Duration = if cfg!(test) {
    Duration::from_millis(50)
} else {
    Duration::from_secs(5)
};
const STALE: Duration = if cfg!(test) {
    Duration::from_millis(600)
} else {
    Duration::from_secs(60)
};
const POLL: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(2)
};

/// Held while the scratch worktree is in use; released when dropped.
pub(super) struct Guard {
    path: PathBuf,
    /// Dropping it wakes the heartbeat thread, which then ends.
    stop: Option<Sender<()>>,
    beat: Option<JoinHandle<()>>,
}

fn beat_file(path: &Path) -> PathBuf {
    path.join("beat")
}

/// A lock whose heartbeat is older than `STALE` (or that never had one for as long).
fn stale(path: &Path) -> bool {
    let modified = fs::metadata(beat_file(path))
        .or_else(|_| fs::metadata(path))
        .and_then(|meta| meta.modified());
    modified.is_ok_and(|time| time.elapsed().is_ok_and(|age| age > STALE))
}

/// Waits up to `wait` for the scratch worktree, then gives up with a sentence.
pub(super) fn lock(home: &Path, wait: Duration) -> Result<Guard, BoxError> {
    fs::create_dir_all(home)?;
    let path = home.join("base.lock");
    let started = Instant::now();
    loop {
        match fs::create_dir(&path) {
            Ok(()) => {
                fs::write(beat_file(&path), std::process::id().to_string())?;
                let (stop, stopped) = mpsc::channel::<()>();
                let beat = {
                    let path = path.clone();
                    thread::spawn(move || {
                        while stopped.recv_timeout(BEAT) == Err(RecvTimeoutError::Timeout) {
                            let _ = fs::File::options()
                                .write(true)
                                .open(beat_file(&path))
                                .and_then(|file| file.set_modified(SystemTime::now()));
                        }
                    })
                };
                return Ok(Guard {
                    path,
                    stop: Some(stop),
                    beat: Some(beat),
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if stale(&path) {
                    let _ = fs::remove_dir_all(&path);
                    continue;
                }
                if started.elapsed() >= wait {
                    return Err(format!(
                        "another ship has used the scratch worktree for {} s",
                        started.elapsed().as_secs()
                    )
                    .into());
                }
                thread::sleep(POLL);
            }
            Err(error) => return Err(error.into()),
        }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        drop(self.stop.take());
        if let Some(beat) = self.beat.take() {
            let _ = beat.join();
        }
        let _ = fs::remove_file(beat_file(&self.path));
        let _ = fs::remove_dir(&self.path);
    }
}

/// Whether `checkout` is a live worktree of the repository at `repo`: its `.git` is the file a
/// worktree has, and both name the same common directory.
fn registered(repo: &Path, checkout: &Path) -> bool {
    checkout.join(".git").is_file()
        && matches!(
            (common_directory(repo), common_directory(checkout)),
            (Some(a), Some(b)) if a == b
        )
}

/// Removes the administrative entry of the worktree at `checkout`, and only that one: the entry
/// of `<common directory>/worktrees` whose `gitdir` file names `<checkout>/.git`. Used when the
/// folder was lost but the entry stayed, or the other way round.
fn forget(repo: &Path, checkout: &Path) -> Result<(), BoxError> {
    let Some(common) = common_directory(repo) else {
        return Ok(());
    };
    let wanted = checkout.join(".git");
    let Ok(entries) = fs::read_dir(common.join("worktrees")) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let named = fs::read_to_string(entry.path().join("gitdir")).unwrap_or_default();
        if Path::new(named.trim()) == wanted {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

/// The scratch worktree at `rev`: made when missing (or when the repository lost it), moved when
/// it stands on another commit. Returns its path.
pub(super) fn prepare(repo: &Path, home: &Path, rev: &str) -> Result<PathBuf, BoxError> {
    let checkout = home::base_checkout(home);
    ours(repo, home, &checkout)?;
    if registered(repo, &checkout) {
        git(&checkout, &["checkout", "--force", "--detach", rev])?;
        git(&checkout, &["clean", "-fdq"])?;
    } else {
        // Whatever stands there is ours and is rebuilt: a folder the repository forgot, or an
        // entry whose folder is gone, cannot be moved. Only this worktree's entry goes.
        forget(repo, &checkout)?;
        if checkout.exists() {
            fs::remove_dir_all(&checkout)?;
        }
        fs::create_dir_all(home)?;
        git(
            repo,
            &[
                "worktree",
                "add",
                "--detach",
                "--force",
                &checkout.to_string_lossy(),
                rev,
            ],
        )?;
    }
    Ok(checkout)
}

#[cfg(test)]
#[path = "tests/base.rs"]
mod tests;
