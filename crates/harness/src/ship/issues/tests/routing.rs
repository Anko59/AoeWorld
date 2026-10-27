use super::*;
use crate::ship::issues::test_stub::Stub;

#[test]
fn game_paths_and_non_game_paths_route_to_the_configured_workstreams() {
    let config = Config::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).unwrap();
    for path in [
        "crates/simulation/src/lib.rs",
        "crates/core/src/lib.rs",
        "./crates/client/src/lib.rs",
        "crates/scenario/src/lib.rs",
        "crates/protocol/src/lib.rs",
        "crates/geodata/src/lib.rs",
        "crates/server/src/lib.rs",
        "crates/client/src/lib.rs",
        "crates/rendering/src/lib.rs",
        "crates/map/src/lib.rs",
        "crates/assets/src/lib.rs",
        "crates/sdk/src/lib.rs",
        "crates/sdk-python/src/lib.rs",
        "ai/model.py",
        "browser/src/game.ts",
        "web/interface/hud.js",
    ] {
        assert_eq!(config.review(path), 575, "{path}");
    }
    for path in [
        "crates/harness/src/main.rs",
        "gates/registry.json",
        ".github/workflows/ci.yml",
        "docs/issues.md",
        "crates/serverish/file.rs",
        "browser/playwright.config.ts",
        "crates/server/README.md",
        "ai/AGENTS.md",
        "web/dev/tools.js",
    ] {
        assert_eq!(config.review(path), 576, "{path}");
    }
    assert_eq!(config.nightly, 574);
}

#[test]
fn workstream_sections_replace_only_the_same_fingerprint_and_keep_ticks() {
    let original = "# Workstream\n\n#### Game findings (owned by the game session)\n\nOwner text\n\n## Other\nOther text\n";
    let first = splice(original, "review-pr-42", "**PR #42**\n- [ ] finding", true).unwrap();
    assert!(first.find("**PR #42**").unwrap() < first.find("## Other").unwrap());
    let ticked = first.replace("- [ ] finding", "- [x] finding");
    let again = splice(
        &ticked,
        "review-pr-42",
        "**PR #42**\n- [ ] finding\n- [ ] new",
        true,
    )
    .unwrap();
    assert_eq!(again.matches("**PR #42**").count(), 1);
    assert!(again.contains("- [x] finding"));
    assert!(again.contains("Owner text"));
    assert!(again.contains("## Other\nOther text"));
    assert!(splice("no heading", "review-pr-42", "x", true).is_err());
}

#[test]
fn closed_configured_issue_fails_and_never_creates() {
    let mut stub = Stub::new(&serde_json::json!([]));
    stub.set(
        "GH_VIEW_JSON",
        r###"{"state":"CLOSED","body":"## Game findings\n","updatedAt":"t"}"###,
    );
    let target = Target::direct(&stub.root).unwrap();
    let error = update(&target, &stub.root, 575, "review-pr-42", "finding", true)
        .unwrap_err()
        .to_string();
    assert!(error.contains("gates/issues.json"), "{error}");
    assert!(error.contains("575"), "{error}");
    assert!(!stub.calls().contains("<create>"));
}

#[test]
fn retries_after_concurrent_edit_and_preserves_the_fresh_body() {
    let mut stub = Stub::new(&serde_json::json!([]));
    let snapshot = |body: &str, time: &str| {
        serde_json::json!({"state":"OPEN", "body":body, "updatedAt":time}).to_string()
    };
    stub.view_responses(&[
        snapshot("old", "1"),
        snapshot("fresh owner text", "2"),
        snapshot("fresh owner text", "2"),
        snapshot("fresh owner text", "2"),
    ]);
    let target = Target::direct(&stub.root).unwrap();
    update(&target, &stub.root, 576, "review-pr-42", "finding", false).unwrap();
    assert_eq!(stub.calls().matches("<view>").count(), 4);
    assert!(stub.bodies().contains("fresh owner text"));
    assert!(!stub.bodies().contains("old"));
}

#[test]
fn a_busy_workstream_is_left_untouched() {
    let mut stub = Stub::new(&serde_json::json!([]));
    let snapshot = |time: &str| {
        serde_json::json!({"state":"OPEN", "body":"owner text", "updatedAt":time}).to_string()
    };
    stub.view_responses(&[
        snapshot("1"),
        snapshot("2"),
        snapshot("2"),
        snapshot("3"),
        snapshot("3"),
        snapshot("4"),
    ]);
    let target = Target::direct(&stub.root).unwrap();
    let error = update(&target, &stub.root, 576, "review-pr-42", "finding", false)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("gates/issues.json") && error.contains("busy"),
        "{error}"
    );
    assert!(!stub.calls().contains("<edit>"));
}

#[test]
fn invalid_destinations_name_the_config_file() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::create_dir(temp.path().join("gates")).unwrap();
    for config in [
        r#"{"game_review":0,"other_review":576,"nightly":574}"#,
        r#"{"game_review":575,"other_review":575,"nightly":574}"#,
        r#"{"game_review":575,"other_review":576,"nightly":574,"extra":1}"#,
    ] {
        std::fs::write(temp.path().join("gates/issues.json"), config).unwrap();
        let error = match Config::load(temp.path()) {
            Ok(_) => panic!("accepted {config}"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("gates/issues.json"));
    }
}
