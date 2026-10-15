use super::*;

fn context_count(output: &str) -> Option<usize> {
    retry::context_failures(output).map(|names| names.len())
}

// Panic messages from the real first-attempt diagnostics retained in tests/fixtures.
const CONTEXT: &str =
    "actual WebGL2 context and linked shaders: \"WebGL2 graphics operation failed\"";
const PIXEL_64: &str = "pixel 64,64: channel 0, expected 255";
const PIXEL_32: &str = "pixel 32,32: channel 0, expected 64";

fn failure_output(messages: &[&str]) -> String {
    let mut output = String::from("failures:\n\n");
    for (index, message) in messages.iter().enumerate() {
        output.push_str(&format!(
            "---- webgl_{index} output ----\n    error output:\n        panicked at crates/rendering/src/game_renderer/tests/webgl.rs:289:37:\n        {message}\n\n        Stack:\n\n"
        ));
    }
    // wasm-bindgen-test 0.3.78 reports the caught Rust panic as a JS exception.
    output = output.replace("        Stack:\n\n", "        Stack:\n\n    JS exception that was thrown:\n        RuntimeError: unreachable\n\n");
    output.push_str("failures:\n\n");
    for index in 0..messages.len() {
        output.push_str(&format!("    webgl_{index}\n"));
    }
    output.push_str(&format!(
        "\ntest result: FAILED. 234 passed; {} failed; 0 ignored; 0 filtered out; finished in 72.91s\n",
        messages.len()
    ));
    output
}

#[test]
fn retry_classification_requires_any_named_context_failure() {
    for (name, output, expected) in [
        ("context only", failure_output(&[CONTEXT; 7]), Some(7)),
        (
            "dev and PR401 mixed",
            failure_output(&[CONTEXT, PIXEL_64]),
            Some(1),
        ),
        ("PR401 pixel assertion", failure_output(&[PIXEL_32]), None),
        (
            "mixed pixel assertion",
            failure_output(&[CONTEXT, PIXEL_32]),
            Some(1),
        ),
        (
            "panic",
            failure_output(&["index out of bounds: the len is 0 but the index is 1"]),
            None,
        ),
        ("empty", String::new(), None),
        (
            "compile error",
            format!(
                "error[E0308]: mismatched types\n{}",
                failure_output(&[CONTEXT])
            ),
            None,
        ),
        (
            "unrelated failure alongside context",
            failure_output(&[
                CONTEXT,
                "index out of bounds: the len is 0 but the index is 1",
            ]),
            Some(1),
        ),
        (
            "fatal message inside another named test",
            failure_output(&[CONTEXT, "fatal runtime error: stack overflow"]),
            Some(1),
        ),
        (
            "fatal runner error outside named tests",
            format!(
                "{}fatal runtime error: stack overflow\n",
                failure_output(&[CONTEXT])
            ),
            None,
        ),
        (
            "missing failure detail",
            failure_output(&[CONTEXT]).replace("1 failed", "2 failed"),
            None,
        ),
        (
            "truncated",
            format!("[output capture truncated]\n{}", failure_output(&[CONTEXT])),
            None,
        ),
        (
            "unreported panic",
            format!(
                "{}panicked at helper.rs:1:1:\nboom\n",
                failure_output(&[CONTEXT])
            ),
            None,
        ),
        (
            "value assertion mentioning WebGL",
            failure_output(&["assertion `left == right` failed: WebGL2 graphics operation failed"]),
            None,
        ),
        (
            "context then value assertion",
            failure_output(&[&format!(
                "{CONTEXT}\n        assertion `left == right` failed"
            )]),
            Some(1),
        ),
        (
            "missing named failure",
            failure_output(&[CONTEXT]).replace("    webgl_0\n", "    other\n"),
            None,
        ),
        ("zero failures", failure_output(&[]), None),
        (
            "malformed summary",
            failure_output(&[CONTEXT]).replace("234 passed", "garbage passed"),
            None,
        ),
        (
            "duplicate test name",
            failure_output(&[CONTEXT, PIXEL_32]).replace("webgl_1", "webgl_0"),
            None,
        ),
        (
            "runner abort",
            format!("{}RuntimeError: unreachable\n", failure_output(&[CONTEXT])),
            None,
        ),
        (
            "runner panic after tests",
            format!(
                "{}thread 'main' panicked at runner.rs:1:1:\nboom\n",
                failure_output(&[CONTEXT])
            ),
            None,
        ),
        (
            "runner error after tests",
            format!(
                "{}Error: browser disconnected\n",
                failure_output(&[CONTEXT])
            ),
            None,
        ),
    ] {
        assert_eq!(context_count(&output), expected, "{name}");
    }
}

#[test]
fn context_creation_and_context_reported_shader_failures_are_retryable() {
    for message in [
        "WebGL2 unavailable",
        "WebGL2 context lost",
        "WebGL2 graphics error 1282",
        "WebGL2 shader: compilation failed",
        "WebGL2 link: linking failed",
    ] {
        let message = format!("actual WebGL2 context and linked shaders: {message:?}");
        assert_eq!(context_count(&failure_output(&[&message])), Some(1));
    }
}

#[test]
fn graphics_errors_are_recognized_at_upload_render_and_unwrap_sites() {
    for label in [
        "actual WebGL2 native atlas upload",
        "actual WebGL2 instanced render",
        "called `Result::unwrap()` on an `Err` value",
    ] {
        let message = format!("{label}: \"WebGL2 graphics operation failed\"");
        assert_eq!(context_count(&failure_output(&[&message])), Some(1));
    }
}

#[test]
fn browser_suites_get_more_than_the_runner_default_of_twenty_seconds() {
    let environment = runner_environment("http://127.0.0.1:1/", "caps.json");
    let timeout = environment
        .iter()
        .find(|(name, _)| *name == "WASM_BINDGEN_TEST_TIMEOUT")
        .map(|(_, value)| value.parse::<u64>().expect("seconds"))
        .expect("whole-suite timeout");
    assert!(timeout >= 120 && timeout < CARGO_DEADLINE.as_secs());
}

#[test]
fn current_checkout_has_report_identity_and_a_free_local_port() {
    assert_eq!(git("rev-parse", "HEAD").expect("revision").len(), 40);
    assert!(unused_port().expect("port") > 0);
}

#[test]
fn real_runner_diagnostics_keep_exceptions_console_dump_and_artifact_limits() {
    let pixel = include_str!("tests/fixtures/pixel-38603174196.txt");
    assert!(pixel.contains(PIXEL_32));
    assert!(pixel.contains("JS exception that was thrown:\n        RuntimeError: unreachable"));
    assert_eq!(context_count(pixel), None);
    let dev = include_str!("tests/fixtures/dev-38560009588.txt");
    assert!(dev.contains(CONTEXT) && dev.contains(PIXEL_64));
    assert!(dev.starts_with("[output capture truncated]"));
    assert_eq!(context_count(dev), None);

    // A complete surviving dev failure block, verbatim including its JS exception,
    // with a synthetic single-failure envelope. Never claim this is the complete dev run.
    let name =
        "game_renderer::webgl_tests::webgl_three_native_materials_use_shared_barycentric_weights";
    let block = include_str!("tests/fixtures/context-section.txt");
    let output = format!(
        "failures:\n\n{block}\nfailures:\n\n    {name}\n\ntest result: FAILED. 0 passed; 1 failed; 0 ignored; 0 filtered out; finished in 1.00s\n"
    );
    assert_eq!(context_count(&output), Some(1));
    // The real runner repeats panics under named Invoking test entries after the summary.
    let output = format!(
        "{output}console output:\n    Invoking test: {name}\n    panicked at crates/rendering/src/game_renderer/tests/webgl.rs:289:37:\n    {CONTEXT}\n"
    );
    assert_eq!(context_count(&output), Some(1));
}

#[test]
fn completion_metadata_must_prove_a_normal_exit_one_and_untruncated_capture() {
    use crate::process::CaptureExit;
    let output = failure_output(&[CONTEXT]);
    assert_eq!(
        retry::classify_completed(&CaptureExit::Failed(Some(1)), &output, b"").unwrap(),
        ["webgl_0"]
    );
    for exit in [
        CaptureExit::Success,
        CaptureExit::Failed(Some(2)),
        CaptureExit::Failed(None),
        CaptureExit::Deadline,
        CaptureExit::Cancelled,
    ] {
        assert!(retry::classify_completed(&exit, &output, b"").is_none());
    }
    for stderr in [
        vec![b' '; 64 * 1024],
        vec![0xff],
        b"error[E0308]: mismatched types".to_vec(),
    ] {
        assert!(
            retry::classify_completed(&CaptureExit::Failed(Some(1)), &output, &stderr).is_none()
        );
    }
    let oversized = format!("{}{}", " ".repeat(8 * 1024 * 1024), output);
    assert!(retry::classify_completed(&CaptureExit::Failed(Some(1)), &oversized, b"").is_none());
}
