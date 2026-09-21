use super::*;

fn init(root: &Path) {
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .current_dir(root)
            .status()
            .unwrap()
            .success()
    );
}

fn write(root: &Path, name: &str, source: &str) {
    let path = root.join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, source).unwrap();
}

#[test]
fn follows_production_modules_but_excludes_external_test_modules() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "#[cfg(test)]\n#[path = \"tests/fixture.rs\"]\nmod fixture;\n#[path = \"tests/production.rs\"]\nmod production;\npub fn after_tests() {}\n",
    );
    write(
        root.path(),
        "crates/demo/src/tests/fixture.rs",
        "pub fn test_only() {}\n",
    );
    write(
        root.path(),
        "crates/demo/src/tests/production.rs",
        "pub fn retained() {}\n",
    );
    let inventory = expected_sources(root.path()).unwrap();
    assert!(!inventory.contains_key("crates/demo/src/tests/fixture.rs"));
    assert!(inventory.contains_key("crates/demo/src/tests/production.rs"));
    assert!(inventory["crates/demo/src/lib.rs"].contains(&6));
}

#[test]
fn ignores_type_only_and_module_only_files_without_filename_exclusions() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "mod types;\npub use types::OnlyType;\n",
    );
    write(
        root.path(),
        "crates/demo/src/types.rs",
        "pub struct OnlyType { pub value: u32 }\n",
    );
    assert!(expected_sources(root.path()).unwrap().is_empty());
}

#[test]
fn cfg_test_does_not_hide_production_lines_after_the_test_module() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "#[cfg(test)]\nmod tests {\n    fn hidden() {}\n}\npub fn visible() {}\n",
    );
    let inventory = expected_sources(root.path()).unwrap();
    let lines = &inventory["crates/demo/src/lib.rs"];
    assert!(lines.contains(&5));
    assert!(!lines.contains(&3));
}

#[test]
fn unguarded_nested_modules_under_tests_directory_are_production() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "#[path = \"tests/production.rs\"]\nmod production;\n",
    );
    write(
        root.path(),
        "crates/demo/src/tests/production.rs",
        "pub fn executable() {}\n",
    );
    assert!(
        expected_sources(root.path())
            .unwrap()
            .contains_key("crates/demo/src/tests/production.rs")
    );
}

#[test]
fn resolves_file_directory_inline_and_path_modules() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "mod flat;\nmod tree;\nmod inline { mod child; #[path = \"path.rs\"] mod selected; }\n#[path = \"alternate/root.rs\"] mod alternate;\npub fn root() {}\n",
    );
    for (name, source) in [
        ("flat.rs", "mod nested;\npub fn flat() {}\n"),
        ("flat/nested.rs", "pub fn flat_nested() {}\n"),
        ("tree/mod.rs", "mod nested;\npub fn tree() {}\n"),
        ("tree/nested/mod.rs", "pub fn tree_nested() {}\n"),
        ("inline/child.rs", "pub fn inline_child() {}\n"),
        ("inline/path.rs", "pub fn inline_path() {}\n"),
        ("alternate/root.rs", "mod child;\npub fn alternate() {}\n"),
        ("alternate/child.rs", "pub fn alternate_child() {}\n"),
        ("bin/tool/main.rs", "mod child;\npub fn binary() {}\n"),
        ("bin/tool/child.rs", "pub fn binary_child() {}\n"),
    ] {
        write(root.path(), &format!("crates/demo/src/{name}"), source);
    }
    let inventory = expected_sources(root.path()).unwrap();
    for name in [
        "flat.rs",
        "flat/nested.rs",
        "tree/mod.rs",
        "tree/nested/mod.rs",
        "inline/child.rs",
        "inline/path.rs",
        "alternate/root.rs",
        "alternate/child.rs",
        "bin/tool/main.rs",
        "bin/tool/child.rs",
    ] {
        assert!(
            inventory.contains_key(&format!("crates/demo/src/{name}")),
            "missing {name}"
        );
    }
}

#[test]
fn unresolved_possible_production_modules_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "#[cfg(test)] mod absent_test;\n#[cfg(any(test, feature = \"optional\"))] mod maybe_production;\npub fn root() {}\n",
    );
    let error = expected_sources(root.path()).unwrap_err().to_string();
    assert!(error.contains("maybe_production"), "{error}");
}

#[test]
fn cargo_metadata_discovers_custom_binary_roots() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "Cargo.toml",
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n[lib]\npath = \"crates/demo/src/custom/lib.rs\"\ncrate-type = [\"cdylib\"]\n[[bin]]\nname = \"custom\"\npath = \"crates/demo/src/custom/start.rs\"\n[[test]]\nname = \"not_production\"\npath = \"crates/demo/tests/fixture.rs\"\n",
    );
    write(
        root.path(),
        "crates/demo/src/custom/lib.rs",
        "pub fn library() {}\n",
    );
    write(
        root.path(),
        "crates/demo/src/custom/start.rs",
        "mod nested;\npub fn custom() {}\n",
    );
    write(
        root.path(),
        "crates/demo/src/custom/nested.rs",
        "pub fn nested() {}\n",
    );
    let inventory = expected_sources(root.path()).unwrap();
    assert!(inventory.contains_key("crates/demo/src/custom/lib.rs"));
    assert!(inventory.contains_key("crates/demo/src/custom/start.rs"));
    assert!(inventory.contains_key("crates/demo/src/custom/nested.rs"));
}

#[test]
fn macro_definitions_are_executable_inventory() {
    let root = tempfile::tempdir().unwrap();
    init(root.path());
    write(
        root.path(),
        "crates/demo/src/lib.rs",
        "macro_rules! make_value { () => { 7 }; }\n",
    );
    let inventory = expected_sources(root.path()).unwrap();
    assert!(inventory["crates/demo/src/lib.rs"].contains(&1));
}
