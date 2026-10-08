use super::{
    super::metrics::{self, class, comment_lines, scan_rust},
    fixture, run,
};

#[test]
fn paths_fall_into_metric_classes() {
    assert_eq!(class("crates/map/src/lib.rs"), "production");
    assert_eq!(class("crates/map/src/tests.rs"), "tests");
    assert_eq!(class("crates/harness/src/ship/mod.rs"), "harness");
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

    let table = metrics::table(&root).unwrap();
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
    let table = metrics::table(&root).unwrap();
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
