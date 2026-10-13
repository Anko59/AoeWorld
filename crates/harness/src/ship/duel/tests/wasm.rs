use super::{authored, focus::CheckReport, green, output, scope::Scope, wasm};
use crate::{
    process::CaptureExit,
    wasm_test::{self, SuiteRun},
};
use std::{fs, path::Path};

fn write(root: &Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}

fn rendering(root: &Path, shore: &str) -> Vec<String> {
    write(
        root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/rendering','crates/x']\n",
    );
    write(
        root,
        "crates/rendering/Cargo.toml",
        "[package]\nname='aoe-rendering'\n",
    );
    write(root, "crates/x/Cargo.toml", "[package]\nname='aoe-x'\n");
    write(root, "crates/rendering/src/lib.rs", "mod surface_mesh;\n");
    write(
        root,
        "crates/rendering/src/surface_mesh/mod.rs",
        "mod tests;\n",
    );
    write(
        root,
        "crates/rendering/src/surface_mesh/tests/mod.rs",
        "mod shore;\n",
    );
    let shore_path = "crates/rendering/src/surface_mesh/tests/shore.rs";
    write(root, shore_path, shore);
    vec![shore_path.into()]
}

const WASM_TEST: &str =
    "use wasm_bindgen_test::wasm_bindgen_test;\n#[wasm_bindgen_test]\nfn shore_is_wet() {}\n";

fn run(package: &'static str, binary: &'static str, exit: CaptureExit, output: &str) -> SuiteRun {
    SuiteRun {
        package,
        binary,
        exit,
        output: output.into(),
    }
}

#[test]
fn the_wasm_packages_are_the_ones_test_wasm_runs() {
    let mut from_arguments: Vec<String> = wasm_test::SUITES
        .iter()
        .map(|suite| {
            let arguments = suite.cargo_arguments();
            let at = arguments.iter().position(|a| a == "-p").unwrap();
            arguments[at + 1].clone()
        })
        .collect();
    from_arguments.dedup();
    assert_eq!(wasm_test::packages(), from_arguments);
    assert!(wasm_test::packages().contains(&"aoe-rendering"));
}

#[test]
fn a_changed_wasm_test_file_selects_the_wasm_suite_not_native_nextest() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let paths = rendering(root, WASM_TEST);
    let scope = Scope::from_paths(root, &paths).unwrap();
    assert_eq!(scope.wasm, ["aoe-rendering"]);
    assert!(scope.packages.is_empty(), "{:?}", scope.packages);
    assert_eq!(scope.manifests, ["crates/rendering/Cargo.toml"]);
    // Native and wasm tests in one file need both runners.
    write(
        root,
        &paths[0],
        &format!("{WASM_TEST}#[test]\nfn native() {{}}\n"),
    );
    let both = Scope::from_paths(root, &paths).unwrap();
    assert_eq!(
        (both.wasm, both.packages),
        (vec!["aoe-rendering".into()], vec!["aoe-rendering".into()])
    );
    // A native-only file of the same package, and any file of another package, stay native.
    write(root, &paths[0], "#[test]\nfn native() {}\n");
    let native = Scope::from_paths(root, &paths).unwrap();
    assert!(native.wasm.is_empty() && native.packages == ["aoe-rendering"]);
    write(
        root,
        "crates/x/tests/t.rs",
        "#[wasm_bindgen_test]\nfn t() {}\n",
    );
    let other = Scope::from_paths(root, &["crates/x/tests/t.rs".into()]).unwrap();
    assert!(other.wasm.is_empty() && other.packages == ["aoe-x"]);
}

#[test]
fn wasm_results_name_passed_and_failed_tests_like_nextest() {
    let output = "running 2 tests\n\ntest aoe_rendering::surface_mesh::tests::shore::a ... ok\ntest aoe_rendering::surface_mesh::tests::shore::b ... FAILED\n\nfailures:\n";
    let report = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(1)),
        output,
    )])
    .unwrap();
    assert!(!report.passed && !report.build_failed && report.exit_only.is_empty());
    assert_eq!(
        report.failed,
        ["aoe-rendering$surface_mesh::tests::shore::b"]
    );
    assert_eq!(report.tests.len(), 2);
    let green = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Success,
        &output.replace("FAILED", "ok"),
    )])
    .unwrap();
    assert!(green.passed && green.failed.is_empty());
    let integration = wasm::results(&[run(
        "aoe-client",
        "browser",
        CaptureExit::Success,
        "test browser::boots ... ok\n",
    )])
    .unwrap();
    assert_eq!(integration.tests, ["aoe-client::browser$boots"]);
}

#[test]
fn wasm_build_failure_is_build_failed_and_a_missing_tool_is_unavailable() {
    let build =
        "error[E0425]: cannot find value\nerror: could not compile `aoe-rendering` (lib test)\n";
    let report = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(101)),
        build,
    )])
    .unwrap();
    assert!(
        report.build_failed
            && !report.passed
            && report.failed[0].contains("wasm package build failed")
    );
    let missing =
        "error: could not execute process `wasm-bindgen-test-runner`: No such file or directory\n";
    let error = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(101)),
        missing,
    )])
    .unwrap_err();
    assert!(error.contains("wasm check unavailable"), "{error}");
    let error =
        wasm::results(&[run("aoe-rendering", "lib", CaptureExit::Deadline, "")]).unwrap_err();
    assert!(error.contains("wasm check unavailable"), "{error}");
}

#[test]
fn unnamed_wasm_results_fall_back_to_exit_status_and_say_so() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let paths = rendering(root, WASM_TEST);
    let red = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(1)),
        "boom\n",
    )])
    .unwrap();
    assert!(!red.passed && red.exit_only == red.failed);
    let (authored, notes) = authored::evidence(root, &paths, &red).unwrap();
    assert!(
        authored && notes[0].contains("judged by exit status"),
        "{notes:?}"
    );
    let pass = wasm::results(&[run("aoe-rendering", "lib", CaptureExit::Success, "ok\n")]).unwrap();
    assert!(pass.passed);
    green::validate(root, &paths, None, &red, &pass).unwrap();
}

#[test]
fn a_failing_wasm_test_in_a_changed_file_is_red_evidence_and_green_requires_it_to_pass() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let paths = rendering(root, WASM_TEST);
    let line = |status| format!("test surface_mesh::tests::shore::shore_is_wet ... {status}\n");
    let red = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(1)),
        // The real wasm-bindgen-test-runner prints `FAIL`, without the crate prefix.
        &line("FAIL"),
    )])
    .unwrap();
    assert_eq!(
        red.failed,
        ["aoe-rendering$surface_mesh::tests::shore::shore_is_wet"]
    );
    assert!(red.exit_only.is_empty());
    assert!(authored::evidence(root, &paths, &red).unwrap().0);
    let pass = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Success,
        &line("ok"),
    )])
    .unwrap();
    green::validate(root, &paths, None, &red, &pass).unwrap();
    // After a build failure, the tester's wasm test must be seen running and passing.
    let build_red = CheckReport {
        build_failed: true,
        ..Default::default()
    };
    green::validate(root, &paths, None, &build_red, &pass).unwrap();
    let none = wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Success,
        "test other ... ok\n",
    )])
    .unwrap();
    assert!(
        green::validate(root, &paths, None, &build_red, &none)
            .unwrap_err()
            .contains("did not run")
    );
    // A failure in a file the tester did not change is not red evidence.
    assert!(
        !authored::evidence(root, &["crates/x/tests/t.rs".into()], &red)
            .unwrap()
            .0
    );
}

#[test]
fn pruning_a_cache_that_never_existed_is_not_an_error() {
    let temp = tempfile::tempdir().unwrap();
    output::prune(&temp.path().join(".cache/duel"), 0).unwrap();
    fs::create_dir_all(temp.path().join("c/run-1")).unwrap();
    output::prune(&temp.path().join("c"), 0).unwrap();
    assert!(!temp.path().join("c/run-1").exists());
}
