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
