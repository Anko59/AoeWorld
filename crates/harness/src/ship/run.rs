//! Run registry gates as written (`sh -c <command>` from the checkout root)
//! with their budgets. Shared by the agent stop rule and `ship`.
use crate::{
    gates::registry::{Capability, Gate},
    process::{Cancellation, CaptureExit, capture_command},
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

pub(crate) fn run_gates(root: &Path, gates: &[&Gate]) -> Vec<GateResult> {
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
                };
            }
            run_one(root, gate)
        })
        .collect()
}

fn run_one(root: &Path, gate: &Gate) -> GateResult {
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
        .trim();
    let passed = matches!(captured.exit, CaptureExit::Success);
    let mut summary: String = last.chars().take(120).collect();
    if matches!(captured.exit, CaptureExit::Deadline) {
        summary = format!("over its {}s budget; {summary}", gate.budget_s);
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
    }
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
