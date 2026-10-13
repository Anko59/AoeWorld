use super::*;

fn reported(file: &str, line: Option<u32>) -> Reported {
    Reported {
        file: file.into(),
        line,
        severity: super::super::protocol::Severity::Major,
        category: "correctness".into(),
        claim: "c".into(),
        trigger: "t".into(),
        expected_vs_actual: "e".into(),
        evidence: "v".into(),
    }
}

const DIFF: &str = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n a\n-b\n+c\n@@ -10,0 +11,3 @@\n+d\n+e\n+f\n";

#[test]
fn changed_lines_count_added_and_removed_lines_only() {
    assert_eq!(changed_lines(DIFF), 5);
    assert!(changed_lines("Binary files a/x and b/x differ\n") > SMALL);
}

#[test]
fn a_finding_is_inside_only_on_a_hunk_line_of_a_changed_file() {
    let lines = Lines::of(DIFF);
    assert!(lines.holds(&reported("x", Some(2))));
    assert!(lines.holds(&reported("./x", Some(13))));
    assert!(lines.holds(&reported("x", None)));
    assert!(!lines.holds(&reported("x", Some(5))));
    assert!(!lines.holds(&reported("y", Some(2))));
}

#[test]
fn an_unreadable_header_keeps_every_finding_inside() {
    let lines = Lines::of("+++ \"b/odd\\tname\"\n@@ -1 +1 @@\n");
    assert!(lines.holds(&reported("elsewhere", Some(1))));
}

#[test]
fn files_differ_by_their_exact_changed_bytes() {
    let entry =
        |path: &str, added: &str| serde_json::json!({"path": path.as_bytes(), "added": added});
    let json = |list: &[serde_json::Value]| serde_json::to_vec(list).unwrap();
    let approved = json(&[entry("a", "1"), entry("b", "2")]);
    let current = json(&[entry("a", "1"), entry("b", "3"), entry("c", "4")]);
    assert_eq!(
        differing(&approved, &current),
        Some(vec!["b".into(), "c".into()])
    );
    assert_eq!(differing(&approved, &approved), None);
    assert_eq!(differing(b"not json", &current), None);
}

#[test]
fn small_follow_ups_add_up_until_a_full_review() {
    use Step::{Full, Other, Small};
    assert_eq!(since_full([Full].into_iter()), 0);
    assert_eq!(since_full([Full, Small(150), Small(40)].into_iter()), 190);
    // A failed review or a closing review of fixes neither adds nor resets;
    // an older full review ends the count.
    assert_eq!(
        since_full([Small(500), Full, Small(90), Other, Small(60)].into_iter()),
        150
    );
    // Slices of 150 lines: the second one is over the limit in total.
    assert!(150 + since_full([Full, Small(150)].into_iter()) > SMALL);
}
