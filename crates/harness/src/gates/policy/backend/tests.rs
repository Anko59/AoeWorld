use super::*;
use std::process::Command;

fn anchor() -> Anchor {
    Anchor::parse(br#"{"schema":1,"repository":"Example/Policy","repository_id":7,"remote_url":"https://github.com/Example/Policy.git","integration_branch":"dev"}"#).unwrap()
}
fn responses() -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    (
        serde_json::json!({"id":7,"full_name":"Example/Policy","other_github_fields":true}),
        serde_json::json!({"name":"dev","protected":true,"commit":{"sha":"1".repeat(40)}}),
        serde_json::json!({"required_status_checks":{"strict":true,"contexts":["required"],"checks":[{"context":"required","app_id":123}]}}),
    )
}
#[test]
fn authenticated_response_normalization_requires_numeric_repository_and_protection() {
    let (repository, branch, protection) = responses();
    let source = observed(&anchor(), &repository, &branch, &protection).unwrap();
    assert_eq!(source.commit, "1".repeat(40));
    assert!(
        source
            .resolve(&anchor(), &"2".repeat(40), &"3".repeat(40))
            .is_err()
    );
    let mut wrong = repository.clone();
    wrong["id"] = serde_json::json!(8);
    assert!(observed(&anchor(), &wrong, &branch, &protection).is_err());
    wrong = repository.clone();
    wrong["full_name"] = serde_json::json!("Other/Policy");
    assert!(observed(&anchor(), &wrong, &branch, &protection).is_err());
    let mut wrong = branch.clone();
    wrong["protected"] = serde_json::json!(false);
    assert!(observed(&anchor(), &repository, &wrong, &protection).is_err());
    wrong = branch.clone();
    wrong["name"] = serde_json::json!("main");
    assert!(observed(&anchor(), &repository, &wrong, &protection).is_err());
    wrong = branch.clone();
    wrong["commit"]["sha"] = serde_json::json!("--upload-pack=evil");
    assert!(observed(&anchor(), &repository, &wrong, &protection).is_err());
    let mut wrong = protection.clone();
    wrong["required_status_checks"]["strict"] = serde_json::json!(false);
    assert!(observed(&anchor(), &repository, &branch, &wrong).is_err());
    wrong = protection.clone();
    wrong["required_status_checks"] = serde_json::Value::Null;
    assert!(observed(&anchor(), &repository, &branch, &wrong).is_err());
    wrong = serde_json::json!({"required_status_checks":{"strict":true,"contexts":[],"checks":[]}});
    assert!(observed(&anchor(), &repository, &branch, &wrong).is_err());
    wrong = serde_json::json!({"required_status_checks":{"strict":true,"contexts":"required"}});
    assert!(observed(&anchor(), &repository, &branch, &wrong).is_err());
}
#[test]
fn advertisement_requires_single_exact_full_oid_and_ref() {
    let valid = format!("{}\trefs/heads/dev\n", "1".repeat(40));
    assert_eq!(advertised_commit(valid.as_bytes()).unwrap(), "1".repeat(40));
    for bad in [
        format!("{valid}{valid}"),
        "abcd\trefs/heads/dev\n".into(),
        format!("{}\trefs/heads/main\n", "1".repeat(40)),
        format!("{}\trefs/heads/dev\r\n", "1".repeat(40)),
        "--fake\trefs/heads/dev\n".into(),
    ] {
        assert!(advertised_commit(bad.as_bytes()).is_err());
    }
}
#[test]
fn git_transport_uses_empty_environment_and_fixed_oid_not_source_refs() {
    let home = tempfile::tempdir().unwrap();
    let configured = anchor();
    let oid = "1".repeat(40);
    let args = git_arguments(
        home.path(),
        &["fetch", "--no-tags", &configured.remote_url, &oid],
    )
    .unwrap();
    assert_eq!(args[0], "-i");
    let git = args.iter().position(|arg| arg == "git").unwrap();
    assert!(args[1..git].iter().all(|arg| {
        [
            "PATH=",
            "HOME=",
            "XDG_CONFIG_HOME=",
            "LANG=",
            "GIT_CONFIG_GLOBAL=",
            "GIT_CONFIG_SYSTEM=",
            "GIT_CONFIG_NOSYSTEM=",
            "GIT_NO_REPLACE_OBJECTS=",
            "GIT_ATTR_NOSYSTEM=",
            "GIT_OPTIONAL_LOCKS=",
            "GIT_TERMINAL_PROMPT=",
        ]
        .iter()
        .any(|prefix| arg.starts_with(prefix))
    }));
    for restriction in [
        "credential.helper=",
        "core.hooksPath=/dev/null",
        "protocol.file.allow=never",
        "protocol.ext.allow=never",
        "http.followRedirects=false",
        "fetch.fsckObjects=true",
    ] {
        assert!(args.iter().any(|arg| arg == restriction));
    }
    assert_eq!(
        &args[args.len() - 4..],
        &[
            "fetch",
            "--no-tags",
            configured.remote_url.as_str(),
            oid.as_str()
        ]
    );
    assert!(
        !args
            .iter()
            .any(|arg| arg == "origin" || arg == "dev" || arg.starts_with("GH_TOKEN="))
    );
}
fn fixture_git(root: &Path, home: &Path, args: &[&str]) -> String {
    let mut fixed = vec![
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "-c",
        "commit.gpgsign=false",
    ];
    fixed.extend(args);
    let result = Command::new("env")
        .current_dir(root)
        .args(git_arguments(home, &fixed).unwrap())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}
#[test]
fn private_fetched_commit_materializes_raw_tree_and_preserves_source_metadata() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("repository");
    let home = owner.path().join("home");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&home).unwrap();
    fixture_git(&root, &home, &["init", "--quiet", "--template="]);
    fs::write(root.join("policy.txt"), b"immutable policy bytes\n").unwrap();
    fixture_git(&root, &home, &["add", "--", "policy.txt"]);
    fixture_git(&root, &home, &["commit", "--quiet", "-m", "fixture"]);
    let commit = fixture_git(&root, &home, &["rev-parse", "HEAD"]);
    let tree = fixture_git(&root, &home, &["rev-parse", "HEAD^{tree}"]);
    let before: Vec<_> = ["HEAD", "index", "config"]
        .iter()
        .map(|file| fs::read(root.join(".git").join(file)).unwrap())
        .collect();
    let observed = observed(
        &anchor(),
        &serde_json::json!({"id":7,"full_name":"Example/Policy"}),
        &serde_json::json!({"name":"dev","protected":true,"commit":{"sha":commit}}),
        &responses().2,
    )
    .unwrap();
    let identity = observed.resolve(&anchor(), &commit, &tree).unwrap();
    let resolved = materialize(owner, identity).unwrap();
    assert_eq!(
        fs::read(resolved.snapshot.root().join("policy.txt")).unwrap(),
        b"immutable policy bytes\n"
    );
    for (index, file) in ["HEAD", "index", "config"].iter().enumerate() {
        assert_eq!(
            fs::read(root.join(".git").join(file)).unwrap(),
            before[index]
        );
    }
    resolved.snapshot.run_checked(|_| Ok(())).unwrap();
    assert_eq!(resolved.identity.commit, commit);
    assert_eq!(resolved.identity.tree, tree);
    assert_eq!(resolved.owner.path().join("repository"), root);
}
