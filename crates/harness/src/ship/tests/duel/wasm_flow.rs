//! A wasm panic through the entry points behind `make duel-check`: native stage, browser stage,
//! the saved `check.json`, and the red/green/prompt decisions made from it.
use super::super::{duel::Env, fixture, run, showcase_security::ENV_LOCK};
use crate::{
    process::CaptureExit,
    ship::{
        duel::{
            authored,
            focus::CheckReport,
            green, implementer_prompt, panic_only_refusal, runner,
            stages::{self, browser_with},
        },
        git,
    },
    wasm_test::SuiteRun,
};
use std::{fs, path::Path};

const SHORE: &str = "crates/rendering/src/surface_mesh/tests/shore.rs";
const FIXTURE: &str = "crates/rendering/src/surface_mesh/tests/fixture.rs";
/// The real nameless-panic output of the third duel: no `... FAIL` line.
const HELPER_PANIC: &str = "running 12 tests\n\ntest surface_mesh::mix::tests::other ... ok\n\npanicked at crates/rendering/src/surface_mesh/tests/fixture.rs:311:5:\nassertion `left != right` failed: case 1: two sheets\n  left: 3\n right: 3\nnote: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\nnote: test exited abnormally\n";
const PASSING: &str = "running 1 test\n\ntest surface_mesh::tests::shore::shore_is_wet ... ok\n";

fn write(root: &Path, path: &str, text: &str) {
    fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
    fs::write(root.join(path), text).unwrap();
}

fn stub(
    output: &'static str,
    exit: CaptureExit,
) -> impl Fn(&Path, &[String], Option<&Path>) -> Result<Vec<SuiteRun>, String> {
    move |_, wasm, _| {
        assert_eq!(wasm, ["aoe-rendering"]);
        Ok(vec![SuiteRun {
            package: "aoe-rendering",
            binary: "lib",
            exit: match exit {
                CaptureExit::Success => CaptureExit::Success,
                _ => CaptureExit::Failed(Some(1)),
            },
            output: output.into(),
        }])
    }
}

fn saved(root: &Path) -> CheckReport {
    serde_json::from_slice(&fs::read(root.join(".cache/duel/check.json")).unwrap()).unwrap()
}

#[test]
fn a_wasm_panic_survives_duel_check_into_the_saved_report_and_every_decision() {
    let _lock = ENV_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let (_temp, root) = fixture("true");
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=['crates/rendering']\n",
    );
    write(
        &root,
        "crates/rendering/Cargo.toml",
        "[package]\nname='aoe-rendering'\n",
    );
    write(&root, "crates/rendering/src/lib.rs", "mod surface_mesh;\n");
    write(
        &root,
        "crates/rendering/src/surface_mesh/mod.rs",
        "mod tests;\n",
    );
    write(
        &root,
        "crates/rendering/src/surface_mesh/tests/mod.rs",
        "mod fixture;\nmod shore;\n",
    );
    write(&root, FIXTURE, "pub fn helper() {}\n");
    write(&root, SHORE, "use super::fixture::helper;\n");
    fs::create_dir_all(root.join(".cache/duel")).unwrap();
    run(&root, &["add", "-A"]);
    run(&root, &["commit", "-qm", "rendering package"]);
    let base = git::git(&root, &["rev-parse", "HEAD"]).unwrap();
    // The tester adds a wasm test and edits the shared helper.
    write(
        &root,
        SHORE,
        "use wasm_bindgen_test::wasm_bindgen_test;\n#[wasm_bindgen_test]\nfn shore_is_wet() {}\n",
    );
    write(&root, FIXTURE, "pub fn helper() { assert_ne!(3, 3); }\n");
    let _env = Env::set(&[
        ("HARNESS_DUEL_BASE", base.clone()),
        ("HARNESS_DUEL_REVISION", String::new()),
    ]);
    let paths = vec![FIXTURE.to_owned(), SHORE.to_owned()];

    // Red: `make duel-check` is the native stage, then the browser stage that runs the wasm suites.
    stages::native(&root).unwrap();
    let panicked = stub(HELPER_PANIC, CaptureExit::Failed(Some(1)));
    let error = browser_with(&root, &panicked).unwrap_err();
    assert!(
        error.contains("fixture.rs:311:5") && error.contains("case 1: two sheets"),
        "{error}"
    );
    let red = saved(&root);
    assert!(!red.passed && red.failed.is_empty());
    assert_eq!(red.panics.len(), 1, "{red:?}");
    assert!(red.panics[0].location.ends_with("fixture.rs:311:5"));
    assert!(red.panics[0].message.contains("case 1: two sheets"));
    // The panic is in a file the tester changed: red, committed. Elsewhere: refused in round 1, dropped later.
    assert!(authored::evidence(&root, &paths, &red).unwrap().0);
    assert!(panic_only_refusal(1, &red, true).is_none());
    let (elsewhere, _) = authored::evidence(&root, &[SHORE.to_owned()], &red).unwrap();
    assert!(!elsewhere);
    let refusal = panic_only_refusal(1, &red, elsewhere).unwrap();
    assert!(refusal.contains("fixture.rs:311:5"), "{refusal}");
    assert!(panic_only_refusal(2, &red, elsewhere).is_none());

    // The implementer is told where and what.
    let prompt = implementer_prompt("Task", "make duel-check", "abc", "tester said", 1, &error);
    assert!(prompt.contains("fixture.rs:311:5") && prompt.contains("case 1: two sheets"));
    assert!(runner::names(&red).contains("fixture.rs:311:5"));

    // Green: the tester's test must be seen running and passing, not just a run without a panic.
    stages::native(&root).unwrap();
    let wrong = stub("test other ... ok\n", CaptureExit::Success);
    browser_with(&root, &wrong).unwrap();
    let error = green::validate(&root, &paths, Some(&base), &red, &saved(&root)).unwrap_err();
    assert!(error.contains("did not run"), "{error}");
    stages::native(&root).unwrap();
    browser_with(&root, &stub(PASSING, CaptureExit::Success)).unwrap();
    green::validate(&root, &paths, Some(&base), &red, &saved(&root)).unwrap();
}
