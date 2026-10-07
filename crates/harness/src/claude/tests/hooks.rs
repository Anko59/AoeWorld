use super::{
    super::{Event, respond, role::Role, shell},
    Fixture,
};
use serde_json::{Value, json};
use std::fs;

fn call(fixture: &Fixture, event: Event, input: Value) -> Option<Value> {
    let bytes = serde_json::to_vec(&input).expect("json");
    respond(event, Some(&bytes), fixture.root())
}

fn decision(answer: &Option<Value>) -> Option<&str> {
    answer.as_ref()?["hookSpecificOutput"]["permissionDecision"].as_str()
}

fn tool(fixture: &Fixture, agent: Option<&str>, name: &str, input: Value) -> Option<Value> {
    let mut event = json!({
        "session_id": "s1",
        "hook_event_name": "PreToolUse",
        "cwd": fixture.root(),
        "tool_name": name,
        "tool_input": input,
    });
    if let Some(agent) = agent {
        event["agent_id"] = json!("a1");
        event["agent_type"] = json!(agent);
    }
    call(fixture, Event::PreToolUse, event)
}

#[test]
fn unreadable_oversized_or_mismatched_input_is_denied() {
    let fixture = Fixture::new();
    assert_eq!(
        decision(&respond(Event::PreToolUse, None, fixture.root())),
        Some("deny")
    );
    assert_eq!(
        decision(&respond(
            Event::PreToolUse,
            Some(b"{not json"),
            fixture.root()
        )),
        Some("deny")
    );
    let other = call(
        &fixture,
        Event::PreToolUse,
        json!({"hook_event_name": "PostToolUse"}),
    );
    assert_eq!(decision(&other), Some("deny"));
    let no_command = tool(&fixture, None, "Bash", json!({}));
    assert_eq!(decision(&no_command), Some("deny"));
    // Other events never block on bad input.
    assert_eq!(respond(Event::Stop, None, fixture.root()), None);
}

#[test]
fn the_role_comes_from_agent_type_alone() {
    assert_eq!(Role::from_agent(None), Role::Main);
    assert_eq!(Role::from_agent(Some("tester")), Role::Tester);
    assert_eq!(Role::from_agent(Some("implementer")), Role::Implementer);
    assert_eq!(Role::from_agent(Some("Explore")), Role::Reviewer);
    assert_eq!(Role::from_agent(Some("rv-security")), Role::Reviewer);
    assert_eq!(Role::from_agent(Some("general-purpose")), Role::Other);
    let fixture = Fixture::new();
    let edit = json!({"file_path": "crates/map/src/lib.rs", "old_string": "x", "new_string": "y"});
    assert_eq!(tool(&fixture, None, "Edit", edit.clone()), None);
    assert_eq!(
        tool(&fixture, Some("implementer"), "Edit", edit.clone()),
        None
    );
    let answer = tool(&fixture, Some("tester"), "Edit", edit);
    assert_eq!(decision(&answer), Some("deny"));
    let reason = answer.expect("deny")["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(reason.contains("Tester writes tests only"), "{reason}");
}

#[test]
fn edits_and_writes_are_judged_by_landing_path() {
    let fixture = Fixture::new();
    let gates = json!({"file_path": fixture.root().join("gates/registry.json"), "content": "{}"});
    assert_eq!(
        decision(&tool(
            &fixture,
            Some("general-purpose"),
            "Write",
            gates.clone()
        )),
        Some("deny")
    );
    assert_eq!(tool(&fixture, None, "Write", gates), None);
    let hook = json!({"file_path": ".git/hooks/pre-commit", "content": "exit 0"});
    assert_eq!(decision(&tool(&fixture, None, "Write", hook)), Some("deny"));
    let notebook = json!({"notebook_path": "baselines/x.ipynb", "new_source": ""});
    assert_eq!(
        decision(&tool(
            &fixture,
            Some("implementer"),
            "NotebookEdit",
            notebook
        )),
        Some("deny")
    );
    assert_eq!(
        tool(
            &fixture,
            Some("reviewer"),
            "Read",
            json!({"file_path": "gates/registry.json"})
        ),
        None
    );
}

#[test]
fn edited_files_are_checked_against_the_structure_rules() {
    let fixture = Fixture::new();
    let root = fixture.root();
    fs::write(root.join("crates/map/src/lib.rs"), "line\n".repeat(501)).expect("long file");
    for index in 0..14 {
        fs::write(root.join(format!("crates/map/src/m{index}.rs")), "\n").expect("module");
    }
    let event = json!({
        "session_id": "s1",
        "hook_event_name": "PostToolUse",
        "cwd": root,
        "tool_name": "Edit",
        "tool_input": {"file_path": "crates/map/src/lib.rs"},
    });
    let answer = call(&fixture, Event::PostToolUse, event).expect("block");
    assert_eq!(answer["decision"], "block");
    let reason = answer["reason"].as_str().unwrap_or_default();
    assert!(reason.contains("501 lines"), "{reason}");
    assert!(reason.contains("code/config files (max 14)"), "{reason}");
    fs::write(root.join("docs/index.md"), "short\n").expect("doc");
    let fine = json!({
        "session_id": "s1",
        "hook_event_name": "PostToolUse",
        "cwd": root,
        "tool_name": "Write",
        "tool_input": {"file_path": "docs/index.md"},
    });
    assert_eq!(call(&fixture, Event::PostToolUse, fine), None);
}

#[test]
fn session_start_gives_context_and_compaction_progress_comes_back() {
    let fixture = Fixture::new();
    let compact =
        json!({"session_id": "s-9", "hook_event_name": "PreCompact", "cwd": fixture.root()});
    assert_eq!(call(&fixture, Event::PreCompact, compact), None);
    let start = json!({"session_id": "s-9", "hook_event_name": "SessionStart", "source": "compact", "cwd": fixture.root()});
    let answer = call(&fixture, Event::SessionStart, start).expect("context");
    let context = answer["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default();
    assert!(context.contains("Acting role: main session"), "{context}");
    assert!(
        context.contains("Progress saved before compaction"),
        "{context}"
    );
    assert!(context.contains("make hooks-install"), "{context}");
    assert!(context.contains("A person merges"), "{context}");
}

#[test]
fn the_shell_flattener_finds_commands_writes_and_substitutions() {
    let parsed =
        shell::parse("A=1 git log | tee out.txt 2>&1 && echo \"$(rm x)\" `ls` >> log; (cd d) &")
            .expect("parse");
    assert_eq!(
        parsed.substitutions,
        vec!["rm x".to_owned(), "ls".to_owned()]
    );
    let writes: Vec<_> = parsed
        .items
        .iter()
        .filter_map(|item| match item {
            shell::Item::Command(simple, _) => Some(simple),
            _ => None,
        })
        .flat_map(|simple| {
            simple
                .redirects
                .iter()
                .filter(|r| r.write)
                .map(|r| r.target.text.clone())
        })
        .collect();
    assert_eq!(writes, vec!["log".to_owned()]);
    let shell::Item::Command(first, join) = &parsed.items[0] else {
        panic!("first item is a command");
    };
    assert_eq!(first.assignments[0].0, "A");
    assert_eq!(*join, shell::Join::Pipe);
    assert!(shell::parse("echo 'open").is_err());
    let braces = shell::parse("cp a{,.bak}").expect("parse");
    let shell::Item::Command(copy, _) = &braces.items[0] else {
        panic!("command");
    };
    assert!(copy.words[1].computed);
}
