use super::*;

#[test]
fn camera_center_stays_inside_world_without_hiding_the_edges() {
    let config = WorldConfig::new(10, 8, crate::Seed(1)).unwrap();
    for (center, expected) in [
        ([0.0, 0.0], [0.0, 0.0]),
        ([1000.0, 1000.0], [10.0, 8.0]),
        ([-2.0, -3.0], [0.0, 0.0]),
    ] {
        let camera = Camera {
            center,
            zoom: 1.0,
            viewport: [512.0, 256.0],
            focus_elevation_meters: 0.0,
        };
        assert_eq!(camera.clamp_center(config).center, expected);
    }
}

#[test]
fn far_zoom_clamping_is_idempotent_when_only_one_world_axis_fits() {
    let config = WorldConfig::new(512, 512, crate::Seed(1)).unwrap();
    let camera = Camera {
        center: [150.0, 180.0],
        zoom: 0.025,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let once = camera.clamp_center(config);
    let twice = once.clamp_center(config);
    assert_eq!(
        once.center, twice.center,
        "a stationary camera must not flicker"
    );
    assert_eq!(once.center, camera.center, "map edges remain inspectable");
}
