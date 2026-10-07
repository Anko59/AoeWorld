//! Claude Code adapter. The committed `.claude/settings.json` sends every hook
//! event through `.claude/hooks/harness.sh` to `aoe-harness claude-hook <event>`.
//! It judges each tool call by the caller's role, checks edited files, applies
//! the stop rule and gives context at session start. It always exits 0; the
//! answer is JSON on stdout, empty for allow.
//!
//! Threat model: against subagents every rule holds, and an escape is a bug.
//! Against the main session (a person present) the rules catch mistakes and
//! shortcuts; a construction built on purpose to defeat them is a documented
//! limit, backstopped by the Git hooks, CI and the person who merges. The policy
//! reads shell text and the files commands name, never the code an interpreter
//! runs. See docs/claude-code.md.
mod args;
mod bash;
mod context;
mod edit;
mod paths;
mod role;
mod rules;
mod session;
mod shell;
mod stop;
#[cfg(test)]
mod tests;

use context::{Access, Context};
use role::Role;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

const INPUT_LIMIT: usize = 4 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum Event {
    SessionStart,
    PreToolUse,
    PostToolUse,
    /// Stop and SubagentStop.
    Stop,
    PreCompact,
}

impl Event {
    fn accepts(self, name: &str) -> bool {
        match self {
            Self::SessionStart => name == "SessionStart",
            Self::PreToolUse => name == "PreToolUse",
            Self::PostToolUse => name == "PostToolUse",
            Self::Stop => name == "Stop" || name == "SubagentStop",
            Self::PreCompact => name == "PreCompact",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct Input {
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    hook_event_name: String,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    agent_type: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<Value>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    scratchpad_dir: Option<PathBuf>,
}

pub(crate) fn run(event: Event) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    let read = std::io::stdin()
        .take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes);
    let input = (read.is_ok() && bytes.len() <= INPUT_LIMIT).then_some(bytes.as_slice());
    let root = std::env::var_os("AOE_CLAUDE_HOOK_ROOT")
        .or_else(|| std::env::var_os("CLAUDE_PROJECT_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    if let Some(answer) = respond(event, input, &root) {
        println!("{answer}");
    }
    Ok(())
}

fn deny(reason: impl Into<String>) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!("AoeWorld harness: {}", reason.into()),
        }
    })
}

/// The hook's answer for one event, `None` meaning allow / nothing to add.
pub(crate) fn respond(event: Event, bytes: Option<&[u8]>, root: &Path) -> Option<Value> {
    let input: Option<Input> = bytes.and_then(|b| serde_json::from_slice(b).ok());
    let Some(input) = input.filter(|i| event.accepts(&i.hook_event_name)) else {
        // Unreadable or oversized input is judged as an agent's: denied.
        return (event == Event::PreToolUse).then(|| {
            deny("the hook input was unreadable, oversized or for another event; retry the call")
        });
    };
    let role = Role::from_agent(input.agent_type.as_deref());
    let context = match Context::new(root, role, input.scratchpad_dir.as_deref()) {
        Ok(context) => context,
        Err(error) if event == Event::PreToolUse => return Some(deny(error)),
        Err(error) => {
            return Some(json!({ "systemMessage": format!("AoeWorld harness: {error}") }));
        }
    };
    let cwd = input.cwd.as_deref();
    match event {
        Event::PreToolUse => pre_tool(&context, &input, cwd).err().map(deny),
        Event::PostToolUse => {
            let file = target(&input)?;
            let findings = edit::findings(&context, cwd, file);
            (!findings.is_empty()).then(|| {
                json!({
                    "decision": "block",
                    "reason": format!(
                        "AoeWorld structure rules (edit cadence): {}. Fix it now; never raise a limit to pass.",
                        findings.join("; ")
                    ),
                })
            })
        }
        Event::Stop => stop::respond(&context, &input.session_id, input.agent_id.as_deref()),
        Event::SessionStart => Some(session::start(&context, &input.session_id, input.source.as_deref())),
        Event::PreCompact => session::pre_compact(&context, &input.session_id)
            .err()
            .map(|e| json!({ "systemMessage": format!("AoeWorld harness could not save progress: {e}") })),
    }
}

/// The file an Edit, Write, MultiEdit or NotebookEdit call changes.
fn target(input: &Input) -> Option<&str> {
    let tool_input = input.tool_input.as_ref()?;
    match input.tool_name.as_deref()? {
        "Edit" | "Write" | "MultiEdit" => tool_input.get("file_path")?.as_str(),
        "NotebookEdit" => tool_input.get("notebook_path")?.as_str(),
        _ => None,
    }
}

fn pre_tool(context: &Context, input: &Input, cwd: Option<&Path>) -> context::Verdict {
    match input.tool_name.as_deref() {
        Some("Bash") => {
            let command = input
                .tool_input
                .as_ref()
                .and_then(|t| t.get("command"))
                .and_then(Value::as_str)
                .ok_or("a Bash call without a command string")?;
            bash::judge(context, command, cwd)
        }
        Some("Edit" | "Write" | "MultiEdit" | "NotebookEdit") => {
            let file = target(input).ok_or("an edit without a file path")?;
            context.write(cwd, &shell::Word::literal(file), Access::Put)
        }
        _ => Ok(()),
    }
}
