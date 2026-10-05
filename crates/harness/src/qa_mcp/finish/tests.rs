use super::*;
use crate::qa_mcp::{BrowserWorker, Server};
use std::{cell::Cell, error::Error, rc::Rc};

struct FakeWorker(Rc<Cell<usize>>);
impl BrowserWorker for FakeWorker {
    fn call(&mut self, _: &str, _: &Value) -> Result<Value, Box<dyn Error>> {
        self.0.set(self.0.get() + 1);
        Ok(json!({"build":"CLAIMED-not-measured-build","scenario":"smoke"}))
    }
}
fn server(root: &Path) -> Server {
    let artifact = root.join("screen.png");
    fs::write(&artifact, [0, 255, 128, 17]).unwrap();
    let mut server = Server::new_at("fast", root.to_owned()).unwrap();
    server.worker = Some(Box::new(FakeWorker(Rc::new(Cell::new(0)))));
    server
        .tool_call("open_session", &json!({"session":"first"}))
        .unwrap();
    for name in qa::REQUIRED {
        server
            .tool_call(
                "record_journey",
                &json!({"journey":name,"evidence":artifact}),
            )
            .unwrap();
    }
    server
}
#[test]
fn actual_mcp_finish_observes_binary_bytes_but_never_authenticates_claimed_pass() {
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    let result = server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    assert_eq!(result["claimed_status"], "PASS");
    assert_eq!(result["status"], "PASS"); // Compatibility alias is also only a claim.
    assert_eq!(
        result["assessment"],
        "STRUCTURAL_EVIDENCE_OBSERVED_NON_AUTHORITATIVE"
    );
    assert_eq!(result["authoritative"], false);
    let direct =
        qa::observation::observe_file_at(&owner.path().join("session.json"), owner.path()).unwrap();
    assert_eq!(result["observation"], serde_json::to_value(direct).unwrap());
    let serialized: Report =
        serde_json::from_slice(&fs::read(owner.path().join("session.json")).unwrap()).unwrap();
    assert_eq!(serialized.build, "CLAIMED-not-measured-build");
    fs::write(owner.path().join("screen.png"), [0, 255, 128, 18]).unwrap();
    let changed = server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    assert_ne!(result["observation"], changed["observation"]);
    assert_eq!(changed["authoritative"], false);
}
#[test]
fn missing_evidence_finish_fails_without_replacing_existing_claim_report() {
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    let path = owner.path().join("session.json");
    let old = fs::read(&path).unwrap();
    server.report.journeys[0].evidence =
        vec![owner.path().join("missing.png").to_str().unwrap().into()];
    assert!(
        server
            .tool_call("finish", &json!({"status":"PASS"}))
            .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), old);
}
#[test]
fn self_evidence_is_rejected_before_pending_file_creation() {
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    server.report.journeys[0].evidence =
        vec![owner.path().join("session.json").to_str().unwrap().into()];
    let before = fs::read_dir(owner.path()).unwrap().count();
    assert!(
        server
            .tool_call("finish", &json!({"status":"PASS"}))
            .is_err()
    );
    assert_eq!(fs::read_dir(owner.path()).unwrap().count(), before);
    assert!(!owner.path().join("session.json").exists());
}
#[test]
fn runtime_argument_closure_rejects_before_report_mutation_or_worker_dispatch() {
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    let counter = Rc::new(Cell::new(0));
    server.worker = Some(Box::new(FakeWorker(counter.clone())));
    let before = serde_json::to_value(&server.report).unwrap();
    for (name, args) in [
        ("finish", json!({"status":"PASS","approve":true})),
        (
            "record_journey",
            json!({"journey":"startup","evidence":"anything","role":"judge"}),
        ),
        (
            "record_finding",
            json!({"title":"t","evidence":"anything","pass":true}),
        ),
        (
            "observe",
            json!({"session":"first","execute":"hidden-script"}),
        ),
        (
            "open_session",
            json!({"session":"first","source_oid":"self-claimed"}),
        ),
        ("finish", Value::Null),
    ] {
        assert!(server.tool_call(name, &args).is_err());
        assert_eq!(serde_json::to_value(&server.report).unwrap(), before);
    }
    assert_eq!(counter.get(), 0);
}
#[test]
fn shared_strict_parser_rejects_nested_duplicate_arguments_before_dispatch() {
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    let before = serde_json::to_value(&server.report).unwrap();
    let input = b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{\"name\":\"finish\",\"arguments\":{\"status\":\"PASS\",\"status\":\"BLOCKED\"}}}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n";
    let mut output = Vec::new();
    crate::qa_mcp::serve_io(&mut server, std::io::Cursor::new(input), &mut output).unwrap();
    assert_eq!(serde_json::to_value(&server.report).unwrap(), before);
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["id"], 2);
}
#[cfg(unix)]
#[test]
fn symlink_directory_hardlink_and_fifo_session_targets_never_clobber_sentinels() {
    use std::os::unix::fs::symlink;
    let owner = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let sentinel = outside.path().join("sentinel");
    fs::write(&sentinel, b"UNCHANGED").unwrap();
    for kind in 0..4 {
        let directory = owner.path().join(format!("case-{kind}"));
        fs::create_dir(&directory).unwrap();
        let mut server = server(&directory);
        let session = directory.join("session.json");
        match kind {
            0 => symlink(&sentinel, &session).unwrap(),
            1 => fs::create_dir(&session).unwrap(),
            2 => fs::hard_link(&sentinel, &session).unwrap(),
            _ => nix::unistd::mkfifo(
                &session,
                nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
            )
            .unwrap(),
        }
        let before = fs::read_dir(&directory).unwrap().count();
        assert!(
            server
                .tool_call("finish", &json!({"status":"PASS"}))
                .is_err()
        );
        assert_eq!(fs::read(&sentinel).unwrap(), b"UNCHANGED");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), before);
    }
}
#[cfg(unix)]
#[test]
fn alias_ancestor_and_report_inode_references_fail_before_any_write() {
    use std::os::unix::fs::symlink;
    let owner = tempfile::tempdir().unwrap();
    let directory = owner.path().join("normal");
    fs::create_dir(&directory).unwrap();
    let mut server = server(&directory);
    let alias = owner.path().join("alias");
    symlink(&directory, &alias).unwrap();
    assert!(finish(&server.report, &alias).is_err());
    assert!(!directory.join("session.json").exists());
    server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    let old = fs::read(directory.join("session.json")).unwrap();
    let reference = directory.join("report-alias");
    symlink(directory.join("session.json"), &reference).unwrap();
    server.report.journeys[0].evidence = vec![reference.to_str().unwrap().into()];
    let before = fs::read_dir(&directory).unwrap().count();
    assert!(
        server
            .tool_call("finish", &json!({"status":"PASS"}))
            .is_err()
    );
    assert_eq!(fs::read(directory.join("session.json")).unwrap(), old);
    assert_eq!(fs::read_dir(&directory).unwrap().count(), before);
}
#[cfg(unix)]
#[test]
fn published_report_is_singly_linked_private_regular_file_with_exact_generated_bytes() {
    use std::os::unix::fs::MetadataExt;
    let owner = tempfile::tempdir().unwrap();
    let mut server = server(owner.path());
    server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    let path = owner.path().join("session.json");
    let metadata = fs::symlink_metadata(&path).unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.mode() & 0o7777, 0o600);
    assert_eq!(metadata.nlink(), 1);
    assert_eq!(
        fs::read(&path).unwrap(),
        serde_json::to_vec_pretty(&server.report).unwrap()
    );
}
