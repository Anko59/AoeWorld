use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn far_build_admission_honours_twenty_burst_and_ten_per_second_and_pending_ack() {
    use super::super::construction::strategic::Admission;
    let mut budget = Admission::default();
    for _ in 0..20 {
        assert!(budget.ready(0.0));
        budget.sent(0.0);
    }
    assert!(!budget.ready(0.0));
    assert!(!budget.ready(99.0));
    assert!(budget.ready(100.0));
    budget.sent(100.0);
    budget.pending = Some(1);
    assert!(!budget.ready(200.0));
    budget.pending = None;
    assert!(budget.ready(200.0));
}
