use std::{fs, path::Path, process::Command};
#[cfg(unix)]
#[test]
fn aliased_task_input_inside_output_is_rejected_without_clobbering_input() {
    let root = tempfile::tempdir().unwrap();
    let owner = tempfile::tempdir().unwrap();
    let output = owner.path().join("output");
    fs::create_dir(&output).unwrap();
    let alias = owner.path().join("output-alias");
    std::os::unix::fs::symlink(&output, &alias).unwrap();
    let original = b"caller-owned task input must remain unchanged\n";
    fs::write(output.join("task-plan.json"), original).unwrap();
    let input = alias.join("task-plan.json");
    let result = cli(
        root.path(),
        &[
            "task-plan",
            "--task",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ],
    );
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must not overlap"));
    assert_eq!(fs::read(output.join("task-plan.json")).unwrap(), original);
}
fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn cli(root: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(root)
        .args(args)
        .env_remove("GITHUB_OUTPUT")
        .env_remove("AOE_BASE_SHA")
        .output()
        .unwrap()
}
#[test]
fn task_plan_uses_immutable_diff_keeps_rename_names_and_preserves_required_preflight() {
    let repo = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--template="]);
    fs::create_dir(repo.path().join("gates")).unwrap();
    fs::create_dir(repo.path().join("docs")).unwrap();
    fs::create_dir_all(repo.path().join("crates/core/tests")).unwrap();
    fs::write(
        repo.path().join("gates/registry.json"),
        include_str!("../../../gates/registry.json"),
    )
    .unwrap();
    fs::write(
        repo.path().join("gates/roles.json"),
        include_str!("../../../gates/roles.json"),
    )
    .unwrap();
    fs::create_dir_all(repo.path().join("skills/harness-ci")).unwrap();
    fs::create_dir_all(repo.path().join("skills/protocol")).unwrap();
    for guide in [
        "AGENTS.md",
        "docs/agent-engineering.md",
        "docs/testing.md",
        "skills/harness-ci/SKILL.md",
        "skills/protocol/SKILL.md",
    ] {
        fs::write(repo.path().join(guide), "Immutable guide\n").unwrap();
    }
    fs::write(repo.path().join("Makefile"), "fmt-check:\n\tfalse\n").unwrap();
    fs::write(
        repo.path().join("crates/core/tests/case.rs"),
        "#[test]\nfn fixture() { assert_eq!(1, 1); }\n",
    )
    .unwrap();
    fs::write(repo.path().join("docs/a*.rs"), "wildcard-old\n").unwrap();
    fs::write(repo.path().join("docs/a-match.rs"), "matching-old\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "base"]);
    let base = git(repo.path(), &["rev-parse", "HEAD"]);
    git(
        repo.path(),
        &["mv", "crates/core/tests/case.rs", "docs/moved.rs"],
    );
    fs::write(
        repo.path().join("docs/moved.rs"),
        "#[test]\n#[ignore]\nfn fixture() {}\n",
    )
    .unwrap();
    git(repo.path(), &["add", "."]);
    fs::write(repo.path().join("docs/a*.rs"), "wildcard-new\n").unwrap();
    fs::write(repo.path().join("docs/a-match.rs"), "matching-new\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "candidate"]);
    let candidate = git(repo.path(), &["rev-parse", "HEAD"]);
    let selected = cli(repo.path(), &["ci-select"]);
    assert!(
        selected.status.success(),
        "{}",
        String::from_utf8_lossy(&selected.stderr)
    );
    let selection: serde_json::Value = serde_json::from_slice(&selected.stdout).unwrap();
    let mut task = serde_json::json!({"version":1,"id":"fixture","kind":"bug","candidate":candidate,"base":base,"registry_hash":selection["registry_hash"],"role":"coordinator","provider":"codex","status":"planned","objective":"Repair regression","acceptance":["Mandatory gates retained"],"todo":["Implement fix"],"artifacts":[],"rounds_remaining":4});
    let input = external.path().join("task.json");
    fs::write(&input, serde_json::to_vec(&task).unwrap()).unwrap();
    let run = || {
        cli(
            repo.path(),
            &[
                "task-plan",
                "--task",
                input.to_str().unwrap(),
                "--output",
                output.path().to_str().unwrap(),
            ],
        )
    };
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let actual = run();
    assert!(
        actual.status.success(),
        "{}",
        String::from_utf8_lossy(&actual.stderr)
    );
    let plan: serde_json::Value = serde_json::from_slice(&actual.stdout).unwrap();
    assert_eq!(plan["authoritative"], false);
    assert_eq!(plan["plan"]["integrity"]["disposition"], "REVIEW_REQUIRED");
    assert!(
        plan["plan"]["handoff"]["gates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|gate| gate == "test-unit")
    );
    let diff: serde_json::Value =
        serde_json::from_slice(&fs::read(output.path().join("task-diff.json")).unwrap()).unwrap();
    assert!(
        diff["paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "crates/core/tests/case.rs")
    );
    assert!(
        diff["paths"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "docs/moved.rs")
    );
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), candidate);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    let literal = diff["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["path"] == "docs/a*.rs")
        .unwrap();
    assert_eq!(literal["removed"], serde_json::json!(["wildcard-old"]));
    assert_eq!(literal["added"], serde_json::json!(["wildcard-new"]));
    fs::write(repo.path().join(".gitattributes"), "*.rs -diff\n").unwrap();
    fs::create_dir_all(repo.path().join(".git/info")).unwrap();
    fs::write(repo.path().join(".git/info/attributes"), "*.rs -diff\n").unwrap();
    assert!(run().status.success());
    let after_attributes: serde_json::Value =
        serde_json::from_slice(&fs::read(output.path().join("task-diff.json")).unwrap()).unwrap();
    assert_eq!(after_attributes, diff);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = external.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let executable = bin.join("codex");
        for body in [
            "#!/bin/sh\nexit 0\n",
            "#!/bin/sh\nprintf '\\377'\n",
            "#!/bin/sh\nprintf 'fake-codex 1.0\\n'\n",
        ] {
            fs::write(&executable, body).unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
            let observed = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
                .current_dir(repo.path())
                .args([
                    "task-plan",
                    "--task",
                    input.to_str().unwrap(),
                    "--output",
                    output.path().to_str().unwrap(),
                ])
                .env(
                    "PATH",
                    format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
                )
                .output()
                .unwrap();
            assert!(
                observed.status.success(),
                "{}",
                String::from_utf8_lossy(&observed.stderr)
            );
            let descriptor: serde_json::Value = serde_json::from_slice(&observed.stdout).unwrap();
            assert_eq!(
                descriptor["plan"]["adapter"]["authoritative_role_identity"],
                false
            );
        }
    }
    // Unstaged role-policy tampering is not adopted from working files.
    fs::write(repo.path().join("gates/roles.json"), "{\"trusted\":true}").unwrap();
    assert!(run().status.success());
    task["registry_hash"] = serde_json::json!(format!(
        "blake3:registry-v2-canonical-v1:{}",
        "0".repeat(64)
    ));
    fs::write(&input, serde_json::to_vec(&task).unwrap()).unwrap();
    assert!(!run().status.success());
    let failure: serde_json::Value =
        serde_json::from_slice(&fs::read(output.path().join("task-plan.json")).unwrap()).unwrap();
    assert_eq!(failure["status"], "UNAVAILABLE");
    assert_eq!(failure["authoritative"], false);
}
