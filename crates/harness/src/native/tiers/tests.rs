use super::*;
use std::{fs, path::Path};

/// Every listed name is a real `#[test]` function in the server crate, so a
/// rename or deletion cannot silently drop a test from both runs.
#[test]
fn every_tier_member_names_a_test_function_in_the_server_crate() {
    let server = Path::new(env!("CARGO_MANIFEST_DIR")).join("../server/src");
    for name in SOURCE_QUALIFICATION {
        let (module, function) = name.rsplit_once("::").unwrap();
        let mut path = server.join(module.replace("::", "/"));
        let file = if path.with_extension("rs").is_file() {
            path.with_extension("rs")
        } else {
            path.push("mod.rs");
            path
        };
        let source = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("{name}: cannot read {}: {error}", file.display()));
        assert!(
            source.contains(&format!("fn {function}(")),
            "{name}: no `fn {function}` in {}",
            file.display()
        );
    }
}

#[test]
fn the_source_qualification_tier_builds_only_the_server_package() {
    // The member check above reads `crates/server/src`: the package list must agree.
    assert_eq!(SOURCE_QUALIFICATION_PACKAGES, ["aoe-server"]);
}

/// Every `#[test]` function under one crate directory's sources, by name.
fn test_functions_in(directory: &Path) -> Vec<String> {
    let mut found = Vec::new();
    collect_test_functions(&directory.join("src"), &mut found);
    found
}

/// Every exhaustive member lives in one of the packages the tier builds, so the
/// scoped run cannot leave one out.
#[test]
fn every_exhaustive_member_is_in_a_package_the_tier_builds() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let functions: Vec<String> = EXHAUSTIVE_PACKAGES
        .iter()
        .flat_map(|package| {
            let directory = package
                .strip_prefix("aoe-")
                .expect("workspace package name");
            test_functions_in(&crates.join(directory))
        })
        .collect();
    for name in EXHAUSTIVE {
        let function = name.rsplit_once("::").unwrap().1;
        assert_eq!(
            functions.iter().filter(|f| *f == function).count(),
            1,
            "{name}: expected `#[test] fn {function}` in one of {EXHAUSTIVE_PACKAGES:?}"
        );
    }
}

/// Every `#[test]` function of the workspace crates' sources, by name.
fn workspace_test_functions() -> Vec<String> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for entry in fs::read_dir(crates).unwrap().flatten() {
        collect_test_functions(&entry.path().join("src"), &mut found);
    }
    found
}

fn collect_test_functions(dir: &Path, found: &mut Vec<String>) {
    {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_test_functions(&path, found);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let source = fs::read_to_string(&path).unwrap();
                let lines: Vec<&str> = source.lines().map(str::trim).collect();
                for (index, line) in lines.iter().enumerate() {
                    let Some(rest) = line.strip_prefix("fn ") else {
                        continue;
                    };
                    let preceding = lines[..index]
                        .iter()
                        .rev()
                        .take_while(|l| l.starts_with("#[") || l.starts_with("///"));
                    if preceding.into_iter().any(|l| *l == "#[test]") {
                        found.push(rest.split('(').next().unwrap().to_owned());
                    }
                }
            }
        }
    }
}

/// Every exhaustive member is a real `#[test]` function in some crate, so a
/// rename or deletion cannot silently drop a test from both runs.
#[test]
fn every_exhaustive_member_names_a_test_function_in_the_workspace() {
    let functions = workspace_test_functions();
    for name in EXHAUSTIVE {
        let function = name.rsplit_once("::").unwrap().1;
        assert_eq!(
            functions.iter().filter(|f| *f == function).count(),
            1,
            "{name}: expected exactly one `#[test] fn {function}` in the workspace"
        );
    }
}

#[test]
fn the_tiers_are_disjoint_and_each_list_has_no_duplicates() {
    for list in [SOURCE_QUALIFICATION, EXHAUSTIVE] {
        let mut sorted = list.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), list.len());
    }
    assert!(
        EXHAUSTIVE
            .iter()
            .all(|name| !SOURCE_QUALIFICATION.contains(name))
    );
    assert_eq!(
        exclude(),
        format!("not ({} | {})", include(), include_exhaustive())
    );
    assert_eq!(
        include_exhaustive().matches("test(=").count(),
        EXHAUSTIVE.len()
    );
    assert_eq!(
        include().matches("test(=").count(),
        SOURCE_QUALIFICATION.len()
    );
}
