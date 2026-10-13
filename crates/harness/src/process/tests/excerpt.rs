use super::*;
use crate::process::excerpt::excerpt_for;

const REAL: &str = include_str!("fixtures/wasm_failure.txt");

fn failed(stdout: &str, stderr: &str) -> Captured {
    Captured {
        exit: CaptureExit::Failed(Some(1)),
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        truncated: false,
        duration: Duration::ZERO,
    }
}

fn excerpt(stdout: &str, stderr: &str) -> String {
    excerpt_for(&failed(stdout, stderr), "run.log").expect("excerpt")
}

#[test]
fn real_wasm_failure_names_the_test_the_panic_and_the_totals() {
    let (stdout, stderr) = REAL.split_once("--- stderr ---").expect("fixture split");
    let text = excerpt(stdout, stderr);
    assert!(text.contains(
        "webgl_four_material_junction_has_matching_pixels_on_every_shared_edge ... FAIL"
    ));
    assert!(
        text.contains("crates/rendering/src/surface_mesh/mix/tests/../../tests/junction.rs:111:5")
    );
    assert!(text.contains("left: 0") && text.contains("right: 255"));
    assert!(text.contains("192 passed; 1 failed"));
    assert!(!text.contains("... ok"));
    assert_eq!(
        text.matches("panicked at").count(),
        1,
        "duplicates collapse:\n{text}"
    );
}

#[test]
fn nextest_failures_are_listed() {
    let text = excerpt(
        "        FAIL [   0.012s] aoe-sim tick::moves_unit\n  PASS [ 0.1s] aoe-sim ok\n",
        "",
    );
    assert!(text.contains("FAIL [   0.012s] aoe-sim tick::moves_unit"));
    assert!(!text.contains("PASS"));
}

#[test]
fn compile_errors_keep_their_location() {
    let text = excerpt(
        "",
        "   Compiling x\nerror[E0425]: cannot find value `y`\n --> src/lib.rs:3:5\n  |\n3 | y\nwarning: unused\n",
    );
    assert!(text.contains("error[E0425]"));
    assert!(text.contains("--> src/lib.rs:3:5"));
    assert!(!text.contains("warning"));
}

#[test]
fn unrecognised_output_falls_back_to_the_last_lines() {
    let body: String = (1..=30).map(|n| format!("line {n}\n\n")).collect();
    let text = excerpt(&body, "");
    assert!(text.contains("line 30") && text.contains("line 16"));
    assert!(!text.contains("line 15\n"));
}

#[test]
fn excerpt_is_bounded_in_lines_bytes_and_line_length() {
    let many: String = (0..500)
        .map(|n| format!("test case_{n} ... FAILED\n"))
        .collect();
    let text = excerpt(&many, "");
    assert!(text.len() <= MAX_BYTES_WITH_NOTE, "{} bytes", text.len());
    assert!(text.lines().count() <= 42);
    assert!(text.contains("more in run.log)"));
    let long = format!("test {} ... FAILED\n", "x".repeat(10_000));
    assert!(excerpt(&long, "").len() < 400);
}

const MAX_BYTES_WITH_NOTE: usize = 4096 + 64;

#[test]
fn success_and_start_failures_print_nothing() {
    let mut captured = failed("test a ... FAILED\n", "");
    captured.exit = CaptureExit::Success;
    assert!(excerpt_for(&captured, "run.log").is_none());
    captured.exit = CaptureExit::Start(io::Error::other("missing"));
    assert!(excerpt_for(&captured, "run.log").is_none());
}

#[test]
fn deadline_and_cancellation_also_get_an_excerpt() {
    for exit in [CaptureExit::Deadline, CaptureExit::Cancelled] {
        let mut captured = failed("test a ... FAILED\n", "");
        captured.exit = exit;
        assert!(excerpt_for(&captured, "run.log").is_some());
    }
}
