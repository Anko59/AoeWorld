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
        pull_request: None,
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
    fs::write(&gh, "#!/bin/sh\nprintf 'GH_HOST=<%s> GH_REPO=<%s>\\n' \"${GH_HOST-unset}\" \"${GH_REPO-unset}\" >> \"$GH_LOG\"\nprintf '<%s>\\n' \"$@\" >> \"$GH_LOG\"\ncase \"$1\" in\n  api) printf '%s' \"$GH_LIST_JSON\" ;;\n  issue) if [ \"$2\" = create ]; then cat > \"$GH_BODY_LOG\"; printf 'https://github.com/project/checkout/issues/12\\n'; fi ;;\nesac\n").unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    let old_path = std::env::var_os("PATH").unwrap_or_default();
    let old_list = std::env::var_os("GH_LIST_JSON");
    let old_log = std::env::var_os("GH_LOG");
    let old_body_log = std::env::var_os("GH_BODY_LOG");
    let old_repo = std::env::var_os("GITHUB_REPOSITORY");
    let old_title = std::env::var_os("ISSUE_TITLE");
    let old_labels = std::env::var_os("ISSUE_LABELS");
    let old_host = std::env::var_os("GH_HOST");
    let old_repo_env = std::env::var_os("GH_REPO");
    let old_role = std::env::var_os("AOE_AGENT_ROLE");
    let title = "Do useful work";
    let print = fingerprint(title);
    let decoy_json = serde_json::json!([[{
        "number": 4,
        "title": "Completely different title",
        "body": format!("{MARKER}{print} -->"),
        "labels": [{"name": "agent-found"}],
        "createdAt": "2026-01-01T00:00:00Z",
        "url": "https://github.com/project/checkout/issues/4"
    }]]);
    unsafe {
        std::env::set_var(
            "PATH",
            format!("{}:{}", bin.display(), old_path.to_string_lossy()),
        );
        std::env::set_var("GH_LOG", &log);
        std::env::set_var("GH_BODY_LOG", temp.path().join("body.log"));
        std::env::set_var("GH_LIST_JSON", decoy_json.to_string());
        std::env::set_var("GITHUB_REPOSITORY", "wrong/override");
        std::env::set_var("ISSUE_TITLE", title);
        std::env::set_var("ISSUE_LABELS", "priority:medium");
        std::env::set_var("GH_HOST", "enterprise.example");
        std::env::set_var("GH_REPO", "wrong/override");
        std::env::remove_var("AOE_AGENT_ROLE");
    }
    let result = issue_with_input(&root, std::io::Cursor::new(b"Body text from stdin"));
    result.unwrap();
    let args = fs::read_to_string(&log).unwrap();
    assert!(!args.contains("<wrong/override>"));
    assert!(args.contains("<github.com/project/checkout>"));
    assert!(args.contains("GH_HOST=<unset> GH_REPO=<unset>"));
    assert!(args.contains("<--hostname>\n<github.com>"));
    let submitted = fs::read_to_string(temp.path().join("body.log")).unwrap();
    assert!(submitted.contains("Body text from stdin"));
    assert!(args.contains("<agent-found>"));
    assert!(args.contains("<priority:medium>"));
    assert!(
        args.contains("<create>"),
        "marker on a different title is not a duplicate"
    );

    let matching = serde_json::json!([[{
        "number": 12,
        "title": title,
        "body": format!("{}\n\n{MARKER}{print} -->", "Body text"),
        "labels": [{"name": "agent-found"}],
        "createdAt": "2026-01-01T00:00:00Z",
        "url": "https://github.com/project/checkout/issues/12"
    }]]);
    fs::write(&log, "").unwrap();
    unsafe { std::env::set_var("GH_LIST_JSON", matching.to_string()) };
    issue_with_input(&root, std::io::Cursor::new(b"Body text")).unwrap();
    let duplicate_args = fs::read_to_string(&log).unwrap();
    assert!(duplicate_args.contains("<comment>"));
    assert!(!duplicate_args.contains("<create>"));

    let old_page: Vec<_> = (1..=501)
        .map(|number| {
            serde_json::json!({
                "number": number, "title": format!("Older task {number}"), "body": "", "labels": [],
                "createdAt": format!("2026-01-{number:02}"), "url": "url"
            })
        })
        .collect();
    let late_duplicate = serde_json::json!({
        "number": 600, "title": title,
        "body": format!("Body\n\n{MARKER}{print} -->"),
        "labels": [{"name": "agent-found"}], "createdAt": "2026-01-01T00:00:00Z",
        "url": "https://github.com/project/checkout/issues/600"
    });
    unsafe {
        std::env::set_var(
            "GH_LIST_JSON",
            serde_json::json!([old_page, [late_duplicate]]).to_string(),
        );
    }
    fs::write(&log, "").unwrap();
    issue_with_input(&root, std::io::Cursor::new(b"Body text")).unwrap();
    let paged_args = fs::read_to_string(&log).unwrap();
    assert!(paged_args.contains("<--paginate>"));
    assert!(paged_args.contains("<--hostname>\n<github.com>"));
    assert!(paged_args.contains("<comment>"));
    assert!(!paged_args.contains("<create>"));

    restore("PATH", Some(old_path));
    restore("GH_LIST_JSON", old_list);
    restore("GH_LOG", old_log);
    restore("GH_BODY_LOG", old_body_log);
    restore("GITHUB_REPOSITORY", old_repo);
    restore("ISSUE_TITLE", old_title);
    restore("ISSUE_LABELS", old_labels);
    restore("GH_HOST", old_host);
    restore("GH_REPO", old_repo_env);
    restore("AOE_AGENT_ROLE", old_role);
}

#[test]
fn pagination_keeps_all_issues_and_fails_above_the_safety_cap() {
    let many: Vec<_> = (1..=501)
        .map(|number| {
            serde_json::json!({
                "number": number, "title": format!("task {number}"), "body": "", "labels": [],
                "createdAt": format!("2026-01-{number:02}"), "url": "url"
            })
        })
        .collect();
    let parsed = parse_issue_pages(&serde_json::json!([many]).to_string()).unwrap();
    assert_eq!(parsed.len(), 501);
    assert_eq!(rank(&parsed).first().unwrap().number, 1);
    let over_cap: Vec<_> = (0..=ISSUE_CAP)
        .map(|number| {
            serde_json::json!({
                "number": number, "title": "x", "body": "", "labels": [], "createdAt": "", "url": ""
            })
        })
        .collect();
    let error = parse_issue_pages(&serde_json::json!([over_cap]).to_string()).unwrap_err();
    assert!(error.to_string().contains("more than 10000"));
}

#[test]
fn body_validation_rejects_trailing_carriage_return_before_trim() {
    assert!(marked_body("summary\r", "title").is_err());
}

#[test]
fn allowed_multibyte_body_is_validated_and_stays_out_of_process_arguments() {
    let body = "界".repeat(43_680);
    let marked = marked_body(&body, "title").unwrap();
    assert!(marked.chars().count() < BODY_LIMIT);
    assert_eq!(marked.len(), body.len() + MARKER.len() + 23);
}

#[test]
fn rust_issue_commands_refuse_unknown_agent_roles() {
    let _guard = ENVIRONMENT.lock().unwrap();
    let previous = std::env::var_os("AOE_AGENT_ROLE");
    unsafe { std::env::set_var("AOE_AGENT_ROLE", "general-purpose") };
    assert!(issue_with_input(Path::new("."), std::io::Cursor::new(b"body")).is_err());
    assert!(next(Path::new(".")).is_err());
    restore("AOE_AGENT_ROLE", previous);
}

fn restore(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}
