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
fn the_two_runs_use_the_same_names_and_the_list_has_no_duplicates() {
    let mut sorted = SOURCE_QUALIFICATION.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), SOURCE_QUALIFICATION.len());
    assert_eq!(exclude(), format!("not ({})", include()));
    assert_eq!(include().matches("test(=").count(), sorted.len());
}
