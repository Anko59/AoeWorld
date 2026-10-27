use super::*;
use std::{collections::BTreeSet, process::Command};

#[test]
fn reports_missing_groups_and_excludes_test_only_coverage() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let file = temp.path().join("crates/protocol/src/lib.rs");
    fs::create_dir_all(file.parent().expect("parent")).expect("directory");
    fs::write(
        &file,
        "pub fn run() {}\n#[cfg(test)]\nmod tests { fn covered() {} }\n",
    )
    .expect("source");
    let lcov = format!("SF:{}\nDA:1,0\nDA:3,1\nend_of_record\n", file.display());
    let report = parse(
        temp.path(),
        &lcov,
        &SourceInventory::from([
            ("crates/protocol/src/lib.rs".to_owned(), BTreeSet::from([1])),
            ("crates/core/src/lib.rs".to_owned(), BTreeSet::from([1])),
        ]),
    )
    .expect("report");
    assert_eq!(report.protocol.measurable, 1);
    assert_eq!(report.protocol.covered, 0);
    assert_eq!(report.missing_sources, ["crates/core/src/lib.rs"]);
}

#[test]
fn coverage_report_inventories_sources_and_rejects_duplicate_records() {
    let temp = tempfile::tempdir().expect("repository");
    let root = temp.path();
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .expect("git init");
    assert!(status.success());
    let names = [
        "crates/harness/src/qa.rs",
        "crates/protocol/src/lib.rs",
        "crates/assets/src/drs.rs",
        "crates/core/src/lib.rs",
    ];
    fs::create_dir_all(root.join("crates/harness/src")).expect("harness directory");
    fs::write(root.join("crates/harness/src/main.rs"), "mod qa;\n").expect("harness root");
    fs::create_dir_all(root.join("crates/assets/src")).expect("assets directory");
    fs::write(root.join("crates/assets/src/lib.rs"), "pub mod drs;\n").expect("assets root");
    let mut lcov = String::new();
    for name in names {
        let source = root.join(name);
        fs::create_dir_all(source.parent().expect("parent")).expect("source directory");
        fs::write(&source, "pub fn covered() {}\n").expect("source");
        lcov.push_str(&format!("SF:{}\nDA:1,1\nend_of_record\n", source.display()));
    }
    let excluded = root.join("crates/client/src/lib.rs");
    fs::create_dir_all(excluded.parent().expect("parent")).expect("client directory");
    fs::write(excluded, "pub fn browser() {}\n").expect("client source");
    assert_eq!(
        expected_sources(root).expect("inventory").len(),
        names.len()
    );
    let report_path = root.join("native.lcov");
    fs::write(&report_path, &lcov).expect("LCOV");
    check_at(root, &report_path).expect("complete report");
    let summary: serde_json::Value = serde_json::from_slice(
        &fs::read(root.join("reports/coverage/summary.json")).expect("summary"),
    )
    .expect("JSON");
    assert_eq!(summary["missing_sources"], serde_json::json!([]));
    let duplicate = lcov.replace("DA:1,1\n", "DA:1,1\nDA:1,1\n");
    fs::write(&report_path, duplicate).expect("duplicate LCOV");
    assert!(check_at(root, &report_path).is_err());
    fs::write(
        &report_path,
        lcov.lines().skip(3).collect::<Vec<_>>().join("\n"),
    )
    .expect("missing source");
    check_at(root, &report_path).expect("report missing source");
    let summary: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("reports/coverage/summary.json")).unwrap())
            .unwrap();
    assert!(!summary["missing_sources"].as_array().unwrap().is_empty());
}

#[test]
fn reports_missing_production_functions_and_macros() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let root = temp.path();
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(root)
        .status()
        .expect("git init");
    assert!(status.success());
    let write_source = |name: &str, source: &str| {
        let path = root.join(name);
        fs::create_dir_all(path.parent().expect("source parent")).expect("source directory");
        fs::write(path, source).expect("source");
    };
    write_source(
        "crates/demo/src/lib.rs",
        "mod function;\nmod macros;\nmod types;\n",
    );
    write_source("crates/demo/src/function.rs", "pub fn uncovered() {}\n");
    write_source(
        "crates/demo/src/macros.rs",
        "macro_rules! uncovered_macro { () => { 1 }; }\n",
    );
    write_source("crates/demo/src/types.rs", "pub struct TypeOnly;\n");
    let expected = expected_sources(root).expect("inventory");
    assert!(expected.contains_key("crates/demo/src/function.rs"));
    assert!(expected.contains_key("crates/demo/src/macros.rs"));
    assert!(!expected.contains_key("crates/demo/src/types.rs"));
    let function = root.join("crates/demo/src/function.rs");
    let lcov = format!("SF:{}\nDA:1,1\nend_of_record\n", function.display());
    let report = parse(root, &lcov, &expected).expect("report");
    assert_eq!(
        report.missing_sources,
        ["crates/demo/src/macros.rs".to_owned()]
    );
}
