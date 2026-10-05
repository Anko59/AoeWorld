use super::*;

const NAME: &str = "run_semantic_case";
fn arguments() -> Value {
    json!({"criterion":"movement-arrival-v1"})
}
struct PanicWorker;
impl BrowserWorker for PanicWorker {
    fn call(&mut self, _: &str, _: &Value) -> Result<Value, Box<dyn Error>> {
        panic!("SECRET_CANDIDATE_WORKER_BYTES_MUST_NOT_BE_OBSERVED")
    }
}

#[test]
fn semantic_catalog_exposes_only_one_fixed_controller_criterion() {
    let catalog = tools();
    let tools = catalog["tools"].as_array().unwrap();
    let tool = tools.iter().find(|tool| tool["name"] == NAME).unwrap();
    assert_eq!(tool["inputSchema"]["additionalProperties"], false);
    assert_eq!(tool["inputSchema"]["required"], json!(["criterion"]));
    assert_eq!(
        tool["inputSchema"]["properties"].as_object().unwrap().len(),
        1
    );
    assert_eq!(
        tool["inputSchema"]["properties"]["criterion"]["enum"],
        json!(["movement-arrival-v1"])
    );
    assert_eq!(tools.iter().filter(|tool| tool["name"] == NAME).count(), 1);
    assert_eq!(tools.len(), 13);
    assert!(!BROWSER_ACTIONS.contains(&NAME));
}

#[test]
fn local_semantics_never_starts_or_calls_a_browser_worker() {
    let mut server = Server::new("fast").unwrap();
    let baseline = server.tool_call(NAME, &arguments()).unwrap();
    assert!(server.worker.is_none());
    assert!(server.report.journeys.is_empty());
    assert_eq!(server.report.status, Status::Blocked);
    assert_eq!(baseline["case"]["executed_actions"], 18);
    assert_eq!(baseline["case"]["asserted"], 178);
    assert_eq!(baseline["controls"]["bad_journey"]["executed_actions"], 17);
    assert_eq!(baseline["calibration"], "VALID_LOCAL_CONTROLS_OBSERVED");
    server.worker = Some(Box::new(PanicWorker));
    assert_eq!(server.tool_call(NAME, &arguments()).unwrap(), baseline);
    assert_eq!(server.report.status, Status::Blocked);
    assert!(server.report.journeys.is_empty());
}

#[test]
fn fake_browser_success_completed_reports_and_existing_paths_cannot_set_case_outcomes() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("qa");
    fs::create_dir(&root).unwrap();
    let artifact = root.join("completed.bin");
    fs::write(&artifact, b"SECRET_CANDIDATE_STDOUT_PASS_APPROVED").unwrap();
    let mut server = Server::new_at("fast", root).unwrap();
    let baseline = server.tool_call(NAME, &arguments()).unwrap();
    server.worker = Some(Box::new(FakeWorker));
    server
        .tool_call("open_session", &json!({"session":"first"}))
        .unwrap();
    for journey in qa::REQUIRED {
        server
            .tool_call(
                "record_journey",
                &json!({"journey":journey,"evidence":artifact}),
            )
            .unwrap();
    }
    let finish = server
        .tool_call("finish", &json!({"status":"PASS"}))
        .unwrap();
    assert_eq!(finish["claimed_status"], "PASS");
    assert_eq!(finish["authoritative"], false);
    assert_eq!(finish["observation"]["journey_execution"], "NOT_ASSESSED");
    assert_eq!(server.report.journeys.len(), 6);
    assert_eq!(server.tool_call(NAME, &arguments()).unwrap(), baseline);
    assert_eq!(server.report.status, Status::Pass);
    assert_eq!(server.report.build, "test-build");
    assert_eq!(server.report.journeys.len(), 6);
    let text = baseline.to_string();
    assert!(!text.contains("SECRET_CANDIDATE"));
    assert!(!text.contains("test-build"));
    assert!(!text.contains(artifact.to_str().unwrap()));
}

#[test]
fn semantic_requests_reject_all_missing_changed_or_extra_arguments_before_dispatch() {
    let mut server = Server::new("fast").unwrap();
    server.worker = Some(Box::new(PanicWorker));
    for args in [
        json!({}),
        json!([]),
        json!({"criterion":null}),
        json!({"criterion":true}),
        json!({"criterion":0}),
        json!({"criterion":""}),
        json!({"criterion":"movement-arrival-v2"}),
        json!({"criterion":"movement-arrival-v1\n"}),
        json!({"criterion":"x".repeat(33)}),
    ] {
        assert!(server.tool_call(NAME, &args).is_err());
    }
    for (key, value) in [
        ("seed", json!(7)),
        ("position", json!([31, 31])),
        ("ticks", json!(16)),
        ("expected", json!(true)),
        ("outcome", json!("PASS")),
        ("path", json!("reports/qa/session.json")),
        ("role", json!("judge")),
        ("authoritative", json!(true)),
        ("completed", json!(true)),
        ("status", json!("PASS")),
        ("approved", json!(true)),
        ("source", json!("751cdb94b527b2c8b0064a4e16f5b0d5c87a21aa")),
    ] {
        let mut args = arguments();
        args[key] = value;
        assert!(server.tool_call(NAME, &args).is_err(), "{key}");
    }
    assert!(server.report.journeys.is_empty());
    assert_eq!(server.report.status, Status::Blocked);
    server.start = Instant::now() - Duration::from_secs(901);
    assert_eq!(
        server.tool_call(NAME, &arguments()),
        Err("QA time budget exhausted".into())
    );
}

#[test]
fn duplicate_criterion_and_envelope_keys_fail_at_existing_raw_ingress() {
    let duplicate_args = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"run_semantic_case","arguments":{"criterion":"invalid","criterion":"movement-arrival-v1"}}}"#;
    let duplicate_envelope = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","method":"tools/call","params":{"name":"run_semantic_case","arguments":{"criterion":"movement-arrival-v1"}}}"#;
    for frame in [duplicate_args, duplicate_envelope] {
        assert!(crate::input_json::parse(frame.as_bytes(), 1_048_576).is_err());
        let mut server = Server::new("fast").unwrap();
        server.worker = Some(Box::new(PanicWorker));
        let input = format!("{frame}\n");
        let mut output = Vec::new();
        serve_io(&mut server, io::Cursor::new(input), &mut output).unwrap();
        assert!(output.is_empty());
        assert!(server.report.journeys.is_empty());
    }
}

#[test]
fn mcp_success_means_response_not_independent_qa_and_transport_failure_returns_error() {
    let request = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":NAME,"arguments":arguments()}});
    let mut server = Server::new("fast").unwrap();
    let response = handle(&mut server, &request).unwrap();
    assert_eq!(response["result"]["isError"], false);
    let payload: Value =
        serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(payload["case"]["outcome"], "LOCAL_CASE_PASS");
    assert_eq!(payload["authoritative"], false);
    assert_eq!(payload["qualification"], "UNQUALIFIED");
    assert_eq!(payload["process_exit"], "NOT_APPLICABLE_IN_PROCESS");
    assert_eq!(payload["publication"], "MCP_RESPONSE_ONLY");
    assert!(server.worker.is_none());
    struct BrokenOutput;
    impl Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let input = format!("{request}\n");
    let error = serve_io(&mut server, io::Cursor::new(input), &mut BrokenOutput).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<serde_json::Error>()
            .unwrap()
            .io_error_kind(),
        Some(io::ErrorKind::BrokenPipe)
    );
    assert!(server.worker.is_none());
    assert_eq!(server.report.status, Status::Blocked);
}
