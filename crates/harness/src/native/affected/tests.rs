use super::*;

fn package(name: &str, dependencies: &[&str]) -> Package {
    Package {
        name: name.into(),
        dir: format!("crates/{name}"),
        dependencies: dependencies.iter().map(|d| (*d).into()).collect(),
        lib: true,
        text: String::new(),
    }
}

/// core <- map <- simulation <- server; core <- protocol <- server; tool stands alone.
fn workspace() -> Vec<Package> {
    vec![
        package("core", &[]),
        package("map", &["core"]),
        package("simulation", &["core", "map"]),
        package("protocol", &["core"]),
        package("server", &["simulation", "protocol", "serde"]),
        package("tool", &[]),
    ]
}

/// `workspace()` where `simulation` reads the geodata matrix and `tool` the showcase doc.
fn readers() -> Vec<Package> {
    let mut packages = workspace();
    packages[2].text =
        "let m = include_str!(\"../../../docs/geodata/reference-matrix.json\");".into();
    packages[5].text = "std::fs::read_to_string(\"docs/showcase.md\")".into();
    packages
}

fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|p| (*p).into()).collect()
}

fn crates(changed: &[&str], dependents: &[&str]) -> Selection {
    Selection::Crates {
        changed: files(changed),
        dependents: files(dependents),
        total: 6,
    }
}

#[test]
fn a_leaf_crate_change_selects_it_alone() {
    let selection = select(&workspace(), &files(&["crates/tool/src/lib.rs"]));
    assert_eq!(selection, crates(&["tool"], &[]));
    assert_eq!(
        selection.line(),
        "test-unit: 1 of 6 crates (changed: tool; dependents: none)"
    );
}

#[test]
fn a_core_crate_change_selects_its_dependents_transitively() {
    let selection = select(&workspace(), &files(&["crates/core/src/ids.rs"]));
    assert_eq!(
        selection,
        crates(&["core"], &["map", "protocol", "server", "simulation"])
    );
    assert_eq!(
        selection.line(),
        "test-unit: 5 of 6 crates (changed: core; dependents: map, protocol, server, simulation)"
    );
}

#[test]
fn a_mid_graph_change_selects_only_the_crates_above_it() {
    let selection = select(
        &workspace(),
        &files(&["crates/map/Cargo.toml", "crates/tool/x"]),
    );
    assert_eq!(
        selection,
        crates(&["map", "tool"], &["server", "simulation"])
    );
}

#[test]
fn a_root_manifest_lockfile_or_build_file_selects_everything() {
    for file in [
        "Cargo.lock",
        "Cargo.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "gates/registry.json",
        "Makefile",
    ] {
        let selection = select(&workspace(), &files(&["crates/tool/src/lib.rs", file]));
        assert_eq!(selection.packages(), None, "{file}");
        assert!(selection.line().contains(file), "{file}");
        assert!(
            selection
                .line()
                .starts_with("test-unit: 6 of 6 crates (all:")
        );
    }
}

#[test]
fn a_web_browser_or_docs_only_change_selects_nothing() {
    let selection = select(
        &workspace(),
        &files(&["web/app.js", "browser/t.ts", "docs/testing.md"]),
    );
    assert_eq!(selection.packages(), Some(vec![]));
    assert_eq!(
        selection.line(),
        "test-unit: 0 of 6 crates (no Rust crate changed; no Rust unit tests run)"
    );
}

#[test]
fn a_crate_name_prefix_is_not_a_crate_directory() {
    let selection = select(&workspace(), &files(&["crates/tool-extra/src/lib.rs"]));
    assert_eq!(selection.packages(), None);
}

#[test]
fn ci_and_the_force_flag_forbid_a_selection() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| *k == name)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert!(allowed(env(&[])));
    assert!(!allowed(env(&[("GITHUB_ACTIONS", "true")])));
    assert!(!allowed(env(&[("AOE_TEST_ALL", "1")])));
    assert!(allowed(env(&[
        ("AOE_TEST_ALL", "0"),
        ("GITHUB_ACTIONS", "false")
    ])));
}

#[test]
fn metadata_gives_directories_and_dependencies_of_the_workspace() {
    let json = r#"{"workspace_root":"/w","packages":[
        {"name":"a","manifest_path":"/w/crates/a/Cargo.toml","dependencies":[]},
        {"name":"b","manifest_path":"/w/crates/b/Cargo.toml","dependencies":[{"name":"a"},{"name":"serde"}]}]}"#;
    let packages = parse_metadata(json).unwrap();
    assert_eq!(packages[1].dir, "crates/b");
    assert_eq!(packages[1].dependencies, ["a", "serde"]);
    assert_eq!(
        select(&packages, &files(&["crates/a/src/lib.rs"])).packages(),
        Some(vec!["a".to_owned(), "b".to_owned()])
    );
    assert!(parse_metadata("{}").is_err());
}

#[test]
fn a_doc_a_test_reads_selects_the_crates_that_name_it() {
    let selection = select(&readers(), &files(&["docs/geodata/reference-matrix.json"]));
    assert_eq!(selection, crates(&["simulation"], &["server"]));
    let selection = select(&readers(), &files(&["docs/showcase.md"]));
    assert_eq!(selection, crates(&["tool"], &[]));
}

#[test]
fn a_directory_a_crate_reads_selects_it_but_a_top_level_directory_does_not() {
    let mut packages = workspace();
    packages[1].text = "Path::new(\"docs/geodata\")".into();
    packages[3].text = "\"docs\"".into();
    let selection = select(&packages, &files(&["docs/geodata/other.json"]));
    assert_eq!(selection, crates(&["map"], &["server", "simulation"]));
}

#[test]
fn an_unreferenced_doc_selects_no_crate() {
    let selection = select(&readers(), &files(&["docs/unread.md", "web/app.js"]));
    assert_eq!(selection.packages(), Some(vec![]));
}

#[test]
fn metadata_marks_the_packages_with_a_library_target() {
    let json = r#"{"workspace_root":"/w","packages":[
        {"name":"bin","manifest_path":"/w/crates/bin/Cargo.toml","dependencies":[],"targets":[{"kind":["bin"]}]},
        {"name":"lib","manifest_path":"/w/crates/lib/Cargo.toml","dependencies":[],"targets":[{"kind":["lib"]},{"kind":["bin"]}]}]}"#;
    let packages = parse_metadata(json).unwrap();
    assert!(!packages[0].lib);
    assert!(packages[1].lib);
}

#[test]
fn the_real_workspace_has_no_doctests_for_the_harness_binary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names = vec!["aoe-harness".to_owned(), "aoe-protocol".to_owned()];
    let libs = library_crates(&root, &names).unwrap();
    assert!(!libs.contains(&"aoe-harness".to_owned()), "{libs:?}");
    assert!(libs.contains(&"aoe-protocol".to_owned()), "{libs:?}");
}
