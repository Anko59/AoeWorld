//! The child processes of the flow: the release build of a checkout and the game score runs of
//! its binary. Their output goes to the terminal as it arrives and its last lines are kept, so a
//! crash can be told in words.
use super::super::BoxError;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// A cold release build of the harness in a container with four jobs.
pub(super) const BUILD_DEADLINE: Duration = Duration::from_secs(45 * 60);
/// Four runs of about a minute each, played at once, with room for a loaded machine.
pub(super) const PLAY_DEADLINE: Duration = Duration::from_secs(20 * 60);

pub(super) struct Finished {
    pub success: bool,
    /// The last lines either stream wrote.
    pub tail: Vec<String>,
    pub seconds: f64,
}

impl Finished {
    pub(super) fn tail_text(&self) -> String {
        self.tail
            .iter()
            .filter(|line| !line.trim().is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" | ")
    }
}

/// Runs `command` to its end or to `deadline`; both streams are echoed to the standard error.
pub(super) fn run(mut command: Command, deadline: Duration) -> Result<Finished, BoxError> {
    let started = Instant::now();
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Its own process group, so the deadline can end the child and what it started: a
    // grandchild that keeps the pipes open would otherwise hold this call until it ends.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = command.spawn()?;
    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        return Err("the child's streams are not piped".into());
    };
    let streams = [
        crate::game_map::spawn_tail(stdout, true),
        crate::game_map::spawn_tail(stderr, true),
    ];
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if started.elapsed() > deadline {
            #[cfg(unix)]
            let _ = nix::sys::signal::killpg(
                nix::unistd::Pid::from_raw(child.id() as i32),
                nix::sys::signal::Signal::SIGKILL,
            );
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut tail: Vec<String> = Vec::new();
    for stream in streams {
        tail.extend(stream.join().unwrap_or_default());
    }
    let seconds = started.elapsed().as_secs_f64();
    match status {
        Some(status) => Ok(Finished {
            success: status.success(),
            tail,
            seconds,
        }),
        None => {
            tail.push(format!("killed after {} s", deadline.as_secs()));
            Ok(Finished {
                success: false,
                tail,
                seconds,
            })
        }
    }
}

/// Where cargo puts the release binary of the checkout at `root`.
pub(super) fn binary(target: &Path) -> PathBuf {
    target.join("release/aoe-harness")
}

pub(super) fn target(root: &Path) -> PathBuf {
    root.join("target")
}

/// The release build of the harness in `root`, into its own `target/`.
pub(super) fn build(root: &Path) -> Result<Finished, BoxError> {
    let mut command = Command::new("cargo");
    command
        .args(["build", "--release", "--locked", "-p", "aoe-harness"])
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target(root));
    run(command, BUILD_DEADLINE)
}

/// What a game score child is told: `game-score` and its flags, run from `root`.
pub(super) fn game_score(binary: &Path, root: &Path, target: &Path, flags: &[String]) -> Command {
    let mut command = Command::new(binary);
    command
        .arg("game-score")
        .args(flags)
        .current_dir(root)
        .env("CARGO_TARGET_DIR", target);
    command
}

#[cfg(test)]
#[path = "tests/launch.rs"]
mod tests;
