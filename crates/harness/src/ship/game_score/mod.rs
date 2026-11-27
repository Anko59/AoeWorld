//! The game score of a pull request, run beside the review. `make ship` starts `make
//! game-score-pr` as a child process once the gates have passed, prints its table in the
//! terminal when it ends (before the pull request is made ready), keeps its section current as
//! one comment of the pull request (`game_score::pr::MARKER`) and posts its one-line summary as
//! the `harness/game-score` commit status. The map cannot be generated on GitHub's runners, so
//! the game score runs here, where the agents work (docs/scale/game-score.md).
//!
//! Informational: a missing map, a busy machine, a crash, a number that got worse, even a
//! comment that cannot be written, never stops a ship, and the status is never a failure.
use super::{Evidence, Options, Result, git, github};
use crate::game_score::pr::{ADDED, MARKER, Outcome};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub(super) const CONTEXT: &str = "harness/game-score";
/// Where the child writes its result and reads the description, inside the checkout so the
/// container sees them; `.cache/` is ignored by Git.
const FOLDER: &str = ".cache/game-score";
/// About two minutes on a hit and ten on a miss, plus the wait for a free long-job slot.
const DEADLINE: Duration = Duration::from_secs(40 * 60);
const POLL: Duration = Duration::from_millis(200);
const RETRY: Duration = if cfg!(test) {
    Duration::from_millis(100)
} else {
    Duration::from_secs(20)
};
/// How long a check that is running is waited for once the review has ended; then it is stopped.
/// Nothing here may hold a ship.
const GRACE: Duration = if cfg!(test) {
    Duration::from_secs(1)
} else {
    Duration::from_secs(10 * 60)
};
/// A status description holds 140 characters.
const DESCRIPTION_LIMIT: usize = 140;

type Gh<'a> = &'a dyn Fn(&Path, &[&str]) -> Result<String>;

#[cfg(test)]
thread_local! {
    /// Tests only: a scripted `make` in place of the real one.
    pub(super) static MAKE: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn make() -> String {
    #[cfg(test)]
    if let Some(make) = MAKE.with(|make| make.borrow().clone()) {
        return make;
    }
    "make".into()
}

fn target_available(root: &Path, make: &str) -> bool {
    std::process::Command::new(make)
        .args(["-rRqp"])
        .current_dir(root)
        .output()
        .is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.starts_with("game-score-pr:"))
        })
}

/// The flags of `make game-score-pr`: the merge base ship pinned, and the description the agent
/// wrote (its `game-score:` lines declare the numbers that got worse on purpose).
fn arguments(merge_base: &str, description: bool) -> String {
    let mut flags = format!("--base {merge_base} --out {FOLDER}/pr.json");
    if description {
        flags.push_str(&format!(" --description {FOLDER}/description.md"));
    }
    flags
}

/// The machine was busy (a long-job slot, a build slot, too little memory): the resource layer
/// refuses with a message that ends in "retry later", and Make fails.
fn busy(log: &str) -> bool {
    log.contains("retry later")
}

/// The resource layer says this when it has given the job its slot.
const ADMITTED: &str = "long job admitted";

/// The game score of one commit, started before the review.
pub(super) struct Started {
    head: String,
    out: PathBuf,
    log: PathBuf,
    stop: Arc<AtomicBool>,
    /// The child holds a long-job slot: it is running, not asking again every 20 s.
    admitted: Arc<AtomicBool>,
    supervisor: Option<JoinHandle<()>>,
}

/// Starts `make game-score-pr` for `evidence.head` when this ship makes a pull request. Whether
/// the change can affect the game, and whether the map is there, are decided by the child.
pub(super) fn start(root: &Path, evidence: &Evidence, options: &Options) -> Option<Started> {
    if options.no_pr || options.review_ahead {
        return None;
    }
    start_with(
        root,
        &evidence.head,
        &evidence.merge_base,
        options.body_file.as_deref(),
    )
}

fn start_with(
    root: &Path,
    head: &str,
    merge_base: &str,
    body_file: Option<&Path>,
) -> Option<Started> {
    let make = make();
    if !target_available(root, &make) {
        eprintln!("game score: not available on this revision");
        return None;
    }
    let folder = root.join(FOLDER);
    let out = folder.join("pr.json");
    let log = folder.join("pr.log");
    let prepared = fs::create_dir_all(&folder).and_then(|()| {
        let _ = fs::remove_file(&out);
        match body_file {
            Some(body) => fs::copy(body, folder.join("description.md")).map(|_| true),
            None => Ok(false),
        }
    });
    let description = match prepared {
        Ok(description) => description,
        Err(error) => {
            eprintln!(
                "game score: could not prepare {} ({error})",
                folder.display()
            );
            return None;
        }
    };
    let flags = arguments(merge_base, description);
    let (stop, admitted) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let supervisor = {
        let (root, log) = (root.to_owned(), log.clone());
        let (stop, admitted) = (Arc::clone(&stop), Arc::clone(&admitted));
        thread::spawn(move || supervise(&root, &make, &flags, &log, (&stop, &admitted)))
    };
    Some(Started {
        head: head.to_owned(),
        out,
        log,
        stop,
        admitted,
        supervisor: Some(supervisor),
    })
}

fn spawn(root: &Path, make: &str, flags: &str, log: &Path) -> std::io::Result<Child> {
    let output = fs::File::create(log)?;
    let mut command = crate::process::explicit_command(
        root,
        make,
        &["--no-print-directory", "game-score-pr"],
        &[("GAME_SCORE_PR_ARGS", flags)],
    );
    command
        .stdin(Stdio::null())
        .stdout(output.try_clone()?)
        .stderr(output);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    command.spawn()
}

fn kill(child: &mut Child) {
    #[cfg(unix)]
    let _ = nix::sys::signal::killpg(
        nix::unistd::Pid::from_raw(child.id() as i32),
        nix::sys::signal::Signal::SIGKILL,
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs the child, again while every long-job slot is busy, until it ends, `stop` is set or the
/// deadline passes. `admitted` says whether the current attempt holds its slot.
fn supervise(
    root: &Path,
    make: &str,
    flags: &str,
    log: &Path,
    (stop, admitted): (&AtomicBool, &AtomicBool),
) {
    let deadline = Instant::now() + DEADLINE;
    let mut told = false;
    loop {
        admitted.store(false, Ordering::SeqCst);
        let mut child = match spawn(root, make, flags, log) {
            Ok(child) => child,
            Err(error) => {
                eprintln!("game score: could not start make game-score-pr ({error})");
                return;
            }
        };
        let exited = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if !stop.load(Ordering::SeqCst) && Instant::now() < deadline => {
                    if !admitted.load(Ordering::SeqCst) {
                        let started =
                            fs::read_to_string(log).is_ok_and(|text| text.contains(ADMITTED));
                        admitted.store(started, Ordering::SeqCst);
                    }
                    thread::sleep(POLL);
                }
                _ => break None,
            }
        };
        let Some(status) = exited else {
            kill(&mut child);
            return;
        };
        let log_text = fs::read_to_string(log).unwrap_or_default();
        if status.success() || !busy(&log_text) || Instant::now() >= deadline {
            return;
        }
        if !told {
            eprintln!("game score: every long-job slot is busy; trying again every 20 s");
            told = true;
        }
        let until = Instant::now() + RETRY;
        while Instant::now() < until && !stop.load(Ordering::SeqCst) {
            thread::sleep(POLL);
        }
        if stop.load(Ordering::SeqCst) {
            return;
        }
    }
}

/// The result file of `head`; a file written for another commit says nothing about this one.
fn read(path: &Path, head: &str) -> Option<Outcome> {
    let outcome: Outcome = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    (outcome.head == head).then_some(outcome)
}

impl Started {
    /// Ends the child and whatever it started, and waits for the supervisor.
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(supervisor) = self.supervisor.take() {
            let _ = supervisor.join();
        }
    }

    fn running(&self) -> bool {
        self.supervisor
            .as_ref()
            .is_some_and(|supervisor| !supervisor.is_finished())
    }

    /// Called when the review has ended. A check that has ended is read. One that was never
    /// admitted (every slot still busy) is stopped at once, one that runs gets `GRACE` more, then
    /// is stopped: an informational number never holds a ship. Prints the table, or the one line
    /// saying why there is none. `None`: no result for this commit.
    pub(super) fn finish(mut self) -> Option<Outcome> {
        let began = Instant::now();
        if self.running() && !self.admitted.load(Ordering::SeqCst) {
            self.stop();
            eprintln!(
                "game score: not started, every long-job slot was busy when the review ended; skipped (informational)"
            );
            return None;
        }
        while self.running() && began.elapsed() < GRACE {
            thread::sleep(POLL);
        }
        if self.running() {
            self.stop();
            eprintln!(
                "game score: still running {} min after the review ended; stopped (informational)",
                GRACE.as_secs() / 60
            );
            return None;
        }
        self.stop();
        let added = began.elapsed().as_secs();
        let mut outcome = read(&self.out, &self.head);
        match &mut outcome {
            Some(outcome) if outcome.ran => {
                outcome.section = outcome.section.replace(
                    ADDED,
                    &format!(
                        "- this check added {added} s to this ship beyond the review (0 when it ended before the review did)"
                    ),
                );
                eprint!("{}", outcome.terminal);
                eprintln!("game score: {}", outcome.summary);
                eprintln!("game score: added {added} s to this ship beyond the review");
            }
            Some(outcome) => eprintln!("game score: {}", outcome.summary),
            None => eprintln!(
                "game score: no result for this commit (informational; see {})",
                self.log.display()
            ),
        }
        outcome
    }
}

impl Drop for Started {
    fn drop(&mut self) {
        self.stop();
    }
}

fn short(sha: &str) -> &str {
    &sha[..12.min(sha.len())]
}

/// The `jq` filter that keeps the ids of the comments of people with write access that start
/// with the marker: anyone can comment on a pull request, and only those comments are ours.
fn find_filter() -> String {
    format!(
        r#".[] | select(.author_association == "OWNER" or .author_association == "MEMBER" or .author_association == "COLLABORATOR") | select(.body | startswith("{MARKER}")) | .id"#
    )
}

/// The status line: the summary, and where it was measured when that is not the pushed commit.
fn description(outcome: &Outcome, pushed: &str) -> String {
    let text = if outcome.head == pushed {
        outcome.summary.clone()
    } else {
        format!("{} (measured at {})", outcome.summary, short(&outcome.head))
    };
    text.chars().take(DESCRIPTION_LIMIT).collect()
}

/// Writes the section as the one comment of the pull request, the newest of ours edited in
/// place, and returns its address.
fn comment(gh: Gh<'_>, root: &Path, repository: &str, number: u64, body: &str) -> Result<String> {
    let comments = format!("repos/{repository}/issues/{number}/comments");
    let found = gh(
        root,
        &["api", "--paginate", &comments, "--jq", &find_filter()],
    )?;
    let existing = found
        .lines()
        .rev()
        .find_map(|line| line.trim().parse::<u64>().ok());
    let body_field = format!("body={body}");
    if let Some(id) = existing {
        let path = format!("repos/{repository}/issues/comments/{id}");
        let edited = gh(
            root,
            &[
                "api",
                "--method",
                "PATCH",
                &path,
                "-f",
                &body_field,
                "--jq",
                ".html_url",
            ],
        );
        if let Ok(url) = edited {
            return Ok(url);
        }
    }
    gh(
        root,
        &[
            "api",
            "--method",
            "POST",
            &comments,
            "-f",
            &body_field,
            "--jq",
            ".html_url",
        ],
    )
}

/// The `harness/game-score` status: success whatever the numbers say (GitHub has no neutral
/// state for a status), with the summary and a link to the comment.
fn status(
    gh: Gh<'_>,
    root: &Path,
    repository: &str,
    pushed: &str,
    outcome: &Outcome,
    url: Option<&str>,
) -> Result<()> {
    let path = format!("repos/{repository}/statuses/{pushed}");
    let description = format!("description={}", description(outcome, pushed));
    let mut args = vec![
        "api",
        "--method",
        "POST",
        &path,
        "-f",
        "state=success",
        "-f",
        "context=harness/game-score",
        "-f",
        &description,
    ];
    let target = url.map(|url| format!("target_url={url}"));
    if let Some(target) = &target {
        args.extend(["-f", target.as_str()]);
    }
    gh(root, &args).map(|_| ())
}

/// Writes the comment and the status of `outcome` for the pull request at `url`, pushed at
/// `pushed`. Failures are warnings.
pub(super) fn publish_with(
    gh: Gh<'_>,
    root: &Path,
    url: &str,
    pushed: &str,
    outcome: Option<&Outcome>,
) {
    let Some(outcome) = outcome.filter(|outcome| outcome.ran) else {
        return;
    };
    let done = (|| -> Result<()> {
        let repository = git::origin_repository(root)?;
        let number = super::postpush::pull_number(url)?;
        let link = comment(gh, root, &repository, number, &outcome.section)
            .map_err(|error| eprintln!("game score: could not write the comment ({error})"))
            .ok();
        status(gh, root, &repository, pushed, outcome, link.as_deref())
    })();
    if let Err(error) = done {
        eprintln!("game score: could not post {CONTEXT} ({error})");
    }
}

pub(super) fn publish(root: &Path, url: &str, pushed: &str, outcome: Option<&Outcome>) {
    publish_with(&github::gh, root, url, pushed, outcome);
}

/// The draft a failed review leaves: its pull request is found by branch.
pub(super) fn publish_draft(root: &Path, evidence: &Evidence, outcome: Option<&Outcome>) {
    if outcome.is_none_or(|outcome| !outcome.ran) {
        return;
    }
    let found = (|| -> Result<_> {
        let repository = git::origin_repository(root)?;
        github::open_pull_request(root, &repository, &evidence.branch)
    })();
    match found {
        Ok(Some(pr)) => publish(root, &pr.url, &evidence.head, outcome),
        Ok(None) => {}
        Err(error) => eprintln!("game score: could not find the draft ({error})"),
    }
}

#[cfg(test)]
mod tests;
