use super::{
    stub::{World, comment},
    update::{listing, node},
};
use crate::ship::{
    git, ship_with,
    tests::{offline, run},
};
use serde_json::json;

#[test]
fn next_cli_prioritizes_only_current_head_latest_carry_refusals() {
    let world = World::new();
    let head = world.head();
    let refusal = |sha: &str, why: &str| {
        format!(
            "`harness/review` was not carried to {}: {why}. Ship again for a review of the change.",
            &sha[..12]
        )
    };
    world.answer("pulls.json", &json!([
        {"number":5,"title":"Needs review","html_url":"https://github.com/project/checkout/pull/5","head":{"sha":head},"auto_merge":{}},
        {"number":6,"title":"Old refusal","head":{"sha":head},"auto_merge":{}},
        {"number":7,"title":"Disarmed","head":{"sha":head},"auto_merge":null},
        {"number":8,"title":"Recovered","head":{"sha":head},"auto_merge":{}},
        {"number":9,"title":"No marker","head":{"sha":head},"auto_merge":{}},
        {"number":10,"title":"Patch edited","head":{"sha":head},"auto_merge":{}}
    ]).to_string());
    world.answer(
        "issues.json",
        &json!([{"number":1,"title":"Critical task","labels":[{"name":"priority:critical"}]}])
            .to_string(),
    );
    world.answer(
        "comments-5.json",
        &json!([
            comment(1, "NONE", &refusal(&head, "patch changed")),
            comment(2, "NONE", &refusal(&head, "policy changed")),
            comment(3, "NONE", "unrelated comment")
        ])
        .to_string(),
    );
    world.answer(
        "comments-6.json",
        &json!([comment(
            1,
            "NONE",
            &refusal(&"a".repeat(40), "no review marker")
        )])
        .to_string(),
    );
    world.answer(
        "comments-8.json",
        &json!([
            comment(1, "NONE", &refusal(&head, "policy changed")),
            comment(
                2,
                "NONE",
                &format!("`harness/review` was carried to {}: unchanged", &head[..12])
            )
        ])
        .to_string(),
    );
    for (number, why) in [(9, "no review marker"), (10, "patch changed")] {
        world.answer(
            &format!("comments-{number}.json"),
            &json!([comment(1, "NONE", &refusal(&head, why))]).to_string(),
        );
    }
    let output = world.cli("next");
    assert!(output.status.success(), "{output:?}");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(
        out.starts_with("#5 Needs review — reship: policy changed"),
        "{out}"
    );
    assert!(out.contains("#1 Critical task"), "{out}");
    assert!(
        out.contains("#9 No marker — reship: no review marker"),
        "{out}"
    );
    assert!(
        out.contains("#10 Patch edited — reship: patch changed"),
        "{out}"
    );
    assert!(out.find("#10").unwrap() < out.find("#1 Critical").unwrap());
    assert!(
        world
            .calls()
            .contains("pulls?state=open&base=dev&per_page=100")
    );
    for absent in ["#6", "#7", "#8"] {
        assert!(!out.contains(absent), "{out}");
    }
    assert!(!world.calls().contains("<POST>"));
}

#[test]
fn updater_cli_uses_merge_wording() {
    let world = World::new();
    let mut pr = node(5, true, false);
    pr["headRefOid"] = "old".into();
    listing(&world, &[pr]);
    world.answer("compare-old.json", "{\"behind_by\":1}");
    let head = world.head();
    world.answer("head-5.json", &super::dispatch::head_json(&head));
    world.publish_pull(5, &head);
    world.answer("pull-5.json", &super::stub::pull_json("open", "dev", &head));
    let output = world.cli("update-prs");
    assert!(output.status.success(), "{output:?}");
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("updated (dev merged in)"), "{out}");
    assert!(!out.contains("rebased"), "{out}");
}

pub(super) fn remote_commit(world: &World, parents: &[&str], name: &str, email: &str) -> String {
    let tree = git::git(&world.root, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(&world.root)
        .args(["commit-tree", &tree, "-m", "remote update"])
        .env("GIT_AUTHOR_NAME", "fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
        .env("GIT_COMMITTER_NAME", name)
        .env("GIT_COMMITTER_EMAIL", email);
    for parent in parents {
        cmd.args(["-p", parent]);
    }
    let output = cmd.output().unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn lease_refresh_accepts_only_github_dev_merge_chains_and_still_detects_later_pushes() {
    let world = World::new();
    let start = world.head();
    run(&world.root, &["push", "-q", "origin", "feature"]);
    let dev = world.move_dev("other.txt", "new dev");
    run(&world.root, &["fetch", "-q", "origin", "dev"]);
    let one = remote_commit(&world, &[&start, &dev], "GitHub", "noreply@github.com");
    let dev2 = world.move_dev("another.txt", "more dev");
    run(&world.root, &["fetch", "-q", "origin", "dev"]);
    let two = remote_commit(&world, &[&one, &dev2], "GitHub", "noreply@github.com");
    for tip in [&one, &two] {
        run(
            &world.root,
            &["push", "-q", "origin", &format!("{tip}:feature")],
        );
        assert_eq!(
            git::lease::refresh(&world.root, "feature", &start, &start, &start).unwrap(),
            *tip
        );
    }
    // A later push must still fail the exact refreshed lease.
    let evidence = crate::ship::judge(&world.root, &offline()).unwrap();
    let stale = remote_commit(&world, &[&two], "Human", "human@example.com");
    run(
        &world.root,
        &["push", "-q", "origin", &format!("{stale}:feature")],
    );
    let error = git::publish::push(&world.root, &evidence, Some(&two))
        .unwrap_err()
        .to_string();
    assert!(error.contains("stale info"), "{error}");
    assert!(
        git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"])
            .unwrap()
            .starts_with(&stale)
    );
    let other = remote_commit(&world, &[&start], "GitHub", "noreply@github.com");
    let cases = [
        other.clone(),
        remote_commit(
            &world,
            &[&start, &dev, &dev2],
            "GitHub",
            "noreply@github.com",
        ),
        remote_commit(&world, &[&start, &dev], "GitHub", "wrong@example.com"),
        remote_commit(&world, &[&start, &dev], "Human", "human@example.com"),
        remote_commit(&world, &[&start, &other], "GitHub", "noreply@github.com"),
        remote_commit(&world, &[&other, &dev], "GitHub", "noreply@github.com"),
        remote_commit(&world, &[&dev, &start], "GitHub", "noreply@github.com"),
    ];
    for tip in cases {
        run(
            &world.root,
            &["push", "-q", "-f", "origin", &format!("{tip}:feature")],
        );
        for expected in [&start[..], ""] {
            assert!(
                git::lease::refresh(&world.root, "feature", expected, &start, &start).is_err(),
                "{tip}"
            );
        }
    }
    // Exercise the full ship path: the remote changes after a passing review.
    run(
        &world.root,
        &["push", "-q", "-f", "origin", &format!("{start}:feature")],
    );
    run(&world.root, &["branch", "-dr", "origin/feature"]);
    assert!(
        git::lease::tracking(&world.root, "feature").is_none(),
        "force also pins a lease without a tracking ref"
    );
    let reviewed = std::cell::Cell::new(false);
    let shipped = ship_with(
        &world.root,
        &crate::ship::Options {
            force: true,
            no_review: false,
            ..offline()
        },
        &|root, tier, _, _, _| {
            let report = crate::ship::review_gate::graded(root, tier, 9);
            run(root, &["push", "-q", "origin", &format!("{two}:feature")]);
            reviewed.set(true);
            Ok(report)
        },
    )
    .unwrap();
    assert!(
        reviewed.get(),
        "the ship passed a scripted 9/10 review before the updater race"
    );
    let remote = git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"]).unwrap();
    assert_eq!(
        remote.split_whitespace().next(),
        Some(shipped.head.as_str())
    );
    use std::io::Write;
    writeln!(std::io::stdout(), "Ship race rehearsal (local Git remote, stubbed API/reviewer):\nremote held two GitHub dev merges; ship with force passed a 9/10 review and replaced them\n$ git ls-remote origin refs/heads/feature\n{remote}\nlate ordinary push rejected: {error}").unwrap();
}
