#[test]
fn watchdog_kill_is_bounded_to_four_exact_container_ids() {
    let four: Vec<_> = (0..4).map(|_| "a".repeat(64)).collect();
    assert!(super::validate_ids(&four).is_ok());
    assert!(super::validate_ids(&vec!["a".repeat(64); 5]).is_err());
    assert!(super::validate_ids(&["other-container".into()]).is_err());
}
