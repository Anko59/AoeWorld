use super::*;

#[test]
fn issue_and_next_are_available_to_main_testers_and_implementers() {
    let fixture = Fixture::new();
    for role in [Role::Main, Role::Tester, Role::Implementer] {
        allowed(
            &fixture,
            role,
            "make issue ISSUE_TITLE=task ISSUE_BODY=body",
        );
        allowed(&fixture, role, "make next");
    }
}

#[test]
fn reviewers_cannot_file_or_select_issues() {
    let fixture = Fixture::new();
    denied(
        &fixture,
        Role::Reviewer,
        "make issue ISSUE_TITLE=task ISSUE_BODY=body",
    );
    denied(&fixture, Role::Reviewer, "make next");
}
