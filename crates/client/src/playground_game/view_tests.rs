use super::*;

#[test]
fn lookahead_reverses_with_the_pan_and_stops_when_the_camera_stops() {
    let mut motion = PanMotion::default();
    assert_eq!(motion.observe([320.0, 320.0], 1.0), [320.0; 2]);
    let forward = motion.observe([336.0, 328.0], 101.0);
    assert!(forward[0] > 368.0);
    assert!(forward[1] > 344.0);
    let reverse = motion.observe([332.0, 326.0], 117.0);
    assert!(reverse[0] < 332.0);
    assert!(reverse[1] < 326.0);
    assert_eq!(motion.observe([332.0, 326.0], 133.0), [332.0, 326.0]);
    assert_eq!(motion.observe([1400.0, 1200.0], 149.0), [1400.0, 1200.0]);
}

#[test]
fn subscription_guard_reuses_small_pans_and_covers_the_new_view_at_map_edges() {
    let first = TileRect::from_xywh(40, 100, 192, 160);
    let guard = guarded_subscription(first, [2000, 2000]);
    assert!(contains(guard, first));
    assert!(contains(guard, TileRect::from_xywh(72, 80, 192, 160)));
    assert!(!contains(guard, TileRect::from_xywh(150, 180, 192, 160)));

    let edge = TileRect::from_xywh(1488, 1495, 512, 505);
    let guard_at_edge = guarded_subscription(edge, [2000, 2000]);
    assert!(contains(guard_at_edge, edge));
    assert!(guard_at_edge.valid(2000, 2000));
    assert!(guard_at_edge.width() <= aoe_protocol::MAX_SUBSCRIPTION_TILES);
    assert!(guard_at_edge.height() <= aoe_protocol::MAX_SUBSCRIPTION_TILES);
}
