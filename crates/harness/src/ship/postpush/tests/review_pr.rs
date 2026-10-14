use super::stub::{World, comment, pull_json};
use crate::ship::{postpush::marker::Marker, tests::run};
use std::{fs, io::Write};

#[test]
fn review_pr_cli_writes_a_marker_then_carry_cli_approves_an_unchanged_rebase() {
    let mut world = World::new();
    run(
        &world.root,
        &["checkout", "-q", "-B", "feature", "origin/dev"],
    );
    fs::write(
        world.root.join("package.json"),
        "{\"dependencies\":{\"example\":\"1.1.0\"}}\n",
    )
    .unwrap();
    run(&world.root, &["add", "package.json"]);
    run(&world.root, &["commit", "-q", "-m", "dependency update"]);
    run(&world.root, &["push", "-q", "origin", "feature"]);
    let approved = world.head();
    world.answer(
        "review-meta.json",
        &serde_json::json!({
            "headRefOid": approved, "headRefName": "feature", "baseRefName": "dev",
            "author": {"login": "dependabot[bot]"}, "isCrossRepository": false
        })
        .to_string(),
    );
    // Script only the external model; run the actual review command and publisher.
    world.script(
        "claude",
        r#"#!/bin/sh
case "$*" in
  *'"grade":'*) answer='{"grade":9,"summary":"scripted clean review"}' ;;
  *) answer='{"findings":[],"verdicts":[]}' ;;
esac
printf 'AOE-REVIEW-BEGIN\n%s\nAOE-REVIEW-END\n' "$answer"
"#,
    );
    world.set("REVIEW_PR", "7");
    world.set("REVIEW_RUNTIME", "claude");
    let reviewed = world.cli("review-pr");
    assert!(reviewed.status.success(), "{reviewed:?}");
    let calls = world.calls();
    assert!(calls.contains("context=harness/review"), "{calls}");
    let body = calls
        .split("<body=")
        .find(|s| s.starts_with("<!-- aoe-harness:review-marker -->"))
        .expect("review-pr must publish the carry marker");
    let marker = Marker::parse(body).unwrap();
    assert_eq!(marker.head, approved);
    assert_eq!(marker.grade, 9);
    assert!(
        calls.find("<!-- aoe-harness:review-marker -->").unwrap()
            < calls.find("<state=success>").unwrap(),
        "the marker must precede the approval status: {calls}"
    );
    writeln!(
        std::io::stdout(),
        "review-pr wrote marker:\n{}",
        marker.render()
    )
    .unwrap();
    world.move_dev("elsewhere.txt", "unrelated dev change\n");
    let head = world.rebase_onto_dev();
    assert_ne!(head, approved);
    world.publish_pull(7, &head);
    world.answer("pull-7.json", &pull_json("open", "dev", &head));
    world.answer(
        "comments-7.json",
        &serde_json::json!([comment(1, "OWNER", &marker.render())]).to_string(),
    );
    world.clear();
    world.set("CARRY_PR", "7");
    let carried = world.cli("carry-review");
    assert!(carried.status.success(), "{carried:?}");
    let calls = world.calls();
    assert!(
        calls.contains(&format!("<repos/project/checkout/statuses/{head}>")),
        "{calls}"
    );
    assert!(calls.contains("<state=success>"), "{calls}");
    assert!(calls.contains("patch and policy unchanged"), "{calls}");
}
