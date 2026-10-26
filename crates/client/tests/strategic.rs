use aoe_client::strategic::{Lod, lod};

#[test]
fn tile_grid_zoom_keeps_detailed_ground_before_strategic_lod() {
    use aoe_client::strategic::lod_for_viewport;
    // The grid pixel regression zooms from 1.0 with a 1500 wheel delta.
    // Keep its readable tile borders; switch to markers only farther out.
    let grid_zoom = (-1500.0_f64 * 0.0015).exp();
    assert_eq!(lod_for_viewport(grid_zoom, [1280.0, 720.0]), Lod::Sprites);
    assert_eq!(lod_for_viewport(0.0999, [1280.0, 720.0]), Lod::Markers);
}

#[test]
fn lod_thresholds_keep_sprites_near_and_bound_distant_views() {
    assert_eq!(lod(3.0), Lod::Sprites);
    assert_eq!(lod(0.25), Lod::Sprites);
    assert_eq!(lod(0.125), Lod::Sprites);
    // The detailed range extends through the grid's far zoom (see above).
    assert_eq!(lod(0.1), Lod::Sprites);
    assert_eq!(lod(0.0999), Lod::Markers);
    assert_eq!(lod(0.03125), Lod::Markers);
    assert_eq!(lod(0.03124), Lod::Map);
    assert_eq!(lod(1.0 / 4096.0), Lod::Map);
}

#[test]
fn strategic_camera_covers_a_16k_world_and_keeps_pointer_anchor() {
    use aoe_core::{Camera, ScreenPoint, WorldConfig};
    let camera = Camera::new([8192.0; 2], [1280.0, 720.0]);
    let pointer = ScreenPoint { x: 700.0, y: 300.0 };
    let before = camera.screen_to_world(pointer);
    let distant = camera.zoom_around(pointer, 0.00001);
    assert_eq!(distant.zoom, 1.0 / 4096.0);
    let after = distant.screen_to_world(pointer);
    for axis in 0..2 {
        assert!((before[axis] - after[axis]).abs() < 1e-8);
    }
    let bounds = distant.visible_tiles(WorldConfig::default(), 0.0);
    assert_eq!(bounds.width(), 16384);
    assert_eq!(bounds.height(), 16384);
    let centered = Camera::new([256.0; 2], [1280.0, 720.0])
        .zoom_around(pointer, 0.001)
        .clamp_center(WorldConfig {
            width_tiles: 512,
            height_tiles: 512,
            ..WorldConfig::default()
        });
    assert_eq!(centered.center, [256.0; 2]);
}

#[test]
fn lod_budgets_large_backing_canvases_as_well_as_small_ones() {
    use aoe_client::strategic::lod_for_viewport;
    assert_eq!(lod_for_viewport(0.125, [1280.0, 720.0]), Lod::Sprites);
    assert_eq!(lod_for_viewport(0.125, [4096.0, 1024.0]), Lod::Markers);
    assert_eq!(lod_for_viewport(1.0, [4096.0, 1024.0]), Lod::Sprites);
}

#[test]
fn strategic_focus_advances_one_lod_on_normal_and_large_canvases() {
    use aoe_client::strategic::{focus_zoom, lod_for_viewport};
    for viewport in [[1280.0, 720.0], [4096.0, 1024.0]] {
        let marker = focus_zoom(1.0 / 4096.0, viewport);
        assert_eq!(lod_for_viewport(marker, viewport), Lod::Markers);
        let sprites = focus_zoom(marker, viewport);
        assert_eq!(lod_for_viewport(sprites, viewport), Lod::Sprites);
        assert_eq!(focus_zoom(3.0, viewport), 3.0);
    }
}
