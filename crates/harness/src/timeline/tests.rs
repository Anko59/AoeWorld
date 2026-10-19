use super::*;
use serde_json::json;
use std::{fs, path::Path, process::Command};

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".cache/timeline-tests").unwrap();
    tempfile::tempdir_in(".cache/timeline-tests").unwrap()
}

fn repository() -> tempfile::TempDir {
    let root = temp();
    for args in [
        vec!["init", "-q", "-b", "topic/nested"],
        vec!["commit", "-q", "--allow-empty", "-m", "fixture"],
    ] {
        assert!(
            Command::new("git")
                .current_dir(root.path())
                .args(args)
                .env_remove("GIT_INDEX_FILE")
                .env_remove("GIT_DIR")
                .env_remove("GIT_WORK_TREE")
                .env("GIT_AUTHOR_NAME", "fixture")
                .env("GIT_AUTHOR_EMAIL", "fixture@example.com")
                .env("GIT_COMMITTER_NAME", "fixture")
                .env("GIT_COMMITTER_EMAIL", "fixture@example.com")
                .status()
                .unwrap()
                .success()
        );
    }
    root
}

fn event(t: f64, stage: &str, event: &str, run: &str, head: &str) -> Entry {
    serde_json::from_value(json!({"t":t,"branch":"topic/nested","head":head,
        "stage":stage,"event":event,"detail":{"run":run}}))
    .unwrap()
}

#[test]
fn journal_appends_and_failed_writes_never_fail_work() {
    let root = repository();
    mark(root.path(), "red", "start", json!({"base":"fixture"}));
    mark(root.path(), "red", "end", json!({"ok":false}));
    let path = root
        .path()
        .join(".git/aoe-ship/timeline/topic/nested.jsonl");
    let entries: Vec<Entry> = fs::read_to_string(&path)
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].branch, "topic/nested");
    assert_eq!(entries[0].head.len(), 40);
    assert!(entries[1].t >= entries[0].t);
    assert_eq!(entries[0].detail["run"], entries[1].detail["run"]);
    assert_eq!(entries[1].detail["ok"], false);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let result: std::result::Result<_, &str> = stage(root.path(), "push", json!({}), || Ok(42));
    assert_eq!(result, Ok(42));
    let result: std::result::Result<(), &str> =
        stage(root.path(), "push", json!({}), || Err("original"));
    assert_eq!(result, Err("original"));
}

#[test]
fn pairing_isolates_runs_retries_and_rebases_and_ignores_incomplete() {
    let (samples, unpaired) = pair(vec![
        event(1., "rebase", "start", "a", "old"),
        event(2., "rebase", "start", "b", "other"),
        event(5., "rebase", "end", "a", "new"),
        event(6., "rebase", "end", "b", "other"),
        event(7., "red", "end", "a", "new"),
        event(8., "red", "start", "a", "new"),
        event(9., "red", "start", "a", "new"),
        event(12., "red", "end", "a", "new"),
        event(13., "push", "start", "a", "new"),
    ]);
    assert_eq!(unpaired, 3);
    assert_eq!(samples.len(), 3);
    assert_eq!(samples[0].seconds(), 4.);
    assert_eq!(samples[0].head, "new");
    assert_eq!(samples[2].seconds(), 3.);
}

fn write_json(root: &Path, name: &str, value: serde_json::Value) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value.to_string()).unwrap();
}

#[test]
fn history_fills_gaps_without_double_counting_journal_or_reused_reviews() {
    let root = temp();
    let journal = root.path().join("timeline/topic/nested.jsonl");
    fs::create_dir_all(journal.parent().unwrap()).unwrap();
    let mut contents = String::new();
    for e in [
        event(10., "gates", "start", "a", "h"),
        event(30., "gates", "end", "a", "h"),
    ] {
        contents.push_str(&serde_json::to_string(&e).unwrap());
        contents.push('\n');
    }
    contents.push_str("{interrupted write\n");
    fs::write(&journal, contents).unwrap();
    write_json(
        root.path(),
        "evidence/h.json",
        json!({"head":"h","branch":"topic/nested",
        "started":10,"finished":30,"cadence":"preflight","verdict":"FAIL",
        "gates":[{"gate":"lint","seconds":15.0}]}),
    );
    write_json(
        root.path(),
        "evidence/older.json",
        json!({"head":"older","branch":"topic/nested",
        "started":1,"finished":5,"cadence":"preflight","verdict":"PASS",
        "gates":[{"gate":"lint","seconds":3.0}]}),
    );
    write_json(
        root.path(),
        "reviews/h-low.json",
        json!({"head":"h","branch":"topic/nested",
        "started":31,"finished":41,"tier":"low","grade":7,"closing":false}),
    );
    write_json(
        root.path(),
        "reviews/reused.json",
        json!({"head":"copy","branch":"topic/nested",
        "started":31,"finished":41,"tier":"low","grade":7,"reused_from":"h"}),
    );
    let data = history::load(root.path()).unwrap();
    assert_eq!(data.malformed, 1);
    assert_eq!(
        data.samples.iter().filter(|s| s.stage == "gates").count(),
        2
    );
    assert_eq!(
        data.samples.iter().filter(|s| s.stage == "review").count(),
        1
    );
    assert_eq!(
        data.samples
            .iter()
            .filter(|s| s.stage == "gate/lint")
            .count(),
        2
    );
    let text = report::render(&data, &[], 0., 50.);
    assert!(text.contains("gates | 2 | 12.0s | 20.0s | 0.007"), "{text}");
    assert!(text.contains("low") && text.contains("7"), "{text}");
    assert!(text.contains("Top 5 time sinks"));
    assert!(!report::render(&data, &[], 42., 50.).contains("gates |"));
}

#[test]
fn github_join_uses_push_head_and_never_invents_historical_push_time() {
    let (samples, _) = pair(vec![
        event(10., "push", "start", "a", "h"),
        event(12., "push", "end", "a", "h"),
    ]);
    let data = history::Data {
        samples,
        ..Default::default()
    };
    let prs: Vec<github::Pr> = serde_json::from_value(
        json!([{"number":42,"headRefName":"topic/nested","headRefOid":"h",
        "createdAt":15,"mergedAt":100,"runs":[{"databaseId":1,"name":"CI","headSha":"h",
        "startedAt":20,"updatedAt":80,"status":"completed","conclusion":"failure"},
        {"databaseId":2,"name":"CI","headSha":"h","startedAt":81,"updatedAt":95,
        "status":"completed","conclusion":"success"},
        {"databaseId":3,"name":"CI","headSha":"h","startedAt":101,"updatedAt":120,
        "status":"completed","conclusion":"success"}]}]),
    )
    .unwrap();
    let text = report::render(&data, &prs, 0., 200.);
    assert!(text.contains("PR #42"), "{text}");
    assert!(text.contains("push-to-merge 88.0s"), "{text}");
    assert!(
        text.contains("CI 60.0s") && text.contains("failure"),
        "{text}"
    );
    let text = report::render(&history::Data::default(), &prs, 0., 200.);
    assert!(
        text.contains("created-to-merge 85.0s (push unavailable)"),
        "{text}"
    );
    assert!(!text.contains("push-to-merge |"), "{text}");
    assert!(
        text.contains("CI 19.0s") && text.contains("after merge; excluded"),
        "{text}"
    );
    let windowed = report::render(&data, &prs, 90., 200.);
    assert!(
        windowed.contains("CI | 1 | 14.0s | 14.0s | 0.004"),
        "{windowed}"
    );
}

#[test]
fn history_counts_each_recorded_retry_without_counting_final_result_twice() {
    let root = temp();
    write_json(
        root.path(),
        "evidence/retry.json",
        json!({
            "head":"retry", "branch":"topic/nested", "started":1, "finished":61,
            "cadence":"preflight", "verdict":"PASS", "gates":[{
                "gate":"lint", "seconds":30.0, "attempts":[{"seconds":10.0},{"seconds":30.0}]
            }]
        }),
    );
    let data = history::load(root.path()).unwrap();
    let text = report::render(&data, &[], 0., 100.);
    assert!(
        text.contains("gate/lint | 2 | 20.0s | 30.0s | 0.011"),
        "{text}"
    );
    assert!(text.contains("gates | 1 | 60.0s | 60.0s | 0.017"), "{text}");
}
