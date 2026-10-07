//! Launch reviewers headless on a runtime, with the tier's model and effort,
//! `AOE_AGENT_ROLE=reviewer` (the hooks keep them read-only) and read-only
//! tools where the runtime allows a tool list. Reviewers of one round run in
//! parallel; each answer is the JSON between the AOE-REVIEW markers.
use super::{config::Model, protocol};
use crate::{
    agents::{Runtime, launch},
    process::{Cancellation, CaptureExit, capture_command},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

/// A session that ran to the end but printed no answer between the markers.
pub(crate) const NO_ANSWER: &str = "no AOE-REVIEW answer";

/// One reviewer session's budget.
const BUDGET: Duration = Duration::from_secs(1800);

/// Numbers each session's private dsh patch file within this process.
static SESSION: AtomicU64 = AtomicU64::new(0);

/// The reviewer command, and the session's own patch file to remove after
/// it (dsh only: parallel sessions never share one).
pub(crate) fn command(
    runtime: Runtime,
    root: &Path,
    model: &Model,
    prompt: &str,
) -> Result<(Command, Option<PathBuf>), String> {
    let mut scratch = None;
    let mut command = match runtime {
        Runtime::Claude => {
            let mut c = Command::new("claude");
            c.args(["-p", "--output-format", "json", "--max-turns", "60"]);
            c.args([
                "--allowedTools=Read,Grep,Glob,Bash",
                "--model",
                &model.model,
                "--effort",
                &model.effort,
            ]);
            c
        }
        Runtime::Codex => {
            let mut c = Command::new("codex");
            c.args([
                "exec",
                "--json",
                "--dangerously-bypass-hook-trust",
                "-s",
                "read-only",
            ]);
            c.args([
                "-m",
                &model.model,
                "-c",
                &format!("model_reasoning_effort=\"{}\"", model.effort),
            ]);
            c.arg("-c").arg(format!(
                "projects.{}.trust_level=\"trusted\"",
                json_string(&root.display().to_string())
            ));
            for setting in launch::codex_hooks(root)? {
                c.arg("-c").arg(setting);
            }
            c
        }
        Runtime::Dsh => {
            let (provider, name) = model
                .model
                .split_once('/')
                .ok_or("dsh models are `provider/model`")?;
            let records = root.join(".cache/agent-hook");
            fs::create_dir_all(&records).map_err(|e| e.to_string())?;
            let patch = records.join(format!(
                "dsh-review-{}-{}.patch.yml",
                std::process::id(),
                SESSION.fetch_add(1, Ordering::Relaxed)
            ));
            let body = format!(
                "{}- id: agent-default-model\n  config:\n    provider: {}\n    model: {}\n    reasoningEffort: {}\n",
                launch::dsh_hooks(root),
                json_string(provider),
                json_string(name),
                json_string(&model.effort)
            );
            fs::write(&patch, body).map_err(|e| e.to_string())?;
            scratch = Some(patch.clone());
            let mut c = Command::new("npx");
            c.args([
                "--yes",
                launch::DSH_PACKAGE,
                "--profile",
                "headless",
                "--patch",
            ]);
            c.arg(patch).arg("--json");
            c
        }
        Runtime::Pi => {
            let mut c = Command::new("pi");
            c.args([
                "-p",
                "--mode",
                "json",
                "--no-session",
                "-a",
                "--tools",
                "read,grep,find,ls,bash",
            ]);
            c.arg("--model")
                .arg(format!("{}:{}", model.model, model.effort));
            c
        }
    };
    command
        .arg(prompt)
        .current_dir(root)
        .env("AOE_AGENT_ROLE", "reviewer");
    Ok((command, scratch))
}

fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

/// The reviewer's answer text, or why there is none.
pub(crate) fn ask(
    runtime: Runtime,
    root: &Path,
    model: &Model,
    prompt: &str,
) -> Result<String, String> {
    let (command, scratch) = command(runtime, root, model, prompt)?;
    let captured = capture_command(command, BUDGET, &Cancellation::default());
    if let Some(patch) = scratch {
        let _ = fs::remove_file(patch);
    }
    let stdout = String::from_utf8_lossy(&captured.stdout);
    let stderr = String::from_utf8_lossy(&captured.stderr);
    answer(&stdout, &stderr, &captured.exit)
}

/// The answer of a session: read from stdout only (where every runtime prints
/// its result; stderr may echo the prompt) and only after a clean exit.
pub(crate) fn answer(stdout: &str, stderr: &str, exit: &CaptureExit) -> Result<String, String> {
    let tail = |text: &str| -> String {
        let chars: Vec<char> = text.chars().collect();
        chars[chars.len().saturating_sub(400)..].iter().collect()
    };
    match (exit, protocol::extract(stdout)) {
        (CaptureExit::Success, Some(answer)) => Ok(answer),
        (CaptureExit::Deadline, _) => Err(format!("reviewer exceeded {}s", BUDGET.as_secs())),
        (CaptureExit::Success, None) => Err(format!("{NO_ANSWER}: {}", tail(stdout))),
        (exit, _) => Err(format!(
            "reviewer session failed ({exit:?}): {}",
            tail(&format!("{stdout}{stderr}"))
        )),
    }
}

/// Ask every prompt in parallel; answers come back in prompt order.
pub(crate) fn ask_all(
    runtime: Runtime,
    root: &Path,
    model: &Model,
    prompts: Vec<String>,
) -> Vec<Result<String, String>> {
    thread::scope(|scope| {
        let handles: Vec<_> = prompts
            .iter()
            .map(|prompt| scope.spawn(move || ask(runtime, root, model, prompt)))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("reviewer thread panicked".into()))
            })
            .collect()
    })
}
