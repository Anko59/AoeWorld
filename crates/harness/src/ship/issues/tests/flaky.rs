use super::*;
use std::io::Write;

fn artifact(id: u64, name: &str, created: &str, expired: bool) -> Artifact {
    Artifact {
        id,
        name: name.to_owned(),
        expired,
        created_at: created.to_owned(),
        workflow_run: Some(Run { id: id * 10 }),
    }
}

fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, text) in entries {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(text.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn line(test: &str, project: &str, error: &str) -> String {
    serde_json::json!({"test": test, "project": project, "error": error, "attempts": 2,
        "shard": "4/4", "sha": "abc", "run_id": "1", "run_attempt": "1"})
    .to_string()
}

#[test]
fn only_unexpired_flaky_record_artifacts_of_the_last_seven_days_count() {
    let today = date("2026-11-26").unwrap();
    let all = [
        artifact(1, "e2e-flaky-1", "2026-11-26T01:00:00Z", false),
        artifact(2, "e2e-flaky-4", "2026-11-19T23:59:59Z", false),
        artifact(3, "e2e-flaky-2", "2026-11-18T23:59:59Z", false),
        artifact(4, "e2e-flaky-3", "2026-11-25T01:00:00Z", true),
        artifact(5, "e2e-4-diagnostics", "2026-11-25T01:00:00Z", false),
        artifact(6, "e2e-flaky-1", "garbage", false),
    ];
    let ids: Vec<u64> = recent(&all, today).iter().map(|a| a.id).collect();
    assert_eq!(ids, [1, 2]);
}

#[test]
fn flags_are_read_from_every_flaky_jsonl_entry_of_the_zip() {
    let text = format!(
        "{}\n\n{}\n",
        line("far-build far construction", "webgpu", "expect(a)"),
        line("memory dense forest", "webgpu", "expect(b)")
    );
    let zip = zip_of(&[
        ("playwright-shard-4-of-4.flaky.jsonl", &text),
        ("playwright-shard-4-of-4.json", "{not read}"),
    ]);
    let found = flags(
        &artifact(7, "e2e-flaky-4", "2026-11-25T01:00:00Z", false),
        &zip,
    )
    .unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].test, "far-build far construction");
    assert_eq!(found[0].artifact, 7);
    assert_eq!(found[0].run, Some(70));
    assert!(flags(&artifact(7, "x", "y", false), b"not a zip").is_err());
}

fn flag(test: &str, project: &str, artifact: u64) -> Flag {
    Flag {
        test: test.to_owned(),
        project: project.to_owned(),
        error: format!("error {artifact}"),
        artifact,
        run: Some(artifact * 10),
    }
}

#[test]
fn a_test_is_reported_at_three_flags_in_three_artifacts_not_at_two() {
    let two = [flag("a", "webgpu", 1), flag("a", "webgpu", 2)];
    assert!(repeated(&two).is_empty());
    // The same artifact twice is one flag.
    let same = [
        flag("a", "webgpu", 1),
        flag("a", "webgpu", 1),
        flag("a", "webgpu", 2),
    ];
    assert!(repeated(&same).is_empty());
    let three = [
        flag("a", "webgpu", 1),
        flag("a", "webgpu", 2),
        flag("a", "webgpu", 3),
        flag("b", "webgpu", 1),
        // Another project of the same test is another test.
        flag("a", "canvas", 4),
    ];
    let found = repeated(&three);
    assert_eq!(found.len(), 1);
    assert_eq!(
        (found[0].test.as_str(), found[0].project.as_str()),
        ("a", "webgpu")
    );
    assert_eq!(found[0].runs, [Some(10), Some(20), Some(30)]);
}

#[test]
fn the_issue_title_is_stable_so_a_later_night_comments_instead_of_opening_another() {
    let repeat = |errors: &[&str]| Repeat {
        test: "far-build far construction".into(),
        project: "webgpu".into(),
        runs: vec![Some(10), Some(20), None],
        errors: errors.iter().map(|e| (*e).to_owned()).collect(),
    };
    let (title, body, note) = text(&repeat(&["expect `x` \u{1b}[0m"]), "github.com/o/r", 3);
    let (later, ..) = text(&repeat(&["other"]), "github.com/o/r", 5);
    assert_eq!(title, "Flaky e2e test: far-build far construction [webgpu]");
    assert_eq!(title, later);
    assert!(
        body.contains("Flagged flaky 3 times in the last 7 days"),
        "{body}"
    );
    assert!(
        body.contains("https://github.com/o/r/actions/runs/10"),
        "{body}"
    );
    assert!(body.contains("e2e-repeat"), "{body}");
    assert!(!body.contains('\u{1b}'), "control characters are dropped");
    assert!(note.starts_with("Flagged again."));
}
