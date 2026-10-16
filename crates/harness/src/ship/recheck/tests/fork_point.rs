use super::*;

/// Build A -> P -> C before the parent moves, for either selected ref namespace.
pub(super) fn inherited(local: bool, gate: &str) -> (Temp, PathBuf, Options, String) {
    let (temp, root) = fixture(gate);
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    fs::write(root.join("obsolete.txt"), "old parent\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "parent P"]);
    let parent = head(&root);
    if !local {
        run(&root, &["push", "-q", "origin", "parent"]);
    }
    run(&root, &["checkout", "-q", "-B", "feature", "parent"]);
    fs::write(root.join("child.txt"), "child\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "child C"]);
    // A short ref would be ambiguous; the replay boundary must use the full ref.
    run(&root, &["tag", "parent", "dev"]);
    let options = Options {
        base: "parent".into(),
        review_ahead: local,
        no_fetch: false,
        no_review: false,
        ..offline()
    };
    (temp, root, options, parent)
}

pub(super) fn move_parent(root: &Path, local: bool, amend: bool) -> String {
    run(root, &["checkout", "-q", "parent"]);
    if amend {
        run(root, &["rm", "-q", "obsolete.txt"]);
    }
    fs::write(root.join("replacement.txt"), "new parent\n").unwrap();
    run(root, &["add", "-A"]);
    if amend {
        run(root, &["commit", "-q", "--amend", "-m", "parent Q"]);
    } else {
        run(root, &["commit", "-q", "-m", "parent gained commit"]);
    }
    let moved = head(root);
    if !local {
        run(
            root,
            &[
                "push",
                "-q",
                "--force-with-lease",
                "origin",
                "refs/heads/parent",
            ],
        );
    }
    run(root, &["checkout", "-q", "feature"]);
    moved
}

fn reach_review(temp: &Temp, root: &Path, options: &Options) {
    let reviewer =
        |_: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
            assert_eq!(runs(temp), 1, "full gates precede review");
            Err::<crate::review::Report, Box<dyn std::error::Error>>("reached review".into())
        };
    assert_eq!(
        ship_with(root, options, &reviewer).unwrap_err().to_string(),
        "reached review"
    );
}

#[test]
fn an_amended_parent_replays_only_the_inherited_child() {
    for local in [true, false] {
        let (temp, root, options, old_parent) = inherited(local, LOGGED);
        let moved = move_parent(&root, local, true);
        reach_review(&temp, &root, &options);
        assert!(contains(&root, &moved));
        assert!(!contains(&root, &old_parent));
        assert!(!root.join("obsolete.txt").exists());
        assert!(root.join("replacement.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("child.txt")).unwrap(),
            "child\n"
        );
        assert_eq!(
            git(&root, &["rev-list", "--count", &format!("{moved}..HEAD")]),
            "1"
        );
        assert_eq!(
            git(&root, &["diff", "--name-only", &moved, "HEAD"]),
            "child.txt"
        );
        assert_eq!(remote_tip(&root, "feature"), "");
    }
}

#[test]
fn a_parent_that_only_gained_commits_rebases_the_inherited_child() {
    for local in [true, false] {
        let (temp, root, options, old_parent) = inherited(local, LOGGED);
        let moved = move_parent(&root, local, false);
        reach_review(&temp, &root, &options);
        assert!(contains(&root, &moved));
        assert!(contains(&root, &old_parent));
        assert!(root.join("obsolete.txt").exists());
        assert!(root.join("replacement.txt").exists());
        assert!(root.join("child.txt").exists());
        assert_eq!(
            git(&root, &["rev-list", "--count", &format!("{moved}..HEAD")]),
            "1"
        );
        assert_eq!(
            git(&root, &["diff", "--name-only", &moved, "HEAD"]),
            "child.txt"
        );
    }
}

#[test]
fn a_rewritten_parent_without_a_usable_reflog_refuses_without_changes() {
    for local in [true, false] {
        let (temp, root, options, _) = inherited(local, LOGGED);
        move_parent(&root, local, true);
        run(&root, &["reflog", "expire", "--expire=now", "--all"]);
        let before = head(&root);
        let reviewer =
            |_: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
                panic!("no review when the child boundary is unknown")
            };
        let error = ship_with(&root, &options, &reviewer)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("rebase the child onto parent yourself, then ship"),
            "{error}"
        );
        assert!(error.contains("nothing was pushed"), "{error}");
        assert_eq!(head(&root), before);
        assert_eq!(git(&root, &["status", "--porcelain"]), "");
        assert_eq!(runs(&temp), 0);
        assert!(evidence::read(&root, &before).unwrap().is_none());
        assert_eq!(remote_tip(&root, "feature"), "");
        assert!(root.join("obsolete.txt").exists());
        assert!(!root.join("replacement.txt").exists());
    }
}
