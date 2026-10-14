use super::stub::{World, comment};
use crate::{
    review::Tier,
    ship::{
        postpush::{
            marker::{Marker, latest, record},
            pull_number,
        },
        review_gate::graded,
    },
};

const URL: &str = "https://github.com/project/checkout/pull/7";

fn report(world: &World) -> crate::review::Report {
    graded(&world.root, Tier::Low, 9)
}

#[test]
fn the_marker_holds_head_patch_policy_tier_and_grade_and_round_trips() {
    let world = World::new();
    let report = report(&world);
    let marker = Marker::of(&world.root, &report).unwrap();
    assert_eq!(marker.head, report.head);
    assert_eq!(marker.tier, "low");
    assert_eq!(marker.grade, 9);
    assert_eq!(
        Some(marker.policy.as_str()),
        report.policy_fingerprint.as_deref()
    );
    assert_eq!(marker.patch.len(), 64);
    assert_eq!(Marker::parse(&marker.render()), Some(marker));
    assert_eq!(pull_number(URL).unwrap(), 7);
}

#[test]
fn the_marker_is_added_once_and_then_edited_in_place() {
    let world = World::new();
    let report = report(&world);
    record(&world.root, URL, &report).unwrap();
    let calls = world.calls();
    assert!(
        calls.contains("<POST>") && calls.contains("<repos/project/checkout/issues/7/comments>"),
        "{calls}"
    );
    assert!(
        calls.contains("<body=<!-- aoe-harness:review-marker -->"),
        "{calls}"
    );
    // A maintainer's marker exists: the next ship edits that comment.
    let old = Marker {
        grade: 8,
        ..Marker::of(&world.root, &report).unwrap()
    };
    world.answer(
        "comments-7.json",
        &serde_json::json!([[
            comment(41, "NONE", &old.render()),
            comment(42, "OWNER", &old.render())
        ]])
        .to_string(),
    );
    world.clear();
    record(&world.root, URL, &report).unwrap();
    let calls = world.calls();
    assert!(
        calls.contains("<PATCH>") && calls.contains("<repos/project/checkout/issues/comments/42>"),
        "{calls}"
    );
    assert!(!calls.contains("<POST>"), "{calls}");
}

#[test]
fn a_marker_write_that_fails_reaches_the_caller_which_only_warns() {
    let world = World::new();
    let report = report(&world);
    let old = Marker::of(&world.root, &report).unwrap();
    world.answer(
        "comments-7.json",
        &serde_json::json!([comment(42, "MEMBER", &old.render())]).to_string(),
    );
    world.answer("write-fails", "");
    // Both writes fail in the stub: the error reaches the caller, which only warns.
    assert!(record(&world.root, URL, &report).is_err());
}

#[test]
fn only_a_maintainers_newest_marker_counts() {
    let world = World::new();
    let first = Marker::of(&world.root, &report(&world)).unwrap();
    let second = Marker {
        grade: 10,
        ..first.clone()
    };
    let comments = |entries: Vec<serde_json::Value>| -> Vec<crate::ship::postpush::Comment> {
        entries
            .into_iter()
            .map(|e| serde_json::from_value(e).unwrap())
            .collect()
    };
    let found = latest(&comments(vec![
        comment(1, "OWNER", &first.render()),
        comment(2, "COLLABORATOR", &second.render()),
        comment(
            3,
            "NONE",
            &Marker {
                grade: 1,
                ..first.clone()
            }
            .render(),
        ),
    ]));
    assert_eq!(found, Some(second));
    assert_eq!(
        latest(&comments(vec![comment(1, "NONE", &first.render())])),
        None
    );
    assert_eq!(Marker::parse("hello"), None);
}
