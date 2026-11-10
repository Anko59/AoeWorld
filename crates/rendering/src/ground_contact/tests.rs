use super::*;

#[test]
fn corpse_frame_clears_its_last_row_without_moving_its_hotspot() {
    let world = 128.0_f32 / 96.0;
    // DE Scout decay extends forty source pixels below the native hotspot.
    let clearance = frame_clearance(1.5, 160.0 * world, 120.0 * world);
    assert!(clearance * 32.0 > f64::from(40.0 * world));
    assert!(clearance < 2.0);
}

#[test]
fn ordinary_frames_keep_the_shared_ground_clearance() {
    assert_eq!(frame_clearance(1.5, 45.0, 40.0), 1.5);
    assert_eq!(frame_clearance(1.5, 32.0, 48.0), 1.5);
}
