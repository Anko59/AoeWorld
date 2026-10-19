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

#[test]
fn disk_hygiene_wasm_logs_survive_snapshot_cleanup_without_retaining_builds() {
    let temp = tempfile::tempdir().unwrap();
    let snapshot = tempfile::tempdir_in(temp.path()).unwrap();
    let root = snapshot.path();
    let paths = rendering(root, WASM_TEST);
    let scope = Scope::from_paths(root, &paths).unwrap();
    let cache = root.join(".cache/duel");
    let retention = temp.path().join("output");
    let runner = |root: &Path, _: &[String], target: Option<&Path>| {
        let target = target
            .map(Path::to_owned)
            .unwrap_or_else(|| root.join("target"));
        write(&target, "object", "compiled output");
        Ok(vec![run(
            "aoe-rendering",
            "lib",
            CaptureExit::Success,
            "test shore_is_wet ... ok\n",
        )])
    };
    let report = super::focus::run_retained(root, &scope, &cache, &retention, &runner).unwrap();
    assert!(report.passed);
    let source = root.to_owned();
    drop(snapshot);
    assert!(!source.exists(), "build output is disposable");
    let logs: Vec<_> = fs::read_dir(&retention)
        .unwrap()
        .map(|entry| entry.unwrap().path().join("wasm.log"))
        .collect();
    assert_eq!(logs.len(), 1);
    assert!(
        fs::read_to_string(&logs[0])
            .unwrap()
            .contains("test shore_is_wet ... ok")
    );
}

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
    assert!(!report.passed && !report.build_failed);
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
fn an_unnamed_wasm_failure_is_unavailable_never_red() {
    // Replaces the removed exit-status fallback (`<pkg>::wasm-suite` as a pseudo-test).
    let crashed = |output: &str| {
        wasm::results(&[run(
            "aoe-rendering",
            "lib",
            CaptureExit::Failed(Some(1)),
            output,
        )])
    };
    for output in [
        "boom\n",
        "test a ... ok\n",
        "error: the lock file needs updating\n",
    ] {
        let error = crashed(output).unwrap_err();
        assert!(
            error.contains("wasm check unavailable") && error.contains("DUEL_CHECK"),
            "{error}"
        );
    }
    // A passing run with no test lines is a pass with nothing named, not a pseudo-test.
    let pass = wasm::results(&[run("aoe-rendering", "lib", CaptureExit::Success, "ok\n")]).unwrap();
    assert!(pass.passed && pass.tests.is_empty() && pass.failed.is_empty());
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

/// Real output of the third duel (dev faa0fa40): a panic in a shared helper aborts the runner
/// after the test lines, with no `... FAIL` line.
const HELPER_PANIC: &str = "running 12 tests\n\ntest surface_mesh::mix::tests::other ... ok\n\npanicked at crates/rendering/src/surface_mesh/mix/tests/fixture.rs:311:5:\nassertion `left != right` failed: case 1: two sheets\n  left: 3\n right: 3\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\nnote: test exited abnormally\n";
const FIXTURE: &str = "crates/rendering/src/surface_mesh/mix/tests/fixture.rs";

fn panicked(output: &str) -> CheckReport {
    wasm::results(&[run(
        "aoe-rendering",
        "lib",
        CaptureExit::Failed(Some(1)),
        output,
    )])
    .unwrap()
}

#[test]
fn a_wasm_panic_is_a_failed_run_naming_the_location_and_assertion() {
    let report = panicked(HELPER_PANIC);
    assert!(!report.passed && !report.build_failed && report.failed.is_empty());
    assert_eq!(report.panics.len(), 1);
    assert_eq!(report.panics[0].location, format!("{FIXTURE}:311:5"));
    assert!(report.panics[0].message.contains("case 1: two sheets"));
    assert!(!report.panics[0].message.contains("RUST_BACKTRACE"));
    let named = super::runner::names(&report);
    assert!(
        named.contains("panicked at") && named.contains("fixture.rs:311:5"),
        "{named}"
    );
    // Aborting after test lines is failed even without a panic line; both texts count.
    for text in ["test exited abnormally", "some tests failed"] {
        let aborted = panicked(&format!("test a ... ok\n{text}\n"));
        assert!(!aborted.passed && aborted.panics.is_empty());
    }
    // Nothing ran: no test line, no panic, no compile error is still unavailable.
    for output in [
        "test exited abnormally\n",
        "",
        "error: the lock file needs updating\n",
    ] {
        let error = wasm::results(&[run(
            "aoe-rendering",
            "lib",
            CaptureExit::Failed(Some(1)),
            output,
        )])
        .unwrap_err();
        assert!(error.contains("wasm check unavailable"), "{error}");
    }
    // Panics are recorded once, bounded, and also next to a named failure.
    let many: String = (0..9)
        .map(|n| format!("panicked at crates/a/src/x.rs:{n}:1:\nboom\n"))
        .collect();
    assert_eq!(panicked(&many).panics.len(), 5);
    let both = panicked(&format!("test a::b ... FAIL\n{HELPER_PANIC}"));
    assert_eq!(both.failed.len(), 1);
    assert_eq!(both.panics.len(), 1);
}

#[test]
fn a_panic_is_red_only_in_a_file_the_tester_changed() {
    let temp = tempfile::tempdir().unwrap();
    let report = panicked(HELPER_PANIC);
    let evidence = |changed: &[&str]| {
        let paths: Vec<String> = changed.iter().map(|p| (*p).to_owned()).collect();
        authored::evidence(temp.path(), &paths, &report).unwrap()
    };
    assert!(evidence(&[FIXTURE]).0);
    let (authored, notes) = evidence(&["crates/rendering/src/other.rs"]);
    assert!(!authored);
    assert!(
        notes.iter().any(
            |n| n.contains("no tester-authored failing test") && n.contains("fixture.rs:311:5")
        ),
        "{notes:?}"
    );
    // Runner-printed paths with `..` components match the changed file's plain path.
    let odd = panicked("panicked at crates/rendering/tests/../../tests/x.rs:4:9:\nboom\n");
    assert_eq!(odd.panics[0].path, "crates/tests/x.rs");
    let paths = vec!["crates/tests/x.rs".to_owned()];
    assert!(authored::evidence(temp.path(), &paths, &odd).unwrap().0);
    assert_eq!(wasm::normalize("./a/b/../c/./d.rs"), "a/c/d.rs");
    assert_eq!(wasm::normalize("../a"), "../a");
}

#[test]
fn green_after_a_panic_only_red_still_requires_the_testers_tests_to_run_and_pass() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let paths = rendering(root, WASM_TEST);
    let red = panicked(HELPER_PANIC);
    let line = |name: &str| format!("test surface_mesh::tests::shore::{name} ... ok\n");
    let pass = |name: &str| {
        wasm::results(&[run(
            "aoe-rendering",
            "lib",
            CaptureExit::Success,
            &line(name),
        )])
        .unwrap()
    };
    green::validate(root, &paths, None, &red, &pass("shore_is_wet")).unwrap();
    assert!(
        green::validate(root, &paths, None, &red, &pass("other"))
            .unwrap_err()
            .contains("did not run")
    );
}
#[test]
fn red_proof_accepts_real_wasm_named_assertion_output() {
    use crate::ship::{git, test_first::red, tests::run};
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    run(root, &["init", "-q", "-b", "feature"]);
    let path = "crates/rendering/src/game_renderer/tests/webgl.rs";
    for (path, text) in [
        (
            "crates/rendering/Cargo.toml",
            "[package]\nname = 'aoe-rendering'\nversion = '0.1.0'\n",
        ),
        ("crates/rendering/src/lib.rs", "mod game_renderer;\n"),
        (
            "crates/rendering/src/game_renderer/mod.rs",
            "#[path = \"tests/webgl.rs\"] mod webgl_tests;\n",
        ),
        (path, "// no tests yet\n"),
    ] {
        fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
        fs::write(root.join(path), text).unwrap();
    }
    run(root, &["add", "-A"]);
    run(root, &["commit", "-qm", "base"]);
    let base = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let function = "webgl_solid_surface_mode_does_not_sample_empty_atlas_or_degenerate_quad";
    fs::write(
        root.join(path),
        format!("#[wasm_bindgen_test]\nfn {function}() {{ assert!(false); }}\n"),
    )
    .unwrap();
    run(root, &["add", "-A"]);
    run(root, &["commit", "-qm", "tests"]);
    fs::create_dir(root.join("web")).unwrap();
    fs::write(root.join("web/index.html"), "fixed").unwrap();
    run(root, &["add", "-A"]);
    run(root, &["commit", "-qm", "fix"]);
    let head = git::git(root, &["rev-parse", "HEAD"]).unwrap();
    let name = format!("aoe-rendering$game_renderer::webgl_tests::{function}");
    let fixture = include_str!("../../../wasm_test/tests/fixtures/pixel-38603174196.txt");
    let record = red::run_with(root, &base, &head, &mut |snapshot, _| {
        if snapshot.join("web/index.html").exists() {
            Ok(CheckReport {
                passed: true,
                tests: vec![name.clone()],
                ..Default::default()
            })
        } else if fs::read_to_string(snapshot.join(path))
            .unwrap()
            .contains(function)
        {
            let report = wasm::results(&[SuiteRun {
                package: "aoe-rendering",
                binary: "lib",
                exit: CaptureExit::Failed(Some(1)),
                output: fixture.into(),
            }])
            .unwrap();
            assert_eq!(report.failed.as_slice(), std::slice::from_ref(&name));
            assert!(!report.panics.is_empty());
            assert!(report.panics.iter().all(|p| p.test.as_ref() == Some(&name)));
            Ok(report)
        } else {
            Ok(CheckReport {
                passed: true,
                ..Default::default()
            })
        }
    })
    .unwrap();
    assert_eq!(record.failed, [name]);
}
