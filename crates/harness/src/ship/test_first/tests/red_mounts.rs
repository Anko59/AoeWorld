//! A changed test file is compiled only where a product file's `mod` line loads it; the red
//! tree gets that one line (and a stub for a new parent module), never the rest of the product.
use super::{commit, repo};
use crate::ship::{
    duel::focus::{CheckReport, CompileError},
    git,
    test_first::red,
};
use std::fs;

const LIB: &str = "crates/sim/src/lib.rs";

fn tip(root: &std::path::Path) -> String {
    git::git(root, &["rev-parse", "HEAD"]).unwrap()
}

fn read(snapshot: &std::path::Path, path: &str) -> String {
    fs::read_to_string(snapshot.join(path)).unwrap_or_default()
}

/// Fails by name while the loaded test file meets product code without `fixed`.
fn runner(snapshot: &std::path::Path, test: &str, name: &str) -> CheckReport {
    let lib = read(snapshot, LIB);
    let loaded = snapshot.join(test).is_file() && lib.contains("mod ");
    let fixed = lib.contains("fixed");
    CheckReport {
        passed: !loaded || fixed,
        tests: if loaded { vec![name.into()] } else { vec![] },
        failed: if loaded && !fixed {
            vec![name.into()]
        } else {
            vec![]
        },
        ..Default::default()
    }
}

#[test]
fn a_new_test_file_mounted_from_an_existing_product_file_is_red_by_name() {
    for (test, mount, name) in [
        (
            "crates/sim/src/tests.rs",
            "#[cfg(test)] mod tests;",
            "sim$tests::feature",
        ),
        (
            "crates/sim/src/tests/new_case.rs",
            "#[cfg(test)]\n#[path = \"tests/new_case.rs\"]\nmod new_case_tests;",
            "sim$new_case_tests::feature",
        ),
    ] {
        let (temp, base) = repo();
        let root = temp.path();
        commit(
            root,
            &[
                (test, "#[test]\nfn feature() {}\n"),
                (
                    LIB,
                    &format!("pub fn f() {{}}\npub fn fixed() {{}}\n{mount}\n"),
                ),
            ],
            "feature and its mounted test",
        );
        let head = tip(root);
        let mut calls = 0;
        let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
            calls += 1;
            if calls == 1 {
                let lib = read(snapshot, LIB);
                assert!(lib.contains("mod "), "red must mount the test: {lib}");
                assert!(!lib.contains("fixed"), "no other product line: {lib}");
            }
            Ok(runner(snapshot, test, name))
        })
        .unwrap_or_else(|e| panic!("{test}: {e}"));
        assert_eq!(record.failed, [name]);
    }
}

#[test]
fn a_test_of_a_brand_new_module_gets_a_stub_parent_and_is_compile_red() {
    let (temp, base) = repo();
    let root = temp.path();
    let tests = "crates/sim/src/feature/tests.rs";
    commit(
        root,
        &[
            (LIB, "pub fn f() {}\npub mod feature;\n"),
            (
                "crates/sim/src/feature.rs",
                "pub fn api() -> bool { true }\n#[cfg(test)] mod tests;\n",
            ),
            (tests, "#[test]\nfn works() { assert!(super::api()); }\n"),
        ],
        "new module with tests",
    );
    let head = tip(root);
    let name = "sim$feature::tests::works";
    let error = |items: &[&str]| CheckReport {
        build_failed: true,
        failed: vec!["package build failed: sim".into()],
        compile_errors: vec![CompileError {
            path: tests.into(),
            code: Some("E0425".into()),
            items: items.iter().map(|i| (*i).to_owned()).collect(),
        }],
        ..Default::default()
    };
    let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
        let stub = read(snapshot, "crates/sim/src/feature.rs");
        if !stub.contains("fn api") {
            assert_eq!(
                stub.trim(),
                "#[cfg(test)] mod tests;",
                "a stub holds only the mount"
            );
            assert!(read(snapshot, LIB).contains("mod feature"));
            return Ok(error(&["api"]));
        }
        Ok(CheckReport {
            passed: true,
            tests: vec![name.into()],
            ..Default::default()
        })
    })
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(record.missing, ["api"]);
    assert_eq!(record.ran, [name]);
}

#[test]
fn a_deleted_test_file_is_unmounted_with_the_line_the_pr_removed() {
    let (temp, _) = repo();
    let root = temp.path();
    let tests = "crates/sim/src/tests.rs";
    commit(
        root,
        &[
            (tests, "#[test]\nfn old() {}\n"),
            (LIB, "pub fn f() {}\n#[cfg(test)] mod tests;\n"),
        ],
        "mounted test",
    );
    let base = tip(root);
    fs::remove_file(root.join(tests)).unwrap();
    commit(
        root,
        &[(LIB, "pub fn f() {}\npub fn g() {}\n")],
        "drop test",
    );
    let head = tip(root);
    let mut seen = false;
    let error = red::run_with(root, &base, &head, &mut |snapshot, _| {
        assert!(!snapshot.join(tests).exists());
        let lib = read(snapshot, LIB);
        assert!(!lib.contains("mod tests"), "E0583 otherwise: {lib}");
        assert!(
            lib.contains("pub fn f()") && !lib.contains("pub fn g()"),
            "{lib}"
        );
        seen = true;
        Err("stop".into())
    })
    .unwrap_err();
    assert!(seen && error == "stop", "{error}");
}

#[test]
fn a_test_module_moved_out_of_its_inline_block_replaces_it_at_red() {
    let (temp, _) = repo();
    let root = temp.path();
    commit(
        root,
        &[(
            LIB,
            "pub fn f() {}\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn old() {}\n}\n",
        )],
        "inline tests",
    );
    let base = tip(root);
    let test = "crates/sim/src/tests.rs";
    commit(
        root,
        &[
            (test, "#[test]\nfn old() {}\n#[test]\nfn feature() {}\n"),
            (
                LIB,
                "pub fn f() {}\npub fn fixed() {}\n#[cfg(test)]\nmod tests;\n",
            ),
        ],
        "tests moved to their own file, and a feature",
    );
    let head = tip(root);
    let mut calls = 0;
    let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
        calls += 1;
        if calls == 1 {
            let lib = read(snapshot, LIB);
            assert!(
                !lib.contains("mod tests {"),
                "the inline block is gone: {lib}"
            );
            assert_eq!(
                lib.matches("mod tests").count(),
                1,
                "one tests module: {lib}"
            );
            assert!(lib.contains("pub fn f()"), "product code stays: {lib}");
            assert!(!lib.contains("fixed"), "no other product line: {lib}");
        }
        Ok(runner(snapshot, test, "sim$tests::feature"))
    })
    .expect("the moved module builds once, and its new test is red by name");
    assert_eq!(record.failed, ["sim$tests::feature"]);
}
