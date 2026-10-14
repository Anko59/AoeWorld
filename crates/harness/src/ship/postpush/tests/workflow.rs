use std::path::Path;

fn read(path: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path),
    )
    .unwrap()
}

#[test]
fn the_workflow_calls_only_the_harness_subcommands() {
    let workflow = read(".github/workflows/post-push.yml");
    let runs: Vec<&str> = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- run: "))
        .collect();
    assert_eq!(
        runs,
        [
            "make tools",
            "make carry-review",
            "make tools",
            "make update-prs"
        ]
    );
    for line in workflow.lines().filter(|l| l.trim().starts_with("- uses:")) {
        assert!(
            line.contains("actions/checkout@") || line.contains("./.github/actions/docker-setup"),
            "{line}"
        );
    }
    assert!(
        !workflow.contains("run: |"),
        "no shell logic in the workflow"
    );
}

#[test]
fn the_workflow_has_the_triggers_tokens_and_concurrency_the_design_names() {
    let workflow = read(".github/workflows/post-push.yml");
    for line in [
        "pull_request_target:",
        "types: [synchronize]",
        "branches: [dev]",
        "statuses: write",
        "pull-requests: write",
        "group: post-push-update",
        "cancel-in-progress: false",
        "GH_TOKEN: ${{ github.token }}",
        "contents: write",
        "actions: write",
        "CARRY_PR: ${{ github.event.pull_request.number }}",
    ] {
        assert!(workflow.contains(line), "{line}");
    }
    // The pull request's own code never runs: the dev checkout is the judge.
    assert_eq!(workflow.matches("ref: dev").count(), 2);
    for forbidden in [
        "secrets.PR_UPDATE_TOKEN",
        "issues: write",
        "head.ref",
        "head.sha",
    ] {
        assert!(!workflow.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn the_make_targets_and_the_hook_route_reach_the_subcommands() {
    let make = read("make/ship.mk");
    assert!(make.contains("carry-review:\n\t@.agents/hooks/harness.sh exec carry-review"));
    assert!(make.contains("update-prs:\n\t@.agents/hooks/harness.sh exec update-prs"));
    assert!(make.contains(" CARRY_PR UPDATE_MAX_PRS"));
    let route = read(".agents/hooks/harness.sh");
    assert!(route.contains("carry-review) probe=crates/harness/src/ship/postpush/carry.rs"));
    assert!(route.contains("update-prs) probe=crates/harness/src/ship/postpush/update.rs"));
}
