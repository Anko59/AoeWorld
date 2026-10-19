use super::*;

#[test]
fn flaky_wasm_pixel_failures_are_named_without_becoming_context_errors() {
    let output = failure_output(&[PIXEL_64, PIXEL_32]);
    assert_eq!(
        retry::failed_tests(&crate::process::CaptureExit::Failed(Some(1)), &output, b""),
        Some(vec!["webgl_0".into(), "webgl_1".into()])
    );
    assert_eq!(context_count(&output), None);
    for exit in [
        crate::process::CaptureExit::Success,
        crate::process::CaptureExit::Failed(Some(2)),
        crate::process::CaptureExit::Deadline,
    ] {
        assert!(retry::failed_tests(&exit, &output, b"").is_none());
    }
    assert!(
        retry::failed_tests(
            &crate::process::CaptureExit::Failed(Some(1)),
            &output.replace("2 failed", "3 failed"),
            b""
        )
        .is_none()
    );
    assert!(
        retry::failed_tests(
            &crate::process::CaptureExit::Failed(Some(1)),
            &output,
            &vec![b'x'; 65536]
        )
        .is_none()
    );
}
