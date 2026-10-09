use super::super::{Ask, Plan, Report, Tier, entry::Policy, review_with, review_with_branch};
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

/// Commits a dev update that changes the grader prompt and the reviewer
/// model, without moving origin/dev; returns origin/dev and that commit.
fn moved_policy(root: &Path) -> (String, String) {
    let started = rev(root, "refs/remotes/origin/dev");
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
    (started, moved)
}

/// Reviews under `policy` and checks that the model, every prompt and the
/// stored fingerprint come from `pinned`, not from `moved`.
fn assert_reviewed_under(root: &Path, policy: &Policy, pinned: &str, moved: &str) {
    assert_eq!(policy.commit, pinned);
    assert_ne!(
        policy.model.model, "claude-moved-model",
        "model from the pin"
    );
    let ask = Fetching {
        root,
        moved: rev(root, "refs/remotes/origin/dev"),
        prompts: Mutex::new(Vec::new()),
    };
    let branch = "feature";
    let report =
        review_with_branch(root, branch, Tier::Low, policy, "t", &Plan::Full, &ask).unwrap();
    let prompts = ask.prompts.lock().unwrap();
    assert!(prompts.iter().any(|p| p.contains("# Grader")));
    assert!(
        prompts.iter().all(|p| !p.contains("MOVED-GRADER-MARKER")),
        "every prompt comes from the pinned commit"
    );
    assert_eq!(report.model, policy.model.model, "the model that ran");
    let at_pin = Report::policy_fingerprint_at(root, pinned, "low").unwrap();
    assert_ne!(
        at_pin,
        Report::policy_fingerprint_at(root, moved, "low").unwrap()
    );
    assert_eq!(
        report.policy_fingerprint.as_deref(),
        Some(at_pin.as_str()),
        "the stored fingerprint is the pinned commit's"
    );
}

#[test]
fn origin_dev_moving_after_the_pin_changes_neither_model_prompts_nor_fingerprint() {
    let temp = fixture("README.md");
    let root = temp.path();
    let (started, moved) = moved_policy(root);
    let policy = Policy::pin(root, None, Tier::Low, Runtime::Claude, &Plan::Full).unwrap();
    // A fetch from another worktree between the pin and the first prompt.
    git(root, &["update-ref", "refs/remotes/origin/dev", &moved]);
    assert_reviewed_under(root, &policy, &started, &moved);
}

#[test]
fn the_judge_revision_is_pinned_rather_than_a_later_origin_dev() {
    let temp = fixture("README.md");
    let root = temp.path();
    let (started, moved) = moved_policy(root);
    // origin/dev moved after the judge binary was chosen at `started`.
    git(root, &["update-ref", "refs/remotes/origin/dev", &moved]);
    let judge = Some(started.as_str());
    let policy = Policy::pin(root, judge, Tier::Low, Runtime::Claude, &Plan::Full).unwrap();
    assert_reviewed_under(root, &policy, &started, &moved);
}

#[test]
fn a_judge_revision_off_origin_dev_is_refused() {
    let temp = fixture("README.md");
    let root = temp.path();
    let feature = rev(root, "HEAD");
    for judge in [feature.as_str(), "no-such-revision"] {
        let refused = Policy::pin(root, Some(judge), Tier::Low, Runtime::Claude, &Plan::Full);
        assert!(refused.is_err(), "{judge} must not be the review policy");
    }
}

#[test]
fn origin_dev_moving_during_a_review_changes_neither_prompts_nor_fingerprint() {
    let temp = fixture("README.md");
    let root = temp.path();
    let (started, moved) = moved_policy(root);

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
