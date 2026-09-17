use super::*;
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
