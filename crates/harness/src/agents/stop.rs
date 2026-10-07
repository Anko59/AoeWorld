//! Stop rule. When Claude or an agent stops, run the registry's static
//! stop-cadence gates the change selects (`make check-fast` in spirit). A
//! fingerprint of the change caches the result, so stopping again without a
//! change is another red round, not another wait. Agents are held for five red
//! rounds, then asked to write BLOCKED.md; the main session is told once per red
//! change and never held after that.
use super::{
    context::{BLOCKED, Context, RECORDS},
    role::Role,
};
use crate::{
    gates::registry::{Cadence, Registry},
    process::{Cancellation, CaptureExit, capture_command},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(crate) const ROUNDS: u32 = 5;
const KEPT_RUNS: usize = 8;
const TAIL_LINES: usize = 40;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum Verdict {
    Pass,
    Fail,
    Incomplete,
}

impl Verdict {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Incomplete => "INCOMPLETE",
        }
    }
}

/// `Static` is the Tester's subset: its tests fail (even to compile) on purpose,
/// so only the cheap edit-cadence gates run when it stops.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Scope {
    Full,
    Static,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Run {
    pub(crate) fingerprint: String,
    pub(crate) scope: Scope,
    pub(crate) verdict: Verdict,
    pub(crate) report: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct Streak {
    pub(crate) rounds: u32,
    pub(crate) started: u64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct State {
    pub(crate) runs: Vec<Run>,
    pub(crate) streaks: BTreeMap<String, Streak>,
    /// The red fingerprint the main session was last held on.
    pub(crate) told: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Allow,
    Note(String),
    Block(String),
}

/// `blocked` is the modification second of a non-empty BLOCKED.md.
pub(crate) fn decide(
    state: &mut State,
    role: Role,
    actor: &str,
    run: &Run,
    blocked: Option<u64>,
    now: u64,
) -> Decision {
    match run.verdict {
        Verdict::Pass => {
            state.streaks.remove(actor);
            if role == Role::Main {
                state.told = None;
            }
            Decision::Allow
        }
        Verdict::Incomplete => Decision::Note(format!(
            "check-fast was INCOMPLETE (a gate was unavailable); nothing was proven.\n{}",
            run.report
        )),
        Verdict::Fail if role == Role::Main => {
            if state.told.as_deref() == Some(run.fingerprint.as_str()) {
                Decision::Note(format!(
                    "check-fast is still red for this exact change; the session stopped anyway.\n{}",
                    run.report
                ))
            } else {
                state.told = Some(run.fingerprint.clone());
                Decision::Block(format!(
                    "{}\nFix these failures, never weakening a test, gate or baseline; or tell the person why you are stopping with a red check.",
                    run.report
                ))
            }
        }
        Verdict::Fail => {
            let streak = state.streaks.entry(actor.to_owned()).or_insert(Streak {
                rounds: 0,
                started: now,
            });
            streak.rounds += 1;
            if streak.rounds <= ROUNDS {
                Decision::Block(format!(
                    "{}\nRed round {}/{ROUNDS}. Fix the failures; never weaken a test, gate or baseline to pass.",
                    run.report, streak.rounds
                ))
            } else if blocked.is_some_and(|written| written >= streak.started) {
                Decision::Note(format!(
                    "Agent stopped red after {ROUNDS} rounds; see {BLOCKED}."
                ))
            } else {
                Decision::Block(format!(
                    "Stop trying. Write {BLOCKED} with what you tried, what still fails and what you need, then stop.\n{}",
                    run.report
                ))
            }
        }
    }
}

pub(crate) fn respond(
    context: &Context,
    session: &str,
    agent: Option<&str>,
) -> Option<serde_json::Value> {
    if context.role == Role::Reviewer {
        return None;
    }
    let scope = if context.role == Role::Tester {
        Scope::Static
    } else {
        Scope::Full
    };
    let actor = format!("{session}/{}", agent.unwrap_or("main"));
    let root = &context.root;
    let run = match fingerprint(root, scope) {
        Ok(fingerprint) => {
            let cached = with_state(root, |state| {
                state
                    .runs
                    .iter()
                    .find(|r| r.fingerprint == fingerprint && r.scope == scope)
                    .cloned()
            })
            .ok()
            .flatten();
            cached.unwrap_or_else(|| execute(root, scope, fingerprint))
        }
        Err(error) => Run {
            fingerprint: String::new(),
            scope,
            verdict: Verdict::Incomplete,
            report: format!("the change could not be fingerprinted: {error}"),
        },
    };
    let blocked = blocked_since(root);
    let now = seconds(SystemTime::now());
    let decision = with_state(root, |state| {
        state
            .runs
            .retain(|r| !(r.fingerprint == run.fingerprint && r.scope == run.scope));
        state.runs.push(run.clone());
        let excess = state.runs.len().saturating_sub(KEPT_RUNS);
        state.runs.drain(..excess);
        decide(state, context.role, &actor, &run, blocked, now)
    })
    .unwrap_or_else(|error| Decision::Note(format!("harness state unavailable: {error}")));
    if run.verdict == Verdict::Pass && scope == Scope::Full && context.role.is_agent() {
        let _ = fs::remove_file(root.join(BLOCKED));
    }
    match decision {
        Decision::Allow => None,
        Decision::Note(message) => Some(serde_json::json!({ "systemMessage": message })),
        Decision::Block(reason) => {
            Some(serde_json::json!({ "decision": "block", "reason": reason }))
        }
    }
}

fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn blocked_since(root: &Path) -> Option<u64> {
    let metadata = fs::metadata(root.join(BLOCKED)).ok()?;
    (metadata.len() > 0).then(|| metadata.modified().map(seconds).unwrap_or(0))
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

fn names(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|b| *b == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// The merge base with `$HARNESS_BASE` (default `origin/dev`), resolved under
/// `refs/remotes/` so a local branch or tag of that name cannot shadow it.
pub(crate) fn merge_base(root: &Path) -> Option<String> {
    let base = std::env::var("HARNESS_BASE").unwrap_or_else(|_| "origin/dev".into());
    let base = match base.strip_prefix("origin/") {
        Some(branch) => format!("refs/remotes/origin/{branch}"),
        None => base,
    };
    let base = git(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{base}^{{commit}}"),
        ],
    )
    .ok()?;
    let base = String::from_utf8_lossy(&base).trim().to_owned();
    let merge = git(root, &["merge-base", &base, "HEAD"]).ok()?;
    Some(String::from_utf8_lossy(&merge).trim().to_owned())
}

fn untracked(root: &Path) -> Result<Vec<String>, String> {
    Ok(names(&git(
        root,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?))
}

fn fingerprint(root: &Path, scope: Scope) -> Result<String, String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(format!("{scope:?}\0").as_bytes());
    hasher.update(&git(root, &["rev-parse", "HEAD"])?);
    hasher.update(merge_base(root).unwrap_or_default().as_bytes());
    hasher.update(&git(
        root,
        &["diff", "HEAD", "--binary", "--no-ext-diff", "--no-textconv"],
    )?);
    for name in untracked(root)? {
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(&fs::read(root.join(&name)).unwrap_or_default());
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn changed(root: &Path, merge_base: Option<&str>) -> Result<Vec<String>, String> {
    let against = merge_base.unwrap_or("HEAD");
    let mut paths = names(&git(
        root,
        &["diff", "--no-renames", "--name-only", "-z", against, "--"],
    )?);
    paths.extend(untracked(root)?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn execute(root: &Path, scope: Scope, fingerprint: String) -> Run {
    let finish = |verdict, report| Run {
        fingerprint: fingerprint.clone(),
        scope,
        verdict,
        report,
    };
    let base = merge_base(root);
    let paths = match changed(root, base.as_deref()) {
        Ok(paths) => paths,
        Err(error) => {
            return finish(
                Verdict::Incomplete,
                format!("changed paths unavailable: {error}"),
            );
        }
    };
    if paths.is_empty() {
        return finish(Verdict::Pass, "no change against the base".into());
    }
    let registry = match Registry::load(root) {
        Ok(registry) => registry,
        Err(error) => return finish(Verdict::Fail, format!("gates/registry.json: {error}")),
    };
    // No base to compare with selects every suite.
    let selection = if base.is_some() {
        registry.classify(&paths)
    } else {
        registry.classify(&[])
    };
    let plan = match registry.plan(Cadence::Stop, &selection.suites) {
        Ok(plan) => plan,
        Err(error) => return finish(Verdict::Fail, format!("stop plan: {error}")),
    };
    let gates: Vec<_> = plan
        .gates
        .iter()
        .filter_map(|id| registry.gates.iter().find(|g| &g.id == id))
        .filter(|g| g.r#static && (scope == Scope::Full || g.cadences.contains(&Cadence::Edit)))
        .collect();
    let mut docker: Option<bool> = None;
    let mut lines = Vec::new();
    let mut logs = Vec::new();
    let mut verdict = Verdict::Pass;
    for gate in gates {
        let needs_docker = gate
            .capabilities
            .contains(&crate::gates::registry::Capability::Docker);
        if needs_docker && !*docker.get_or_insert_with(docker_available) {
            lines.push(format!(
                "  UNAVAILABLE {} (Docker is not reachable)",
                gate.id
            ));
            if verdict == Verdict::Pass {
                verdict = Verdict::Incomplete;
            }
            continue;
        }
        let mut command = Command::new("sh");
        command.arg("-c").arg(&gate.command).current_dir(root);
        let captured = capture_command(
            command,
            Duration::from_secs(u64::from(gate.budget_s)),
            &Cancellation::default(),
        );
        let mut output = String::from_utf8_lossy(&captured.stdout).into_owned();
        output.push_str(&String::from_utf8_lossy(&captured.stderr));
        let output = strip_ansi(&output);
        let last = output
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or_default()
            .trim()
            .to_owned();
        let seconds = captured.duration.as_secs_f32();
        let passed = matches!(captured.exit, CaptureExit::Success);
        let label = match captured.exit {
            CaptureExit::Success => "PASS",
            CaptureExit::Deadline => "FAIL (over budget)",
            _ => "FAIL",
        };
        lines.push(format!(
            "  {label} {} {seconds:.1}s {}",
            gate.id,
            last.chars().take(120).collect::<String>()
        ));
        if !passed {
            verdict = Verdict::Fail;
            let tail: Vec<&str> = output.lines().collect();
            let tail = &tail[tail.len().saturating_sub(TAIL_LINES)..];
            logs.push(format!(
                "--- {} (last {TAIL_LINES} lines) ---\n{}",
                gate.id,
                tail.join("\n")
            ));
        }
    }
    let header = format!(
        "check-fast: static stop-cadence gates from gates/registry.json ({} scope, base {}): {}",
        if scope == Scope::Full {
            "full"
        } else {
            "tester"
        },
        base.as_deref()
            .map_or("missing, so every suite", |b| &b[..b.len().min(12)]),
        verdict.label()
    );
    finish(verdict, [vec![header], lines, logs].concat().join("\n"))
}

fn docker_available() -> bool {
    let mut command = Command::new("docker");
    command.arg("info");
    matches!(
        capture_command(command, Duration::from_secs(10), &Cancellation::default()).exit,
        CaptureExit::Success
    )
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Read-modify-write `.cache/agent-hook/state.json` under an exclusive lock.
pub(crate) fn with_state<T>(
    root: &Path,
    change: impl FnOnce(&mut State) -> T,
) -> Result<T, String> {
    let directory = root.join(RECORDS);
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(directory.join("state.lock"))
        .map_err(|e| e.to_string())?;
    let _lock = nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusive)
        .map_err(|(_, errno)| errno.to_string())?;
    let path = directory.join("state.json");
    let mut state: State = fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    let result = change(&mut state);
    let temporary = directory.join("state.json.tmp");
    let mut file = File::create(&temporary).map_err(|e| e.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&state).map_err(|e| e.to_string())?)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
    Ok(result)
}

/// The last recorded run, for SessionStart context.
pub(crate) fn last_run(root: &Path) -> Option<Run> {
    let bytes = fs::read(root.join(RECORDS).join("state.json")).ok()?;
    serde_json::from_slice::<State>(&bytes).ok()?.runs.pop()
}
