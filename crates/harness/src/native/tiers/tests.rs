use super::*;
use std::{fs, path::Path};

/// Every listed name resolves, as a module path, to a real `#[test]` function
/// in some workspace crate, so a rename or deletion cannot silently drop a test
/// from both runs. The path is checked, not just the function name: a member
/// whose module moved is as broken as one that was renamed.
#[test]
fn every_tier_member_names_a_test_function_at_its_module_path() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    for name in SOURCE_QUALIFICATION {
        let (module, function) = name.rsplit_once("::").unwrap();
        let mut tried = Vec::new();
        let found = fs::read_dir(&crates).unwrap().flatten().any(|entry| {
            let base = entry.path().join("src").join(module.replace("::", "/"));
            for file in [base.with_extension("rs"), base.join("mod.rs")] {
                if let Ok(source) = fs::read_to_string(&file) {
                    tried.push(file.clone());
                    if source.contains(&format!("fn {function}(")) {
                        return true;
                    }
                }
            }
            false
        });
        assert!(
            found,
            "{name}: no `fn {function}` at that module path; read {tried:?}"
        );
    }
}

/// Every source-qualification member lives in one of the packages the tier
/// builds (`aoe-server` and `aoe-simulation` today), so the scoped run cannot
/// leave one out.
#[test]
fn every_source_qualification_member_is_in_a_package_the_tier_builds() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let functions: Vec<String> = SOURCE_QUALIFICATION_PACKAGES
        .iter()
        .flat_map(|package| {
            let directory = package
                .strip_prefix("aoe-")
                .expect("workspace package name");
            test_functions_in(&crates.join(directory))
        })
        .collect();
    for name in SOURCE_QUALIFICATION {
        let function = name.rsplit_once("::").unwrap().1;
        assert!(
            functions.iter().any(|f| f == function),
            "{name} lives outside {SOURCE_QUALIFICATION_PACKAGES:?}"
        );
    }
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
                    let Some(rest) = line
                        .strip_prefix("fn ")
                        .or_else(|| line.strip_prefix("async fn "))
                    else {
                        continue;
                    };
                    let preceding = lines[..index]
                        .iter()
                        .rev()
                        .take_while(|l| l.starts_with("#[") || l.starts_with("///"));
                    if preceding
                        .into_iter()
                        .any(|l| *l == "#[test]" || l.starts_with("#[tokio::test"))
                    {
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

#[test]
fn coverage_runs_what_test_unit_runs_minus_benchmark_and_timing_tests() {
    let coverage = super::coverage();
    // Every post-merge tier test is excluded, as in `make test-unit`.
    assert!(coverage.contains(&super::include()), "{coverage}");
    assert!(
        coverage.contains(&super::include_exhaustive()),
        "{coverage}"
    );
    assert!(
        coverage.contains("test(/::benchmark::/)") && coverage.contains("test(/::performance::/)")
    );
    assert!(coverage.starts_with("not ("), "{coverage}");
}
