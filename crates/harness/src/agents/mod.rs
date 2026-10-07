//! One judge for every coding-agent runtime. Claude Code, Codex, DeepSeek
//! Harness and pi send their hook events through `.agents/hooks/harness.sh` to
//! `aoe-harness agent-hook --runtime <runtime> <event>`. It judges each tool call
//! by the caller's role, checks edited files, applies the stop rule and gives
//! context at session start. It always exits 0; the answer is Claude-shaped JSON
//! on stdout (Codex and the DeepSeek bridge share that protocol, and the pi
//! extension translates it), empty for allow.
//!
//! Threat model: against subagents every rule holds, and an escape is a bug.
//! Against the main session (a person present) the rules catch mistakes and
//! shortcuts; a construction built on purpose to defeat them is a documented
//! limit, backstopped by the Git hooks, CI and the person who merges. The policy
//! reads shell text and the files commands name, never the code an interpreter
//! runs. See docs/agent-runtimes.md.
mod args;
mod bash;
mod context;
mod edit;
mod paths;
mod role;
mod rules;
mod runtime;
mod session;
mod shell;
mod smoke;
mod stop;
#[cfg(test)]
mod tests;

use context::{Access, Context};
use role::Role;
use runtime::Call;
pub(crate) use runtime::Runtime;
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

/// Agent-runtime commands, flattened into the harness CLI.
#[derive(clap::Subcommand)]
pub(crate) enum Commands {
    /// Hook JSON on stdin, answer on stdout (`claude-hook` is the old name).
    #[command(alias = "claude-hook")]
    AgentHook {
        #[arg(long, value_enum, default_value = "claude")]
        runtime: Runtime,
        #[arg(value_enum)]
        event: Event,
    },
    /// Run a runtime's CLI headless and check its tool calls reach the judge.
    AgentSmoke {
        #[arg(long, value_enum)]
        runtime: Runtime,
    },
}

pub(crate) fn execute(command: Commands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Commands::AgentHook { runtime, event } => run(runtime, event),
        Commands::AgentSmoke { runtime } => smoke::check(runtime, Path::new(".")),
    }
}

fn run(runtime: Runtime, event: Event) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    let read = std::io::stdin()
        .take(INPUT_LIMIT as u64 + 1)
        .read_to_end(&mut bytes);
    let input = (read.is_ok() && bytes.len() <= INPUT_LIMIT).then_some(bytes.as_slice());
    let root = std::env::var_os("AOE_AGENT_HOOK_ROOT")
        .or_else(|| std::env::var_os("CLAUDE_PROJECT_DIR"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let role = std::env::var("AOE_AGENT_ROLE").ok();
    if let Some(answer) = respond(runtime, event, input, &root, role.as_deref()) {
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
/// `launched_role` is `AOE_AGENT_ROLE` from the process that launched the agent
/// (the harness sets it for each role it starts); it wins over `agent_type`,
/// which Codex and DeepSeek Harness do not report for subagents.
pub(crate) fn respond(
    runtime: Runtime,
    event: Event,
    bytes: Option<&[u8]>,
    root: &Path,
    launched_role: Option<&str>,
) -> Option<Value> {
    let input: Option<Input> = bytes.and_then(|b| serde_json::from_slice(b).ok());
    let Some(input) = input.filter(|i| event.accepts(&i.hook_event_name)) else {
        // Unreadable or oversized input is judged as an agent's: denied.
        return (event == Event::PreToolUse).then(|| {
            deny("the hook input was unreadable, oversized or for another event; retry the call")
        });
    };
    let role = Role::from_agent(launched_role.or(input.agent_type.as_deref()));
    let context = match Context::new(root, role, input.scratchpad_dir.as_deref()) {
        Ok(context) => context,
        Err(error) if event == Event::PreToolUse => return Some(deny(error)),
        Err(error) => {
            return Some(json!({ "systemMessage": format!("AoeWorld harness: {error}") }));
        }
    };
    let cwd = input.cwd.as_deref();
    match event {
        Event::PreToolUse => pre_tool(&context, runtime, &input, cwd).err().map(deny),
        Event::PostToolUse => {
            let Call::Writes(files) = tool_call(runtime, &input, cwd) else {
                return None;
            };
            let findings: Vec<String> = files
                .iter()
                .filter(|(_, access)| *access == Access::Put)
                .flat_map(|(file, _)| edit::findings(&context, cwd, file))
                .collect();
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
        Event::SessionStart => Some(session::start(
            &context,
            runtime,
            &input.session_id,
            input.source.as_deref(),
        )),
        Event::PreCompact => session::pre_compact(&context, &input.session_id)
            .err()
            .map(|e| json!({ "systemMessage": format!("AoeWorld harness could not save progress: {e}") })),
    }
}

fn tool_call(runtime: Runtime, input: &Input, cwd: Option<&Path>) -> Call {
    match (input.tool_name.as_deref(), input.tool_input.as_ref()) {
        (Some(tool), Some(tool_input)) => runtime::call(runtime, tool, tool_input, cwd),
        (Some(_), None) => Call::Opaque("a tool call without input"),
        (None, _) => Call::Opaque("a tool call without a name"),
    }
}

fn pre_tool(
    context: &Context,
    runtime: Runtime,
    input: &Input,
    cwd: Option<&Path>,
) -> context::Verdict {
    match tool_call(runtime, input, cwd) {
        Call::Shell { command, cwd } => bash::judge(context, &command, cwd.as_deref()),
        Call::Writes(files) if files.is_empty() && context.role.is_agent() => {
            Err("this edit names no files; edit named files".into())
        }
        Call::Writes(files) => files.iter().try_for_each(|(file, access)| {
            context.write(cwd, &shell::Word::literal(file), *access)
        }),
        Call::Opaque(why) => Err(why.into()),
        Call::Other => Ok(()),
    }
}
