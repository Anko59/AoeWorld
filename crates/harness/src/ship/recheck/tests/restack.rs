use super::*;
use crate::ship::tests::stack::stacked_fixture;
use std::{io::Write, os::unix::fs::PermissionsExt, process::Command};

#[test]
fn a_deleted_stacked_parent_reports_restack_through_the_ship_path() {
    let (temp, root) = stacked_fixture();
    let options = Options {
        base: "parent".into(),
        ..offline()
    };
    let before = judge(&root, &options).unwrap();
    run(
        &temp.path().join("origin.git"),
        &["update-ref", "-d", "refs/heads/parent"],
    );
    let error = ship(&root, &options).unwrap_err().to_string();
    assert!(error.contains("restack"), "{error}");
    assert!(error.contains("git rebase --onto origin/dev"), "{error}");
    assert!(error.contains(&before.base), "{error}");
    assert!(error.contains("SHIP_BASE=dev"), "{error}");
    assert_eq!(head(&root), before.head);
    assert!(remote_tip(&root, "feature").is_empty());
    writeln!(std::io::stdout(), "Ship path rehearsal (deleted local parent):\n{error}\nASSERT: pinned parent tip named; child unchanged; nothing pushed").unwrap();
}

#[test]
fn the_ship_command_reports_a_parent_merged_during_its_gates() {
    let _guard = crate::ship::issues::tests::ENVIRONMENT.lock().unwrap();
    let (temp, root) = stacked_fixture();
    let parent = git(&root, &["rev-parse", "parent"]);
    let original = head(&root);
    fs::write(root.join("Makefile"), "fmt-check:\n\t@git --git-dir=../origin.git update-ref refs/heads/dev refs/heads/parent\n\t@touch ../merged\n").unwrap();
    run(&root, &["commit", "-q", "-am", "merge parent during gates"]);
    let head = head(&root);
    let task = "<!-- level: low -->\n## Why\n> \"explain a merged parent\" — the user\n## What\nRehearse the restack diagnostic.\n";
    let body = temp.path().join("body.md");
    fs::write(&body, task).unwrap();
    {
        let _base = crate::review::base::scope("parent");
        let mut report = graded(&root, Tier::Low, 9);
        report.task_fingerprint = Some(crate::review::Report::task_fingerprint(task));
        report.store(&root).unwrap();
    }
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let real_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|path| path.join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let scripts = [
        (
            "git",
            format!(
                "#!/bin/sh\nif [ \"$3 $4 $5\" = 'remote get-url origin' ]; then\n printf 'https://github.com/project/checkout.git\\n'\nelse\n exec '{}' \"$@\"\nfi\n",
                real_git.display()
            ),
        ),
        (
            "gh",
            format!(
                r##"#!/bin/sh
case "$1 $2" in
  '--version ') echo 'gh version 2.100.0' ;;
  'pr list') case " $* " in
    *' --head parent '*)
      state=OPEN
      [ -f '{}/merged' ] && state=MERGED
      printf '[{{"number":12,"state":"%s","url":"https://github.com/project/checkout/pull/12","body":"# 🤖 For AI\\n## Adversarial review","isCrossRepository":false}}]' "$state" ;;
    *) : ;;
  esac ;;
  *) echo 'unexpected GitHub write' >&2; exit 99 ;;
esac
"##,
                temp.path().display()
            ),
        ),
    ];
    for (name, script) in scripts {
        fs::write(bin.join(name), script).unwrap();
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
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
        .args(["ship", "--title", "Ship: parent diagnostic", "--body-file"])
        .arg(body)
        .env("SHIP_BASE", "parent")
        .env(
            "PATH",
            format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
        )
        .env_remove("AOE_JUDGE_REV")
        .env_remove("SHIP_VIDEO")
        .env_remove("SHIP_TIER")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "{stderr}");
    assert!(stderr.contains("restack"), "{stderr}");
    assert!(stderr.contains("already merged"), "{stderr}");
    assert!(stderr.contains("SHIP_BASE=dev"), "{stderr}");
    assert_eq!(super::head(&root), head);
    assert_ne!(head, original);
    assert!(remote_tip(&root, "feature").is_empty());
    assert_eq!(
        remote_tip(&root, "parent"),
        parent,
        "even a retained parent branch must stop"
    );
    writeln!(std::io::stdout(), "CLI rehearsal (local Git remote, scripted GitHub):\n{stderr}ASSERT: parent merged during gates; retained branch; child unchanged; nothing pushed").unwrap();
}

#[test]
fn an_unreachable_origin_is_not_reported_as_a_restack() {
    let (temp, root) = stacked_fixture();
    let options = Options {
        base: "parent".into(),
        ..offline()
    };
    let before = judge(&root, &options).unwrap();
    run(
        &root,
        &[
            "remote",
            "set-url",
            "origin",
            temp.path().join("missing-origin.git").to_str().unwrap(),
        ],
    );
    let error = before_push(&root, &before, None, &options)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("could not fetch"), "{error}");
    assert!(!error.contains("restack"), "{error}");
    assert_eq!(head(&root), before.head);
}
