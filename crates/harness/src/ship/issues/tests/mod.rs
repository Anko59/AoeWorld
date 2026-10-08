use super::*;
use std::{fs, os::unix::fs::PermissionsExt, process::Command, sync::Mutex};

static ENVIRONMENT: Mutex<()> = Mutex::new(());

fn item(number: u64, title: &str, labels: &[&str], created_at: &str) -> Issue {
    Issue {
        number,
        title: title.into(),
        body: String::new(),
        labels: labels
            .iter()
            .map(|name| Label {
                name: (*name).into(),
            })
            .collect(),
        created_at: created_at.into(),
        url: format!("https://github.com/o/r/issues/{number}"),
    }
}

#[test]
fn issue_text_limits_count_characters_and_allow_only_newline_and_tab_controls() {
    assert!(validate_text("title", &"x".repeat(TITLE_LIMIT), TITLE_LIMIT).is_ok());
    assert!(validate_text("title", &"x".repeat(TITLE_LIMIT + 1), TITLE_LIMIT).is_err());
    assert!(validate_text("body", &"x".repeat(BODY_LIMIT), BODY_LIMIT).is_ok());
    assert!(validate_text("body", &"x".repeat(BODY_LIMIT + 1), BODY_LIMIT).is_err());
    assert!(validate_text("body", "line\n\tindented", BODY_LIMIT).is_ok());
    for control in ['\0', '\r', '\u{7f}', '\u{1b}'] {
        assert!(validate_text("body", &format!("a{control}b"), BODY_LIMIT).is_err());
    }
    assert!(marked_body(&"x".repeat(BODY_LIMIT), "title").is_err());
}

#[test]
fn issue_labels_are_limited_to_priorities_blocked_and_area_labels() {
    for label in [
        "priority:critical",
        "priority:high",
        "priority:medium",
        "priority:low",
        "blocked",
        "area:harness",
        "area:game-core",
        "area:game core",
        "area:review/testing.v2",
    ] {
        assert!(allowed_label(label), "{label}");
    }
    for label in [
        "bug",
        "review-follow-up",
        "area:",
        "area:\t",
        "priority:urgent",
    ] {
        assert!(!allowed_label(label), "{label}");
    }
    assert_eq!(
        labels_from(Some("priority:high,area:harness,blocked".into())).unwrap(),
        ["agent-found", "priority:high", "area:harness", "blocked"]
    );
    assert!(labels_from(Some("bug".into())).is_err());
}

#[test]
fn only_matching_title_fingerprint_marker_and_agent_label_deduplicates() {
    let title = "Repair queue ordering";
    let marker = format!("{MARKER}{} -->", fingerprint(title));
    let mut decoy = item(9, "A different task", &[AGENT_LABEL], "2026-01-01");
    decoy.body = marker.clone();
    assert_eq!(duplicate_index(&[decoy.clone()], title), None);
    decoy.title = title.into();
    decoy.body.clear();
    assert_eq!(duplicate_index(&[decoy.clone()], title), None);
    decoy.body = marker;
    assert_eq!(duplicate_index(&[decoy.clone()], title), Some(0));
    decoy.labels.clear();
    assert_eq!(duplicate_index(&[decoy], title), None);
}

#[test]
fn next_ranks_priority_then_oldest_and_skips_blocked() {
    let issues = [
        item(1, "new critical", &["priority:critical"], "2026-01-02"),
        item(2, "old critical", &["priority:critical"], "2026-01-01"),
        item(3, "high", &["priority:high"], "2025-01-01"),
        item(4, "medium", &["priority:medium"], "2025-01-01"),
        item(5, "plain", &[], "2020-01-01"),
        item(
            6,
            "blocked critical",
            &["priority:critical", "blocked"],
            "2010-01-01",
        ),
        item(7, "low", &["priority:low"], "2010-01-01"),
    ];
    let ranked = rank(&issues);
    assert_eq!(
        ranked.iter().map(|issue| issue.number).collect::<Vec<_>>(),
        [2, 1, 3, 4, 5, 7]
    );
}

#[test]
fn make_issue_uses_origin_ignores_repo_override_and_does_not_match_marker_decoy() {
    let _guard = ENVIRONMENT.lock().unwrap();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("checkout");
    fs::create_dir_all(&root).unwrap();
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    Command::new("git")
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/project/checkout.git",
        ])
        .current_dir(&root)
        .status()
        .unwrap();
    let bin = temp.path().join("bin");
    fs::create_dir(&bin).unwrap();
    let log = temp.path().join("gh-args.log");
    let gh = bin.join("gh");
    fs::write(&gh, "#!/bin/sh\nprintf '<%s>\\n' \"$@\" >> \"$GH_LOG\"\ncase \"$1 $2\" in\n  'issue list') printf '%s' \"$GH_LIST_JSON\" ;;\n  'issue create') printf 'https://github.com/project/checkout/issues/12\\n' ;;\nesac\n").unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let old_list = std::env::var_os("GH_LIST_JSON");
    let old_log = std::env::var_os("GH_LOG");
    let old_repo = std::env::var_os("GITHUB_REPOSITORY");
    let old_title = std::env::var_os("ISSUE_TITLE");
    let old_body = std::env::var_os("ISSUE_BODY");
    let old_labels = std::env::var_os("ISSUE_LABELS");
    let title = "Do useful work";
    let print = fingerprint(title);
    let decoy_json = serde_json::json!([{
        "number": 4,
        "title": "Completely different title",
        "body": format!("{MARKER}{print} -->"),
        "labels": [{"name": "agent-found"}],
        "createdAt": "2026-01-01T00:00:00Z",
        "url": "https://github.com/project/checkout/issues/4"
    }]);
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{}", bin.display(), old_path.to_string_lossy()),
        );
        std::env::set_var("GH_LOG", &log);
        std::env::set_var("GH_LIST_JSON", decoy_json.to_string());
        std::env::set_var("GITHUB_REPOSITORY", "wrong/override");
        std::env::set_var("ISSUE_TITLE", title);
        std::env::set_var("ISSUE_BODY", "Body text from the environment");
        std::env::set_var("ISSUE_LABELS", "priority:medium");
    }
    let result = issue(&root);
    result.unwrap();
    let args = fs::read_to_string(&log).unwrap();
    assert!(args.contains("<project/checkout>"));
    assert!(!args.contains("<wrong/override>"));
    assert!(args.contains("Body text from the environment"));
    assert!(args.contains("<agent-found>"));
    assert!(args.contains("<priority:medium>"));
    assert!(
        args.contains("<create>"),
        "marker on a different title is not a duplicate"
    );

    let matching = serde_json::json!([{
        "number": 12,
        "title": title,
        "body": format!("{}\n\n{MARKER}{print} -->", "Body text"),
        "labels": [{"name": "agent-found"}],
        "createdAt": "2026-01-01T00:00:00Z",
        "url": "https://github.com/project/checkout/issues/12"
    }]);
    fs::write(&log, "").unwrap();
    unsafe { std::env::set_var("GH_LIST_JSON", matching.to_string()) };
    issue(&root).unwrap();
    let duplicate_args = fs::read_to_string(&log).unwrap();
    assert!(duplicate_args.contains("<comment>"));
    assert!(!duplicate_args.contains("<create>"));

    restore("PATH", Some(old_path));
    restore("GH_LIST_JSON", old_list);
    restore("GH_LOG", old_log);
    restore("GITHUB_REPOSITORY", old_repo);
    restore("ISSUE_TITLE", old_title);
    restore("ISSUE_BODY", old_body);
    restore("ISSUE_LABELS", old_labels);
}

fn restore(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}
