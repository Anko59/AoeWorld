use super::stub::{World, comment};
use crate::ship::postpush::update::{execute, update};
use std::fs;

fn node(number: u64, auto: bool, cross: bool) -> serde_json::Value {
    serde_json::json!({
        "id": format!("PR_{number}"),
        "number": number,
        "headRefOid": format!("head{number}"),
        "isCrossRepository": cross,
        "autoMergeRequest": if auto { serde_json::json!({"enabledAt": "t"}) } else { serde_json::Value::Null },
    })
}

fn listing(world: &World, nodes: &[serde_json::Value]) {
    let data = serde_json::json!({"data": {"repository": {"pullRequests": {"nodes": nodes}}}});
    world.answer("list.json", &data.to_string());
}

fn behind(world: &World, number: u64, by: u64) {
    world.answer(
        &format!("compare-head{number}.json"),
        &format!("{{\"behind_by\": {by}}}"),
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
        calls.contains("<oid=head5>") && calls.contains("updateMethod: REBASE"),
        "{calls}"
    );
    assert!(!calls.contains("<POST>"), "{calls}");
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
        calls.contains("README.md") && calls.contains("could not be rebased"),
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
fn a_failure_without_conflicting_files_posts_no_comment_and_fails_the_run() {
    let world = World::new();
    listing(&world, &[node(5, true, false)]);
    behind(&world, 5, 2);
    world.answer("rebase-fails-PR_5", "502 bad gateway");
    let error = update(&world.root, 5).unwrap_err().to_string();
    assert!(error.contains("#5") && error.contains("502"), "{error}");
    assert!(!world.calls().contains("<--method>"));
}

#[test]
fn without_the_personal_token_the_workflow_does_nothing() {
    let mut world = World::new();
    world.set("GITHUB_ACTIONS", "true");
    world.set("GH_TOKEN", "");
    world.set("AOE_AGENT_ROLE", "main");
    execute(&world.root).unwrap();
    assert_eq!(
        fs::read_to_string(world.temp.path().join("gh.log")).unwrap(),
        ""
    );
}
