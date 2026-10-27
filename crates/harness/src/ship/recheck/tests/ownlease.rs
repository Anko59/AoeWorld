//! Earlier pushes are recognized through the actual ship path and a local bare origin.
use super::{start::fetching, *};

fn add(root: &Path, file: &str) {
    fs::write(root.join(file), format!("{file}\n")).unwrap();
    run(root, &["add", file]);
    run(root, &["commit", "-q", "-m", file]);
}

#[test]
fn ownlease_replaces_an_earlier_push_or_prefix_on_older_dev() {
    for prefix in [false, true] {
        for already_rebased in [false, true] {
            let (temp, root) = fixture("true");
            add(&root, "second.txt");
            run(&root, &["push", "-q", "origin", "feature"]);
            let earlier = head(&root);
            if prefix {
                add(&root, "third.txt");
            }
            let moved = advance(&temp, "dev", "dev-only.txt", "dev advanced\n");
            if already_rebased {
                run(&root, &["fetch", "-q", "origin"]);
                run(&root, &["rebase", "-q", "origin/dev"]);
            }
            let shipped = ship(&root, &fetching()).unwrap();
            assert_ne!(shipped.head, earlier);
            assert_eq!(remote_tip(&root, "feature"), shipped.head);
            assert!(contains(&root, &moved));
            assert_eq!(
                git(&root, &["rev-list", "--count", "origin/dev..HEAD"]),
                if prefix { "3" } else { "2" }
            );
            assert_eq!(git(&root, &["status", "--porcelain"]), "");
            println!(
                "ownlease: prefix={prefix}, already_rebased={already_rebased}: replaced {earlier} with {}",
                shipped.head
            );
        }
    }
}

#[test]
fn ownlease_refuses_foreign_modified_extra_and_reordered_remote_patches() {
    for case in ["foreign", "modified", "extra", "reordered"] {
        let (temp, root) = fixture("true");
        add(&root, "second.txt");
        let local = head(&root);
        run(&root, &["checkout", "-q", "-b", "remote-writer"]);
        match case {
            "foreign" => run(
                &root,
                &[
                    "commit",
                    "-q",
                    "--amend",
                    "--no-edit",
                    "--author=Other <other@example.com>",
                ],
            ),
            "modified" => {
                fs::write(root.join("second.txt"), "different\n").unwrap();
                run(&root, &["commit", "-q", "-a", "--amend", "--no-edit"]);
            }
            "extra" => add(&root, "extra.txt"),
            "reordered" => {
                run(&root, &["reset", "--hard", "dev"]);
                run(&root, &["cherry-pick", &local, &format!("{local}^")]);
            }
            _ => unreachable!(),
        }
        run(&root, &["push", "-q", "origin", "HEAD:feature"]);
        let remote = head(&root);
        run(&root, &["checkout", "-q", "feature"]);
        advance(&temp, "dev", "dev-only.txt", "dev advanced\n");
        let error = ship(&root, &fetching()).unwrap_err().to_string();
        assert!(error.contains("SHIP_FORCE=1"), "{case}: {error}");
        assert_eq!(remote_tip(&root, "feature"), remote, "{case}");
        println!("ownlease: {case} refused; remote retained: {error}");
    }
}

#[test]
fn ownlease_push_uses_evidence_even_when_the_checkout_advanced_after_a_rebase() {
    for force in [true, false] {
        for explicit in [false, true] {
            let (_temp, root) = fixture("true");
            let started = head(&root);
            run(&root, &["push", "-q", "origin", "feature"]);
            add(&root, "reviewed.txt");
            let evidence = judge(&root, &offline()).unwrap();
            add(&root, "newer.txt");
            run(&root, &["push", "-q", "origin", "feature"]);
            let newer = head(&root);
            let error = crate::ship::postpush::draft::push(
                &root,
                &evidence,
                &Options { force, ..offline() },
                &started,
                Some(&started),
                explicit.then(|| started.clone()),
            )
            .unwrap_err()
            .to_string();
            assert!(
                error.contains("changed beyond the expected head")
                    || error.contains("SHIP_FORCE=1"),
                "force={force}, explicit={explicit}: {error}"
            );
            assert_eq!(remote_tip(&root, "feature"), newer);
            assert_eq!(head(&root), newer);
        }
    }
}

#[test]
fn ownlease_keeps_the_exact_lease_against_a_later_remote_writer() {
    let (temp, root) = fixture("true");
    run(&root, &["push", "-q", "origin", "feature"]);
    let earlier = head(&root);
    advance(&temp, "dev", "dev-only.txt", "dev advanced\n");
    run(&root, &["fetch", "-q", "origin"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let evidence = judge(&root, &offline()).unwrap();
    let lease = git::lease::refresh_default(&root, "feature", &evidence.head, &evidence.head, true)
        .unwrap();
    assert_eq!(lease, earlier);
    let foreign = advance(&temp, "feature", "foreign.txt", "another writer\n");
    let error = git::publish::push(&root, &evidence, Some(&lease))
        .unwrap_err()
        .to_string();
    assert!(error.contains("stale info"), "{error}");
    assert_eq!(remote_tip(&root, "feature"), foreign);
}

#[test]
fn ownlease_ignores_github_merges_of_dev_on_the_earlier_push() {
    let (temp, root) = fixture("true");
    run(&root, &["push", "-q", "origin", "feature"]);
    let earlier = head(&root);
    advance(&temp, "dev", "dev-only.txt", "dev advanced\n");
    run(&root, &["fetch", "-q", "origin"]);
    run(&root, &["checkout", "-q", "-b", "updater"]);
    let merged = std::process::Command::new("git")
        .current_dir(&root)
        .args(["merge", "-q", "--no-edit", "origin/dev"])
        .env("GIT_COMMITTER_NAME", "GitHub")
        .env("GIT_COMMITTER_EMAIL", "noreply@github.com")
        .output()
        .unwrap();
    assert!(merged.status.success(), "{merged:?}");
    run(&root, &["push", "-q", "origin", "HEAD:feature"]);
    let updater = head(&root);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    assert_ne!(head(&root), earlier);
    let shipped = ship(&root, &fetching()).unwrap();
    assert_ne!(shipped.head, updater);
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
}
