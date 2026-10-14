use super::{
    stub::{World, comment, pull_json},
    update::{listing, node},
};
use crate::ship::postpush::update::{HEAD_TIMEOUT, POLL_INTERVAL, update_within};
use crate::{
    review::Tier,
    ship::{postpush::marker::Marker, review_gate::graded},
};
use std::time::{Duration, Instant};

pub(super) fn head_json(head: &str) -> String {
    serde_json::json!({"data": {"repository": {"pullRequest": {"headRefOid": head}}}}).to_string()
}

fn landed(world: &World) -> (String, String, String) {
    let marker = Marker::of(&world.root, &graded(&world.root, Tier::Low, 9)).unwrap();
    let old = world.head();
    let base = world.move_dev("elsewhere.txt", "dev moved\n");
    let head = world.rebase_onto_dev();
    assert_ne!(old, head);
    let mut pr = node(5, true, false);
    pr["headRefOid"] = old.clone().into();
    listing(world, &[pr]);
    world.answer(&format!("compare-{old}.json"), "{\"behind_by\":1}");
    world.answer(
        &format!("compare-{head}.json"),
        &serde_json::json!({"merge_base_commit": {"sha": base}}).to_string(),
    );
    world.answer("head-5.json", &head_json(&head));
    world.publish_pull(5, &head);
    world.answer("pull-5.json", &pull_json("open", "dev", &head));
    world.answer(
        "comments-5.json",
        &serde_json::json!([comment(1, "OWNER", &marker.render())]).to_string(),
    );
    (old, head, base)
}

fn dispatch_calls(calls: &str) -> Vec<&str> {
    calls
        .split("---\n")
        .filter(|call| call.contains("/dispatches>"))
        .collect()
}

#[test]
fn the_updater_cli_carries_then_dispatches_the_branch_with_its_rebased_base() {
    let mut world = World::new();
    world.set("GITHUB_ACTIONS", "true");
    world.set("GH_TOKEN", "workflow-token");
    let (old, head, base) = landed(&world);
    // GitHub first returns the old head, then the accepted rebase lands.
    world.answer("poll-old-5.json", &head_json(&old));
    let output = world.cli("update-prs");
    assert!(output.status.success(), "{output:?}");
    let calls = world.calls();
    let carry = calls
        .find(&format!("<repos/project/checkout/statuses/{head}>"))
        .unwrap();
    let dispatch = calls
        .find("<repos/project/checkout/actions/workflows/ci.yml/dispatches>")
        .unwrap();
    assert!(carry < dispatch, "{calls}");
    assert!(
        calls.contains("<context=harness/review>") && calls.contains("<state=success>"),
        "{calls}"
    );
    let dispatched = dispatch_calls(&calls);
    assert_eq!(dispatched.len(), 1, "{calls}");
    assert!(dispatched[0].contains("<POST>"), "{calls}");
    assert!(dispatched[0].contains("<ref=feature/5>"), "{calls}");
    assert!(
        dispatched[0].contains(&format!("<inputs[base]={base}>")),
        "{calls}"
    );
    assert_eq!(calls.matches("UpdatedHead").count(), 2, "{calls}");
}

#[test]
fn a_head_that_never_moves_reports_failure_and_does_not_dispatch() {
    let world = World::new();
    let (old, _, _) = landed(&world);
    world.answer("head-5.json", &head_json(&old));
    // The CLI waits two minutes; the same wait is exercised here with a one-second bound.
    assert_eq!(HEAD_TIMEOUT, Duration::from_secs(120));
    assert_eq!(POLL_INTERVAL, Duration::from_secs(5));
    let started = Instant::now();
    let error = update_within(
        &world.root,
        5,
        Duration::from_secs(1),
        Duration::from_millis(100),
    )
    .unwrap_err()
    .to_string();
    assert!(started.elapsed() >= Duration::from_secs(1), "{error}");
    assert!(
        error.contains("#5") && error.contains("head did not move"),
        "{error}"
    );
    let calls = world.calls();
    assert!(
        calls.contains("<!-- aoe-harness:update-failure") && calls.contains("head did not move"),
        "{calls}"
    );
    assert!(dispatch_calls(&calls).is_empty(), "{calls}");
    assert!(!calls.contains("/statuses/"), "{calls}");
    assert!(error.contains("within 1 seconds"), "{error}");
    assert!(calls.matches("UpdatedHead").count() >= 2, "{calls}");
}

#[test]
fn dispatch_failure_is_commented_and_fails_the_updater_cli() {
    let world = World::new();
    landed(&world);
    world.answer("dispatch-fails", "");
    let output = world.cli("update-prs");
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("dispatch denied"),
        "{output:?}"
    );
    let calls = world.calls();
    assert_eq!(dispatch_calls(&calls).len(), 1, "{calls}");
    assert!(
        calls.contains("<!-- aoe-harness:update-failure") && calls.contains("dispatch denied"),
        "{calls}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("update-prs: #5 update failed"),
        "{output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("failure comment could not be posted"),
        "{output:?}"
    );
}

#[test]
fn carry_failure_is_commented_and_prevents_dispatch() {
    let world = World::new();
    landed(&world);
    world.answer("carry-fails", "");
    let output = world.cli("update-prs");
    assert!(!output.status.success(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("status denied"),
        "{output:?}"
    );
    let calls = world.calls();
    assert!(
        calls.contains("<!-- aoe-harness:update-failure") && calls.contains("status denied"),
        "{calls}"
    );
    assert!(dispatch_calls(&calls).is_empty(), "{calls}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("update-prs: #5 update failed"),
        "{output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("failure comment could not be posted"),
        "{output:?}"
    );
}
