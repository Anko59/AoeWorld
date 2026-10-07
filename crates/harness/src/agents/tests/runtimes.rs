use super::{
    super::{
        Event, Runtime,
        context::Access,
        respond,
        runtime::{Call, call, patch_targets},
    },
    Fixture,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

fn hook(
    fixture: &Fixture,
    runtime: Runtime,
    role: Option<&str>,
    tool: &str,
    input: Value,
) -> Option<Value> {
    let event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": fixture.root(),
        "tool_name": tool,
        "tool_input": input,
    });
    let bytes = serde_json::to_vec(&event).expect("json");
    respond(
        runtime,
        Event::PreToolUse,
        Some(&bytes),
        fixture.root(),
        role,
    )
}

fn denied(answer: &Option<Value>) -> bool {
    answer
        .as_ref()
        .is_some_and(|a| a["hookSpecificOutput"]["permissionDecision"] == "deny")
}

const PATCH_GATES: &str =
    "*** Begin Patch\n*** Update File: gates/registry.json\n@@\n-x\n+y\n*** End Patch\n";
const PATCH_CODE: &str =
    "*** Begin Patch\n*** Add File: crates/map/src/new.rs\n+pub fn f() {}\n*** End Patch\n";

#[test]
fn codex_patches_name_every_file_they_touch() {
    let patch = "*** Begin Patch\n*** Add File: a.rs\n+x\n*** Update File: b.rs\n*** Move to: c.rs\n@@\n*** Delete File: d.rs\n*** End Patch\n";
    assert_eq!(
        patch_targets(patch),
        vec![
            ("a.rs".into(), Access::Put),
            ("b.rs".into(), Access::Put),
            ("c.rs".into(), Access::Put),
            ("d.rs".into(), Access::Remove),
        ]
    );
}

#[test]
fn codex_patches_and_commands_get_the_same_rules() {
    let fixture = Fixture::new();
    let patch = |text: &str| json!({ "command": text });
    assert!(!denied(&hook(
        &fixture,
        Runtime::Codex,
        None,
        "apply_patch",
        patch(PATCH_GATES)
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "apply_patch",
        patch(PATCH_GATES)
    )));
    assert!(!denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "apply_patch",
        patch(PATCH_CODE)
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("tester"),
        "apply_patch",
        patch(PATCH_CODE)
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "apply_patch",
        patch("no headers")
    )));
    let push = json!({ "command": "git push origin dev" });
    assert!(denied(&hook(&fixture, Runtime::Codex, None, "Bash", push)));
}

#[test]
fn deepseek_tools_are_mapped_including_workdir() {
    let fixture = Fixture::new();
    let bash = |command: &str, workdir: &str| json!({ "command": command, "workdir": workdir });
    assert!(!denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "bash",
        bash("echo x > out.txt", "/tmp")
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "bash",
        bash("echo x > registry.json", "gates")
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        None,
        "bash",
        bash("cargo build", ".")
    )));
    let editor = |command: &str, path: &str| json!({ "command": command, "path": path });
    assert!(!denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("reviewer"),
        "str_replace_editor",
        editor("view", "gates/registry.json")
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "str_replace_editor",
        editor("create", "gates/x.json")
    )));
    let write = json!({ "file_path": "crates/map/src/tests.rs", "content": "" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "write",
        write
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "pwsh",
        json!({ "command": "ls" })
    )));
}

#[test]
fn pi_tools_use_path_and_the_launched_role_wins() {
    let fixture = Fixture::new();
    let edit = json!({ "path": "crates/map/src/lib.rs", "edits": [] });
    assert!(denied(&hook(
        &fixture,
        Runtime::Pi,
        Some("tester"),
        "edit",
        edit.clone()
    )));
    assert!(!denied(&hook(
        &fixture,
        Runtime::Pi,
        Some("implementer"),
        "edit",
        edit
    )));
    // A launched role restricts even when the runtime reports a generic agent type.
    let event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": fixture.root(),
        "agent_type": "general-purpose",
        "tool_name": "Write",
        "tool_input": { "file_path": "crates/map/src/lib.rs", "content": "" },
    });
    let bytes = serde_json::to_vec(&event).expect("json");
    let answer = respond(
        Runtime::Claude,
        Event::PreToolUse,
        Some(&bytes),
        fixture.root(),
        Some("reviewer"),
    );
    assert!(denied(&answer));
}

#[test]
fn reads_and_unknown_tools_are_not_judged() {
    let cwd = Path::new("/tmp");
    assert_eq!(
        call(Runtime::Pi, "read", &json!({ "path": "x" }), Some(cwd)),
        Call::Other
    );
    assert_eq!(
        call(Runtime::Codex, "update_plan", &json!({}), Some(cwd)),
        Call::Other
    );
    assert!(matches!(
        call(Runtime::Codex, "Bash", &json!({}), Some(cwd)),
        Call::Opaque(_)
    ));
}

#[test]
fn codex_edits_are_checked_against_the_structure_rules() {
    let fixture = Fixture::new();
    fs::write(
        fixture.root().join("crates/map/src/lib.rs"),
        "line\n".repeat(501),
    )
    .expect("long");
    let event = json!({
        "session_id": "s1",
        "hook_event_name": "PostToolUse",
        "cwd": fixture.root(),
        "tool_name": "apply_patch",
        "tool_input": { "command": "*** Begin Patch\n*** Update File: crates/map/src/lib.rs\n*** End Patch\n" },
    });
    let bytes = serde_json::to_vec(&event).expect("json");
    let answer = respond(
        Runtime::Codex,
        Event::PostToolUse,
        Some(&bytes),
        fixture.root(),
        None,
    )
    .expect("block");
    assert_eq!(answer["decision"], "block");
}

#[test]
fn codex_launches_carry_the_committed_hooks_inline() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let settings = super::super::launch::codex_hooks(&root).expect("hooks.json");
    let pre = settings
        .iter()
        .find(|s| s.starts_with("hooks.PreToolUse="))
        .expect("PreToolUse");
    assert!(pre.contains(r#""matcher"="^(Bash|apply_patch)$""#), "{pre}");
    assert!(pre.contains("harness.sh codex pre-tool-use"), "{pre}");
    assert!(settings.iter().any(|s| s.starts_with("hooks.Stop=")));
}

#[test]
fn roles_from_the_launcher_and_the_runtime_narrow_each_other() {
    use super::super::role::Role;
    assert_eq!(Role::resolve(None, None), Role::Main);
    assert_eq!(Role::resolve(Some(""), Some("tester")), Role::Tester);
    assert_eq!(Role::resolve(Some("main"), Some("tester")), Role::Tester);
    assert_eq!(Role::resolve(Some("implementer"), None), Role::Implementer);
    assert_eq!(
        Role::resolve(Some("implementer"), Some("general-purpose")),
        Role::Implementer
    );
    assert_eq!(
        Role::resolve(Some("implementer"), Some("reviewer")),
        Role::Reviewer
    );
    assert_eq!(
        Role::resolve(Some("tester"), Some("implementer")),
        Role::Reviewer
    );
    assert_eq!(Role::resolve(Some("general-purpose"), None), Role::Other);
}

#[test]
fn smoke_verdicts_never_hide_a_protected_write() {
    use super::super::smoke::{protected_denial_seen, verdict};
    assert_eq!(verdict(false, false, false, true), "FAIL");
    assert_eq!(verdict(false, true, true, true), "FAIL");
    assert_eq!(verdict(true, false, true, true), "INCOMPLETE");
    assert_eq!(verdict(true, true, true, false), "INCOMPLETE");
    assert_eq!(verdict(true, true, false, true), "FAIL");
    assert_eq!(verdict(true, true, true, true), "PASS");
    let reason = "AoeWorld harness: `gates/agent-smoke.txt` is in the protected gates class";
    assert!(protected_denial_seen(reason));
    assert!(!protected_denial_seen(
        "AoeWorld harness: `cargo` is not run on the host"
    ));
    let claude = json!({
        "result": "done; gates/agent-smoke.txt",
        "permission_denials": [{"tool_name": "Bash", "tool_input": {"command": "cargo --version"}}],
    });
    assert!(!protected_denial_seen(&claude.to_string()));
    let claude = json!({
        "permission_denials": [{"tool_name": "Bash", "tool_input": {"command": "echo smoke > gates/agent-smoke.txt"}}],
    });
    assert!(protected_denial_seen(&claude.to_string()));
}

#[test]
fn launches_carry_the_role_and_quote_their_paths() {
    use super::super::launch::{command, dsh_patch};
    let patch = dsh_patch(Path::new("/odd: path #1"));
    assert!(
        patch.contains(r#"configPath: "/odd: path #1/.dsh/hooks.json""#),
        "{patch}"
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let codex = command(Runtime::Codex, &root, "tester", "do it").expect("codex");
    let args: Vec<String> = codex
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.iter().any(|a| a.starts_with("hooks.PreToolUse=")),
        "{args:?}"
    );
    assert_eq!(args.last().map(String::as_str), Some("do it"));
    let role = codex
        .get_envs()
        .find(|(name, _)| *name == "AOE_AGENT_ROLE")
        .and_then(|(_, value)| value);
    assert_eq!(role.and_then(|v| v.to_str()), Some("tester"));
}

#[test]
fn runtime_path_rewrites_and_hidden_working_directories_are_closed() {
    let fixture = Fixture::new();
    // pi strips a leading `@` before writing; file: URLs are refused.
    let at = json!({ "path": "@gates/registry.json", "content": "{}" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Pi,
        Some("implementer"),
        "write",
        at
    )));
    let url = json!({ "path": "file:///etc/x", "content": "" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Pi,
        Some("implementer"),
        "write",
        url
    )));
    // Codex trims hunk headers on both sides.
    let padded = "*** Begin Patch\n*** Add File: crates/map/src/ok.rs\n+x\n  *** Add File: gates/evil.json\n+{}\n*** End Patch\n";
    assert!(denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "apply_patch",
        json!({ "command": padded })
    )));
    // Codex runs Bash in a `workdir` its hooks never see: agents write absolutely.
    let relative = json!({ "command": "echo x > registry.json" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "Bash",
        relative.clone()
    )));
    assert!(!denied(&hook(
        &fixture,
        Runtime::Codex,
        None,
        "Bash",
        relative
    )));
    let absolute = json!({ "command": "echo x > /tmp/aoe-codex-probe.txt" });
    assert!(!denied(&hook(
        &fixture,
        Runtime::Codex,
        Some("implementer"),
        "Bash",
        absolute
    )));
    // dsh's persistent shell keeps `cd`; agents pass workdir instead.
    for wrapped in [
        "builtin cd gates",
        "command cd gates",
        "time cd gates",
        "ls; pushd gates",
    ] {
        let call = json!({ "command": wrapped });
        assert!(
            denied(&hook(
                &fixture,
                Runtime::Dsh,
                Some("implementer"),
                "bash",
                call
            )),
            "{wrapped}"
        );
    }
    let cd = json!({ "command": "cd gates" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "bash",
        cd.clone()
    )));
    assert!(!denied(&hook(&fixture, Runtime::Dsh, None, "bash", cd)));
    // Tools that run model-written code are refused.
    assert!(denied(&hook(
        &fixture,
        Runtime::Dsh,
        Some("implementer"),
        "workflow",
        json!({ "script": "x" })
    )));
    assert!(denied(&hook(
        &fixture,
        Runtime::Pi,
        Some("implementer"),
        "powershell",
        json!({ "command": "x" })
    )));
    // Role names are matched case-insensitively.
    let write = json!({ "file_path": "crates/map/src/lib.rs", "content": "" });
    assert!(denied(&hook(
        &fixture,
        Runtime::Claude,
        Some("Tester"),
        "Write",
        write
    )));
}
