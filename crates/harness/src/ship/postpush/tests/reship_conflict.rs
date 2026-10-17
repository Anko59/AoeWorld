use super::stub::{World, comment};
use crate::ship::postpush::update::{conflict_message, conflict_tag};
use serde_json::json;

fn pull(number: u64, title: &str, head: &str) -> serde_json::Value {
    json!({
        "number": number,
        "title": title,
        "html_url": format!("https://github.com/project/checkout/pull/{number}"),
        "head": {"sha": head},
        "auto_merge": {}
    })
}

#[test]
fn next_lists_a_conflict_on_the_current_head_first_and_only_that_head() {
    let world = World::new();
    let head = world.head();
    let stale = "a".repeat(40);
    let files = vec!["crates/a.rs".to_owned(), "web/b.ts".to_owned()];
    let conflict_on = |sha: &str| format!("{}\n{}", conflict_tag(sha), conflict_message(&files));
    world.answer(
        "pulls.json",
        &json!([
            pull(5, "Conflicted", &head),
            pull(6, "Stale conflict", &head),
            pull(7, "No comment", &head),
            pull(8, "Refused", &head)
        ])
        .to_string(),
    );
    world.answer("issues.json", "[]");
    world.answer(
        "comments-5.json",
        &json!([comment(1, "NONE", &conflict_on(&head))]).to_string(),
    );
    world.answer(
        "comments-6.json",
        &json!([comment(1, "NONE", &conflict_on(&stale))]).to_string(),
    );
    world.answer("comments-7.json", "[]");
    world.answer(
        "comments-8.json",
        &json!([comment(
            1,
            "NONE",
            &format!(
                "`harness/review` was not carried to {}: policy changed. Ship again for a review of the change.",
                &head[..12]
            )
        )])
        .to_string(),
    );
    let output = world.cli("next");
    assert!(output.status.success(), "{output:?}");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(
        out.starts_with(
            "#5 Conflicted — reship: conflicts with dev in `crates/a.rs`, `web/b.ts`\nhttps://github.com/project/checkout/pull/5\n"
        ),
        "{out}"
    );
    assert!(out.contains("#8 Refused — reship: policy changed"), "{out}");
    assert!(out.find("#5").unwrap() < out.find("#8").unwrap(), "{out}");
    for absent in ["#6", "#7"] {
        assert!(!out.contains(absent), "{out}");
    }
    assert!(!world.calls().contains("<POST>"));
}
