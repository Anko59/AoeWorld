use super::{fixture, run, unstored};
use crate::review::{Report, Tier};
use crate::ship::git;
use std::path::Path;

fn advance_dev_with_policy(root: &Path, update: impl FnOnce(&mut serde_json::Value)) {
    run(root, &["checkout", "-q", "dev"]);
    let path = root.join("gates/review.json");
    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    update(&mut config);
    std::fs::write(path, config.to_string()).unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(root, &["add", "gates/review.json", "BASE.md"]);
    run(root, &["commit", "-q", "-m", "change review policy"]);
    run(root, &["push", "-q", "origin", "dev"]);
    run(root, &["fetch", "-q", "origin", "dev"]);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "-q", "origin/dev"]);
}

#[test]
fn reuse_requires_the_current_review_config_fingerprint() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |config| {
        config["tiers"]["low"]["personas"] = serde_json::json!(["quick", "test-integrity"]);
    });
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );

    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    std::fs::write(
        root.join("gates/review/personas/quick.md"),
        "# Updated quick reviewer\n",
    )
    .unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(&root, &["add", "gates/review/personas/quick.md", "BASE.md"]);
    run(&root, &["commit", "-q", "-m", "change reviewer prompt"]);
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_rust_prompt_change_on_origin_dev_prevents_reuse() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    run(&root, &["checkout", "-q", "dev"]);
    let prompt = root.join("crates/harness/src/review/prompt.rs");
    let text = std::fs::read_to_string(&prompt).unwrap();
    std::fs::write(
        &prompt,
        format!("{text}\n// changed reviewer instruction\n"),
    )
    .unwrap();
    std::fs::write(root.join("BASE.md"), "moved base\n").unwrap();
    run(
        &root,
        &["add", "crates/harness/src/review/prompt.rs", "BASE.md"],
    );
    run(
        &root,
        &["commit", "-q", "-m", "change Rust reviewer prompt"],
    );
    run(&root, &["push", "-q", "origin", "dev"]);
    run(&root, &["fetch", "-q", "origin", "dev"]);
    run(&root, &["checkout", "-q", "feature"]);
    run(&root, &["rebase", "-q", "origin/dev"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none(),
        "a changed Rust prompt means the review engine fingerprint changed"
    );
}

#[test]
fn reuse_requires_the_current_runtime_model_policy() {
    let (_temp, root) = fixture("true");
    let source = unstored(&root, Tier::Low, 9);
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |config| {
        config["models"]["claude"]["average"]["model"] = serde_json::json!("claude-next-model");
    });
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_report_without_a_policy_fingerprint_is_never_reused() {
    let (_temp, root) = fixture("true");
    let mut source = unstored(&root, Tier::Low, 9);
    source.policy_fingerprint = None;
    source.store(&root).unwrap();
    advance_dev_with_policy(&root, |_| {});
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    assert!(
        Report::reuse_for_change(&root, &head, "feature", &["low"])
            .unwrap()
            .is_none()
    );
}
