use super::*;

#[test]
fn red_records_are_main_session_only_and_git_metadata_is_protected() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        denied(&fixture, role, "make red");
        denied(&fixture, role, "make red-checkout");
        denied(&fixture, role, "printf '{}' > .git/aoe-ship/red/head.json");
        denied(
            &fixture,
            role,
            "printf '{}' > .cache/agent-hook/red/output/run-x/playwright.json",
        );
    }
    allowed(&fixture, Role::Main, "make red");
}

#[test]
fn issue_and_next_are_available_to_main_testers_and_implementers() {
    let fixture = Fixture::new();
    for role in [Role::Main, Role::Tester, Role::Implementer] {
        allowed(&fixture, role, "printf 'task\\n\\nbody\\n' | make issue");
        allowed(&fixture, role, "make next");
    }
    for role in [Role::Tester, Role::Implementer] {
        denied(&fixture, role, "make issue ISSUE_TITLE=x");
        denied(
            &fixture,
            role,
            "printf 'task\\n\\nbody\\n' | make issue SHIP_TITLE=x",
        );
        denied(&fixture, role, "make next SHIP_TITLE=x");
    }
}

#[test]
fn reviewers_cannot_file_or_select_issues() {
    let fixture = Fixture::new();
    denied(
        &fixture,
        Role::Reviewer,
        "printf 'task\\n\\nbody\\n' | make issue",
    );
    denied(&fixture, Role::Reviewer, "make next");
}

#[test]
fn unknown_agents_cannot_file_or_select_issues() {
    let fixture = Fixture::new();
    denied(
        &fixture,
        Role::Other,
        "printf 'task\\n\\nbody\\n' | make issue",
    );
    denied(&fixture, Role::Other, "make next");
}

#[test]
fn only_the_main_session_runs_nightly_triage() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        denied(&fixture, role, "make nightly-triage");
    }
    allowed(&fixture, Role::Main, "make nightly-triage");
}

#[test]
fn duel_and_its_variables_belong_only_to_main() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        denied(&fixture, role, "make duel");
        denied(&fixture, role, "make test-unit DUEL_CHECK=true");
        denied(&fixture, role, "DUEL_RUNTIME=codex make test-unit");
    }
    allowed(
        &fixture,
        Role::Main,
        "make duel DUEL_TASK=task.md DUEL_CHECK='make test-unit'",
    );
    allowed(&fixture, Role::Main, "DUEL_RUNTIME=codex make duel");
}

#[test]
fn every_role_reads_metrics_but_only_the_main_session_files_regressions() {
    let fixture = Fixture::new();
    for role in [
        Role::Main,
        Role::Tester,
        Role::Implementer,
        Role::Reviewer,
        Role::Other,
    ] {
        allowed(&fixture, role, "make metrics");
    }
    for role in [Role::Tester, Role::Implementer, Role::Reviewer] {
        allowed(&fixture, role, "make metrics METRICS_REV=HEAD~1");
        denied(&fixture, role, "make metric-regressions");
    }
    allowed(&fixture, Role::Main, "make metric-regressions");
}

#[test]
fn the_model_ledger_and_usage_are_main_session_only() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        denied(&fixture, role, "make ledger");
        denied(&fixture, role, "make usage");
    }
    allowed(&fixture, Role::Main, "make ledger");
    allowed(&fixture, Role::Main, "make usage");
}

#[test]
fn level_floor_is_read_only_for_every_role_and_level_stats_is_main_only() {
    let fixture = Fixture::new();
    for role in [
        Role::Main,
        Role::Tester,
        Role::Implementer,
        Role::Reviewer,
        Role::Other,
    ] {
        allowed(&fixture, role, "make level-floor");
    }
    allowed(&fixture, Role::Main, "make level-stats");
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        denied(&fixture, role, "make level-stats");
    }
}

#[test]
fn describe_check_is_read_only_for_every_role() {
    let fixture = Fixture::new();
    for role in [
        Role::Main,
        Role::Tester,
        Role::Implementer,
        Role::Reviewer,
        Role::Other,
    ] {
        allowed(
            &fixture,
            role,
            "make describe-check SHIP_BODY=body.md SHIP_TITLE=t SHIP_VIDEO=v.webm",
        );
    }
}
