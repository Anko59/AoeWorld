use super::*;

const PASS: &str = r#"{"revision":"abc","pass":true,"failures":[],"ticks":{"p50_ms":1.2,"p95_ms":4.5,"max_ms":12.0},"session_drops":0,"scenarios":[{"name":"gather","pass":true,"detail":"done"}],"recording":"match.webm"}"#;
const FAIL: &str = r#"{"revision":"abc","pass":false,"failures":["lost"],"ticks":{"p50_ms":1.0,"p95_ms":2.0,"max_ms":3.0},"session_drops":1,"scenarios":[{"name":"attack","pass":false,"detail":"failed"}]}"#;

#[test]
fn contract_and_rendering() {
    let pass = Report::parse(PASS, "abc").unwrap();
    assert!(pass.pass);
    assert!(Report::parse(PASS, "different").is_none());
    let fail = Report::parse(FAIL, "abc").unwrap();
    assert!(!fail.pass);
    let section = fail.section();
    for text in [
        "Playtest (advisory)",
        "FAIL",
        "attack",
        "failed",
        "2 ms",
        "1",
        "lost",
    ] {
        assert!(section.contains(text), "{section}: missing {text}");
    }
    assert!(pass.section().contains("match.webm"));
    assert!(pass.summary().contains("1/1 scenarios pass"));
}

#[test]
fn gameplay_paths_have_directory_boundaries() {
    for path in [
        "crates/simulation/a",
        "crates/server/a",
        "crates/client/a",
        "crates/sdk/a",
        "sdk/a",
        "ai/a",
        "crates/protocol/a",
    ] {
        assert!(gameplay(path), "{path}");
    }
    for path in ["docs/a", "crates/serverless/a", "a/ai/a"] {
        assert!(!gameplay(path), "{path}");
    }
}

#[cfg(unix)]
#[test]
fn scripted_make_without_target_is_unavailable() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let make = dir.path().join("make");
    std::fs::write(&make, "#!/bin/sh\nprintf 'other:\n'\n").unwrap();
    std::fs::set_permissions(&make, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!target_available(dir.path(), make.to_str().unwrap()));
    std::fs::write(&make, "#!/bin/sh\nprintf 'playtest:\n'\n").unwrap();
    assert!(target_available(dir.path(), make.to_str().unwrap()));
}

#[test]
fn ship_continues_when_gameplay_has_no_playtest_target() {
    use crate::ship::{
        git, ship,
        tests::{fixture, offline, run},
    };
    let (_temp, root) = fixture("true");
    std::fs::create_dir_all(root.join("ai")).unwrap();
    std::fs::write(root.join("ai/change.py"), "change\n").unwrap();
    run(&root, &["add", "ai/change.py"]);
    run(&root, &["commit", "-q", "-m", "gameplay"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let result = ship(&root, &offline()).expect("advisory playtest must not block ship");
    assert_eq!(result.head, head);
}
