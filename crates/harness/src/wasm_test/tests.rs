use super::*;

// Exact panic messages from the 2026-10-14 first-attempt logs:
// 38560009588 and 38612471456 (context + pixel 64), 38603174196 (pixel 32).
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
fn retry_classification_requires_every_failure_to_be_only_a_context_error() {
    for (name, output, expected) in [
        ("context only", failure_output(&[CONTEXT; 7]), Some(7)),
        (
            "dev and PR401 mixed",
            failure_output(&[CONTEXT, PIXEL_64]),
            None,
        ),
        ("PR401 pixel assertion", failure_output(&[PIXEL_32]), None),
        (
            "mixed pixel assertion",
            failure_output(&[CONTEXT, PIXEL_32]),
            None,
        ),
        (
            "panic",
            failure_output(&["index out of bounds: the len is 0 but the index is 1"]),
            None,
        ),
        ("empty", String::new(), None),
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
            None,
        ),
        (
            "missing named failure",
            failure_output(&[CONTEXT]).replace("    webgl_0\n", "    other\n"),
            None,
        ),
        ("zero failures", failure_output(&[]), None),
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
        assert_eq!(retry::context_only_failures(&output), expected, "{name}");
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
        assert_eq!(
            retry::context_only_failures(&failure_output(&[&message])),
            Some(1)
        );
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
        assert_eq!(
            retry::context_only_failures(&failure_output(&[&message])),
            Some(1)
        );
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
