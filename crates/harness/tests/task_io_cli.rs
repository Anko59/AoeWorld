//! Offline realCLI regressions. Copy to crates/harness/tests/task_io_cli.rs.
#![cfg(unix)]
#[path = "task_io_cli/fixture.rs"]
mod fixture;
use fixture::*;
use serde_json::json;
use std::os::unix::fs::symlink;
use std::{
    fs,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[test]
fn actual_artifact_bytes_bind_digest_but_never_authenticate_review_or_role() {
    let mut fixture = Fixture::new();
    let bytes = br#"{"claim":"reviewed","authoritative":false}"#;
    let path = fixture.artifact(bytes);
    let result = fixture.success(&fixture.run());
    assert_eq!(
        result["artifacts"][0]["blake3"],
        blake3::hash(bytes).to_hex().to_string()
    );
    assert_eq!(result["artifacts"][0]["kind"], "review");
    assert!(
        result["limits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|line| line.as_str().unwrap().contains("unauthenticated"))
    );
    fixture.task["artifacts"][0]["blake3"] = json!("0".repeat(64));
    fixture.unavailable(&fixture.run(), "matching actual full BLAKE3 digest");
    fixture.task["artifacts"][0]["blake3"] = serde_json::Value::Null;
    fixture.unavailable(&fixture.run(), "matching actual full BLAKE3 digest");
    fixture.task["artifacts"][0]["path"] = json!("task-artifacts/io-fixture/missing.json");
    fixture.unavailable(&fixture.run(), "No such file");
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn artifact_link_alias_and_size_rejections_replace_stale_ready_without_source_mutation() {
    for mode in [
        "leaf-symlink",
        "parent-symlink",
        "hardlink",
        "oversized",
        "directory",
    ] {
        let mut fixture = Fixture::new();
        let bytes = b"observed artifact";
        let real = fixture.owner.path().join("artifact-real");
        fs::write(&real, bytes).unwrap();
        let namespace = fixture.output.join("task-artifacts/io-fixture");
        if mode == "parent-symlink" {
            fs::create_dir(fixture.output.join("task-artifacts")).unwrap();
            symlink(fixture.owner.path(), &namespace).unwrap();
        } else {
            fs::create_dir_all(&namespace).unwrap();
        }
        let path = namespace.join("result.json");
        match mode {
            "leaf-symlink" => symlink(&real, &path).unwrap(),
            "parent-symlink" => fs::write(fixture.owner.path().join("result.json"), bytes).unwrap(),
            "hardlink" => fs::hard_link(&real, &path).unwrap(),
            "oversized" => fs::write(&path, vec![b'x'; 1024 * 1024 + 1]).unwrap(),
            "directory" => fs::create_dir(&path).unwrap(),
            _ => unreachable!(),
        }
        fixture.task["artifacts"] = json!([{"kind":"test-evidence","path":"task-artifacts/io-fixture/result.json","blake3":blake3::hash(bytes).to_hex().to_string()}]);
        let reason = match mode {
            "leaf-symlink" | "parent-symlink" => "symlink",
            "hardlink" => "hardlink",
            _ => "bounded regular",
        };
        fixture.unavailable(&fixture.run(), reason);
    }
}

#[test]
fn declared_artifact_changed_during_provider_observation_is_not_accepted_with_old_digest() {
    let mut fixture = Fixture::new();
    let path = fixture.artifact(b"original bytes");
    // Test-only Python mutation uses one fixed path owned by this TempDir.
    fixture.provider(&format!("#!/usr/bin/python3\nfrom pathlib import Path\nPath({}).write_bytes(b'changed bytes')\nprint('fixture-codex 1.0')\n", serde_json::to_string(path.to_str().unwrap()).unwrap()));
    fixture.unavailable(&fixture.run(), "matching actual full BLAKE3 digest");
}

#[test]
fn immutable_guide_bytes_are_required_not_stale_worktree_replacements() {
    for mode in ["missing", "non-utf8", "oversized", "symlink"] {
        let mut fixture = Fixture::new();
        let guide = fixture.repo.join("skills/protocol/SKILL.md");
        assert_eq!(
            fs::canonicalize(&guide).unwrap(),
            fs::canonicalize(&fixture.repo)
                .unwrap()
                .join("skills/protocol/SKILL.md")
        );
        match mode {
            "missing" => {
                git(&fixture.repo, &["rm", "--", "skills/protocol/SKILL.md"]);
            }
            "non-utf8" => fs::write(&guide, b"\xff\n").unwrap(),
            "oversized" => fs::write(&guide, "sixteen-byte-row!\n".repeat(1930)).unwrap(),
            "symlink" => {
                // git rm verifies an explicit tracked fixture-owned path before deletion.
                git(&fixture.repo, &["rm", "--", "skills/protocol/SKILL.md"]);
                fs::create_dir_all(guide.parent().unwrap()).unwrap();
                symlink("../../docs/testing.md", &guide).unwrap();
            }
            _ => unreachable!(),
        }
        fixture.commit_candidate();
        fixture.freeze_as_base();
        // Guide variant exists in both immutable endpoints, so nonUTF8/size failures
        // reach guide reading rather than being rejected early as changed diff bytes.
        // Working replacement is intentionally NOT a valid substitute for missing commit data.
        if mode == "missing" {
            fs::create_dir_all(guide.parent().unwrap()).unwrap();
            fs::write(&guide, "Working-only replacement\n").unwrap();
        }
        let result = fixture.run();
        assert!(!result.status.success());
        let descriptor = fixture.descriptor();
        assert_eq!(descriptor["status"], "UNAVAILABLE");
        assert_eq!(descriptor["authoritative"], false);
        fixture.preserve();
    }
}

#[test]
fn task_contract_endpoint_changes_and_budget_states_do_not_mint_readiness() {
    let mut fixture = Fixture::new();
    fixture.provider(&format!("#!/usr/bin/python3\nfrom pathlib import Path\np=Path({})\np.write_bytes(p.read_bytes()+b'\\n')\nprint('fixture-codex 1.0')\n", serde_json::to_string(fixture.input.to_str().unwrap()).unwrap()));
    fixture.unavailable(&fixture.run(), "task contract changed");
    fixture.provider("#!/bin/sh\nprintf 'fixture-codex 1.0\\n'\n");
    fixture.task["rounds_remaining"] = json!(0);
    fixture.unavailable(&fixture.run(), "exhausted");
    fixture.task["status"] = json!("blocked");
    let result = fixture.success(&fixture.run());
    assert_eq!(result["plan"]["task"]["status"], "blocked");
    assert_eq!(
        result["plan"]["semantic_handoffs"],
        json!([{"actor":"coordinator","next":"blocked"}])
    );
    fixture.task["rounds_remaining"] = json!(4);
    fixture.task["status"] = json!("planned");
    for field in ["candidate", "base"] {
        let previous = fixture.task[field].clone();
        fixture.task[field] = json!("f".repeat(40));
        fixture.unavailable(&fixture.run(), "git");
        fixture.task[field] = previous;
    }
}

#[test]
fn input_output_alias_rejects_before_pending_write_and_preserves_caller_bytes() {
    let fixture = Fixture::new();
    let target = fixture.output.join("task-plan.json");
    let bytes = serde_json::to_vec(&fixture.task).unwrap();
    fs::write(&target, &bytes).unwrap();
    let alias = fixture.owner.path().join("output-alias");
    symlink(&fixture.output, &alias).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(&fixture.repo)
        .args([
            "task-plan",
            "--task",
            alias.join("task-plan.json").to_str().unwrap(),
            "--output",
            fixture.output.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert_eq!(
        fs::read(&target).unwrap(),
        bytes,
        "pending output must not clobber aliased input"
    );
    fixture.preserve();
    // EXPECTED FAIL before parent's early canonicalized containment guard.
}

#[test]
fn binary_control_and_truncated_or_failed_diff_are_incomplete_not_silently_dropped() {
    for payload in [
        b"\xff\n".to_vec(),
        b"control\0bytes\n".to_vec(),
        vec![b'x'; 70000],
    ] {
        let mut fixture = Fixture::new();
        fs::write(fixture.repo.join("case.rs"), payload).unwrap();
        fixture.commit_candidate();
        assert!(!fixture.run().status.success());
        let result = fixture.descriptor();
        assert_eq!(result["status"], "UNAVAILABLE");
        assert_eq!(result["authoritative"], false);
        fixture.preserve();
    }
    for mode in ["failed", "truncated"] {
        let fixture = Fixture::new();
        fixture.transport(mode);
        fixture.unavailable(&fixture.run(), "immutable diff unavailable");
    }
}

#[test]
fn bounded_diff_timeout_and_sigterm_keep_nonready_evidence() {
    let fixture = Fixture::new();
    fixture.transport("timeout");
    fixture.save();
    fixture.stale();
    let mut child = fixture
        .command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_marker(&fixture.owner.path().join("started"), &mut child);
    let observed = Instant::now();
    let out = child.wait_with_output().unwrap();
    fixture.unavailable(&out, "immutable diff unavailable");
    assert!(
        observed.elapsed() < Duration::from_secs(9),
        "ten-second fake diff must be cancelled by five-second bound"
    );
    let fixture = Fixture::new();
    fixture.transport("cancel");
    fixture.save();
    fixture.stale();
    let mut child = fixture
        .command()
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    wait_marker(&fixture.owner.path().join("started"), &mut child);
    assert!(
        Command::new("kill")
            .args(["-TERM", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let out = child.wait_with_output().unwrap();
    fixture.unavailable(&out, "immutable diff unavailable");
}
