use super::*;
use history::Step;

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
    let lines = Lines::of("+++ \"b/odd\\qname\"\n@@ -1 +1 @@\n");
    assert!(lines.holds(&reported("elsewhere", Some(1))));
    // Without a prefix, with a stray quote, or without any name at all.
    for diff in ["+++ x\n", "--- \"a/x\n", "diff --git a/x b/y\n"] {
        assert!(
            Lines::of(diff).holds(&reported("elsewhere", Some(1))),
            "{diff}"
        );
    }
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

#[test]
fn every_added_and_removed_content_line_counts() {
    // `++i;` added, `-- x` and `---` removed: they look like headers.
    let diff = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,3 +1,2 @@\n+++i;\n--- x\n----\n c\n\\ No newline at end of file\n";
    assert_eq!(changed_lines(diff), 3);
    // A header line after a hunk is not a content line.
    let two = format!("{diff}diff --git a/y b/y\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-a\n+b\n");
    assert_eq!(changed_lines(&two), 5);
    assert!(changed_lines("@@ nonsense @@\n") > SMALL);
}

#[test]
fn a_finding_on_a_file_without_hunks_is_inside() {
    let diff = "diff --git a/gone.rs b/gone.rs\ndeleted file mode 100644\n--- a/gone.rs\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n\
diff --git a/old.rs b/new.rs\nsimilarity index 100%\nrename from old.rs\nrename to new.rs\n\
diff --git a/m.sh b/m.sh\nold mode 100644\nnew mode 100755\n\
diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n\
diff --git a/p q.rs b/p q.rs\nold mode 100644\nnew mode 100755\n\
diff --git a/sp ace.md b/sp ace.md\n--- a/sp ace.md\t\n+++ b/sp ace.md\t\n@@ -3 +3 @@\n-a\n+b\n";
    let lines = Lines::of(diff);
    for file in ["gone.rs", "old.rs", "new.rs", "m.sh", "img.png", "p q.rs"] {
        assert!(lines.holds(&reported(file, None)), "{file}");
        assert!(lines.holds(&reported(file, Some(40))), "{file}");
    }
    // A file with hunks still needs a line inside them, even with a space.
    assert!(lines.holds(&reported("sp ace.md", Some(3))));
    assert!(!lines.holds(&reported("sp ace.md", Some(9))));
    assert!(!lines.holds(&reported("other.rs", None)));
}

#[test]
fn quoted_paths_are_decoded_or_everything_is_inside() {
    let diff = "diff --git \"a/q\\\"x\" \"b/q\\\"x\"\n--- \"a/q\\\"x\"\n+++ \"b/q\\\"x\"\n@@ -1 +1 @@\n-a\n+b\n\
diff --git \"a/\\303\\251.md\" \"b/\\303\\251.md\"\n--- \"a/\\303\\251.md\"\n+++ \"b/\\303\\251.md\"\n@@ -5 +5 @@\n-a\n+b\n";
    let lines = Lines::of(diff);
    assert!(lines.holds(&reported("q\"x", Some(1))));
    assert!(!lines.holds(&reported("q\"x", Some(2))));
    assert!(lines.holds(&reported("\u{e9}.md", Some(5))));
    assert!(!lines.holds(&reported("elsewhere", Some(1))));
    // An escape that cannot be decoded exactly keeps everything inside.
    let bad = "--- \"a/\\777\"\n+++ \"b/\\777\"\n@@ -1 +1 @@\n-a\n+b\n";
    assert!(Lines::of(bad).holds(&reported("elsewhere", Some(1))));
}

#[test]
fn a_passing_review_of_fixes_settles_what_failed_before_it() {
    use Step::{FailedFull, FailedSmall, Fixes, Full, Other, Small};
    // Slicing by failing: 150 fails, its fixes pass, the next 150 is too much.
    let history = [Full, FailedSmall(150), Fixes];
    assert_eq!(since_full(history.into_iter()), 150);
    assert!(150 + since_full(history.into_iter()) > SMALL);
    // Until the fixes pass, nothing is approved; the latest failure wins.
    assert_eq!(since_full([Full, FailedSmall(150)].into_iter()), 0);
    assert_eq!(
        since_full([FailedSmall(10), FailedSmall(30), Fixes].into_iter()),
        30
    );
    // A failed full review of an increment, once its fixes pass, was blind.
    assert_eq!(
        since_full([Small(90), FailedFull, Other, Fixes].into_iter()),
        0
    );
    // Fixes without anything pending, and a passing report, settle nothing.
    assert_eq!(since_full([Small(90), Fixes, Fixes].into_iter()), 90);
    assert_eq!(
        since_full([FailedSmall(50), Small(20), Fixes].into_iter()),
        20
    );
    assert_eq!(
        since_full([Small(90), FailedFull, Full, Fixes].into_iter()),
        0
    );
}
