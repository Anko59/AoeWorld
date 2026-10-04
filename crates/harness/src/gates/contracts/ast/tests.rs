use super::*;
fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn fixture(text: &str) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    put(root.path(), "crates/demo/src/lib.rs", text);
    root
}
fn case(root: &Path, symbol: &str) -> Result<Binding> {
    bind(root, "crates/demo/src/lib.rs", symbol, "test")
}
#[test]
fn production_functions_and_inherent_methods_have_exact_local_names_and_source_hashes() {
    let text = "pub fn decode() {}\nstruct World;\nimpl World { pub fn step(&self) {} }\n#[cfg(not(test))] fn native() {}";
    let root = fixture(text);
    let value = bind(root.path(), "crates/demo/src/lib.rs", "decode", "function").unwrap();
    assert_eq!(value.line, 1);
    assert_eq!(
        value.file_blake3,
        blake3::hash(text.as_bytes()).to_hex().to_string()
    );
    assert_eq!(value.path, "crates/demo/src/lib.rs");
    assert_eq!(value.kind, "function");
    assert_eq!(value.symbol, "decode");
    assert!(
        bind(
            root.path(),
            "crates/demo/src/lib.rs",
            "World::step",
            "method"
        )
        .is_ok()
    );
    assert!(bind(root.path(), "crates/demo/src/lib.rs", "native", "function").is_ok());
    for (symbol, kind) in [
        ("Other::step", "method"),
        ("World::step", "function"),
        ("decode", "method"),
        ("decode", "test"),
        ("decode", "unknown"),
    ] {
        assert!(bind(root.path(), "crates/demo/src/lib.rs", symbol, kind).is_err());
    }
}
#[test]
fn inline_test_context_and_external_declared_test_chain_are_required() {
    let root = fixture(
        "#[cfg(test)] mod tests { #[test] fn real() { let x = 1; assert_eq!(x, 1); } mod nested; }",
    );
    put(
        root.path(),
        "crates/demo/src/tests/nested.rs",
        "#[test] fn external() { let result = Some(1); assert!(matches!(result, Some(_))); }",
    );
    assert!(case(root.path(), "tests::real").is_ok());
    assert!(
        bind(
            root.path(),
            "crates/demo/src/tests/nested.rs",
            "external",
            "test"
        )
        .is_ok()
    );
    put(
        root.path(),
        "crates/demo/src/unused_tests.rs",
        "#[test] fn external() { let x = 1; assert_eq!(x, 1); }",
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/unused_tests.rs",
            "external",
            "test"
        )
        .is_err()
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/lib.rs",
            "tests::real",
            "function"
        )
        .is_err()
    );
}
#[test]
fn external_file_modules_reset_local_symbol_namespace_and_inline_child_directories_work() {
    let root = fixture("mod outer;");
    put(
        root.path(),
        "crates/demo/src/outer.rs",
        "pub fn boundary() {} #[cfg(test)] mod tests { mod nested; }",
    );
    put(
        root.path(),
        "crates/demo/src/outer/tests/nested.rs",
        "#[test] fn real() { let x = 1; assert_ne!(x, 2); }",
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/outer.rs",
            "boundary",
            "function"
        )
        .is_ok()
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/outer/tests/nested.rs",
            "real",
            "test"
        )
        .is_ok()
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/outer.rs",
            "outer::boundary",
            "function"
        )
        .is_err()
    );
}
#[test]
fn fake_names_empty_tests_and_obviously_vacuous_assertions_are_not_bindings() {
    for body in [
        "",
        "assert!(true);",
        "assert!(false);",
        "assert!(true || unknown());",
        "assert_eq!(1, 1);",
        "let x=1; assert_eq!(x,x);",
        "let x=1; assert_ne!(x,x);",
        "let x=1; assert!(x == x);",
        "if false { let x=1; assert_eq!(x,1); }",
        "while false { let x=1; assert_eq!(x,1); }",
        "return; let x=1; assert_eq!(x,1);",
        "let _unused=|| { let x=1; assert_eq!(x,1); };",
        "assert!(matches!(1, 1));",
        "custom_check!();",
    ] {
        let root = fixture(&format!(
            "#[cfg(test)] mod tests {{ #[test] fn real() {{ {body} }} }}"
        ));
        assert!(case(root.path(), "tests::real").is_err(), "accepted {body}");
    }
    let root =
        fixture("// #[test] fn fake() { assert!(true); }\nconst TEXT: &str = \"fn fake() {}\";");
    assert!(case(root.path(), "fake").is_err());
}
#[test]
fn invalid_case_attributes_profiles_and_macro_masking_fail_closed() {
    for text in [
        "mod tests { #[test] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] #[ignore] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] #[should_panic] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] fn real(x:u8) { assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] fn real<T>() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] async fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[cfg(feature=\"optional\")] #[test] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[cfg_attr(test, test)] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(not(test))] mod tests { #[test] fn real() { let x=1; assert_eq!(x,1); } }",
        "macro_rules! assert_eq { ($($t:tt)*) => {}; } #[cfg(test)] mod tests { #[test] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { use custom::assert_eq; #[test] fn real() { let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] fn real() { macro_rules! assert_eq { ($($t:tt)*) => {}; } let x=1; assert_eq!(x,1); } }",
        "#[cfg(test)] mod tests { #[test] fn real() { #[cfg(feature=\"optional\")] { let x=1; assert_eq!(x,1); } } }",
        "#[cfg(test)] mod tests { generate_test!(real); }",
    ] {
        let root = fixture(text);
        assert!(case(root.path(), "tests::real").is_err(), "accepted {text}");
    }
}
#[test]
fn duplicate_bindings_trait_impls_and_unknown_ancestor_cfg_cannot_qualify() {
    for text in [
        "fn real() {} fn real() {}",
        "#[cfg(feature=\"x\")] fn real() {} fn real() {}",
        "#[cfg_attr(feature=\"x\", inline)] fn real() {}",
    ] {
        let root = fixture(text);
        assert!(bind(root.path(), "crates/demo/src/lib.rs", "real", "function").is_err());
    }
    let root = fixture(
        "struct World; trait Step { fn step(&self); } impl Step for World { fn step(&self) {} }",
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/lib.rs",
            "World::step",
            "method"
        )
        .is_err()
    );
    let root = fixture("#[cfg(feature=\"x\")] mod outer;");
    put(root.path(), "crates/demo/src/outer.rs", "fn real() {}");
    assert!(bind(root.path(), "crates/demo/src/outer.rs", "real", "function").is_err());
}
#[test]
fn ambiguous_missing_path_alias_and_non_normal_sources_are_rejected() {
    let root = fixture("mod outer;");
    put(root.path(), "crates/demo/src/outer.rs", "fn real() {}");
    put(root.path(), "crates/demo/src/outer/mod.rs", "fn real() {}");
    assert!(bind(root.path(), "crates/demo/src/outer.rs", "real", "function").is_err());
    let root = fixture("#[path=\"hidden.rs\"] mod outer;");
    put(root.path(), "crates/demo/src/hidden.rs", "fn real() {}");
    assert!(bind(root.path(), "crates/demo/src/hidden.rs", "real", "function").is_err());
    for path in [
        "/crates/demo/src/lib.rs",
        "crates/demo/src/../src/lib.rs",
        "crates/demo/src/./lib.rs",
        "crates//demo/src/lib.rs",
        "crates/demo/src/missing.rs",
        "docs/code.rs",
        "crates/demo/src\\lib.rs",
    ] {
        assert!(bind(root.path(), path, "real", "function").is_err());
    }
    let root = fixture("mod absent; fn real() {}");
    put(
        root.path(),
        "crates/demo/src/absent/child.rs",
        "fn real() {}",
    );
    assert!(
        bind(
            root.path(),
            "crates/demo/src/absent/child.rs",
            "real",
            "function"
        )
        .is_err()
    );
}
#[test]
fn file_and_inline_depth_bounds_reject_without_guessing() {
    let root = fixture(&format!("//{}", "x".repeat(MAX_BYTES as usize)));
    assert!(bind(root.path(), "crates/demo/src/lib.rs", "real", "function").is_err());
    let depth = MAX_DEPTH + 1;
    let root = fixture(&format!(
        "{}fn real() {{}}{}",
        "mod nested {".repeat(depth),
        "}".repeat(depth)
    ));
    let symbol = format!("{}real", "nested::".repeat(depth));
    assert!(bind(root.path(), "crates/demo/src/lib.rs", &symbol, "function").is_err());
    let root = fixture("this is not valid Rust");
    assert!(bind(root.path(), "crates/demo/src/lib.rs", "real", "function").is_err());
}
#[cfg(unix)]
#[test]
fn symlink_sources_and_directory_aliases_never_affirm_bindings() {
    use std::os::unix::fs::symlink;
    let root = fixture("mod linked;");
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("actual.rs"), "fn real() {}").unwrap();
    symlink(
        outside.path().join("actual.rs"),
        root.path().join("crates/demo/src/linked.rs"),
    )
    .unwrap();
    assert!(bind(root.path(), "crates/demo/src/linked.rs", "real", "function").is_err());
    symlink(outside.path(), root.path().join("crates/demo/src/alias")).unwrap();
    assert!(
        bind(
            root.path(),
            "crates/demo/src/alias/actual.rs",
            "real",
            "function"
        )
        .is_err()
    );
    let alias = outside.path().join("root-alias");
    symlink(root.path(), &alias).unwrap();
    assert!(bind(&alias, "crates/demo/src/lib.rs", "real", "function").is_err());
}
#[test]
fn declared_local_globs_are_checked_not_blindly_trusted_or_masked() {
    let text = "mod helpers; use helpers::*; #[cfg(test)] mod tests { use super::*; #[test] fn real() { assert_eq!(observed(), 1); } }";
    let root = fixture(text);
    put(
        root.path(),
        "crates/demo/src/helpers.rs",
        "pub fn observed() -> u8 { 1 }",
    );
    assert!(case(root.path(), "tests::real").is_ok());
    put(
        root.path(),
        "crates/demo/src/helpers.rs",
        "mod inner; pub use inner::*;",
    );
    put(
        root.path(),
        "crates/demo/src/helpers/inner.rs",
        "pub fn observed() -> u8 { 1 }",
    );
    assert!(case(root.path(), "tests::real").is_ok());
    put(
        root.path(),
        "crates/demo/src/helpers/inner.rs",
        "macro_rules! assert_eq { ($($t:tt)*) => {}; } pub(crate) use assert_eq;",
    );
    assert!(case(root.path(), "tests::real").is_err());
    put(
        root.path(),
        "crates/demo/src/helpers.rs",
        "pub use foreign::*;",
    );
    assert!(case(root.path(), "tests::real").is_err());
    let root = fixture(
        "use foreign::*; #[cfg(test)] mod tests { #[test] fn real() { let x=1; assert_eq!(x,1); } }",
    );
    assert!(case(root.path(), "tests::real").is_err());
    let root = fixture(
        "mod std { pub use foreign::assert_eq; } use std::*; #[cfg(test)] mod tests { #[test] fn real() { let x=1; assert_eq!(x,1); } }",
    );
    assert!(case(root.path(), "tests::real").is_err());
}
#[test]
fn real_narrow_protocol_route_and_index_witness_bindings_are_reachable() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    for (path, symbol, kind) in [
        ("crates/protocol/src/lib.rs", "decode_client", "function"),
        ("crates/protocol/src/lib.rs", "decode_server", "function"),
        (
            "crates/protocol/src/lib.rs",
            "tests::messages_round_trip",
            "test",
        ),
        (
            "crates/protocol/src/lib.rs",
            "tests::rejects_oversized_data",
            "test",
        ),
        (
            "crates/protocol/src/lib.rs",
            "tests::a_short_frame_cannot_request_a_huge_snapshot_allocation",
            "test",
        ),
        (
            "crates/protocol/src/lib.rs",
            "tests::enforces_wire_and_collection_limits_for_both_directions",
            "test",
        ),
        (
            "crates/simulation/src/game_waypoints.rs",
            "GameWorld::issue_move_waypoints",
            "method",
        ),
        (
            "crates/simulation/src/game_waypoints.rs",
            "tests::waypoint_orders_require_tile_centers_and_walkable_edges",
            "test",
        ),
        (
            "crates/simulation/src/game_waypoints.rs",
            "tests::one_order_retains_the_final_destination_and_remaining_route",
            "test",
        ),
        (
            "crates/harness/src/gates/scopes/mod.rs",
            "Snapshot::prepare_index",
            "method",
        ),
        (
            "crates/harness/src/gates/scopes/mod.rs",
            "Snapshot::verify_source",
            "method",
        ),
        (
            "crates/harness/src/gates/scopes/witness.rs",
            "referenced_objects",
            "function",
        ),
        (
            "crates/harness/src/gates/scopes/tests/index_context.rs",
            "effective_source_index_is_distinct_from_default_and_never_leaks_into_private_git",
            "test",
        ),
        (
            "crates/harness/src/gates/scopes/tests/index_context.rs",
            "missing_external_or_symlink_index_never_falls_back_to_default",
            "test",
        ),
        (
            "crates/harness/src/gates/scopes/tests/witness.rs",
            "fingerprint_binds_referenced_blob_bytes_not_only_index_object_names",
            "test",
        ),
    ] {
        let value = bind(root, path, symbol, kind)
            .unwrap_or_else(|error| panic!("{path}::{symbol}: {error}"));
        assert_eq!(value.file_blake3.len(), 64);
        assert!(value.line > 0);
    }
}
