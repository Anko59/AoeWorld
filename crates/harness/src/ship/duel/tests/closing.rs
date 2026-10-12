use super::{authored, focus, green, output, scope::Scope, tree};
use crate::ship::{
    git,
    tests::{fixture, run},
};
use std::fs;

fn write(root: &std::path::Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}

fn failed(name: &str) -> focus::CheckReport {
    focus::CheckReport {
        failed: vec![name.into()],
        ..Default::default()
    }
}

#[test]
fn mounted_and_bin_only_unit_tests_are_attributed_to_their_changed_file() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "crates/x/Cargo.toml", "[package]\nname='aoe-x'\n");
    write(
        root,
        "crates/x/src/main.rs",
        "mod ship;\n#[cfg(test)]\n#[path = \"tests/top.rs\"]\nmod top_tests;\n",
    );
    write(
        root,
        "crates/x/src/ship/mod.rs",
        "#[cfg(test)]\n#[path = \"tests/fixes.rs\"]\nmod fixes_tests;\n",
    );
    write(
        root,
        "crates/x/src/ship/tests/fixes.rs",
        "#[test]\nfn own() {}\n",
    );
    write(root, "crates/x/src/tests/top.rs", "#[test]\nfn own() {}\n");
    for (name, path, expected) in [
        (
            "aoe-x::bin/aoe-x$ship::fixes_tests::own",
            "crates/x/src/ship/tests/fixes.rs",
            true,
        ),
        (
            "aoe-x::bin/aoe-x$ship::tests::fixes::own",
            "crates/x/src/ship/tests/fixes.rs",
            false,
        ),
        (
            "aoe-x::bin/aoe-x$top_tests::own",
            "crates/x/src/tests/top.rs",
            true,
        ),
        (
            "aoe-x::bin/other$top_tests::own",
            "crates/x/src/tests/top.rs",
            false,
        ),
        (
            "aoe-x::bin/aoe-x$ship::fixes_tests::own",
            "crates/x/src/tests/top.rs",
            false,
        ),
    ] {
        let paths = [path.to_owned()];
        assert_eq!(
            authored::has_failure(root, &paths, &failed(name)).unwrap(),
            expected,
            "{name}"
        );
    }
    // A file no crate root reaches falls back to function names and says so.
    write(root, "crates/x/src/loose.rs", "#[test]\nfn own() {}\n");
    let (accepted, notes) = authored::evidence(
        root,
        &["crates/x/src/loose.rs".into()],
        &failed("aoe-x::bin/aoe-x$generated::own"),
    )
    .unwrap();
    assert!(accepted && notes[0].contains("falling back"), "{notes:?}");
}

#[test]
fn fixtures_beside_rust_tests_are_covered_by_the_package() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers = [\"crates/x\"]\n",
    );
    write(root, "crates/x/Cargo.toml", "[package]\nname='aoe-x'\n");
    write(root, "crates/x/tests/foo.rs", "#[test]\nfn a() {}\n");
    write(root, "crates/x/tests/fixtures/sample.json", "{}");
    let paths: Vec<String> = [
        "crates/x/tests/fixtures/sample.json",
        "crates/x/tests/foo.rs",
    ]
    .map(String::from)
    .into();
    assert_eq!(Scope::from_paths(root, &paths).unwrap().packages, ["aoe-x"]);
    let fixture_only = Scope::from_paths(root, &paths[..1]).unwrap();
    assert_eq!(fixture_only.packages, ["aoe-x"]);
    write(root, "fuzz/testdata/map.bin", "");
    let error = Scope::from_paths(root, &["fuzz/testdata/map.bin".into()]).unwrap_err();
    assert!(error.contains("unsupported focused test path"), "{error}");
}

#[test]
fn green_after_build_failure_requires_only_tests_added_this_round() {
    let (_temp, root) = fixture("true");
    let path = "crates/x/tests/check.rs";
    write(&root, "crates/x/Cargo.toml", "[package]\nname='aoe-x'\n");
    write(
        &root,
        path,
        "#[test]\n#[ignore]\nfn slow() {}\n#[cfg(unix)]\n#[test]\nfn gated() {}\n#[test]\nfn existing() {}\n",
    );
    run(&root, &["add", "."]);
    run(&root, &["commit", "-qm", "existing"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let source = fs::read_to_string(root.join(path)).unwrap();
    write(&root, path, &format!("{source}#[test]\nfn added() {{}}\n"));
    let build_red = focus::CheckReport {
        build_failed: true,
        ..Default::default()
    };
    let ran = |tests: &[&str]| focus::CheckReport {
        passed: true,
        tests: tests.iter().map(|t| format!("aoe-x::check${t}")).collect(),
        ..Default::default()
    };
    let paths = [path.to_owned()];
    assert!(green::validate(&root, &paths, Some(&head), &build_red, &ran(&["added"])).is_ok());
    let error = green::validate(&root, &paths, Some(&head), &build_red, &ran(&[])).unwrap_err();
    assert!(
        error.contains("check.rs::added") && !error.contains("slow"),
        "{error}"
    );
    assert!(
        !error.contains("gated") && !error.contains("existing"),
        "{error}"
    );
}

#[test]
fn tester_cannot_remove_playwright_specs_or_files() {
    let (_temp, root) = fixture("true");
    let old = "browser/tests/old.spec.ts";
    write(
        &root,
        old,
        "test('a', async () => {});\ntest('b', async () => {});\n",
    );
    run(&root, &["add", "."]);
    run(&root, &["commit", "-qm", "existing"]);
    let head = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    write(
        &root,
        "browser/tests/new.spec.ts",
        "test('c', async () => {});\n",
    );
    write(
        &root,
        old,
        "test('a', async () => {});\ntest('b', async () => {});\ntest('d', async () => {});\n",
    );
    assert!(tree::validate(&root, true, &head, "feature").is_ok());
    write(&root, old, "test('a', async () => {});\n");
    let error = tree::validate(&root, true, &head, "feature").unwrap_err();
    assert!(error.contains("old.spec.ts::'b'"), "{error}");
    assert_eq!(
        fs::read_to_string(root.join(old)).unwrap().lines().count(),
        2
    );
    fs::remove_file(root.join(old)).unwrap();
    write(
        &root,
        "browser/tests/new.spec.ts",
        "test('c', async () => {});\n",
    );
    let error = tree::validate(&root, true, &head, "feature").unwrap_err();
    assert!(error.contains("old.spec.ts::'a'"), "{error}");
}

#[test]
fn prune_keeps_only_the_newest_run_directories() {
    let temp = tempfile::tempdir().unwrap();
    let cache = temp.path();
    let now = std::time::SystemTime::now();
    for (name, age) in [("run-old", 30), ("run-mid", 20), ("run-new", 10)] {
        let directory = cache.join(name);
        fs::create_dir(&directory).unwrap();
        fs::File::open(&directory)
            .unwrap()
            .set_modified(now - std::time::Duration::from_secs(age))
            .unwrap();
    }
    fs::write(cache.join("scope.json"), "{}").unwrap();
    fs::create_dir(cache.join("baseline-x")).unwrap();
    output::prune(cache, 1).unwrap();
    assert!(cache.join("run-new").is_dir());
    assert!(!cache.join("run-old").exists() && !cache.join("run-mid").exists());
    assert!(cache.join("scope.json").is_file() && cache.join("baseline-x").is_dir());
    output::prune(cache, 0).unwrap();
    assert!(!cache.join("run-new").exists());
}

#[test]
fn a_pre_existing_failure_in_a_changed_file_is_not_red_evidence() {
    // The tester appended a passing test to a file that already held a
    // failing one: the only failure is untouched, so nothing the tester
    // wrote failed.
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(root, "crates/x/Cargo.toml", "[package]\nname='aoe-x'\n");
    write(
        root,
        "crates/x/tests/net.rs",
        "#[test]\nfn fetches() {}\n#[test]\nfn new_case() {}\n",
    );
    let paths = ["crates/x/tests/net.rs".to_owned()];
    let mut report = failed("aoe-x::net$fetches");
    assert!(authored::has_failure(root, &paths, &report).unwrap());
    report.untouched = vec!["aoe-x::net$fetches".into()];
    assert!(!authored::has_failure(root, &paths, &report).unwrap());
    // A failure the tester did write still counts beside it.
    report.failed.push("aoe-x::net$new_case".into());
    assert!(authored::has_failure(root, &paths, &report).unwrap());
}
