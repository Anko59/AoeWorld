use super::*;
use crate::ship::issues::test_stub::Stub;

#[test]
fn the_issue_command_with_a_large_request_streams_its_canned_duplicate_note() {
    use std::io::Write;
    use std::process::Stdio;
    let title = "Large multibyte checklist";
    let listing = serde_json::json!([{
        "number": 12, "title": title, "body": marked_body("old", title).unwrap(),
        "labels": [{"name":"agent-found"}], "user":{"login":"someone"},
        "html_url":"https://github.com/project/checkout/issues/12"
    }]);
    let stub = Stub::new(&listing);
    let body = "🦀".repeat(50_000);
    let document = format!("{title}\n\n{body}");
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("aoe-harness");
    let mut child = Command::new(binary)
        .current_dir(&stub.root)
        .arg("issue")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_remove("AOE_JUDGE_REV")
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(document.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    let calls = stub.calls();
    assert!(
        calls.contains("<issue>\n<comment>\n<12>\n<--body-file>\n<->"),
        "{calls}"
    );
    assert!(!calls.contains("<--body>"));
    assert!(!calls.contains("<create>"));
    assert!(
        stub.bodies()
            .contains("Duplicate request filed; keeping this issue as the shared task.")
    );
    writeln!(std::io::stdout(), "CLI issue rehearsal (scripted GitHub):\n{stdout}{stderr}ASSERT: {}-byte request accepted; canned duplicate note streamed; one comment on #12; no issue created", body.len()).unwrap();
}

#[test]
fn a_large_multibyte_comment_uses_the_real_shared_comment_process_path() {
    use std::io::Write;
    let title = "Large note";
    let stub = Stub::new(&serde_json::json!([{
        "number":12, "title":title, "body":marked_body("old", title).unwrap(),
        "labels":[{"name":"agent-found"}],
        "html_url":"https://github.com/project/checkout/issues/12"
    }]));
    let note = "🦀".repeat(50_000);
    let outcome = Target::load(&stub.root)
        .unwrap()
        .file(&stub.root, title, &[], "body", &note)
        .unwrap();
    assert!(outcome.contains("commented"));
    assert!(stub.calls().contains("<--body-file>\n<->"));
    assert!(stub.bodies().contains(&note));
    writeln!(
        std::io::stdout(),
        "Shared comment process rehearsal: {} UTF-8 bytes delivered unchanged on stdin",
        note.len()
    )
    .unwrap();
}
