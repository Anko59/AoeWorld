use crate::{
    review::{Report, Tier},
    ship::{
        git,
        review_gate::graded,
        tests::{fixture, run},
    },
};
use std::{fs, io::Write, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn the_ship_command_reports_only_the_prs_commits_after_an_updater_merge() {
    rehearse_merge(false);
}

#[test]
fn the_ship_command_reports_product_and_build_added_in_a_dev_merge() {
    rehearse_merge(true);
}

fn rehearse_merge(own_changes: bool) {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    let (temp, root) = fixture("true");
    run(&root, &["checkout", "-q", "dev"]);
    fs::create_dir_all(root.join("crates/demo/src")).unwrap();
    fs::write(
        root.join("crates/demo/src/lib.rs"),
        "pub fn incoming() {}\n",
    )
    .unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "incoming dev product"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["merge", "-q", "--no-ff", "--no-commit", "dev"]);
    if own_changes {
        fs::create_dir_all(root.join("crates/simulation/src")).unwrap();
        fs::write(
            root.join("crates/simulation/src/own.rs"),
            "pub fn untested() {}\n",
        )
        .unwrap();
        fs::create_dir_all(root.join(".cargo")).unwrap();
        fs::write(root.join(".cargo/config.toml"), "[build]\n").unwrap();
        run(&root, &["add", "-A"]);
    }
    run(&root, &["commit", "-q", "-m", "merge dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let task = "<!-- level: low -->\n## Why\n> \"attribute only PR commits\" — the user\n## What\nRehearse test-first after merging dev.\n";
    let body = temp.path().join("body.md");
    fs::write(&body, task).unwrap();
    let mut review = graded(&root, Tier::Low, 9);
    review.task_fingerprint = Some(Report::task_fingerprint(task));
    review.store(&root).unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("git"))
        .find(|file| file.is_file())
        .unwrap();
    fs::write(bin.join("git"), format!("#!/bin/sh\nif [ \"$3 $4 $5\" = 'remote get-url origin' ]; then\n printf 'https://github.com/project/checkout.git\\n'\nelse\n exec '{}' \"$@\"\nfi\n", real_git.display())).unwrap();
    fs::write(
        bin.join("gh"),
        r#####"#!/bin/sh
case "$1 $2" in
  '--version ') echo 'gh version 2.100.0' ;;
  'issue view') printf '%s\n' '{"state":"OPEN","body":"#### Game findings (owned by the game session)\n","updatedAt":"t"}' ;;
  'issue comment') cat >/dev/null ;;
  'pr list') printf '{"url":"https://github.com/project/checkout/pull/43","baseRefName":"dev"}' ;;
  'pr view') printf '{"isDraft":false,"autoMergeRequest":null}' ;;
  'api '*) case "$*" in
    *'/actions/workflows/ci.yml/runs?'*) printf '{"workflow_runs":[]}' ;;
    *) printf '[]' ;;
  esac ;;
esac
case " $* " in *' --body-file - '*) cat >/dev/null ;; esac
"#####,
    )
    .unwrap();
    for tool in ["git", "gh"] {
        fs::set_permissions(bin.join(tool), fs::Permissions::from_mode(0o755)).unwrap();
    }
    let binary = std::env::current_exe()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("aoe-harness");
    let output = Command::new(binary)
        .current_dir(&root)
        .args(["ship", "--body-file"])
        .arg(body)
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env_remove("AOE_JUDGE_REV")
        .env_remove("SHIP_BASE")
        .env_remove("SHIP_VIDEO")
        .env_remove("SHIP_TIER")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stdout}\n{stderr}");
    assert!(
        stderr.contains("review: reusing the passing low review of this commit"),
        "{stderr}"
    );
    if own_changes {
        assert!(stderr.contains("merge (product)"), "{stderr}");
        assert!(
            stderr.contains("before the first product commit: 0\n"),
            "{stderr}"
        );
        assert!(
            stderr.contains("build-time files changed: .cargo/config.toml\n"),
            "{stderr}"
        );
        assert!(!stderr.contains("test-first: not applicable"), "{stderr}");
    } else {
        assert!(stderr.contains("test-first: not applicable"), "{stderr}");
        assert!(!stderr.contains("merge (product)"), "{stderr}");
    }
    assert_eq!(
        git::git(&root, &["rev-parse", "refs/remotes/origin/feature"]).unwrap(),
        head
    );
    let assertion = if own_changes {
        "merge-added product and build counted; zero tests before product; review accepted and local remote updated"
    } else {
        "incoming dev product and its merge excluded; docs-only PR stays not applicable"
    };
    writeln!(std::io::stdout(), "CLI test-first rehearsal (local Git remote, scripted GitHub):\n{stdout}{stderr}ASSERT: {assertion}").unwrap();
}
