//! Run registry gates as written (`sh -c <command>` from the checkout root)
//! with their budgets. Shared by the agent stop rule and `ship`.
use crate::{
    gates::registry::{Capability, Gate},
    process::{Cancellation, CaptureExit, capture_command, load},
    slots::SlotEnv,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command, time::Duration};

const TAIL_LINES: usize = 40;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum GateVerdict {
    Pass,
    Fail,
    /// The gate needs Docker and Docker is not reachable: nothing was proven.
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct GateResult {
    pub(crate) gate: String,
    pub(crate) verdict: GateVerdict,
    pub(crate) seconds: f32,
    /// Last non-empty output line (at most 120 characters).
    pub(crate) summary: String,
    /// The last 40 lines of output when the gate did not pass.
    pub(crate) tail: String,
    /// Every run, only when a timeout on an overloaded machine was retried (then two).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) attempts: Vec<Attempt>,
}

/// One run of a gate that was retried after its deadline expired on an overloaded machine.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Attempt {
    pub(crate) seconds: f32,
    pub(crate) verdict: GateVerdict,
    /// Whether this run hit its budget.
    pub(crate) timed_out: bool,
    /// The machine load at the deadline (empty when it did not time out, or could not be read).
    pub(crate) load: String,
}

/// What a local gate that times out on an overloaded machine gets: one more run after the load
/// falls. CI never retries, and nothing else is retried.
#[expect(dead_code, reason = "the retry itself is wired in the next commit")]
pub(crate) struct Retry {
    /// False in CI.
    local: bool,
    /// The load now, read at each deadline.
    load: Box<load::Reader<'static>>,
    /// Waits, bounded, until the 1-minute load is at or below the core count.
    wait: Box<dyn Fn() -> Result<(), String>>,
}

impl Retry {
    pub(crate) fn new(
        local: bool,
        load: Box<load::Reader<'static>>,
        wait: Box<dyn Fn() -> Result<(), String>>,
    ) -> Self {
        Self { local, load, wait }
    }

    /// The policy of this process: local runs retry, CI never does.
    pub(crate) fn here() -> Self {
        Self::new(true, Box::new(load::read), Box::new(|| Ok(())))
    }
}

/// Whether `get` reports CI (`GITHUB_ACTIONS` or `CI` set to a non-empty value).
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the retry itself is wired in the next commit")
)]
pub(crate) fn in_ci(_get: impl Fn(&str) -> Option<String>) -> bool {
    false
}

impl GateResult {
    pub(crate) fn line(&self) -> String {
        let label = match self.verdict {
            GateVerdict::Pass => "PASS",
            GateVerdict::Fail => "FAIL",
            GateVerdict::Unavailable => "UNAVAILABLE",
        };
        format!(
            "  {label} {} {:.1}s {}",
            self.gate, self.seconds, self.summary
        )
    }
}

type TakeSlot =
    fn(&Path, &str, &SlotEnv) -> crate::slots::queue::Result<crate::slots::guard::Guard>;

pub(crate) fn run_gates(root: &Path, gates: &[&Gate], slots: &SlotEnv) -> Vec<GateResult> {
    run_gates_with(root, gates, slots, crate::slots::hold)
}

/// `run_gates` for a local preflight: `test-unit` tests only the crates the
/// changes since `merge_base` affect (never in CI or with `AOE_TEST_ALL=1`).
pub(crate) fn run_gates_selecting(
    root: &Path,
    gates: &[&Gate],
    slots: &SlotEnv,
    merge_base: &str,
) -> Vec<GateResult> {
    let select =
        crate::native::affected::allowed(crate::native::affected::real_env).then_some(merge_base);
    run_gates_scoped(root, gates, slots, crate::slots::hold, select)
}

/// `run_gates` with the retry policy injected for fixtures.
pub(crate) fn run_gates_retrying(
    root: &Path,
    gates: &[&Gate],
    slots: &SlotEnv,
    take: TakeSlot,
    _retry: &Retry,
) -> Vec<GateResult> {
    run_gates_scoped(root, gates, slots, take, None)
}

/// The same gate path with acquisition injected for queue/load fixtures.
pub(crate) fn run_gates_with(
    root: &Path,
    gates: &[&Gate],
    slots: &SlotEnv,
    take: TakeSlot,
) -> Vec<GateResult> {
    run_gates_retrying(root, gates, slots, take, &Retry::here())
}

fn run_gates_scoped(
    root: &Path,
    gates: &[&Gate],
    slots: &SlotEnv,
    take: TakeSlot,
    select: Option<&str>,
) -> Vec<GateResult> {
    let mut docker: Option<bool> = None;
    gates
        .iter()
        .map(|gate| {
            if gate.capabilities.contains(&Capability::Docker)
                && !*docker.get_or_insert_with(docker_available)
            {
                return GateResult {
                    gate: gate.id.clone(),
                    verdict: GateVerdict::Unavailable,
                    seconds: 0.0,
                    summary: "Docker is not reachable".into(),
                    tail: String::new(),
                    attempts: Vec::new(),
                };
            }
            run_one(root, gate, slots, take, select)
        })
        .collect()
}

fn run_one(
    root: &Path,
    gate: &Gate,
    slots: &SlotEnv,
    take: TakeSlot,
    select: Option<&str>,
) -> GateResult {
    // A heavy gate queues for its slot first: its budget starts when it runs, not when it waits.
    let slot = match take(root, &gate.id, slots) {
        Ok(slot) => slot,
        Err(error) => {
            return GateResult {
                gate: gate.id.clone(),
                verdict: GateVerdict::Unavailable,
                seconds: 0.0,
                summary: error.to_string(),
                tail: String::new(),
                attempts: Vec::new(),
            };
        }
    };
    let mut command = Command::new("sh");
    command.arg("-c").arg(&gate.command).current_dir(root);
    command.envs(slot.env());
    if let (Some(base), "test-unit") = (select, gate.id.as_str()) {
        command.env("AOE_TEST_SELECT", base);
    }
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
        .trim();
    let passed = matches!(captured.exit, CaptureExit::Success);
    let mut summary: String = last.chars().take(120).collect();
    if matches!(captured.exit, CaptureExit::Deadline) {
        summary = over_budget(gate.budget_s, &summary, &load::read);
    }
    let tail = if passed {
        String::new()
    } else {
        let lines: Vec<&str> = output.lines().collect();
        lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
    };
    GateResult {
        gate: gate.id.clone(),
        verdict: if passed {
            GateVerdict::Pass
        } else {
            GateVerdict::Fail
        },
        seconds: captured.duration.as_secs_f32(),
        summary,
        tail,
        attempts: Vec::new(),
    }
}

/// The summary of a gate that hit its budget, with the machine load then.
pub(super) fn over_budget(budget_s: u32, last: &str, reader: &load::Reader<'_>) -> String {
    let load = load::note(reader);
    let load = if load.is_empty() {
        load
    } else {
        format!(" ({load})")
    };
    format!("over its {budget_s}s budget{load}; {last}")
}

fn docker_available() -> bool {
    let mut command = Command::new("docker");
    command.arg("info");
    matches!(
        capture_command(command, Duration::from_secs(10), &Cancellation::default()).exit,
        CaptureExit::Success
    )
}

pub(crate) fn strip_ansi(text: &str) -> String {
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

#[cfg(test)]
mod tests;
