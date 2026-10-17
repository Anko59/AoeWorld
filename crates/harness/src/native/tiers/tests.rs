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

/// Every `#[test]` function of the workspace crates' sources, by name.
fn workspace_test_functions() -> Vec<String> {
    fn walk(dir: &Path, found: &mut Vec<String>) {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
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
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut found = Vec::new();
    for entry in fs::read_dir(crates).unwrap().flatten() {
        walk(&entry.path().join("src"), &mut found);
    }
    found
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
