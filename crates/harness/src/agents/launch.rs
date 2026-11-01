//! Headless runtime launches: the bounded smoke probe and editing work for
//! the tester → implementer duel. Every launch carries the role in
//! `AOE_AGENT_ROLE`, which the judge trusts over the runtime's own `agent_type`.
//!
//! Codex reads `.codex/` from the main repository root, not from a linked
//! worktree, so a launch passes the committed `.codex/hooks.json` inline
//! (`-c hooks.<Event>=[…]`) and trusts the checkout for that invocation.
use super::Runtime;
use serde_json::Value;
use std::{fs, path::Path, process::Command};

pub(crate) const DSH_PACKAGE: &str = "@deepseek-ai/dsh@0.2.0-rc.2";

pub(crate) fn command(
    runtime: Runtime,
    root: &Path,
    role: &str,
    prompt: &str,
) -> Result<Command, String> {
    let mut command = match runtime {
        Runtime::Claude => {
            let mut c = Command::new("claude");
            c.args([
                "-p",
                "--output-format",
                "json",
                "--allowedTools=Bash",
                "--max-turns",
                "8",
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
                "workspace-write",
            ]);
            // In-process sub-agents multiply contexts and poll each other (`wait_agent`),
            // re-reading every context on each poll; harness agents are single sessions.
            c.args(["--disable", "multi_agent"]);
            c.arg("-c").arg(format!(
                "projects.{}.trust_level=\"trusted\"",
                toml_string(&root.display().to_string())
            ));
            for setting in codex_hooks(root)? {
                c.arg("-c").arg(setting);
            }
            c
        }
        Runtime::Dsh => {
            // In the harness records, which agents cannot rewrite before launch.
            let records = root.join(super::context::RECORDS);
            let patch = records.join("dsh-hooks.patch.yml");
            fs::create_dir_all(&records).map_err(|e| e.to_string())?;
            fs::write(&patch, dsh_patch(root)).map_err(|e| e.to_string())?;
            let mut c = Command::new("npx");
            // Launcher flags (`--profile`, `--patch`) precede the app's (`--json`).
            c.args(["--yes", DSH_PACKAGE, "--profile", "headless", "--patch"]);
            c.arg(patch).arg("--json");
            c
        }
        Runtime::Pi => {
            let mut c = Command::new("pi");
            c.args(["-p", "--mode", "json", "--no-session", "-a"]);
            c
        }
    };
    command
        .arg(prompt)
        .current_dir(root)
        .env("AOE_AGENT_ROLE", role);
    Ok(command)
}

/// Editing launch with the project hooks active; no smoke-only turn/tool limit.
pub(crate) fn work(
    runtime: Runtime,
    root: &Path,
    role: &str,
    prompt: &str,
    model: Option<&str>,
    effort: Option<&str>,
) -> Result<Command, String> {
    if runtime == Runtime::Dsh {
        return Err("dsh is not supported yet for duel".into());
    }
    let mut c = if runtime == Runtime::Claude {
        let mut c = Command::new("claude");
        c.args([
            "-p",
            "--output-format",
            "json",
            "--permission-mode",
            "dontAsk",
            "--allowedTools=Read,Grep,Glob,Bash,Edit,Write,MultiEdit",
            "--max-turns",
            "200",
        ]);
        if root.join(format!(".claude/agents/{role}.md")).is_file() {
            c.args(["--agent", role]);
        }
        c
    } else {
        // Reuse the smoke's trust and inline-hook setup, replacing its prompt.
        let smoke = command(runtime, root, role, "")?;
        let mut c = Command::new(smoke.get_program());
        let args: Vec<_> = smoke.get_args().collect();
        for arg in &args[..args.len() - 1] {
            c.arg(if *arg == "workspace-write" {
                std::ffi::OsStr::new("danger-full-access")
            } else {
                arg
            });
        }
        c
    };
    if runtime == Runtime::Codex {
        c.args(["-c", "approval_policy=\"never\""]);
    }
    if let Some(model) = model {
        match runtime {
            Runtime::Codex => {
                c.args(["-m", model]);
            }
            Runtime::Pi => {
                if let Some((provider, model)) = model.split_once('/') {
                    c.args(["--provider", provider, "--model", model]);
                } else {
                    c.args(["--model", model]);
                }
            }
            _ => {
                c.args(["--model", model]);
            }
        }
    }
    if let Some(effort) = effort {
        match runtime {
            Runtime::Codex => {
                c.arg("-c")
                    .arg(format!("model_reasoning_effort={}", toml_string(effort)));
            }
            Runtime::Pi => {
                c.args(["--thinking", effort]);
            }
            _ => {
                c.args(["--effort", effort]);
            }
        }
    }
    c.arg(prompt).current_dir(root).env("AOE_AGENT_ROLE", role);
    // Git hooks can launch the main session, but their index must not leak into agents.
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            c.env_remove(name);
        }
    }
    Ok(c)
}

/// The dsh patch that mounts the Claude hooks bridge, without a model.
pub(crate) fn dsh_hooks(root: &Path) -> String {
    format!(
        "- insert:\n    - name: '@deepseek-ai/dsh-hooks-claude-code'\n      config:\n        configPath: {}\n",
        // A JSON string is a valid YAML double-quoted scalar.
        toml_string(&root.join(".dsh/hooks.json").display().to_string())
    )
}

/// The DeepSeek bridge reads one absolute hook config per process. The model
/// comes from the person's dsh setup unless `AOE_DSH_PROVIDER`/`AOE_DSH_MODEL`
/// name one already authenticated there (for example `openai-codex`).
pub(crate) fn dsh_patch(root: &Path) -> String {
    let mut patch = dsh_hooks(root);
    if let (Ok(provider), Ok(model)) = (
        std::env::var("AOE_DSH_PROVIDER"),
        std::env::var("AOE_DSH_MODEL"),
    ) {
        patch.push_str(&format!(
            "- id: agent-default-model\n  config:\n    provider: {}\n    model: {}\n",
            toml_string(&provider),
            toml_string(&model)
        ));
    }
    patch
}

/// `.codex/hooks.json` as `-c hooks.<Event>=<inline TOML>` settings.
pub(crate) fn codex_hooks(root: &Path) -> Result<Vec<String>, String> {
    let path = root.join(".codex/hooks.json");
    let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let events = value
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or("`.codex/hooks.json` has no `hooks` object")?;
    Ok(events
        .iter()
        .map(|(event, groups)| format!("hooks.{event}={}", toml_inline(groups)))
        .collect())
}

fn toml_string(text: &str) -> String {
    // A JSON string is a valid TOML basic string for the characters we emit.
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}

fn toml_inline(value: &Value) -> String {
    match value {
        Value::Null => "\"\"".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => toml_string(s),
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(toml_inline).collect::<Vec<_>>().join(",")
        ),
        Value::Object(map) => format!(
            "{{{}}}",
            map.iter()
                .map(|(key, value)| format!("{}={}", toml_string(key), toml_inline(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}
