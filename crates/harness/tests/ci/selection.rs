use super::*;
use serde_json::{Value, json};
use std::fs;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn dispatch_base_and_cadence_are_recomputed_by_both_cli_commands() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Fixture"]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    fs::create_dir(root.join("gates")).unwrap();
    fs::write(
        root.join("gates/registry.json"),
        include_bytes!("../../../../gates/registry.json"),
    )
    .unwrap();
    fs::create_dir(root.join("docs")).unwrap();
    fs::write(root.join("docs/fixture.md"), "base\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    let base = git(root, &["rev-parse", "HEAD"]);
    git(root, &["update-ref", "refs/remotes/origin/dev", &base]);
    fs::write(root.join("docs/fixture.md"), "feature\n").unwrap();
    git(root, &["commit", "-qam", "docs"]);
    for (event, branch, explicit, expected) in [
        (
            "workflow_dispatch",
            "feature/example",
            "",
            Some(base.as_str()),
        ),
        ("workflow_dispatch", "dev", "", None),
        ("workflow_dispatch", "main", "", None),
        ("push", "feature/example", "", None),
        (
            "pull_request",
            "feature/example",
            base.as_str(),
            Some(base.as_str()),
        ),
    ] {
        let setup = |name| {
            let mut cmd = command(root, name);
            cmd.env("GITHUB_EVENT_NAME", event)
                .env("GITHUB_REF_NAME", branch)
                .env("AOE_BASE_SHA", explicit);
            cmd
        };
        let select = proof(&mut setup("ci-select"));
        assert!(select.status.success(), "{select:?}");
        let manifest: Value = serde_json::from_slice(&select.stdout).unwrap();
        assert_eq!(manifest["requested_base"], json!(expected));
        assert_eq!(manifest["base"], json!(expected.map(|_| &base)));
        assert_eq!(manifest["jobs"]["browser"], json!(expected.is_none()));
        if expected.is_some() {
            assert_eq!(manifest["paths"], json!(["docs/fixture.md"]));
            assert!(
                !manifest["gates"]
                    .as_array()
                    .unwrap()
                    .contains(&json!("coverage"))
            );
        }
        let mut results = json!({"select":"success"});
        for (job, selected) in manifest["jobs"].as_object().unwrap() {
            results[job] = json!(if selected == &json!(true) {
                "success"
            } else {
                "skipped"
            });
        }
        if event == "workflow_dispatch" && branch == "feature/example" {
            // dev advances between the selection and the final check.
            let moved = git(
                root,
                &[
                    "commit-tree",
                    "-p",
                    &base,
                    "-m",
                    "dev moved",
                    &format!("{base}^{{tree}}"),
                ],
            );
            git(root, &["update-ref", "refs/remotes/origin/dev", &moved]);
        }
        let mut check = setup("ci-check");
        check
            .env(
                "AOE_SELECTION_JSON",
                String::from_utf8_lossy(&select.stdout).trim(),
            )
            .env("AOE_JOB_RESULTS_JSON", results.to_string());
        assert!(proof(&mut check).status.success());
        if event == "workflow_dispatch" && branch == "feature/example" {
            // The manifest cannot replace the independently chosen requested base.
            let mut forged = manifest.clone();
            forged["requested_base"] = Value::Null;
            check.env("AOE_SELECTION_JSON", forged.to_string());
            assert!(!proof(&mut check).status.success());
        }
    }
}
