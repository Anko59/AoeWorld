use super::*;

fn check(root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    super::check(root, root)
}

fn kinds(source: &str) -> Vec<(&'static str, String)> {
    lint("crates/simulation/src/x.rs", source)
        .expect("source parses")
        .into_iter()
        .map(|finding| (finding.kind, finding.expr))
        .collect()
}

#[test]
fn a_counter_compared_to_a_literal_or_constant_is_a_cap() {
    let found = kinds("fn f(s: &mut S) -> bool { s.rays_used >= 12 }");
    assert_eq!(found, [("cmp", "s.rays_used >= 12".to_owned())]);
    let found = kinds("const STEP: u32 = 3; fn f(s: &S) -> bool { STEP < s.spent }");
    assert_eq!(found, [("cmp", "s.spent < STEP".to_owned())]);
}

#[test]
fn a_cap_named_constant_is_reported_once_as_a_use() {
    let found = kinds("fn f(s: &S) -> bool { s.rays_used == MAX_RAYS_PER_TICK }");
    assert_eq!(found, [("use", "MAX_RAYS_PER_TICK".to_owned())]);
}

#[test]
fn truncating_and_refilling_budgets_are_caps() {
    let source = "fn f(s: &mut S, v: Vec<u8>) { s.repick_budget = 4; s.repick_budget -= 1;\
                  let _ = v.iter().take(8); let _ = v.iter().take(1); }";
    assert_eq!(
        kinds(source),
        [
            ("refill", "s.repick_budget = 4".to_owned()),
            ("refill", "s.repick_budget -=".to_owned()),
            ("take", ".take(8)".to_owned()),
        ]
    );
}

#[test]
fn ordinary_arithmetic_and_ones_are_not_caps() {
    assert!(kinds("fn f(a: u32, b: u32) -> bool { a < b && a > 0 && b == 1 }").is_empty());
    assert!(kinds("fn f(s: &S) -> bool { s.members.len() > 1 }").is_empty());
}

#[test]
fn test_code_is_not_behaviour_code() {
    let source = "#[cfg(test)] mod tests { fn f(s: &S) -> bool { s.used > 9 } }\n\
                  #[test] fn g(s: &S) -> bool { s.used > 9 }";
    assert!(kinds(source).is_empty());
}

fn finding(expr: &str, line: usize) -> Finding {
    Finding {
        file: "crates/simulation/src/x.rs".to_owned(),
        kind: "cmp",
        expr: expr.to_owned(),
        line,
    }
}

fn allowance(expr: &str, count: usize) -> Allowlist {
    Allowlist {
        entries: vec![Allowed {
            file: "crates/simulation/src/x.rs".to_owned(),
            kind: "cmp".to_owned(),
            expr: expr.to_owned(),
            count,
        }],
    }
}

#[test]
fn the_allowlist_holds_today_and_a_new_cap_fails() {
    let today = [finding("a.used >= 3", 5)];
    assert!(compare(&today, &allowance("a.used >= 3", 1)).is_empty());
    let new = [finding("a.used >= 3", 5), finding("a.used >= 3", 9)];
    let problems = compare(&new, &allowance("a.used >= 3", 1));
    assert_eq!(problems.len(), 1);
    assert!(
        problems[0].contains("x.rs:9: new world cap"),
        "{problems:?}"
    );
    let problems = compare(
        &today,
        &Allowlist {
            entries: Vec::new(),
        },
    );
    assert_eq!(problems.len(), 1);
}

#[test]
fn a_removed_cap_must_leave_the_allowlist() {
    let problems = compare(&[], &allowance("a.used >= 3", 1));
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("only 0 remain"), "{problems:?}");
}

#[test]
fn rendered_allowlist_round_trips() {
    let findings = [finding("a.used >= 3", 5), finding("a.used >= 3", 9)];
    let parsed: Allowlist = serde_json::from_str(&render(&findings)).expect("valid json");
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].count, 2);
    assert!(compare(&findings, &parsed).is_empty());
}

fn git(root: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn repository(source: &str, manifest: bool) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    git(root, &["init", "--quiet", "--template="]);
    git(root, &["config", "user.email", "fixture@example.invalid"]);
    git(root, &["config", "user.name", "Fixture"]);
    fs::create_dir_all(root.join("crates/simulation/src")).unwrap();
    fs::create_dir_all(root.join("gates")).unwrap();
    fs::write(root.join("crates/simulation/src/x.rs"), source).unwrap();
    if manifest {
        write_allowlist(root);
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "base"]);
    git(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    temp
}

fn write_allowlist(root: &Path) {
    fs::write(root.join(ALLOWLIST), render(&scan(root).unwrap())).unwrap();
}

#[test]
fn a_new_cap_cannot_grant_itself_an_allowance() {
    for manifest in [false, true] {
        let repo = repository("fn f(s: &S) -> bool { s.used > 3 }", manifest);
        let root = repo.path();
        // Commit the expansion too: comparing to HEAD would miss this trigger.
        fs::write(
            root.join("crates/simulation/src/x.rs"),
            "fn f(s: &S) -> bool { s.used > 3 || s.spent > 10 }",
        )
        .unwrap();
        write_allowlist(root);
        git(root, &["add", "."]);
        git(root, &["commit", "-qm", "new cap and allowance"]);
        let problems = check(root).unwrap();
        assert!(
            problems
                .iter()
                .any(|p| p.contains("allowance") && p.contains("base")),
            "{problems:?}"
        );
    }
}

#[test]
fn increasing_an_existing_allowance_is_rejected_but_shrinking_is_allowed() {
    let repo = repository("fn f(s: &S) -> bool { s.used > 3 }", true);
    let root = repo.path();
    fs::write(
        root.join("crates/simulation/src/x.rs"),
        "fn f(s: &S) -> bool { s.used > 3 || s.used > 3 }",
    )
    .unwrap();
    write_allowlist(root);
    assert!(!check(root).unwrap().is_empty());
    fs::write(root.join("crates/simulation/src/x.rs"), "fn f() {}").unwrap();
    write_allowlist(root);
    assert!(check(root).unwrap().is_empty());
}

#[test]
fn gameplay_match_file_and_nested_production_sources_are_scanned() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::create_dir_all(root.join("crates/server/src/gameplay_match/tests")).unwrap();
    for name in [
        "gameplay_match.rs",
        "gameplay_match/routes.rs",
        "gameplay_match/tests/caps.rs",
    ] {
        fs::write(
            root.join("crates/server/src").join(name),
            "fn f(s: &S) -> bool { s.used > 10 }",
        )
        .unwrap();
    }
    let findings = scan(root).unwrap();
    assert_eq!(findings.len(), 2, "{findings:?}");
    assert!(
        findings
            .iter()
            .any(|f| f.file.ends_with("gameplay_match.rs"))
    );
    assert!(
        findings
            .iter()
            .any(|f| f.file.ends_with("gameplay_match/routes.rs"))
    );
}

#[test]
fn unavailable_or_invalid_base_is_an_error() {
    let repo = repository("fn f() {}", true);
    let root = repo.path();
    fs::write(root.join(ALLOWLIST), "not JSON").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "invalid base"]);
    git(root, &["update-ref", "refs/remotes/origin/dev", "HEAD"]);
    write_allowlist(root);
    assert!(
        check(root).is_err(),
        "must not fall back from a malformed base"
    );
    git(root, &["update-ref", "-d", "refs/remotes/origin/dev"]);
    assert!(
        check(root)
            .unwrap_err()
            .to_string()
            .contains("base unavailable")
    );
}

#[test]
fn exported_index_checks_its_manifest_against_source_history() {
    use crate::gates::scopes::{Kind, Snapshot};
    let repo = repository("fn f(s: &S) -> bool { s.used > 3 }", true);
    let root = repo.path();
    let snapshot = Snapshot::prepare_independent(root, Kind::Index).unwrap();
    assert!(super::check(snapshot.root(), root).unwrap().is_empty());
    drop(snapshot);
    fs::write(
        root.join("crates/simulation/src/x.rs"),
        "fn f(s: &S) -> bool { s.used > 3 || s.spent > 10 }",
    )
    .unwrap();
    write_allowlist(root);
    git(root, &["add", "."]);
    let snapshot = Snapshot::prepare_independent(root, Kind::Index).unwrap();
    let problems = super::check(snapshot.root(), root).unwrap();
    assert!(
        problems
            .iter()
            .any(|p| p.contains("allowance increased from base")),
        "{problems:?}"
    );
}

#[test]
fn splitting_an_expanded_allowance_into_duplicates_cannot_bypass_the_ratchet() {
    let base = allowance("s.used > 3", 1);
    let mut expanded = allowance("s.used > 3", 1);
    expanded.entries.extend(allowance("s.used > 3", 1).entries);
    assert_eq!(ratchet::compare(&expanded, &base).len(), 1);
}
