use super::*;

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
    }
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

#[test]
fn agents_run_focused_checks_and_need_a_lease_for_whole_suites() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Other] {
        for suite in [
            "make test-unit",
            "make preflight",
            "make coverage",
            "make test-harness",
        ] {
            denied(&fixture, role, suite);
        }
        allowed(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map 'AOE_NATIVE_TEST_FILTER=test(regions)'",
        );
        // A focused name with a filter that selects everything is a whole suite.
        denied(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation 'AOE_NATIVE_TEST_FILTER=all()'",
        );
        denied(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map",
        );
        denied(
            &fixture,
            role,
            "make test-harness-focused HARNESS_TEST_FILTER=s",
        );
        denied(&fixture, role, "make test-wasm-focused");
        allowed(
            &fixture,
            role,
            "make test-harness-focused HARNESS_TEST_FILTER=budget",
        );
        allowed(
            &fixture,
            role,
            "make test-e2e AOE_E2E_SPECS='a.spec.ts b.spec.ts'",
        );
        denied(&fixture, role, "make test-e2e");
        denied(&fixture, role, "make test-e2e AOE_E2E_SPECS='a b c d e f'");
        denied(&fixture, role, "make lease AOE_LEASE_TARGET=test-unit");
    }
    allowed(&fixture, Role::Main, "make test-unit");
    allowed(&fixture, Role::Main, "make preflight");
}

#[test]
fn launched_sessions_are_budgeted_like_agents_until_a_lease() {
    use crate::agents::rules::budget;
    let fixture = Fixture::new();
    let mut context = fixture.context(Role::Main);
    assert!(
        budget::check(&context, "test-unit", &[]).is_ok(),
        "a person's session"
    );
    context.coordinated = true;
    let refused = budget::check(&context, "test-unit", &[]).unwrap_err();
    assert!(
        refused.contains("make lease AOE_LEASE_TARGET=test-unit"),
        "{refused}"
    );
    let path = budget::grant(&context.common, "test-unit", 30, "landscape").unwrap();
    assert!(path.starts_with(&context.common));
    assert!(budget::check(&context, "test-unit", &[]).is_ok());
    assert!(
        budget::check(&context, "coverage", &[]).is_err(),
        "a lease covers one suite"
    );
    assert!(!budget::leased(
        &context.common,
        "test-unit",
        budget::now() + 31 * 60
    ));
    assert!(
        budget::grant(&context.common, "fmt", 30, "x").is_err(),
        "fmt needs no lease"
    );
    assert!(budget::grant(&context.common, "test-unit", 0, "x").is_err());
}

/// Review of #836: each way around the budget that the first version allowed.
#[test]
fn the_budget_has_no_way_around_it() {
    use crate::agents::rules::budget;
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Other] {
        denied(&fixture, role, "make test-e2e 'AOE_E2E_SPECS=**/*.spec.ts'");
        denied(
            &fixture,
            role,
            "make test-e2e AOE_E2E_SPECS=one.spec.ts AOE_E2E_SPECS=",
        );
        denied(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-simulation 'AOE_NATIVE_TEST_FILTER=test(a)'",
        );
        denied(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map 'AOE_NATIVE_TEST_FILTER=kind(lib)'",
        );
        denied(
            &fixture,
            role,
            "make test-client-memory AOE_E2E_SPECS=tests/memory/fog.spec.ts",
        );
        // Closing review: tab-separated specs and a negated filter.
        denied(
            &fixture,
            role,
            "make test-e2e 'AOE_E2E_SPECS=a.spec.ts\tb.spec.ts\tc.spec.ts\td.spec.ts\te.spec.ts\tf.spec.ts'",
        );
        for filter in [
            "not test(slow)",
            "test(regions) | not test(slow)",
            "kind(lib) - test(slow)",
            "all() & test(regions)",
        ] {
            denied(
                &fixture,
                role,
                &format!(
                    "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map 'AOE_NATIVE_TEST_FILTER={filter}'"
                ),
            );
        }
        allowed(
            &fixture,
            role,
            "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-map 'AOE_NATIVE_TEST_FILTER=test(regions) | test(=churn::flush)'",
        );
        // Subtracting from or intersecting a named set can only shrink it.
        for filter in [
            "test(gameplay) - test(slow)",
            "test(gameplay) & kind(lib)",
            "test(foo-bar)",
        ] {
            allowed(
                &fixture,
                role,
                &format!(
                    "make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server 'AOE_NATIVE_TEST_FILTER={filter}'"
                ),
            );
        }
    }
    // Reviewers use the focused checks that replace preflight for them.
    allowed(
        &fixture,
        Role::Reviewer,
        "make test-harness-focused HARNESS_TEST_FILTER=agents::",
    );
    // A launched session can neither grant itself a lease nor be refused one by name.
    let mut context = fixture.context(Role::Main);
    context.coordinated = true;
    assert!(budget::check(&context, "lease", &[]).is_err());
    assert!(budget::grant(&context.common, "test-native-focused", 30, "harness").is_ok());
    let all = [("AOE_NATIVE_TEST_FILTER", "all()")];
    assert!(
        budget::check(&context, "test-native-focused", &all).is_ok(),
        "leased"
    );
}
