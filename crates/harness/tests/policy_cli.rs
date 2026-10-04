use std::{fs, path::Path, process::Command};
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
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
    assert!(result.status.success());
    String::from_utf8(result.stdout).unwrap().trim().to_owned()
}
#[test]
fn invalid_inputs_replace_stale_preparation_and_do_not_modify_candidate_git() {
    let repo = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--template="]);
    fs::write(repo.path().join("file"), "candidate\n").unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "fixture"]);
    let commit = git(repo.path(), &["rev-parse", "HEAD"]);
    let index = fs::read(repo.path().join(".git/index")).unwrap();
    let config = fs::read(repo.path().join(".git/config")).unwrap();
    let anchor = external.path().join("anchor.json");
    fs::write(&anchor, serde_json::to_vec(&serde_json::json!({"schema":1,"repository":"Anko59/AoeWorld","repository_id":42,"remote_url":"https://github.com/Anko59/AoeWorld.git","integration_branch":"dev"})).unwrap()).unwrap();
    let invoke = |candidate: &str, path: &Path| {
        fs::write(
            output.path().join("preparation.json"),
            "{\"status\":\"PREPARED_NON_AUTHORITATIVE\"}",
        )
        .unwrap();
        let result = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(repo.path())
            .args([
                "policy-prepare",
                "--anchor",
                path.to_str().unwrap(),
                "--candidate",
                candidate,
                "--output",
                output.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        let descriptor: serde_json::Value =
            serde_json::from_slice(&fs::read(output.path().join("preparation.json")).unwrap())
                .unwrap();
        assert_eq!(descriptor["status"], "UNAVAILABLE");
        assert_eq!(descriptor["authoritative"], false);
    };
    invoke("HEAD", &anchor);
    let invalid = external.path().join("invalid.json");
    fs::write(&invalid, b"{\"trusted\":true}").unwrap();
    invoke(&commit, &invalid);
    #[cfg(unix)]
    {
        let linked = external.path().join("linked.json");
        fs::hard_link(&anchor, &linked).unwrap();
        invoke(&commit, &linked);
        let symlinked = external.path().join("symlink.json");
        std::os::unix::fs::symlink(&anchor, &symlinked).unwrap();
        invoke(&commit, &symlinked);
    }
    assert_eq!(git(repo.path(), &["rev-parse", "HEAD"]), commit);
    assert_eq!(fs::read(repo.path().join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(repo.path().join(".git/config")).unwrap(), config);
}

#[cfg(unix)]
#[test]
fn symlinked_anchor_into_evidence_is_rejected_before_stale_descriptor_replacement() {
    let repo = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--template="]);
    let reserved = output.path().join("preparation.json");
    let sentinel = b"{\"status\":\"preserve-me\"}";
    fs::write(&reserved, sentinel).unwrap();
    let alias = external.path().join("anchor.json");
    std::os::unix::fs::symlink(&reserved, &alias).unwrap();

    let result = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
        .current_dir(repo.path())
        .args([
            "policy-prepare",
            "--anchor",
            alias.to_str().unwrap(),
            "--candidate",
            &"a".repeat(40),
            "--output",
            output.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("must not overlap evidence output"));
    assert_eq!(fs::read(&reserved).unwrap(), sentinel);
}

#[cfg(unix)]
#[test]
fn dangling_anchor_aliases_do_not_create_reserved_output_targets() {
    let repo = tempfile::tempdir().unwrap();
    let output = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--template="]);
    let reserved = output.path().join("preparation.json");
    let direct_alias = external.path().join("dangling-anchor.json");
    std::os::unix::fs::symlink(&reserved, &direct_alias).unwrap();
    let directory_alias = external.path().join("output-alias");
    std::os::unix::fs::symlink(output.path(), &directory_alias).unwrap();
    let candidate = "a".repeat(40);

    for alias in [direct_alias, directory_alias.join("preparation.json")] {
        let result = Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(repo.path())
            .args([
                "policy-prepare",
                "--anchor",
                alias.to_str().unwrap(),
                "--candidate",
                &candidate,
                "--output",
                output.path().to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!reserved.exists(), "input alias must not create its target");
    }
}
