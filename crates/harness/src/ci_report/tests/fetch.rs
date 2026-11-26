use crate::ci_report::fetch::{comments, file_history};
use crate::ci_report::series::Commit;
use serde_json::json;
use std::{fs, sync::Mutex};

const MARKED: &str = "<!-- aoe-perf-comment -->\nbody";

fn line(id: u64, pr: u64, updated: &str, bot: bool, body: Option<&str>) -> String {
    json!({
        "id": id,
        "issue_url": format!("https://api.github.com/repos/o/r/issues/{pr}"),
        "created_at": "2026-11-01T00:00:00Z",
        "updated_at": updated,
        "bot": bot,
        "body": body,
    })
    .to_string()
}

/// A scripted `gh`: records the arguments of each call and answers with the next reply.
struct Script {
    calls: Mutex<Vec<String>>,
    replies: Mutex<Vec<Result<String, String>>>,
}

impl Script {
    fn new(replies: Vec<Result<String, String>>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            replies: Mutex::new(replies.into_iter().rev().collect()),
        }
    }

    fn ask(&self, args: &[&str]) -> Result<String, String> {
        self.calls.lock().unwrap().push(args.join(" "));
        self.replies
            .lock()
            .unwrap()
            .pop()
            .expect("an unexpected extra call")
    }

    fn call(&self, index: usize) -> String {
        self.calls.lock().unwrap()[index].clone()
    }
}

#[test]
fn the_first_fetch_lists_everything_keeps_only_the_bots_marked_comments_and_caches_them() {
    let dir = tempfile::tempdir().unwrap();
    let reply = [
        line(1, 405, "2026-11-01T10:00:00Z", true, Some(MARKED)),
        line(2, 406, "2026-11-01T11:00:00Z", false, None),
        line(3, 407, "2026-11-01T12:00:00Z", false, Some(MARKED)),
    ]
    .join("\n");
    let script = Script::new(vec![Ok(reply)]);
    let found = comments(dir.path(), "o/r", &|args| script.ask(args)).unwrap();
    assert_eq!(
        found.len(),
        1,
        "a quoted marker from a user is not the bot's comment"
    );
    assert_eq!((found[0].id, found[0].pr), (1, 405));
    let call = script.call(0);
    assert!(
        call.contains("repos/o/r/issues/comments?per_page=100"),
        "{call}"
    );
    assert!(!call.contains("since="), "{call}");
    assert!(dir.path().join("comments.jsonl").exists());
}

#[test]
fn a_second_fetch_asks_only_for_what_changed_since_the_last_one_and_merges() {
    let dir = tempfile::tempdir().unwrap();
    let first = [
        line(1, 405, "2026-11-01T10:00:00Z", true, Some(MARKED)),
        line(2, 406, "2026-11-02T09:00:00Z", false, None),
    ]
    .join("\n");
    let second = [
        // The cursor is inclusive, so the last comment comes again: it is not a new version.
        line(2, 406, "2026-11-02T09:00:00Z", false, None),
        line(1, 405, "2026-11-03T08:00:00Z", true, Some(MARKED)),
        line(9, 410, "2026-11-03T09:00:00Z", true, Some(MARKED)),
    ]
    .join("\n");
    let script = Script::new(vec![Ok(first), Ok(second)]);
    let gh = |args: &[&str]| script.ask(args);
    assert_eq!(comments(dir.path(), "o/r", &gh).unwrap().len(), 1);
    let found = comments(dir.path(), "o/r", &gh).unwrap();
    let versions: Vec<_> = found.iter().map(|c| (c.id, c.updated.as_str())).collect();
    assert_eq!(
        versions,
        [
            (1, "2026-11-01T10:00:00Z"),
            (1, "2026-11-03T08:00:00Z"),
            (9, "2026-11-03T09:00:00Z"),
        ],
        "an edited comment keeps its earlier version: its dev counts are still true"
    );
    let second_call = script.call(1);
    assert!(
        second_call.contains("since=2026-11-02T09:00:00Z"),
        "{second_call}"
    );
    let third = Script::new(vec![Ok(String::new())]);
    let again = comments(dir.path(), "o/r", &|args| third.ask(args)).unwrap();
    assert_eq!(again.len(), 3, "an empty reply changes nothing");
    assert!(third.call(0).contains("since=2026-11-03T09:00:00Z"));
}

#[test]
fn an_unreadable_cache_is_ignored_and_a_failed_listing_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("comments.jsonl"), "not json\n").unwrap();
    fs::write(dir.path().join("comments.cursor"), "2026-11-01T00:00:00Z").unwrap();
    let script = Script::new(vec![Ok(line(
        1,
        5,
        "2026-11-04T00:00:00Z",
        true,
        Some(MARKED),
    ))]);
    let found = comments(dir.path(), "o/r", &|args| script.ask(args)).unwrap();
    assert_eq!(found.len(), 1);
    assert!(
        !script.call(0).contains("since="),
        "a broken cache starts over"
    );
    let script = Script::new(vec![Err("gh: HTTP 401".to_owned())]);
    let error = comments(dir.path(), "o/r", &|args| script.ask(args)).unwrap_err();
    assert!(error.contains("401"), "{error}");
}

#[test]
fn a_file_history_pairs_each_commit_that_touched_it_with_its_text_oldest_first() {
    let commit = |i: usize| Commit {
        sha: format!("{i:040x}"),
        at: String::new(),
        subject: String::new(),
    };
    let commits: Vec<_> = (0..6).map(commit).collect();
    let git = |args: &[&str]| -> Result<String, String> {
        match args[0] {
            "log" => Ok(format!("{:040x}\n{:040x}\n{}\n", 4, 1, "f".repeat(40))),
            "show" => Ok(format!(
                "text of {}",
                &args[1][..args[1].find(':').unwrap()][36..]
            )),
            other => Err(other.to_owned()),
        }
    };
    let found = file_history(&git, &commits, "baselines/perf/micro.json").unwrap();
    assert_eq!(
        found,
        vec![
            (1, "text of 0001".to_owned()),
            (4, "text of 0004".to_owned())
        ],
        "a commit outside the dev log is skipped"
    );
}
