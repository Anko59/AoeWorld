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

#[test]
fn window_joins_retained_final_push_before_filtering_completed_intervals() {
    // Hours since October 17 00:00: push Oct 17 23:00, merge Oct 19 10:00.
    let (samples, _) = pair(vec![
        event(22. * 3600., "push", "start", "a", "final"),
        event(23. * 3600., "push", "end", "a", "final"),
        event(57. * 3600., "red", "start", "a", "final"),
        event(58. * 3600., "red", "end", "a", "final"),
    ]);
    let data = history::Data {
        samples,
        ..Default::default()
    };
    let prs: Vec<github::Pr> = serde_json::from_value(json!([{
        "number":42,"headRefName":"topic/nested","headRefOid":"final",
        "createdAt":22. * 3600.,"mergedAt":58. * 3600.
    }]))
    .unwrap();
    let text = report::render(&data, &prs, 36. * 3600., 60. * 3600.);
    assert!(text.contains("push-to-merge 126000.0s"), "{text}");
    assert!(
        text.contains("push-to-merge | 1 | 126000.0s | 126000.0s | 35.000"),
        "{text}"
    );
    assert!(!text.contains("push unavailable"), "{text}");
    assert!(
        !text.contains("push |"),
        "old local stage stays outside aggregate: {text}"
    );
    assert!(
        !text
            .split("Local stages not joined")
            .nth(1)
            .unwrap()
            .contains("red 3600.0s"),
        "{text}"
    );
    assert!(!report::render(&data, &prs, 59. * 3600., 60. * 3600.).contains("push-to-merge |"));
}

#[test]
fn history_retry_rows_keep_each_attempts_own_verdict() {
    let root = temp();
    write_json(
        root.path(),
        "evidence/retry.json",
        json!({
            "head":"retry", "branch":"topic/nested", "started":1, "finished":61,
            "cadence":"preflight", "verdict":"PASS", "gates":[{
                "gate":"lint", "seconds":30., "verdict":"PASS", "attempts":[
                    {"seconds":10.,"verdict":"FAIL"},{"seconds":30.,"verdict":"PASS"}
                ]
            }, {"gate":"fmt-check","seconds":1.,"verdict":"PASS"}]
        }),
    );
    let text = report::render(&history::load(root.path()).unwrap(), &[], 0., 100.);
    assert!(
        text.contains("gate/lint 10.0s [retry; history, verdict=\"FAIL\"]"),
        "{text}"
    );
    assert!(
        text.contains("gate/lint 30.0s [retry; history, verdict=\"PASS\"]"),
        "{text}"
    );
    assert!(
        text.contains("gate/fmt-check 1.0s [retry; history, verdict=\"PASS\"]"),
        "{text}"
    );
}

#[test]
fn github_query_keeps_ci_created_before_the_completion_window() {
    let root = repository();
    assert!(
        Command::new("git")
            .current_dir(root.path())
            .args([
                "remote",
                "add",
                "origin",
                "https://github.com/example/repository.git"
            ])
            .status()
            .unwrap()
            .success()
    );
    // Window Oct 12 00:05 .. Oct 19 00:05; CI Oct 11 23:50 .. Oct 12 00:10.
    let since = 1_791_763_500.;
    let (prs, warnings) = github::read_with(root.path(), since, &|args| {
        Ok(if args[..2] == ["pr", "list"] {
            json!([{"number":42,"headRefName":"topic/nested","headRefOid":"h",
                "createdAt":since-1800.,"mergedAt":since+360.}])
            .to_string()
        } else {
            assert_eq!(&args[..2], ["run", "list"]);
            // GitHub's creation-date filter would discard this completed run.
            if args.contains(&"--created") {
                "[]".into()
            } else {
                json!([{"databaseId":1,"name":"CI","headSha":"h",
                    "startedAt":since-900.,"updatedAt":since+300.,
                    "status":"completed","conclusion":"success"}])
                .to_string()
            }
        })
    });
    assert!(warnings.is_empty(), "{warnings:?}");
    let text = report::render(&history::Data::default(), &prs, since, since + 7. * 86400.);
    assert!(
        text.contains("CI | 1 | 1200.0s | 1200.0s | 0.333"),
        "{text}"
    );
}

#[test]
fn pairing_keeps_incomplete_review_rounds_separate_and_displays_round_metadata() {
    let mut entries = vec![
        event(1., "review-round", "start", "a", "h"),
        event(5., "review-round", "start", "a", "h"),
        event(8., "review-round", "end", "a", "h"),
        event(10., "review-round", "end", "a", "h"),
    ];
    for (e, round) in entries.iter_mut().zip([1, 2, 2, 1]) {
        e.detail["round"] = json!(round);
        e.detail["kind"] = json!(if round == 1 { "blind" } else { "cross" });
        e.detail["tier"] = json!("high");
    }
    let (samples, unpaired) = pair(entries);
    assert_eq!(unpaired, 0);
    assert_eq!(samples.len(), 2);
    assert_eq!(samples[0].seconds(), 9.);
    assert_eq!(samples[1].seconds(), 3.);
    let text = report::render(
        &history::Data {
            samples,
            ..Default::default()
        },
        &[],
        0.,
        20.,
    );
    assert!(text.contains("round=1, kind=\"blind\""), "{text}");
    assert!(text.contains("round=2, kind=\"cross\""), "{text}");
}
