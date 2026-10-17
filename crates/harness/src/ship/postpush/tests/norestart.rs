//! A waiting pull request whose head has CI running is not updated: the update
//! would cancel that run and start it from zero (docs/post-push.md).
use super::{
    stub::World,
    update::{behind, listing, node, rebased},
};
use crate::ship::postpush::update::update;

fn run(world: &World, number: u64, status: &str, conclusion: serde_json::Value) {
    world.answer(
        &format!("runs-head{number}.json"),
        &serde_json::json!({"workflow_runs": [{"status": status, "conclusion": conclusion}]})
            .to_string(),
    );
}

fn one_behind_pull_request(number: u64) -> World {
    let world = World::new();
    listing(&world, &[node(number, true, false)]);
    behind(&world, number, 2);
    world
}

#[test]
fn a_run_in_progress_or_queued_on_the_head_skips_the_update() {
    for state in ["in_progress", "queued"] {
        let world = one_behind_pull_request(5);
        run(&world, 5, state, serde_json::Value::Null);
        update(&world.root, 5).unwrap();
        assert!(rebased(&world).is_empty(), "{state}: {}", world.calls());
        assert!(!world.calls().contains("/dispatches"), "{state}");
    }
}

#[test]
fn a_pending_required_status_skips_the_update() {
    let world = one_behind_pull_request(5);
    world.answer(
        "status-head5.json",
        r#"{"statuses":[{"context":"required","state":"pending"}]}"#,
    );
    update(&world.root, 5).unwrap();
    assert!(rebased(&world).is_empty(), "{}", world.calls());
}

#[test]
fn finished_runs_and_runless_heads_are_updated() {
    for (label, outcome) in [
        ("success", Some(("completed", serde_json::json!("success")))),
        ("failure", Some(("completed", serde_json::json!("failure")))),
        ("no run", None),
    ] {
        let world = one_behind_pull_request(5);
        if let Some((status, conclusion)) = outcome {
            run(&world, 5, status, conclusion);
            world.answer(
                "status-head5.json",
                r#"{"statuses":[{"context":"required","state":"success"}]}"#,
            );
        }
        update(&world.root, 5).unwrap();
        assert_eq!(rebased(&world), ["PR_5"], "{label}: {}", world.calls());
    }
}

#[test]
fn a_skipped_pull_request_does_not_use_the_bound_and_order_is_kept() {
    let world = World::new();
    listing(
        &world,
        &[
            node(7, true, false),
            node(3, true, false),
            node(5, true, false),
        ],
    );
    for number in [3, 5, 7] {
        behind(&world, number, 1);
    }
    run(&world, 3, "in_progress", serde_json::Value::Null);
    update(&world.root, 1).unwrap();
    assert_eq!(rebased(&world), ["PR_5"]);
}

#[test]
fn the_command_prints_the_skip_line() {
    let world = one_behind_pull_request(5);
    run(&world, 5, "in_progress", serde_json::Value::Null);
    let output = world.cli("update-prs");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{text}");
    assert!(
        text.contains("update-prs: #5 skipped: CI in progress on head5"),
        "{text}"
    );
}
