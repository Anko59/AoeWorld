use super::shot_clock;
use wasm_bindgen_test::wasm_bindgen_test;

// An archer's attack: ten 70 ms frames released at frame 5, reloading
// every 2 s, at 20 ticks a second.
const ANIMATION: (f64, f64) = (700.0, 70.0);
fn clock(shot: Option<u64>, now: f64) -> f64 {
    shot_clock(shot, now, 20.0, 2000.0, ANIMATION, 5)
}

#[wasm_bindgen_test]
fn the_release_frame_plays_on_the_tick_of_every_shot() {
    for k in 0..3 {
        assert_eq!(clock(Some(100), 100.0 + 40.0 * f64::from(k)), 350.0);
    }
}

#[wasm_bindgen_test]
fn the_windup_plays_before_the_next_shot_and_the_rest_after_it() {
    // 350 ms before the second shot the animation starts at its first frame.
    assert_eq!(clock(Some(100), 140.0 - 7.0), 0.0);
    assert_eq!(clock(Some(100), 140.0 - 3.5), 175.0);
    // 345 ms after a shot the animation is at its last frame (frame 9).
    assert!((clock(Some(100), 106.9) - 695.0).abs() < 1e-6);
}

#[wasm_bindgen_test]
fn the_first_frame_is_held_between_swings_before_a_shot_and_after_a_pause() {
    assert_eq!(clock(Some(100), 112.0), 0.0);
    assert_eq!(clock(None, 112.0), 0.0);
    assert_eq!(clock(Some(100), 100.0 + 20.0 * 7.0), 0.0);
    // A shot newer than the clock (clock skew) holds the release frame.
    assert_eq!(clock(Some(100), 99.0), 350.0);
}
