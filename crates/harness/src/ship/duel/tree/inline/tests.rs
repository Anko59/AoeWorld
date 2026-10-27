use super as inline;

#[test]
fn cfg_test_only_under_not_is_product_code() {
    for predicate in [
        "not(test)",
        "all(unix, not(test))",
        "any(not(test), windows)",
        "not(all(test, unix))",
        "not(not(test))",
    ] {
        let before = format!("#[cfg({predicate})]\nfn production() {{ let value = 1; }}\n");
        assert!(inline::test_regions(&before).is_empty(), "{predicate}");
    }
    assert_eq!(
        inline::test_regions("#[cfg(any(not(unix), test))]\nfn positive() {}\n"),
        [(1, 2)]
    );
}

#[test]
fn test_regions_cover_cfg_test_items() {
    let text = "fn a() {}\n#[cfg(test)]\nmod b;\n#[cfg(test)] mod c { fn d() {} }\n#[cfg(all(test, unix))]\nfn e() {\n    // }\n    let x = 1;\n}\nfn f() {}\n";
    assert_eq!(inline::test_regions(text), [(2, 3), (4, 4), (5, 9)]);
    let patch = "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -2,0 +3,2 @@\n+a\n+\n@@ -5 +7 @@\n-b\n+c\n";
    let changes = inline::all_changes(patch);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "x.rs");
    assert_eq!(changes[0].1.new, [3, 4, 7]);
    assert_eq!(changes[0].1.old, [5]);
}

#[test]
fn test_markers_are_attributes_naming_a_test() {
    for marker in [
        "#[test]",
        "  #[tokio::test(flavor = \"x\")]",
        "#[rstest]",
        "#[wasm_bindgen_test]",
    ] {
        assert!(inline::test_marker(marker), "{marker}");
    }
    for other in [
        "#[cfg(test)]",
        "#[cfg_attr(test, x)]",
        "#[derive(Debug)]",
        "fn test() {}",
    ] {
        assert!(!inline::test_marker(other), "{other}");
    }
}

#[test]
fn test_attributes_cover_whole_items_and_preceding_attributes() {
    let text = "#[allow(dead_code)]\n#[tokio::test]\nasync fn check() {\n    let x = \"}\";\n}\nfn product() {}\n";
    assert_eq!(inline::test_attributes(text), [(1, 5)]);
}
