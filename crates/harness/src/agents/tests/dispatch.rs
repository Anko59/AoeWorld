use super::*;

#[test]
fn issue_and_next_are_available_to_main_testers_and_implementers() {
    let fixture = Fixture::new();
    for role in [Role::Main, Role::Tester, Role::Implementer] {
        allowed(
            &fixture,
            role,
            "make issue ISSUE_TITLE=task ISSUE_LABELS=priority:medium",
        );
        allowed(&fixture, role, "make next");
    }
}

#[test]
fn reviewers_cannot_file_or_select_issues() {
    let fixture = Fixture::new();
    denied(&fixture, Role::Reviewer, "make issue ISSUE_TITLE=task");
    denied(&fixture, Role::Reviewer, "make next");
}

#[test]
fn unknown_agents_cannot_file_or_select_issues() {
    let fixture = Fixture::new();
    denied(&fixture, Role::Other, "make issue ISSUE_TITLE=task");
    denied(&fixture, Role::Other, "make next");
}

#[test]
fn make_dollar_assignments_are_denied_in_command_and_environment_forms() {
    let fixture = Fixture::new();
    for role in [Role::Tester, Role::Implementer, Role::Reviewer, Role::Other] {
        for command in [
            "make issue ISSUE_TITLE='$(shell touch x)'",
            "ISSUE_TITLE='$(shell touch x)' make issue",
            "env ISSUE_TITLE='$(shell touch x)' make issue",
            "make issue ISSUE_LABELS='$(shell touch x)'",
            "ISSUE_LABELS='$(shell touch x)' make issue",
            "env ISSUE_LABELS='$(shell touch x)' make issue",
        ] {
            denied(&fixture, role, command);
        }
    }
}
