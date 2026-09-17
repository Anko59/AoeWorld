use super::*;

struct FakeWorker;

impl BrowserWorker for FakeWorker {
    fn call(&mut self, name: &str, _: &Value) -> Result<Value, Box<dyn Error>> {
        if name == "open_session" {
            Ok(json!({"build":"test-build","scenario":"smoke"}))
        } else {
            Ok(json!({"ok":true}))
        }
    }
}

#[test]
fn worker_endpoint_must_be_an_explicit_local_http_target() {
    for endpoint in [
        "https://127.0.0.1:8080",
        "http://example.com:8080",
        "http://127.0.0.1",
        "http://user@127.0.0.1:8080",
        "http://127.0.0.1:8080/hidden",
        "http://127.0.0.1:8080/?x=1",
        "http://127.0.0.1:8080/#fragment",
    ] {
        assert!(Worker::start_at(endpoint).is_err(), "{endpoint}");
    }
}

#[test]
fn worker_json_protocol_rejects_failure_and_closed_streams() {
    let mut input = Vec::new();
    let mut output = io::Cursor::new(b"{\"ok\":true,\"result\":{\"tick\":5}}\n".to_vec());
    assert_eq!(
        exchange(
            &mut input,
            &mut output,
            "observe",
            &json!({"session":"first"})
        )
        .expect("worker result")["tick"],
        5
    );
    assert!(
        String::from_utf8(input)
            .expect("request")
            .contains("\"observe\"")
    );
    let mut rejected = io::Cursor::new(b"{\"ok\":false,\"error\":\"blocked\"}\n".to_vec());
    assert!(exchange(&mut Vec::new(), &mut rejected, "observe", &json!({})).is_err());
    assert!(
        exchange(
            &mut Vec::new(),
            &mut io::Cursor::new(Vec::new()),
            "observe",
            &json!({})
        )
        .is_err()
    );
}

#[test]
fn mcp_stdio_ignores_notifications_and_rejects_oversized_frames() {
    let mut server = Server::new("fast").expect("server");
    let input = b"not-json\n{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/list\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/call\",\"params\":{\"name\":\"hidden\",\"arguments\":{}}}\n";
    let mut output = Vec::new();
    serve_io(&mut server, io::Cursor::new(input), &mut output).expect("MCP stream");
    let responses: Vec<Value> = output
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("JSON"))
        .collect();
    assert_eq!(responses.len(), 2);
    assert!(responses[0]["result"]["tools"].is_array());
    assert_eq!(responses[1]["result"]["isError"], true);
    let oversized = format!("{}\n", "x".repeat(1_048_577));
    assert!(serve_io(&mut server, io::Cursor::new(oversized), &mut Vec::new()).is_err());
}

#[test]
fn mcp_records_a_complete_report_with_real_evidence_files() {
    let temp = tempfile::tempdir().expect("directory");
    let root = temp.path().join("qa");
    fs::create_dir_all(&root).expect("evidence directory");
    let screenshot = root.join("screen.png");
    fs::write(&screenshot, b"screen").expect("evidence");
    let evidence = screenshot.to_str().expect("path");
    let mut server = Server::new_at("fast", root.clone()).expect("server");
    server.worker = Some(Box::new(FakeWorker));
    server
        .tool_call("open_session", &json!({"session":"first"}))
        .expect("open session");
    for journey in qa::REQUIRED {
        server
            .tool_call(
                "record_journey",
                &json!({"journey":journey,"evidence":evidence}),
            )
            .expect("record journey");
    }
    let result = server
        .tool_call("finish", &json!({"status":"PASS"}))
        .expect("finish");
    assert_eq!(result["status"], "PASS");
    let report: Report =
        serde_json::from_slice(&fs::read(root.join("session.json")).expect("report"))
            .expect("report JSON");
    qa::validate(&report).expect("valid report");
    assert_eq!(report.build, "test-build");
    assert_eq!(report.journeys.len(), qa::REQUIRED.len());
}

#[test]
fn mcp_reports_findings_and_rejects_expired_budgets() {
    let temp = tempfile::tempdir().expect("directory");
    let root = temp.path().join("qa");
    fs::create_dir_all(&root).expect("evidence directory");
    let screenshot = root.join("screen.png");
    fs::write(&screenshot, b"screen").expect("evidence");
    let evidence = screenshot.to_str().expect("path");
    let mut server = Server::new_at("fast", root).expect("server");
    server.worker = Some(Box::new(FakeWorker));
    server
        .tool_call("open_session", &json!({"session":"first"}))
        .expect("open session");
    server
        .tool_call(
            "record_finding",
            &json!({
                "title":"camera jump", "reproduction":"pan and zoom", "expected":"stable view",
                "actual":"view jumps", "evidence":evidence
            }),
        )
        .expect("finding");
    assert!(
        server
            .tool_call("finish", &json!({"status":"PASS"}))
            .is_err()
    );
    assert!(
        server
            .tool_call("finish", &json!({"status":"FINDINGS"}))
            .is_ok()
    );
    server.start = Instant::now() - Duration::from_secs(901);
    assert_eq!(
        server.tool_call("observe", &json!({"session":"first"})),
        Err("QA time budget exhausted".to_owned())
    );
}
#[test]
fn contract_exposes_only_restricted_tools() {
    let available = tools();
    let names: Vec<_> = available["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["name"].as_str())
        .collect();
    assert!(names.contains(&"observe"));
    assert!(!names.contains(&"evaluate"));
    assert!(
        validate_action(
            "activate",
            &json!({"session":"first","role":"button","label":"Reconnect"})
        )
        .is_ok()
    );
    assert!(
        validate_action(
            "activate",
            &json!({"session":"first","role":"script","label":"x"})
        )
        .is_err()
    );
    let mut server = Server::new("fast").unwrap();
    let response = handle(
        &mut server,
        &json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":VERSION}}),
    )
    .unwrap();
    assert_eq!(response["result"]["protocolVersion"], VERSION);
    assert!(
        server
            .tool_call("finish", &json!({"status":"PASS"}))
            .is_err()
    );
}

#[test]
fn browser_actions_reject_unbounded_or_hidden_inputs() {
    for (name, args) in [
        ("observe", json!({"session":"UPPER"})),
        ("open_session", json!({"session":"a","capability":"script"})),
        (
            "open_session",
            json!({"session":"a","configuration":"arbitrary-url"}),
        ),
        (
            "select_scenario",
            json!({"session":"a","scenario":"secret"}),
        ),
        (
            "canvas_input",
            json!({"session":"a","action":"key","key":"Delete"}),
        ),
        (
            "canvas_input",
            json!({"session":"a","action":"click","x":-1,"y":0}),
        ),
        (
            "canvas_input",
            json!({"session":"a","action":"wheel","x":0,"y":0,"delta":1001}),
        ),
        ("canvas_input", json!({"session":"a","action":"script"})),
        (
            "wait_text",
            json!({"session":"a","text":"x","timeout_ms":10001}),
        ),
    ] {
        assert!(validate_action(name, &args).is_err(), "{name}: {args}");
    }
    assert!(
        validate_action(
            "open_session",
            &json!({"session":"a","capability":"webgpu-disabled","configuration":"invalid-scenario"})
        )
        .is_ok()
    );
    assert!(
        validate_action(
            "canvas_input",
            &json!({"session":"a","action":"key","key":"ArrowUp"})
        )
        .is_ok()
    );
    assert!(
        validate_action(
            "canvas_input",
            &json!({"session":"a","action":"wheel","x":5,"y":5,"delta":-20})
        )
        .is_ok()
    );
    assert!(
        validate_action(
            "wait_text",
            &json!({"session":"a","text":"ready","timeout_ms":1000})
        )
        .is_ok()
    );
}

#[test]
fn mcp_rejects_unlisted_methods_and_incomplete_reports() {
    let mut server = Server::new("fast").expect("server");
    let ping = handle(
        &mut server,
        &json!({"jsonrpc":"2.0","id":2,"method":"ping"}),
    )
    .expect("ping");
    assert_eq!(ping["result"], json!({}));
    let unknown = handle(
        &mut server,
        &json!({"jsonrpc":"2.0","id":3,"method":"hidden"}),
    )
    .expect("unknown");
    assert_eq!(unknown["error"]["code"], -32601);
    assert!(handle(&mut server, &json!({"jsonrpc":"2.0","method":"ping"})).is_none());
    assert!(
        server
            .tool_call(
                "record_journey",
                &json!({"journey":"unknown","evidence":"x"})
            )
            .is_err()
    );
    assert!(
        server
            .tool_call("record_finding", &json!({"title":"x"}))
            .is_err()
    );
    assert!(
        server
            .tool_call("finish", &json!({"status":"FINDINGS"}))
            .is_err()
    );
    assert!(
        server
            .tool_call("finish", &json!({"status":"invalid"}))
            .is_err()
    );
}
