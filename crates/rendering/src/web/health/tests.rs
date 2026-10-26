use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn continuous_queue_progress_refreshes_the_unchanged_five_second_deadline() {
    let health = Health::default();
    {
        let mut state = health.state();
        state.frames = 2;
        state.waiting_since = Some(now() - DEADLINE_MS - 1.0);
    }
    health.completed(false);
    assert!(
        health.check().is_ok(),
        "one frame completed; the queue is progressing"
    );
    assert_eq!(health.state().frames, 1);
    health.completed(false);
    assert!(health.state().waiting_since.is_none());
}

#[wasm_bindgen_test]
fn a_queue_without_progress_still_expires_after_five_seconds() {
    let health = Health::default();
    let mut state = health.state();
    state.frames = 1;
    state.waiting_since = Some(now() - DEADLINE_MS - 1.0);
    drop(state);
    assert!(health.check().is_err());
}
