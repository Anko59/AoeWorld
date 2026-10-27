use super::*;

#[test]
fn format_validation_includes_the_fuzz_workspace() {
    assert_eq!(
        format_invocations(false),
        [
            vec!["fmt", "--all"],
            vec!["fmt", "--manifest-path", "fuzz/Cargo.toml"],
        ]
    );
    assert_eq!(
        format_invocations(true),
        [
            vec!["fmt", "--all", "--", "--check"],
            vec!["fmt", "--manifest-path", "fuzz/Cargo.toml", "--", "--check"],
        ]
    );
}
