//! Run registry gates as written (`sh -c <command>` from the checkout root)
//! with their budgets. Shared by the agent stop rule and `ship`.
use crate::{
    gates::registry::{Capability, Gate},
    process::{Cancellation, CaptureExit, capture_command, load},
    slots::{SlotEnv, guard::Guard},
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

impl Attempt {
    fn describe(&self) -> String {
        let outcome = match (self.timed_out, self.verdict) {
            (true, _) => "timed out",
            (false, GateVerdict::Pass) => "passed",
            (false, _) => "failed",
        };
        if self.load.is_empty() {
            format!("{outcome} after {:.1}s", self.seconds)
        } else {
            format!("{outcome} after {:.1}s; {}", self.seconds, self.load)
        }
    }
}

/// What a local gate that times out on an overloaded machine gets: one more run after the load
/// falls. CI never retries, and nothing else is retried.
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
        Self::new(
            !in_ci(|name| std::env::var(name).ok()),
            Box::new(load::read),
            Box::new(|| {
                crate::slots::wait_for_load(&Cancellation::default())
                    .map_err(|error| error.to_string())
            }),
        )
    }
}

impl Retry {
    /// No retry and no load wait: for the agent stop check, which must answer at once.
    pub(crate) fn never() -> Self {
        Self::new(false, Box::new(load::read), Box::new(|| Ok(())))
    }
}

/// Whether `get` reports CI (`GITHUB_ACTIONS` or `CI` set to a non-empty value).
pub(crate) fn in_ci(get: impl Fn(&str) -> Option<String>) -> bool {
    ["GITHUB_ACTIONS", "CI"]
        .iter()
        .any(|name| get(name).is_some_and(|value| !value.is_empty()))
}

impl GateResult {
    pub(crate) fn line(&self) -> String {
        let label = match self.verdict {
            GateVerdict::Pass => "PASS",
            GateVerdict::Fail => "FAIL",
            GateVerdict::Unavailable => "UNAVAILABLE",
        };
        let attempts: String = self
            .attempts
            .iter()
            .enumerate()
            .map(|(index, attempt)| format!("\n    attempt {}: {}", index + 1, attempt.describe()))
            .collect();
        format!(
            "  {label} {} {:.1}s {}{attempts}",
            self.gate, self.seconds, self.summary
        )
    }
}

type TakeSlot =
    fn(&Path, &str, &SlotEnv) -> crate::slots::queue::Result<crate::slots::guard::Guard>;

pub(crate) fn run_gates(root: &Path, gates: &[&Gate], slots: &SlotEnv) -> Vec<GateResult> {
    run_gates_with(root, gates, slots, crate::slots::guard::hold_timed)
}

/// `run_gates` without the overload retry: the agent stop check runs inside a hook and must
/// never block for a load wait, so a timeout there fails at once as before.
pub(crate) fn run_gates_once(root: &Path, gates: &[&Gate], slots: &SlotEnv) -> Vec<GateResult> {
    run_gates_scoped(
        root,
        gates,
        slots,
        crate::slots::hold,
        None,
        &Retry::never(),
        false,
    )
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
    run_gates_scoped(
        root,
        gates,
        slots,
        crate::slots::guard::hold_timed,
        select,
        &Retry::here(),
        true,
    )
}

/// `run_gates` with the retry policy injected for fixtures.
pub(crate) fn run_gates_retrying(
    root: &Path,
    gates: &[&Gate],
    slots: &SlotEnv,
    take: TakeSlot,
    retry: &Retry,
) -> Vec<GateResult> {
    run_gates_scoped(root, gates, slots, take, None, retry, true)
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
    retry: &Retry,
    observe: bool,
) -> Vec<GateResult> {
    let mut docker: Option<bool> = None;
    gates
        .iter()
        .map(|gate| {
            if gate.capabilities.contains(&Capability::Docker)
                && !*docker.get_or_insert_with(docker_available)
            {
                GateResult {
                    gate: gate.id.clone(),
                    verdict: GateVerdict::Unavailable,
                    seconds: 0.0,
                    summary: "Docker is not reachable".into(),
                    tail: String::new(),
                    attempts: Vec::new(),
                }
            } else {
                run_one(root, gate, slots, take, select, retry, observe)
            }
        })
        .collect()
}

fn run_one(
    root: &Path,
    gate: &Gate,
    slots: &SlotEnv,
    take: TakeSlot,
    select: Option<&str>,
    retry: &Retry,
    observe: bool,
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
    let first = run_once(root, gate, &slot, select, &retry.load, observe);
    if !(retry.local && first.overloaded) {
        return first.result;
    }
    // The same budget again, once the load has fallen; a refused wait keeps the first timeout.
    let earlier = first.attempt;
    let mut result = first.result;
    let waited = if observe {
        crate::timeline::stage(
            root,
            "load-wait",
            serde_json::json!({"gate":gate.id,"retry":true}),
            || (retry.wait)(),
        )
    } else {
        (retry.wait)()
    };
    if let Err(reason) = waited {
        result.summary = format!("{}; not retried: {reason}", result.summary);
        result.attempts = vec![earlier];
        return result;
    }
    let later = run_once(root, gate, &slot, select, &retry.load, observe);
    let mut result = later.result;
    result.attempts = vec![earlier, later.attempt];
    result
}

/// One run of a gate's command under its budget.
struct Run {
    result: GateResult,
    attempt: Attempt,
    /// The load at the deadline said the machine was overloaded (only ever set on a timeout).
    overloaded: bool,
}

fn run_once(
    root: &Path,
    gate: &Gate,
    slot: &Guard,
    select: Option<&str>,
    reader: &load::Reader<'_>,
    observe: bool,
) -> Run {
    let mut command = Command::new("sh");
    command.arg("-c").arg(&gate.command).current_dir(root);
    command.envs(slot.env());
    if let (Some(base), "test-unit") = (select, gate.id.as_str()) {
        command.env("AOE_TEST_SELECT", base);
    }
    let stage = format!("gate/{}", gate.id);
    if observe {
        crate::timeline::mark(root, &stage, "start", serde_json::json!({}));
    }
    let captured = capture_command(
        command,
        Duration::from_secs(u64::from(gate.budget_s)),
        &Cancellation::default(),
    );
    if observe {
        crate::timeline::mark(
            root,
            &stage,
            "end",
            serde_json::json!({
                "ok":matches!(captured.exit, CaptureExit::Success), "timed_out":matches!(captured.exit, CaptureExit::Deadline)
            }),
        );
    }
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
    let timed_out = matches!(captured.exit, CaptureExit::Deadline);
    let load = if timed_out { reader() } else { None };
    let mut summary: String = last.chars().take(120).collect();
    if timed_out {
        summary = over_budget(gate.budget_s, &summary, &|| load);
    }
    let tail = if passed {
        String::new()
    } else {
        let lines: Vec<&str> = output.lines().collect();
        lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
    };
    let verdict = if passed {
        GateVerdict::Pass
    } else {
        GateVerdict::Fail
    };
    let seconds = captured.duration.as_secs_f32();
    Run {
        result: GateResult {
            gate: gate.id.clone(),
            verdict,
            seconds,
            summary,
            tail,
            attempts: Vec::new(),
        },
        attempt: Attempt {
            seconds,
            verdict,
            timed_out,
            load: load.as_ref().map(load::Load::note).unwrap_or_default(),
        },
        overloaded: load.is_some_and(|load| load.overloaded()),
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
