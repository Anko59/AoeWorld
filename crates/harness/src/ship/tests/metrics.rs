use super::{
    super::levels::RowSet,
    super::metrics::{self, class, comment_lines, scan_rust},
    fixture, run,
};

#[test]
fn paths_fall_into_metric_classes() {
    assert_eq!(class("crates/map/src/lib.rs"), "production");
    assert_eq!(class("crates/map/src/tests.rs"), "tests");
    assert_eq!(class("crates/harness/src/ship/mod.rs"), "harness");
    // Harness test files count as tests, not as harness lines (#170 review).
    assert_eq!(class("crates/harness/src/agents/tests/bash.rs"), "tests");
    assert_eq!(class("crates/harness/src/ship/metrics/tests.rs"), "tests");
    assert_eq!(class("crates/harness/tests/policy.rs"), "tests");
    assert_eq!(class(".claude/settings.json"), "harness");
    assert_eq!(class("docs/review.md"), "docs");
    assert_eq!(class("Cargo.toml"), "config");
}

#[test]
fn comment_density_ignores_markers_inside_regular_and_raw_strings() {
    let source = r####"let glob = "src/*";
let next = 1;
let ordinary = "/* marker */";
let raw = r#"/* raw marker */"#;
let raw_hashes = r###"src/* and // markers"###;
// a real comment
let done = true;
"####;
    assert_eq!(comment_lines(source), 1);
}

#[test]
fn lexer_handles_lifetimes_chars_and_attributes_only_in_code() {
    let source = r####"fn f<'a, /* note */ 'b>() { let s = "it's"; }
const FIXTURE: &str = r#"
#[test]
fn fake() {}
"#;
// #[test]
/*
#[test]
*/
#[test]
fn real() {}
let apostrophe = '\'';
"####;
    let (_, comments, attributes) = scan_rust(source);
    assert_eq!(comments, 5);
    assert_eq!(attributes, 1);
    assert_eq!(
        comment_lines("fn f<'a>() { /* comment */ let s = \"it's\"; }"),
        1
    );
    assert_eq!(comment_lines("fn f<'a, /* note */ 'b>() {}"), 1);
    let escaped_newline = scan_rust("let text = \"body\\\n#[test]\n\";\n");
    assert_eq!(escaped_newline.0, 3);
    assert_eq!(escaped_newline.2, 0);
}

#[test]
fn raw_string_with_large_delimiter_is_scanned_without_delimiter_allocations() {
    let hashes = "#".repeat(200_000);
    let mut source = format!("let value = r{hashes}\"");
    source.push_str(&"\" body ".repeat(20_000));
    source.push('"');
    source.push_str(&hashes);
    source.push_str(";\n// counted\n");
    assert_eq!(comment_lines(&source), 1);
    assert_eq!(scan_rust(&source).2, 0);
}

#[test]
fn metrics_are_revision_bound_and_the_table_uses_merge_base() {
    let (_temp, root) = fixture("true");
    std::fs::create_dir_all(root.join("crates/x/src")).unwrap();
    std::fs::write(
        root.join("crates/x/src/lib.rs"),
        "// adds one\npub fn f() -> u8 { 1 }\n* longitude_width_degrees.to_radians()\n*cursor += 1;\n/* block start\n * inside block\n*/\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/x/src/tests.rs"),
        "#[test]\nfn one() {}\n#[test]\nfn two() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/x/src/large.rs"),
        vec![b'x'; 1024 * 1024 + 1],
    )
    .unwrap();
    std::fs::write(root.join("crates/x/src/binary.rs"), b"\0#[test]\n").unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "metrics input"]);

    let head = metrics::repository(&root, "HEAD").unwrap();
    assert_eq!(head.tests.get("unit"), Some(&2));
    assert_eq!(head.comments.get("production"), Some(&4));
    assert_eq!(head.lines.get("production"), Some(&7));
    assert_eq!(head.lines.get("tests"), Some(&4));

    let table = metrics::table(&root, "dev", RowSet::Summary).unwrap();
    assert!(
        table.contains("| Lines changed: production | | +— / −— | |"),
        "{table}"
    );
    assert!(
        table.contains("| Tests: unit (source #[test] attributes) | 0 | 2 (+2) | 🟢 |"),
        "{table}"
    );
    assert!(
        table.contains("| 📊 Metric | merge base | this PR | |"),
        "{table}"
    );
    assert!(
        table.contains("Comment density in production code (%)"),
        "{table}"
    );
}

#[test]
fn cat_file_drains_a_large_blob_while_feeding_thousands_of_ids() {
    let (_temp, root) = fixture("true");
    let source = root.join("crates/x/src");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::write(source.join("000-large.rs"), vec![b'x'; 1024 * 1024 + 1]).unwrap();
    for index in 0..2200 {
        let body = format!(
            "pub fn item_{index}() {{}}\n// {index}\n{}",
            "x".repeat(1000)
        );
        std::fs::write(source.join(format!("item-{index:04}.rs")), body).unwrap();
    }
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "many source blobs"]);

    let head = metrics::repository(&root, "HEAD").unwrap();
    assert!(
        head.lines.get("production").copied().unwrap_or_default() > 4000,
        "all ordinary source blobs should be consumed: {head:?}"
    );
}

#[test]
fn integration_test_paths_are_only_crate_top_level_tests_and_missing_values_are_dashes() {
    let (_temp, root) = fixture("true");
    std::fs::create_dir_all(root.join("crates/x/src/ship/tests")).unwrap();
    std::fs::create_dir_all(root.join("crates/x/tests")).unwrap();
    std::fs::write(
        root.join("crates/x/src/ship/tests/unit.rs"),
        "#[test]\nfn unit() {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/x/tests/integration.rs"),
        "#[test]\nfn integration() {}\n",
    )
    .unwrap();
    std::fs::write(root.join("config.bin"), [0, 1, 2]).unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-q", "-m", "metrics inputs"]);

    let head = metrics::repository(&root, "HEAD").unwrap();
    assert_eq!(head.tests.get("unit"), Some(&1));
    assert_eq!(head.tests.get("integration"), Some(&1));
    let table = metrics::table(&root, "dev", RowSet::Summary).unwrap();
    assert!(
        table.contains("| Lines changed: config | | +— / −— | |"),
        "{table}"
    );
    assert!(
        table.contains("| Comment density in production code (%) | — | — | |"),
        "{table}"
    );
    assert!(
        table.contains("| Production lines | — | — (—) | |"),
        "{table}"
    );
}

fn commit_all(root: &std::path::Path, message: &str) {
    run(root, &["add", "-A"]);
    run(root, &["commit", "-q", "-m", message]);
}

fn write(root: &std::path::Path, path: &str, text: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn the_snapshot_counts_every_static_metric_from_blobs() {
    let (_temp, root) = fixture("true");
    write(
        &root,
        "Cargo.lock",
        "[[package]]\nname = \"ours\"\n\n[[package]]\nname = \"serde\"\nsource = \"registry+x\"\n\n[[package]]\nname = \"git\"\nsource = \"git+y\"\n",
    );
    write(
        &root,
        "fuzz/Cargo.lock",
        "[[package]]\nname = \"a\"\nsource = \"registry+x\"\n",
    );
    write(
        &root,
        "browser/package-lock.json",
        r#"{"packages": {"": {}, "node_modules/a": {}, "node_modules/b": {}}}"#,
    );
    write(
        &root,
        "crates/x/src/lib.rs",
        "// TODO: later\nunsafe fn f() {}\n// unsafe in a comment\nfn forbid_unsafe_code() {}\n",
    );
    write(&root, "crates/x/tests/it.rs", "#[test]\nfn a() {}\n");
    write(
        &root,
        "crates/y/src/web.rs",
        "#[wasm_bindgen_test]\nfn a() {}\n#[wasm_bindgen_test::wasm_bindgen_test]\nfn b() {}\n",
    );
    write(&root, "fuzz/fuzz_targets/drs.rs", "fn main() {}\n");
    write(
        &root,
        "browser/tests/a.spec.ts",
        "// note\ntest(\"one\", async () => {});\n  test(\"two\", async () => {});\ntest.describe(\"x\", () => {});\n",
    );
    write(&root, "docs/long.md", &"line\n".repeat(460));
    write(&root, "docs/huge.md", &"line\n".repeat(501));
    write(&root, "docs/gates.md", "generated\n");
    write(&root, "script.sh", "# FIXME\necho\n");
    commit_all(&root, "static inputs");

    let m = metrics::repository(&root, "HEAD").unwrap();
    assert_eq!(m.dependencies.get("cargo"), Some(&2));
    assert_eq!(m.dependencies.get("fuzz"), Some(&1));
    assert_eq!(m.dependencies.get("npm"), Some(&2));
    assert_eq!(m.unsafe_count, 1);
    assert_eq!(m.todo_count, 2);
    assert_eq!(m.tests.get("integration"), Some(&1));
    assert_eq!(m.tests.get("wasm"), Some(&2));
    assert_eq!(m.tests.get("fuzz"), Some(&1));
    assert_eq!(m.tests.get("playwright"), Some(&2));
    assert_eq!(m.tests.values().sum::<u64>(), 6);
    assert_eq!(m.dependencies.values().sum::<u64>(), 5);
    assert_eq!(m.files_near_limit, 1);
    assert_eq!(m.files_over_limit, vec!["docs/huge.md".to_owned()]);
    let largest = m.largest_file.unwrap();
    assert_eq!(
        (largest.path.as_str(), largest.lines),
        ("docs/huge.md", 501)
    );
    // Cargo.lock 10, fuzz/Cargo.lock 3, package-lock 1, docs/gates.md 1.
    assert_eq!(m.lines.get("generated"), Some(&15));
    assert_eq!(m.comments.get("config"), Some(&1));
    assert_eq!(metrics::class("browser/package-lock.json"), "generated");
}

#[test]
fn the_snapshot_file_is_written_per_commit_and_reads_only_fresh_reports() {
    use std::time::{Duration, SystemTime};
    let (_temp, root) = fixture("true");
    write(&root, "crates/x/src/lib.rs", "fn f() {}\n");
    commit_all(&root, "one");
    let sha = super::super::git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    let absent = metrics::snapshot::compute(&root, "HEAD").unwrap();
    assert_eq!(absent.measured, Default::default());
    assert_eq!(absent.revision, sha);

    write(
        &root,
        "reports/coverage/summary.json",
        r#"{"overall": {"covered": 90, "measurable": 120}}"#,
    );
    write(&root, "reports/perf/optimized.wasm", "wasm!");
    write(&root, "reports/perf/optimized.wasm.gz", "gz");
    write(&root, "target/release/aoe-server", "binary");
    let report = serde_json::json!({
        "revision": sha, "dirty": false,
        "workloads": [{"scenario": "smoke", "total_entities": 8000, "encoded_bytes": 47794,
            "tick_duration_ns": [614121, 81436, 187177, 81645]}]
    });
    write(&root, "reports/perf/smoke.json", &report.to_string());
    let measured = metrics::snapshot::compute(&root, "HEAD").unwrap().measured;
    assert_eq!(measured.coverage_percent, Some(75.0));
    assert_eq!(
        (measured.wasm_bytes, measured.wasm_gzip_bytes),
        (Some(5), Some(2))
    );
    assert_eq!(measured.native_bytes, Some(6));
    let perf = measured.perf.unwrap();
    assert_eq!(
        (perf.p95_tick_ns, perf.entities, perf.bytes),
        (614121, 8000, 47794)
    );

    let snapshot = metrics::snapshot::compute(&root, "HEAD").unwrap();
    let path = metrics::snapshot::write(&root, &snapshot).unwrap();
    assert!(
        path.ends_with(format!("aoe-ship/metrics/{sha}.json")),
        "{path:?}"
    );
    // Reports gone: the earlier snapshot of the same commit still supplies them.
    std::fs::remove_dir_all(root.join("reports")).unwrap();
    let kept = metrics::snapshot::compute(&root, "HEAD").unwrap();
    assert_eq!(kept.measured.coverage_percent, Some(75.0));

    // A report older than the commit, a dirty one and another commit's are ignored.
    std::fs::remove_file(path).unwrap();
    write(
        &root,
        "reports/coverage/summary.json",
        r#"{"overall": {"covered": 1, "measurable": 2}}"#,
    );
    let old = SystemTime::now() - Duration::from_secs(3600 * 24 * 365);
    std::fs::File::options()
        .write(true)
        .open(root.join("reports/coverage/summary.json"))
        .unwrap()
        .set_modified(old)
        .unwrap();
    let dirty = report.to_string().replace("false", "true");
    write(&root, "reports/perf/smoke.json", &dirty);
    let stale = metrics::snapshot::compute(&root, "HEAD").unwrap().measured;
    assert_eq!((stale.coverage_percent, stale.perf), (None, None));
    write(&root, "crates/x/src/more.rs", "fn g() {}\n");
    commit_all(&root, "two");
    let parent = metrics::snapshot::compute(&root, "HEAD~1").unwrap();
    assert_eq!(parent.measured, Default::default());
    assert!(metrics::snapshot::compute(&root, "no-such-revision").is_err());
}

#[test]
fn make_metrics_writes_and_prints_the_snapshot_of_a_named_revision() {
    let (_temp, root) = fixture("true");
    write(&root, "crates/x/src/lib.rs", "fn f() {}\n");
    commit_all(&root, "one");
    let first = super::super::git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    write(&root, "crates/x/src/more.rs", "fn g() {}\n");
    commit_all(&root, "two");
    // SAFETY: the other tests in this module do not read METRICS_REV.
    unsafe { std::env::set_var("METRICS_REV", &first) };
    let done = metrics::snapshot::command(&root);
    unsafe { std::env::set_var("METRICS_REV", "--output=x") };
    let refused = metrics::snapshot::command(&root);
    unsafe { std::env::remove_var("METRICS_REV") };
    done.unwrap();
    assert!(refused.unwrap_err().contains("not a revision"));
    let stored = metrics::snapshot::path(&root, &first).unwrap();
    let text = std::fs::read_to_string(stored).unwrap();
    assert!(
        text.contains(&first) && text.contains("\"production\": 1"),
        "{text}"
    );
}

#[test]
fn the_table_grows_with_the_level_and_hides_absent_measured_rows() {
    let (_temp, root) = fixture("true");
    write(&root, "crates/x/src/lib.rs", "fn f() {}\n");
    commit_all(&root, "base");
    run(
        &root,
        &["push", "-q", "-f", "origin", "HEAD:refs/heads/dev"],
    );
    write(&root, "crates/x/src/more.rs", "// TODO\nunsafe fn g() {}\n");
    write(
        &root,
        "Cargo.lock",
        "[[package]]\nname = \"a\"\nsource = \"registry+x\"\n",
    );
    commit_all(&root, "change");
    let sha = super::super::git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    write(
        &root,
        "reports/coverage/summary.json",
        r#"{"overall": {"covered": 1, "measurable": 2}}"#,
    );
    write(&root, "reports/perf/smoke.json", &serde_json::json!({
        "revision": sha, "dirty": false,
        "workloads": [{"scenario": "smoke", "total_entities": 5, "encoded_bytes": 6, "tick_duration_ns": [9]}]
    }).to_string());
    let at = |rows| metrics::table(&root, "dev", rows).unwrap();
    let low = at(RowSet::Summary);
    let medium = at(RowSet::Standard);
    let high = at(RowSet::Extended);
    let max = at(RowSet::Full);
    assert!(
        low.contains("| Largest human-authored file (lines, limit 500) |"),
        "{low}"
    );
    assert!(!low.contains("Dependencies"), "{low}");
    assert!(
        medium.contains("| Dependencies: Cargo (third-party packages) | 0 | 1 (+1) | 🔴 |"),
        "{medium}"
    );
    assert!(
        medium.contains("| `unsafe` lines in production code | 0 | 1 (+1) | 🔴 |"),
        "{medium}"
    );
    assert!(
        medium.contains("| Lines changed: generated | | +3 / −0 | |"),
        "{medium}"
    );
    assert!(!medium.contains("Lines total"), "{medium}");
    assert!(high.contains("| Lines total: docs |"), "{high}");
    assert!(!high.contains("Line coverage"), "{high}");
    assert!(
        !high.contains("WASM size"),
        "absent measured rows are left out: {high}"
    );
    assert!(max.contains("| Line coverage (%) | — | 50.0 |"), "{max}");
    assert!(
        max.contains("| Perf smoke: p95 tick (ns) | — | 9 (—) |"),
        "{max}"
    );
    assert!(max.lines().count() > high.lines().count());
}
