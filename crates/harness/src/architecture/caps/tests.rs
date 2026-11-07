use super::*;

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
