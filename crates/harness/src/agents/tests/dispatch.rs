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
