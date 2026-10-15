use super::{robustness::remote_commit, stub::World};
use crate::ship::{
    Options, git, ship_with,
    tests::{offline, run},
};
use std::io::Write;

#[test]
fn refused_refresh_does_not_authorize_an_unchanged_forced_retry() {
    let world = World::new();
    let start = world.head();
    run(&world.root, &["push", "-q", "origin", "feature"]);
    let ordinary = remote_commit(&world, &[&start], "Human", "human@example.com");
    // Transfer the object separately: publishing from another writer must not update our ref.
    run(
        &world.root,
        &[
            "push",
            "-q",
            "origin",
            &format!("{ordinary}:refs/heads/object-transfer"),
        ],
    );
    run(
        &world.origin(),
        &["update-ref", "refs/heads/feature", &ordinary],
    );
    let forced = Options {
        force: true,
        ..offline()
    };
    for attempt in 1..=2 {
        let error = crate::ship::ship(&world.root, &forced)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("refusing push"),
            "attempt {attempt}: {error}"
        );
        assert_eq!(
            git::lease::tracking(&world.root, "feature").as_deref(),
            Some(start.as_str()),
            "refused refresh must not change the next attempt's lease"
        );
        assert!(
            git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"])
                .unwrap()
                .starts_with(&ordinary)
        );
        writeln!(
            std::io::stdout(),
            "forced retry {attempt}: {error}; ordinary remote commit protected"
        )
        .unwrap();
    }
}

#[test]
fn default_ship_handles_only_the_github_dev_merge_race_during_review() {
    for github_merge in [true, false] {
        let world = World::new();
        let start = world.head();
        run(&world.root, &["push", "-q", "origin", "feature"]);
        let dev = world.move_dev("dev-only", "dev ahead before ship");
        run(&world.root, &["fetch", "-q", "origin", "dev"]);
        let tip = if github_merge {
            remote_commit(&world, &[&start, &dev], "GitHub", "noreply@github.com")
        } else {
            remote_commit(&world, &[&start], "Human", "human@example.com")
        };
        run(
            &world.root,
            &[
                "push",
                "-q",
                "origin",
                &format!("{tip}:refs/heads/object-transfer"),
            ],
        );
        let options = Options {
            no_review: false,
            ..offline()
        };
        assert!(!options.force);
        let result = ship_with(&world.root, &options, &|root, tier, _, _, _| {
            let report = crate::ship::review_gate::graded(root, tier, 9);
            run(&world.origin(), &["update-ref", "refs/heads/feature", &tip]);
            Ok(report)
        });
        let remote = git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"]).unwrap();
        if github_merge {
            let shipped = result.unwrap();
            assert!(
                shipped.recheck.is_none(),
                "dev stayed fixed throughout ship"
            );
            assert!(remote.starts_with(&shipped.head));
            writeln!(
                std::io::stdout(),
                "default ship: GitHub dev merge replaced after review, without SHIP_FORCE; {remote}"
            )
            .unwrap();
        } else {
            assert!(result.is_err(), "ordinary writer must be protected");
            assert!(remote.starts_with(&tip));
            writeln!(
                std::io::stdout(),
                "default ship: ordinary remote commit refused and retained"
            )
            .unwrap();
        }
    }
}

#[test]
fn next_cli_pins_every_lookup_to_origin_despite_gh_host() {
    let mut world = World::new();
    world.set("GH_HOST", "enterprise.example");
    let head = world.head();
    world.answer("pulls.json", &serde_json::json!([{"number":5,"title":"Needs review","head":{"sha":head},"auto_merge":{}}]).to_string());
    world.answer("comments-5.json", "[]");
    world.answer(
        "issues.json",
        "[{\"number\":1,\"title\":\"Ready task\",\"labels\":[]}]",
    );
    let output = world.cli("next");
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("#1 Ready task"));
    let calls = world.calls();
    let requests: Vec<_> = calls
        .split("---\n")
        .filter(|c| c.starts_with("<api>\n"))
        .collect();
    assert_eq!(requests.len(), 3, "PR, comments, issues: {calls}");
    for request in requests {
        assert!(
            request.contains("<--hostname>\n<github.com>"),
            "origin hostname missing: {request}"
        );
    }
    writeln!(
        std::io::stdout(),
        "GH_HOST=enterprise.example: all three next API requests pinned to github.com"
    )
    .unwrap();
}

#[test]
fn default_ship_keeps_a_safe_fast_forward_after_the_recheck_rebase() {
    let world = World::new();
    run(&world.root, &["push", "-q", "origin", "feature"]);
    let shipped = ship_with(
        &world.root,
        &Options {
            no_review: false,
            ..offline()
        },
        &|root, tier, _, _, _| {
            let report = crate::ship::review_gate::graded(root, tier, 9);
            let dev = world.move_dev("new-dev", "moved during review");
            // This ordinary remote tip is included in the rebased evidence head,
            // even though it is not an ancestor of the ship's starting head.
            run(&world.origin(), &["update-ref", "refs/heads/feature", &dev]);
            Ok(report)
        },
    )
    .unwrap();
    assert!(shipped.recheck.is_some());
    assert!(
        git::git(&world.root, &["ls-remote", "origin", "refs/heads/feature"])
            .unwrap()
            .starts_with(&shipped.head)
    );
}
