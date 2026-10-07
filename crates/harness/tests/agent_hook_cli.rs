//! The Claude Code hook through the real binary: JSON on stdin, JSON on stdout,
//! exit 0 whatever the decision.
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn hook(root: &Path, event: &str, input: &[u8]) -> (bool, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aoe-harness"));
    for (name, _) in std::env::vars_os() {
        if name.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(name);
        }
    }
    let mut child = command
        .args(["claude-hook", event])
        .env("AOE_CLAUDE_HOOK_ROOT", root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    (
        output.status.success(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

fn checkout() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let status = Command::new("git")
        .args(["init", "--quiet", "--template="])
        .current_dir(root.path())
        .env_remove("GIT_DIR")
        .status()
        .unwrap();
    assert!(status.success());
    root
}

fn bash(root: &Path, agent: Option<&str>, command: &str) -> serde_json::Value {
    let mut input = serde_json::json!({
        "session_id": "s",
        "hook_event_name": "PreToolUse",
        "cwd": root,
        "tool_name": "Bash",
        "tool_input": {"command": command},
    });
    if let Some(agent) = agent {
        input["agent_type"] = agent.into();
    }
    let (ok, stdout) = hook(root, "pre-tool-use", input.to_string().as_bytes());
    assert!(ok, "the hook always exits 0");
    if stdout.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&stdout).unwrap()
    }
}

#[test]
fn allowed_calls_print_nothing_and_denials_name_the_remedy() {
    let root = checkout();
    assert_eq!(
        bash(root.path(), None, "make lint"),
        serde_json::Value::Null
    );
    let denied = bash(root.path(), None, "git push origin dev");
    let output = &denied["hookSpecificOutput"];
    assert_eq!(output["hookEventName"], "PreToolUse");
    assert_eq!(output["permissionDecision"], "deny");
    let reason = output["permissionDecisionReason"].as_str().unwrap();
    assert!(reason.contains("pull request against `dev`"), "{reason}");
    let tester = bash(root.path(), Some("tester"), "git commit -m x");
    assert_eq!(tester["hookSpecificOutput"]["permissionDecision"], "deny");
}

#[test]
fn garbage_input_is_denied_with_exit_zero() {
    let root = checkout();
    let (ok, stdout) = hook(root.path(), "pre-tool-use", b"\xff not json");
    assert!(ok);
    let answer: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(answer["hookSpecificOutput"]["permissionDecision"], "deny");
}
