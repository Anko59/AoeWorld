#![cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_DIR")
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
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end_matches('\n')
        .to_owned()
}

#[test]
fn structure_cli_ignores_foreign_hook_git_environment_and_audits_private_files() {
    let owner = tempfile::tempdir().unwrap();
    let private = owner.path().join("private");
    let foreign = owner.path().join("source");
    for root in [&private, &foreign] {
        fs::create_dir(root).unwrap();
        git(root, &["init", "--quiet", "--template="]);
    }
    fs::write(private.join("long.rs"), "// valid control\n").unwrap();
    git(&private, &["add", "long.rs"]);
    let check = || {
        Command::new(env!("CARGO_BIN_EXE_aoe-harness"))
            .current_dir(&private)
            .env("GIT_DIR", foreign.join(".git"))
            .env("GIT_INDEX_FILE", ".git/next-index-unavailable.lock")
            .args(["structure-check"])
            .output()
            .unwrap()
    };
    assert!(check().status.success(), "valid private control");
    fs::write(private.join("long.rs"), "// overlimit\n".repeat(501)).unwrap();
    let failure = check();
    assert!(!failure.status.success());
    assert!(String::from_utf8_lossy(&failure.stderr).contains("501 lines (max 500)"));
}

#[test]
fn actual_commit_hooks_use_pending_index_for_all_and_pathspec_in_linked_worktrees() {
    for linked in [false, true] {
        for all in [false, true] {
            let owner = tempfile::tempdir().unwrap();
            let main = owner.path().join("main");
            fs::create_dir(&main).unwrap();
            git(&main, &["init", "--quiet", "--template="]);
            for file in ["a", "b"] {
                fs::write(main.join(file), "base\n").unwrap();
            }
            fs::write(main.join(".gitignore"), "/.cache/\n").unwrap();
            git(&main, &["add", "."]);
            git(&main, &["commit", "--quiet", "-m", "base"]);
            let checkout = if linked {
                let path = owner.path().join("linked");
                git(
                    &main,
                    &[
                        "worktree",
                        "add",
                        "--quiet",
                        "--detach",
                        path.to_str().unwrap(),
                        "HEAD",
                    ],
                );
                path
            } else {
                main.clone()
            };
            for file in ["a", "b"] {
                fs::write(checkout.join(file), "staged\n").unwrap();
            }
            git(&checkout, &["add", "a", "b"]);
            for file in ["a", "b"] {
                fs::write(checkout.join(file), "working\n").unwrap();
            }
            let hooks = owner.path().join("hooks");
            fs::create_dir(&hooks).unwrap();
            let hook = hooks.join("pre-commit");
            fs::write(
                &hook,
                "#!/bin/sh\nexec \"$AOE_HARNESS_BIN\" scope-check > \"$AOE_SCOPE_PROOF\"\n",
            )
            .unwrap();
            fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
            git(
                &checkout,
                &["config", "core.hooksPath", hooks.to_str().unwrap()],
            );
            let proof = owner.path().join("proof.json");
            let mut command = Command::new("git");
            command
                .current_dir(&checkout)
                .env("AOE_HARNESS_BIN", env!("CARGO_BIN_EXE_aoe-harness"))
                .env("AOE_SCOPE_PROOF", &proof)
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "-c",
                    "commit.gpgsign=false",
                    "commit",
                    "--quiet",
                    "-m",
                    "pending",
                ]);
            if all {
                command.arg("-a");
            } else {
                command.args(["--", "a"]);
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let proof: serde_json::Value =
                serde_json::from_slice(&fs::read(proof).unwrap()).unwrap();
            assert_eq!(
                proof["identity"]["tree"],
                git(&checkout, &["rev-parse", "HEAD^{tree}"])
            );
            assert_eq!(proof["authoritative"], false);
            let index = proof["identity"]["effective_index"].as_str().unwrap();
            assert!(index.ends_with(".lock"), "pending temporary index: {index}");
            if !all {
                assert!(index.contains("next-index-"));
            }
            if linked {
                assert!(index.contains("worktrees/linked/"));
            }
            assert_eq!(git(&checkout, &["show", "HEAD:a"]), "working");
            assert_eq!(
                git(&checkout, &["show", "HEAD:b"]),
                if all { "working" } else { "base" }
            );
        }
    }
}
