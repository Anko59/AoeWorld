use super::visible;

#[test]
fn hud_culling_keeps_marker_heads_at_the_viewport_edge() {
    let size = [1000.0, 700.0];
    assert!(visible([500.0, 900.0], 690.0, size, false));
    assert!(visible([-32.0, 200.0], 150.0, size, false));
    assert!(!visible([1100.0, 200.0], 150.0, size, false));
    assert!(!visible([500.0, 900.0], 800.0, size, false));
}

#[test]
fn offscreen_selection_stays_available_without_retaining_the_world() {
    let size = [1000.0, 700.0];
    for point in [[-10000.0, 200.0], [500.0, 10000.0], [10000.0, -10000.0]] {
        assert!(!visible(point, point[1] - 40.0, size, false));
        assert!(visible(point, point[1] - 40.0, size, true));
    }
    assert!(!visible([f64::NAN, 200.0], 160.0, size, false));
}
