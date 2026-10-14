use super::stub::{World, comment, pull_json};
use crate::{
    review::Tier,
    ship::{
        postpush::{
            carry::{Decision, carry, decide},
            marker::Marker,
        },
        review_gate::graded,
    },
};

/// The marker of a passing review of the feature branch's head.
fn approved(world: &World) -> Marker {
    Marker::of(&world.root, &graded(&world.root, Tier::Low, 9)).unwrap()
}

fn base(world: &World) -> String {
    crate::ship::git::git(&world.root, &["rev-parse", "refs/remotes/origin/dev"]).unwrap()
}

#[test]
fn an_identical_patch_over_a_moved_dev_is_carried() {
    let world = World::new();
    let marker = approved(&world);
    world.move_dev("elsewhere.txt", "dev moved\n");
    let head = world.rebase_onto_dev();
    assert_ne!(head, marker.head);
    run_fetch(&world);
    let decision = decide(&world.root, Some(&marker), &head, &base(&world)).unwrap();
    println!("identical patch, moved dev: {decision:?}");
    assert_eq!(decision, Decision::Carry { from: marker.head });
}

fn run_fetch(world: &World) {
    crate::ship::tests::run(&world.root, &["fetch", "-q", "origin"]);
}

#[test]
fn one_changed_line_is_not_carried() {
    let world = World::new();
    let marker = approved(&world);
    world.move_dev("elsewhere.txt", "dev moved\n");
    std::fs::write(world.root.join("README.md"), "z\n").unwrap();
    crate::ship::tests::run(&world.root, &["commit", "-q", "-a", "--amend", "--no-edit"]);
    let head = world.rebase_onto_dev();
    run_fetch(&world);
    let decision = decide(&world.root, Some(&marker), &head, &base(&world)).unwrap();
    println!("one line changed: {decision:?}");
    let Decision::Refuse(why) = decision else {
        panic!("carried a changed patch")
    };
    assert!(why.starts_with("patch changed"), "{why}");
}

#[test]
fn a_changed_review_policy_is_not_carried() {
    let world = World::new();
    let marker = approved(&world);
    world.move_dev("gates/review/grader.md", "a stricter grader\n");
    let head = world.rebase_onto_dev();
    run_fetch(&world);
    let Decision::Refuse(why) = decide(&world.root, Some(&marker), &head, &base(&world)).unwrap()
    else {
        panic!("carried across a policy change")
    };
    assert!(why.starts_with("policy changed"), "{why}");
}

#[test]
fn a_missing_marker_is_not_carried() {
    let world = World::new();
    let head = world.head();
    let Decision::Refuse(why) = decide(&world.root, None, &head, &base(&world)).unwrap() else {
        panic!("carried without a marker")
    };
    assert!(why.contains("no review marker"), "{why}");
}

/// Everything `carry` needs from GitHub for pull request 7 at `head`.
fn pull_seven(world: &World, head: &str, comments: &[serde_json::Value]) {
    world.publish_pull(7, head);
    world.answer("pull-7.json", &pull_json("open", "dev", head));
    world.answer(
        "comments-7.json",
        &serde_json::Value::from(comments).to_string(),
    );
}

#[test]
fn the_carry_command_posts_success_on_the_new_head_only_for_an_unchanged_patch() {
    let world = World::new();
    let marker = approved(&world);
    let note = comment(1, "OWNER", &marker.render());
    world.move_dev("elsewhere.txt", "dev moved\n");
    let head = world.rebase_onto_dev();
    pull_seven(&world, &head, std::slice::from_ref(&note));
    carry(&world.root, 7).unwrap();
    let calls = world.calls();
    assert!(
        calls.contains(&format!("<repos/project/checkout/statuses/{head}>")),
        "{calls}"
    );
    assert!(calls.contains("<state=success>") && calls.contains("<context=harness/review>"));
    assert!(
        calls.contains(&format!(
            "<description=carried from {}: patch and policy unchanged>",
            &marker.head[..12]
        )),
        "{calls}"
    );
    // The same marker, a changed patch: no status, one comment saying why.
    world.clear();
    std::fs::write(world.root.join("README.md"), "z\n").unwrap();
    crate::ship::tests::run(&world.root, &["commit", "-q", "-a", "-m", "one more line"]);
    let changed = world.head();
    pull_seven(&world, &changed, &[note]);
    carry(&world.root, 7).unwrap();
    let calls = world.calls();
    assert!(!calls.contains("state=success"), "{calls}");
    assert!(
        calls.contains("issues/7/comments") && calls.contains("patch changed"),
        "{calls}"
    );
}

#[test]
fn a_marker_from_a_stranger_is_not_believed() {
    let world = World::new();
    let marker = approved(&world);
    let head = world.head();
    pull_seven(&world, &head, &[comment(1, "NONE", &marker.render())]);
    carry(&world.root, 7).unwrap();
    let calls = world.calls();
    assert!(!calls.contains("state=success"), "{calls}");
    assert!(calls.contains("no review marker"), "{calls}");
}

#[test]
fn pull_requests_that_are_closed_or_not_into_dev_are_left_alone() {
    let world = World::new();
    let head = world.head();
    world.answer("pull-7.json", &pull_json("open", "release/1", &head));
    carry(&world.root, 7).unwrap();
    world.answer("pull-7.json", &pull_json("closed", "dev", &head));
    carry(&world.root, 7).unwrap();
    let calls = world.calls();
    assert!(!calls.contains("<--method>"), "{calls}");
}
