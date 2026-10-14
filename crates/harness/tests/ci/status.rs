use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn dispatch_status_cli_posts_exact_result_and_refuses_other_contexts() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let log = root.join("calls");
    fs::write(
        root.join("gh"),
        "#!/bin/sh\nprintf '<%s>\\n' \"$@\" >> \"$GH_LOG\"\n[ ! -f \"$GH_FAIL\" ]\n",
    )
    .unwrap();
    fs::set_permissions(root.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
    let setup = || {
        let mut cmd = command(root, "ci-dispatch-status");
        cmd.env(
            "PATH",
            format!("{}:{}", root.display(), std::env::var("PATH").unwrap()),
        )
        .env("GH_LOG", &log)
        .env("GH_FAIL", root.join("fail"))
        .env("GH_TOKEN", "stub-token")
        .env("GITHUB_ACTIONS", "true")
        .env("GITHUB_EVENT_NAME", "workflow_dispatch")
        .env("GITHUB_REF_NAME", "feature/example")
        .env("GITHUB_SHA", "abc123")
        .env("GITHUB_REPOSITORY", "owner/repo")
        .env("GITHUB_SERVER_URL", "https://github.com")
        .env("GITHUB_RUN_ID", "1234")
        .env("JOB_STATUS", "success");
        cmd
    };
    for (status, state) in [
        ("success", "success"),
        ("failure", "failure"),
        ("cancelled", "failure"),
        ("skipped", "failure"),
    ] {
        fs::write(&log, "").unwrap();
        assert!(proof(setup().env("JOB_STATUS", status)).status.success());
        let calls = fs::read_to_string(&log).unwrap();
        assert_eq!(
            calls,
            format!(
                "<api>\n<--method>\n<POST>\n<repos/owner/repo/statuses/abc123>\n<-f>\n<context=required>\n<-f>\n<state={state}>\n<-f>\n<description=workflow_dispatch run 1234: {status}>\n<-f>\n<target_url=https://github.com/owner/repo/actions/runs/1234>\n"
            )
        );
        use std::io::Write;
        std::io::stdout().write_all(calls.as_bytes()).unwrap();
    }
    for (key, value) in [
        ("GITHUB_ACTIONS", "false"),
        ("GITHUB_EVENT_NAME", "push"),
        ("GITHUB_REF_NAME", "dev"),
        ("GITHUB_REF_NAME", "main"),
        ("GITHUB_REF_NAME", ""),
    ] {
        fs::write(&log, "").unwrap();
        assert!(!proof(setup().env(key, value)).status.success());
        assert_eq!(fs::read_to_string(&log).unwrap(), "");
    }
    fs::write(root.join("fail"), "").unwrap();
    assert!(!proof(&mut setup()).status.success());
}

#[test]
fn workflow_scopes_status_write_and_always_reports_the_dispatch_result() {
    let workflow = include_str!("../../../../.github/workflows/ci.yml");
    assert!(!workflow.contains("inputs.base"));
    assert!(!workflow.contains("    inputs:"));
    assert_eq!(
        workflow
            .matches("AOE_BASE_SHA: ${{ github.event.pull_request.base.sha }}")
            .count(),
        2
    );
    let required = workflow
        .split("\n  required:\n")
        .nth(1)
        .unwrap()
        .split("\n  dev-artifacts:")
        .next()
        .unwrap();
    for line in [
        "permissions: { contents: read, statuses: write }",
        "if: ${{ always() && github.event_name == 'workflow_dispatch' }}",
        "GH_TOKEN: ${{ github.token }}",
        "JOB_STATUS: ${{ job.status }}",
        "run: make ci-dispatch-status",
    ] {
        assert!(required.contains(line), "{line}");
    }
    assert_eq!(workflow.matches("statuses: write").count(), 1);
    assert!(
        required.find("run: make ci-check").unwrap()
            < required.find("run: make ci-dispatch-status").unwrap()
    );
    let make = include_str!("../../../../Makefile");
    for target in ["ci-select:", "ci-check:"] {
        let recipe = make.split(target).nth(1).unwrap().lines().nth(1).unwrap();
        assert!(
            recipe.contains("-e GITHUB_EVENT_NAME -e GITHUB_REF_NAME"),
            "{recipe}"
        );
    }
}
