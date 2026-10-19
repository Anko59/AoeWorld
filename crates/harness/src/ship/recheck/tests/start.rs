use super::*;

pub(super) fn fetching() -> Options {
    Options {
        no_fetch: false,
        ..offline()
    }
}

#[test]
fn an_initially_behind_ship_rebases_before_gates() {
    let (temp, root) = fixture("test -f other.txt && git rev-parse HEAD >> ../gate.log");
    let before = head(&root);
    let moved = advance(&temp, "dev", "other.txt", "o\n");
    let shipped = ship(&root, &fetching()).unwrap();
    assert_ne!(shipped.head, before);
    assert!(contains(&root, &moved));
    assert_eq!(shipped.base, moved);
    assert_eq!(shipped.merge_base, moved);
    assert_eq!(shipped.changed, ["README.md"]);
    assert!(
        shipped.recheck.is_none(),
        "full gates ran after the initial rebase"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("gate.log"))
            .unwrap()
            .trim(),
        shipped.head
    );
    assert_eq!(remote_tip(&root, "feature"), shipped.head);
}

#[test]
fn an_initially_current_ship_keeps_its_commit() {
    let (temp, root) = fixture(LOGGED);
    let before = head(&root);
    let shipped = ship(&root, &fetching()).unwrap();
    assert_eq!(shipped.head, before);
    assert!(shipped.recheck.is_none());
    assert_eq!(runs(&temp), 1);
}

#[test]
fn an_initial_conflict_restores_the_branch_before_gates_or_review() {
    let (temp, root) = fixture(LOGGED);
    let before = head(&root);
    advance(&temp, "dev", "README.md", "conflict\n");
    let options = Options {
        no_review: false,
        ..fetching()
    };
    let reviewer =
        |_: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
            panic!("review must not run on an initial conflict")
        };
    let error = ship_with(&root, &options, &reviewer)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("README.md") && error.contains("before the gates"),
        "{error}"
    );
    assert!(error.contains("nothing was pushed"), "{error}");
    assert_eq!(head(&root), before);
    assert_eq!(git(&root, &["status", "--porcelain"]), "");
    assert_eq!(git(&root, &["symbolic-ref", "--short", "HEAD"]), "feature");
    assert_eq!(runs(&temp), 0);
    assert!(evidence::read(&root, &before).unwrap().is_none());
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn an_initially_behind_published_ship_uses_the_original_lease_even_if_dev_moves_again() {
    for again in [false, true] {
        let gate = "if [ ! -f ../gate.log ]; then git -C ../other -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m again && git -C ../other push -q origin dev && touch ../gate.log; fi";
        let (temp, root) = fixture(if again { gate } else { "true" });
        run(&root, &["push", "-q", "origin", "feature"]);
        advance(&temp, "dev", "other.txt", "o\n");
        let shipped = ship(&root, &fetching()).unwrap();
        assert_eq!(remote_tip(&root, "feature"), shipped.head);
        assert_eq!(shipped.recheck.is_some(), again);
    }
}

#[test]
fn the_initial_rebase_also_precedes_review() {
    let (temp, root) = fixture(LOGGED);
    let before = head(&root);
    let moved = advance(&temp, "dev", "other.txt", "o\n");
    let options = Options {
        no_review: false,
        ..fetching()
    };
    let reviewer =
        |root: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
            assert!(contains(root, &moved));
            assert_ne!(head(root), before);
            assert_eq!(runs(&temp), 1);
            Err::<crate::review::Report, Box<dyn std::error::Error>>(
                "review reached after rebase".into(),
            )
        };
    let error = ship_with(&root, &options, &reviewer)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "review reached after rebase");
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn an_initially_behind_local_parent_is_rebased_before_review_ahead() {
    let (temp, root) = fixture(LOGGED);
    run(&root, &["checkout", "-q", "-b", "parent", "dev"]);
    fs::write(root.join("parent.txt"), "p\n").unwrap();
    run(&root, &["add", "parent.txt"]);
    run(&root, &["commit", "-q", "-m", "parent"]);
    let parent = head(&root);
    run(&root, &["checkout", "-q", "feature"]);
    let options = Options {
        base: "parent".into(),
        review_ahead: true,
        no_review: false,
        ..fetching()
    };
    let reviewer =
        |root: &Path, _: Tier, _: crate::agents::Runtime, _: &str, _: &crate::review::Plan| {
            assert!(contains(root, &parent));
            assert_eq!(runs(&temp), 1);
            assert_eq!(
                git(root, &["rev-list", "--count", &format!("{parent}..HEAD")]),
                "1"
            );
            Err::<crate::review::Report, Box<dyn std::error::Error>>(
                "local parent reviewed after rebase".into(),
            )
        };
    let error = ship_with(&root, &options, &reviewer)
        .unwrap_err()
        .to_string();
    assert_eq!(error, "local parent reviewed after rebase");
    assert_eq!(remote_tip(&root, "feature"), "");
}

#[test]
fn an_initially_behind_stack_rebases_only_its_child_commits() {
    let (temp, root) = fixture("test -f p2.txt && test ! -f dev-only.txt");
    run(&root, &["branch", "parent", "dev"]);
    run(&root, &["push", "-q", "origin", "parent"]);
    let moved = advance(&temp, "parent", "p2.txt", "p\n");
    advance(&temp, "dev", "dev-only.txt", "d\n");
    let options = Options {
        base: "parent".into(),
        ..fetching()
    };
    let shipped = ship(&root, &options).unwrap();
    assert_eq!(shipped.base, moved);
    assert_eq!(shipped.changed, ["README.md"]);
    assert_eq!(
        git(&root, &["rev-list", "--count", &format!("{moved}..HEAD")]),
        "1"
    );
    assert!(!root.join("dev-only.txt").exists());
}
