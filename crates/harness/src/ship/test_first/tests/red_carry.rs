//! Real Git rebases; only the red/green runner is injected.
use super::{commit, repo};
use crate::ship::{duel::focus::CheckReport, git, test_first::red, tests::run};
use std::{fs, path::Path};

const SPEC: &str = "browser/tests/carry.spec.ts";
const PRODUCT: &str = "web/index.html";
const NAME: &str = "carry.spec.ts::new behavior [webgpu]";
const CONTEXT: &str =
    "// base\n// 2\n// 3\n// 4\n// 5\n// 6\n// 7\n// 8\n// 9\n// 10\n// 11\n// 12\n";

fn tip(root: &Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn prove(root: &Path, base: &str) -> red::Record {
    red::run_with(root, base, &tip(root), &mut |snapshot, _| {
        let has_test = fs::read_to_string(snapshot.join(SPEC))
            .unwrap()
            .contains("new behavior");
        let green = !has_test || fs::read_to_string(snapshot.join(PRODUCT)).unwrap() == "after\n";
        Ok(CheckReport {
            passed: green,
            tests: if has_test { vec![NAME.into()] } else { vec![] },
            failed: if green { vec![] } else { vec![NAME.into()] },
            locations: [(NAME.into(), (SPEC.into(), CONTEXT.lines().count() + 1))].into(),
            ..Default::default()
        })
    })
    .unwrap()
}

fn proof() -> (tempfile::TempDir, String, String) {
    let (temp, _) = repo();
    let root = temp.path();
    commit(
        root,
        &[(SPEC, CONTEXT), (PRODUCT, "before\n")],
        "base inputs",
    );
    let base = tip(root);
    commit(
        root,
        &[
            (
                SPEC,
                &format!("{CONTEXT}test('new behavior', () => {{}});\n"),
            ),
            (PRODUCT, "after\n"),
        ],
        "feature",
    );
    prove(root, &base);
    let head = tip(root);
    (temp, base, head)
}

fn rebase(root: &Path, base: &str, changes: &[(&str, &str)]) -> String {
    run(root, &["checkout", "-q", "-b", "incoming", base]);
    commit(root, changes, "dev advances");
    let next_base = tip(root);
    run(root, &["checkout", "-q", "feature"]);
    run(root, &["rebase", "incoming"]);
    next_base
}

fn record_path(root: &Path, head: &str) -> std::path::PathBuf {
    root.join(git::git(root, &["rev-parse", "--git-path", "aoe-ship/red"]).unwrap())
        .join(format!("{head}.json"))
}

#[test]
fn red_carry_after_clean_rebase_reaches_head_and_review_facts() {
    let (temp, base, old) = proof();
    let root = temp.path();
    let bytes = fs::read(record_path(root, &old)).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let _scope = crate::review::base::scope_at("dev", &base);
    assert_eq!(
        saved["change_fingerprint"],
        git::change_fingerprint(root, "dev", &old).unwrap().1
    );
    let base = rebase(root, &base, &[("README.md", "dev moved\n")]);
    let head = tip(root);
    assert_ne!(head, old);
    assert!(!record_path(root, &head).exists());
    let expected =
        format!("test-first red: PASS (carried from {old}; patch unchanged after rebase)");
    let decision = red::require(root, &base, &head).unwrap();
    assert!(decision.contains(&expected), "{decision}");
    assert_eq!(red::require(root, &base, "HEAD").unwrap(), decision);
    let facts = crate::review::facts(root, &base).unwrap();
    assert!(facts.contains(&expected), "{facts}");
    let linked = tempfile::tempdir().unwrap();
    run(
        root,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.path().to_str().unwrap(),
            &head,
        ],
    );
    assert_eq!(
        red::require(linked.path(), &base, "HEAD").unwrap(),
        decision
    );
    assert_eq!(fs::read(record_path(root, &old)).unwrap(), bytes);
    assert!(
        !record_path(root, &head).exists(),
        "carry does not manufacture a fresh run"
    );
    println!("{decision}");
}

#[test]
fn red_carry_refuses_edited_tests_or_product_after_rebase() {
    for (path, text) in [
        (SPEC, "test('new behavior', () => { changed(); });\n"),
        (PRODUCT, "another implementation\n"),
    ] {
        let (temp, base, _) = proof();
        let root = temp.path();
        let base = rebase(root, &base, &[("README.md", "dev moved\n")]);
        commit(root, &[(path, text)], "edit after proof");
        let error = red::require(root, &base, "HEAD").unwrap_err();
        assert!(
            error.contains("missing matching red record"),
            "{path}: {error}"
        );
    }
}

#[test]
fn red_carry_refuses_another_pr_with_the_same_tests_and_different_product() {
    let (temp, base, old) = proof();
    let root = temp.path();
    run(root, &["checkout", "-q", "-b", "other-pr", &base]);
    run(root, &["checkout", &old, "--", SPEC]);
    commit(root, &[(PRODUCT, "different fix\n")], "another PR");
    assert_eq!(
        git::git(root, &["rev-parse", &format!("{old}:{SPEC}")]).unwrap(),
        git::git(root, &["rev-parse", &format!("HEAD:{SPEC}")]).unwrap()
    );
    assert!(red::require(root, &base, "HEAD").is_err());
}

#[test]
fn red_carry_refuses_changed_test_blobs_even_when_patch_fingerprint_matches() {
    let (temp, base, old) = proof();
    let root = temp.path();
    let base = rebase(
        root,
        &base,
        &[(SPEC, &CONTEXT.replace("// base", "// upstream"))],
    );
    let _scope = crate::review::base::scope_at("dev", &base);
    let current = git::change_fingerprint(root, "dev", "HEAD").unwrap().1;
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(record_path(root, &old)).unwrap()).unwrap();
    assert_eq!(saved["change_fingerprint"], current);
    assert!(
        red::require(root, &base, "HEAD").is_err(),
        "test blob changed in upstream context"
    );
}

#[test]
fn red_carry_ignores_corrupt_sources_and_refuses_an_invalid_exact_record() {
    for exact in [false, true] {
        let (temp, base, old) = proof();
        let root = temp.path();
        let base = rebase(root, &base, &[("README.md", "dev moved\n")]);
        let head = tip(root);
        let path = record_path(root, if exact { &head } else { &old });
        fs::write(path, b"not a red record").unwrap();
        assert!(red::require(root, &base, "HEAD").is_err());
    }
}

#[test]
fn red_carry_revalidates_indexed_records_instead_of_trusting_the_fingerprint() {
    for field in [
        "base",
        "change_fingerprint",
        "paths",
        "blobs",
        "failed",
        "base_failed",
    ] {
        let (temp, base, old) = proof();
        let root = temp.path();
        let base = rebase(root, &base, &[("README.md", "dev moved\n")]);
        let path = record_path(root, &old);
        let mut saved: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        saved[field] = match field {
            "base" => serde_json::json!(old),
            "change_fingerprint" => serde_json::json!("0".repeat(64)),
            "base_failed" => serde_json::json!([NAME]),
            _ => serde_json::json!([]),
        };
        fs::write(path, serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(red::require(root, &base, "HEAD").is_err(), "{field}");
    }
}
