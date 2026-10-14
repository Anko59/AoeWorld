use super::*;

#[path = "command.rs"]
mod command;

fn snapshot(body: &str, updated: &str) -> String {
    serde_json::json!({"body": body, "updatedAt": updated}).to_string()
}

#[test]
fn another_prs_checked_section_and_every_surrounding_byte_survive() {
    let own = plan(43, OTHER_PR, &minor("new.rs", 8)).section.unwrap();
    let prefix = "Owner's intro  \n\n## PR #42 — other\n\n- [x] already fixed  \n\n";
    let suffix = format!(
        "\n\n## PR #44 — later\n- [X] done\n\n{}  \n",
        marker_line(STANDING_TITLE)
    );
    let current = format!("{prefix}## PR #43 — old\n- [ ] superseded{suffix}");
    assert_eq!(
        standing_body(&current, &own),
        format!("{prefix}{}{suffix}", own.render())
    );
    let removed = Section {
        items: vec![],
        ..own
    };
    assert_eq!(
        standing_body(&current, &removed),
        format!("{prefix}{suffix}")
    );
    let without = format!("{prefix}{suffix}");
    assert_eq!(standing_body(&without, &removed), without);
    assert_eq!(standing_body("", &removed), "");
    let added = plan(43, OTHER_PR, &minor("new.rs", 8)).section.unwrap();
    let inserted = standing_body(&without, &added);
    assert_eq!(
        inserted.replacen(&format!("\n{}\n\n", added.render()), "", 1),
        without
    );
    assert!(inserted.starts_with(prefix));
    assert!(inserted.contains("- [X] done\n\n"));
    assert!(inserted.ends_with(&format!("{}  \n", marker_line(STANDING_TITLE))));
}

#[test]
fn a_changed_timestamp_retries_from_the_fresh_body_and_preserves_the_tick() {
    let section = plan(42, PR, &minor("other.rs", 1)).section.unwrap();
    let (listing, stale) = listed_with_standing(&[], &section);
    let fresh = stale
        .replace("- [ ]", "- [x]")
        .replace(INTRO, "Owner's edited intro  ");
    let mut stub = Stub::new(&listing);
    stub.view_responses(&[
        snapshot(&stale, "first"),
        "second".into(),
        snapshot(&fresh, "second"),
        "second".into(),
    ]);
    let problems = file(&stub.root, OTHER_PR, &minor("new.rs", 8));
    assert!(problems.is_empty(), "{problems:?}");
    let calls = stub.calls();
    assert_eq!(calls.matches("<issue>\n<view>").count(), 4, "{calls}");
    assert_eq!(calls.matches("<issue>\n<edit>").count(), 1, "{calls}");
    assert!(calls.contains("<pr>\n<comment>\n<43>"), "{calls}");
    let bodies = stub.bodies();
    let edited = bodies.split("\n===\n").next().unwrap();
    let own = plan(43, OTHER_PR, &minor("new.rs", 8)).section.unwrap();
    assert_eq!(edited, standing_body(&fresh, &own));
    assert!(edited.contains("- [x] other.rs:1"), "{edited}");
    assert!(!edited.contains("- [ ] other.rs:1"), "{edited}");
    println!(
        "fresh-read: 4 views, 1 edit; PR #42 checkbox and owner intro preserved; PR #43 comment posted"
    );
}

#[test]
fn three_timestamp_changes_leave_the_issue_untouched_and_still_post_the_comment() {
    let section = plan(42, PR, &minor("other.rs", 1)).section.unwrap();
    let (listing, body) = listed_with_standing(&[], &section);
    let mut stub = Stub::new(&listing);
    let responses: Vec<_> = (0..3)
        .flat_map(|n| [snapshot(&body, &n.to_string()), (n + 1).to_string()])
        .collect();
    stub.view_responses(&responses);
    let problems = file(&stub.root, OTHER_PR, &minor("new.rs", 8));
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("standing issue was busy"),
        "{problems:?}"
    );
    let calls = stub.calls();
    assert_eq!(calls.matches("<issue>\n<view>").count(), 6, "{calls}");
    assert!(!calls.contains("<edit>"), "{calls}");
    assert!(calls.contains("<pr>\n<comment>\n<43>"), "{calls}");
    let comment = stub.bodies();
    assert!(comment.contains("- [ ] new.rs:8"), "{comment}");
    assert!(!comment.contains("also recorded"), "{comment}");
    println!(
        "busy: 6 views, 0 edits; PR #43 comment posted; {}",
        problems[0]
    );
}
