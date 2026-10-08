use super::super::{Ask, Plan, Report, Tier, review_with};
use super::flow::{fixture, git};
use crate::agents::Runtime;
use std::{fs, path::Path, process::Command, sync::Mutex};

fn rev(root: &Path, name: &str) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", name])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// Moves origin/dev to `moved` when the first session starts, as a fetch
/// from another worktree would, and records every prompt it is asked.
struct Fetching<'a> {
    root: &'a Path,
    moved: String,
    prompts: Mutex<Vec<String>>,
}

impl Ask for Fetching<'_> {
    fn all(&self, prompts: Vec<String>) -> Vec<Result<String, String>> {
        git(
            self.root,
            &["update-ref", "refs/remotes/origin/dev", &self.moved],
        );
        let answers = prompts
            .iter()
            .map(|p| {
                Ok(if p.contains("# Grader") {
                    r#"{"grade": 9, "summary": "Fine."}"#.into()
                } else {
                    r#"{"findings": []}"#.into()
                })
            })
            .collect();
        self.prompts.lock().unwrap().extend(prompts);
        answers
    }
}

#[test]
fn origin_dev_moving_during_a_review_changes_neither_prompts_nor_fingerprint() {
    let temp = fixture("README.md");
    let root = temp.path();
    let started = rev(root, "refs/remotes/origin/dev");
    // A dev update that changes the grader prompt and the reviewer model.
    git(root, &["checkout", "-q", "dev"]);
    let grader = root.join("gates/review/grader.md");
    let text = fs::read_to_string(&grader).unwrap();
    fs::write(&grader, format!("{text}\nMOVED-GRADER-MARKER\n")).unwrap();
    let path = root.join("gates/review.json");
    let mut config: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    config["models"]["claude"]["average"]["model"] = serde_json::json!("claude-moved-model");
    fs::write(&path, config.to_string()).unwrap();
    git(root, &["commit", "-q", "-am", "move the review policy"]);
    let moved = rev(root, "HEAD");
    git(root, &["checkout", "-q", "feature"]);

    let ask = Fetching {
        root,
        moved: moved.clone(),
        prompts: Mutex::new(Vec::new()),
    };
    let report = review_with(root, Tier::Low, Runtime::Claude, "t", &Plan::Full, &ask).unwrap();
    assert_eq!(rev(root, "refs/remotes/origin/dev"), moved, "dev moved");
    let prompts = ask.prompts.lock().unwrap();
    assert!(prompts.iter().any(|p| p.contains("# Grader")));
    assert!(
        prompts.iter().all(|p| !p.contains("MOVED-GRADER-MARKER")),
        "every prompt comes from the commit the review started from"
    );
    assert_ne!(report.model, "claude-moved-model");
    let at_start = Report::policy_fingerprint_at(root, &started, "low").unwrap();
    assert_ne!(
        at_start,
        Report::policy_fingerprint_at(root, &moved, "low").unwrap()
    );
    assert_eq!(
        report.policy_fingerprint.as_deref(),
        Some(at_start.as_str()),
        "the stored fingerprint is the policy the review ran under"
    );
}
