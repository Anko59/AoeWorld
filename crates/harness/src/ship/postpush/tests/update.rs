use super::stub::{World, comment};
use crate::ship::postpush::update::{execute, update};
use std::fs;

pub(super) fn node(number: u64, auto: bool, cross: bool) -> serde_json::Value {
    serde_json::json!({
        "id": format!("PR_{number}"),
        "number": number,
        "headRefOid": format!("head{number}"),
        "headRefName": format!("feature/{number}"),
        "isCrossRepository": cross,
        "autoMergeRequest": if auto { serde_json::json!({"enabledAt": "t"}) } else { serde_json::Value::Null },
    })
}

pub(super) fn listing(world: &World, nodes: &[serde_json::Value]) {
    let data = serde_json::json!({"data": {"repository": {"pullRequests": {"nodes": nodes}}}});
    world.answer("list.json", &data.to_string());
}

fn behind(world: &World, number: u64, by: u64) {
    world.answer(
        &format!("compare-head{number}.json"),
        &format!("{{\"behind_by\": {by}}}"),
    );
    let head = world.head();
    world.publish_pull(number, &head);
    world.answer(
        &format!("head-{number}.json"),
        &super::dispatch::head_json(&head),
    );
    world.answer(
        &format!("pull-{number}.json"),
        &super::stub::pull_json("open", "dev", &head),
    );
}

fn rebased(world: &World) -> Vec<String> {
    world
        .calls()
        .lines()
        .filter_map(|line| line.strip_prefix("<id="))
        .map(|id| id.trim_end_matches('>').to_owned())
        .collect()
}

#[test]
fn only_auto_merge_pull_requests_that_are_behind_are_rebased() {
    let world = World::new();
    listing(
        &world,
        &[
            node(5, true, false),
            node(6, true, false),
            node(7, false, false),
            node(8, true, true),
        ],
    );
    for number in [5, 7, 8] {
        behind(&world, number, 3);
    }
    behind(&world, 6, 0);
    update(&world.root, 5).unwrap();
    assert_eq!(rebased(&world), ["PR_5"]);
    let calls = world.calls();
    assert!(
        calls.contains("<oid=head5>")
            && calls.contains("updateMethod: MERGE")
            && !calls.contains("updateMethod: REBASE"),
        "{calls}"
    );
    assert_eq!(
        calls
            .matches("<repos/project/checkout/actions/workflows/ci.yml/dispatches>")
            .count(),
        1,
        "{calls}"
    );
    // A rebase that lands posts no conflict or failure comment.
    assert!(!calls.contains("aoe-harness:update-"), "{calls}");
}

#[test]
fn a_run_handles_at_most_the_bound_oldest_first() {
    let world = World::new();
    listing(
        &world,
        &[
            node(9, true, false),
            node(3, true, false),
            node(6, true, false),
            node(4, true, false),
        ],
    );
    for number in [3, 4, 6, 9] {
        behind(&world, number, 1);
    }
    update(&world.root, 2).unwrap();
    assert_eq!(rebased(&world), ["PR_3", "PR_4"]);
}

#[test]
fn a_conflicting_pull_request_is_left_alone_with_one_comment_naming_the_files() {
    let world = World::new();
    // The pull request changes README.md; dev changes it differently.
    let head = world.head();
    world.publish_pull(9, &head);
    world.move_dev("README.md", "dev's README\n");
    let mut conflicted = node(9, true, false);
    conflicted["headRefOid"] = head.clone().into();
    listing(&world, &[conflicted]);
    world.answer(&format!("compare-{head}.json"), "{\"behind_by\": 1}");
    world.answer("rebase-fails-PR_9", "merge conflict between base and head");
    update(&world.root, 5).unwrap();
    let calls = world.calls();
    assert!(
        calls.contains("<repos/project/checkout/issues/9/comments>"),
        "{calls}"
    );
    assert!(
        calls.contains("README.md") && calls.contains("could not be updated (dev merged in)"),
        "{calls}"
    );
    assert_eq!(
        calls.matches("issues/9/comments").count(),
        2,
        "one listing, one comment: {calls}"
    );
    // The same conflict on the next push of dev does not repeat the comment.
    let tag = format!("<!-- aoe-harness:update-conflict {head} -->\nsaid before");
    world.answer(
        "comments-9.json",
        &serde_json::json!([comment(1, "NONE", &tag)]).to_string(),
    );
    world.clear();
    update(&world.root, 5).unwrap();
    assert!(!world.calls().contains("<--method>"), "{}", world.calls());
}

#[test]
fn a_failure_without_conflicting_files_comments_and_fails_the_run() {
    let world = World::new();
    let head = world.head();
    world.publish_pull(5, &head);
    world.move_dev("unrelated.txt", "dev moved\n");
    let mut pr = node(5, true, false);
    pr["headRefOid"] = head.clone().into();
    listing(&world, &[pr]);
    world.answer(&format!("compare-{head}.json"), "{\"behind_by\":2}");
    world.answer("rebase-fails-PR_5", "502 bad gateway");
    let error = update(&world.root, 5).unwrap_err().to_string();
    assert!(error.contains("#5") && error.contains("502"), "{error}");
    let calls = world.calls();
    assert!(calls.contains("<POST>"), "{calls}");
    assert!(calls.contains("502 bad gateway"), "{calls}");
    assert!(calls.contains("no conflicting files"), "{calls}");
    // An earlier transient failure must not suppress a later conflict diagnosis.
    world.answer(
        "comments-5.json",
        &serde_json::json!([comment(
            1,
            "NONE",
            &format!("<!-- aoe-harness:update-failure {head} -->\nprior API failure")
        )])
        .to_string(),
    );
    world.move_dev("README.md", "conflicting dev change\n");
    world.clear();
    update(&world.root, 5).unwrap();
    let calls = world.calls();
    assert!(
        calls.contains("<POST>") && calls.contains("README.md"),
        "{calls}"
    );
}

#[test]
fn a_missing_token_is_not_silently_skipped() {
    let mut world = World::new();
    world.set("GITHUB_ACTIONS", "true");
    world.set("GH_TOKEN", "");
    world.set("AOE_AGENT_ROLE", "main");
    listing(&world, &[]);
    execute(&world.root).unwrap();
    assert!(world.calls().contains("ListWaiting"));
}

#[test]
fn replay_names_the_first_conflict_even_when_the_final_tree_merge_is_clean() {
    use crate::ship::{git, tests::run};
    let world = World::new();
    run(&world.root, &["checkout", "-q", "--detach", "origin/dev"]);
    fs::write(world.root.join("f.txt"), "a\n").unwrap();
    run(&world.root, &["add", "f.txt"]);
    run(&world.root, &["commit", "-q", "-m", "base f"]);
    run(&world.root, &["push", "-q", "origin", "HEAD:dev"]);
    run(&world.root, &["checkout", "-q", "-B", "feature"]);
    fs::write(world.root.join("f.txt"), "c\n").unwrap();
    fs::write(world.root.join("g.txt"), "feature\n").unwrap();
    run(&world.root, &["add", "-A"]);
    run(
        &world.root,
        &["commit", "-q", "-m", "first conflicting commit"],
    );
    fs::write(world.root.join("f.txt"), "a\n").unwrap();
    run(&world.root, &["commit", "-q", "-am", "restore f"]);
    let head = world.head();
    world.publish_pull(9, &head);
    let dev = world.move_dev("f.txt", "b\n");
    run(&world.root, &["fetch", "-q", "origin", "dev"]);
    assert!(git::git(&world.root, &["merge-tree", "--write-tree", &dev, &head]).is_ok());
    let mut pr = node(9, true, false);
    pr["headRefOid"] = head.clone().into();
    listing(&world, &[pr]);
    world.answer(&format!("compare-{head}.json"), "{\"behind_by\":1}");
    world.answer("rebase-fails-PR_9", "rebase conflict");
    let before = git::git(&world.root, &["worktree", "list", "--porcelain"]).unwrap();
    let output = world.cli("update-prs");
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("conflicts with dev in `f.txt`"));
    let calls = world.calls();
    assert!(calls.contains("1 files conflict: `f.txt`"), "{calls}");
    assert_eq!(calls.matches("<POST>").count(), 1, "{calls}");
    assert_eq!(world.head(), head, "the real branch stays untouched");
    assert_eq!(
        git::git(&world.root, &["worktree", "list", "--porcelain"]).unwrap(),
        before
    );
}

#[test]
fn a_non_conflict_local_replay_failure_is_explained_in_the_comment() {
    use std::os::unix::fs::PermissionsExt;
    let world = World::new();
    let head = world.head();
    world.publish_pull(9, &head);
    world.move_dev("unrelated.txt", "dev moved\n");
    let mut pr = node(9, true, false);
    pr["headRefOid"] = head.clone().into();
    listing(&world, &[pr]);
    world.answer(&format!("compare-{head}.json"), "{\"behind_by\":1}");
    world.answer("rebase-fails-PR_9", "API rebase refused");
    let hook = world.root.join(".git/hooks/pre-rebase");
    fs::write(
        &hook,
        "#!/bin/sh\necho 'replay blocked by fixture hook' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o755)).unwrap();
    let before = crate::ship::git::git(&world.root, &["worktree", "list", "--porcelain"]).unwrap();
    let output = world.cli("update-prs");
    assert!(!output.status.success(), "{output:?}");
    let calls = world.calls();
    assert!(calls.contains("<POST>"), "{calls}");
    assert!(calls.contains("API rebase refused"), "{calls}");
    assert!(
        calls.contains("local commit replay failed")
            && calls.contains("replay blocked by fixture hook"),
        "{calls}"
    );
    assert_eq!(world.head(), head);
    assert_eq!(
        crate::ship::git::git(&world.root, &["worktree", "list", "--porcelain"]).unwrap(),
        before
    );
}

#[test]
fn the_conflict_replay_needs_no_configured_committer_identity() {
    // A fresh CI runner has no user.name/user.email: the replay must still
    // create the first commit and stop at the one that conflicts.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("repo");
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["-c", "user.name=setup", "-c", "user.email=setup@localhost"])
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    std::fs::create_dir_all(&root).unwrap();
    git(&["init", "-q", "-b", "dev"]);
    std::fs::write(root.join("f.txt"), "a\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "base"]);
    let fork = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-qb", "feature"]);
    std::fs::write(root.join("g.txt"), "new\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-qm", "adds g"]);
    std::fs::write(root.join("f.txt"), "c\n").unwrap();
    git(&["commit", "-qam", "changes f"]);
    git(&["checkout", "-q", "dev"]);
    std::fs::write(root.join("f.txt"), "b\n").unwrap();
    git(&["commit", "-qam", "dev changes f"]);
    let base = git(&["rev-parse", "HEAD"]);
    git(&["checkout", "-q", "feature"]);
    let mut replay = super::super::replay::replay_command(&root, &base, &fork);
    let replay = replay
        .env("HOME", &home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_AUTHOR_NAME")
        .env_remove("GIT_AUTHOR_EMAIL")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&replay.stderr);
    assert!(!replay.status.success(), "the second commit conflicts");
    assert!(!stderr.contains("identity unknown"), "{stderr}");
    let unmerged = git(&["diff", "--name-only", "--diff-filter=U"]);
    assert_eq!(
        unmerged, "f.txt",
        "the replay reached the conflicting commit"
    );
}
