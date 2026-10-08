use super::*;
use aoe_core::TileCoord;
use wasm_bindgen_test::wasm_bindgen_test;

fn camera(zoom: f64) -> SceneCamera {
    SceneCamera {
        center: [0.0; 2],
        zoom,
        viewport: [256.0; 2],
        focus_elevation_meters: 800.0,
    }
}
fn bounds() -> TileRect {
    TileRect::from_xywh(-8, -8, 16, 16)
}
fn world_lines(camera: SceneCamera, bounds: TileRect) -> Vec<(usize, i64)> {
    grid_lines(camera, bounds)
        .into_iter()
        .map(|(a, b)| {
            let projection = camera_projection(camera);
            let a = projection.screen_to_world_at_height(a, camera.focus_elevation_meters);
            let b = projection.screen_to_world_at_height(b, camera.focus_elevation_meters);
            let axis = usize::from((a[1] - b[1]).abs() < (a[0] - b[0]).abs());
            assert!((a[axis] - b[axis]).abs() < 0.000001);
            assert!((a[axis] - a[axis].round()).abs() < 0.000001);
            (axis, a[axis].round() as i64)
        })
        .collect()
}

#[wasm_bindgen_test]
fn adaptive_grid_is_world_anchored_uniform_power_two_and_negative_aligned() {
    for (zoom, step) in [(1.0, 1), (0.25, 2), (0.125, 4), (0.0625, 8)] {
        let mut c = camera(zoom);
        let original = world_lines(c, bounds());
        assert!(!original.is_empty());
        for &(axis, value) in &original {
            assert_eq!(
                value.rem_euclid(step),
                0,
                "axis {axis} must use absolute multiples"
            );
        }
        for axis in [0, 1] {
            let values = original
                .iter()
                .filter(|(kind, _)| *kind == axis)
                .map(|(_, value)| *value)
                .collect::<Vec<_>>();
            assert!(values.windows(2).all(|pair| pair[1] - pair[0] == step));
        }
        let separation = aoe_core::ISO_TILE_WIDTH * aoe_core::ISO_TILE_HEIGHT
            / aoe_core::ISO_TILE_WIDTH.hypot(aoe_core::ISO_TILE_HEIGHT)
            * zoom
            * step as f64;
        assert!(separation >= 24.0);
        if step > 1 {
            assert!(separation * 0.5 < 24.0);
        }
        c.center = [-0.75, 0.5];
        let panned = world_lines(c, TileRect::from_xywh(-9, -7, 16, 16));
        assert!(!panned.is_empty());
        assert!(panned.iter().all(|(_, value)| value.rem_euclid(step) == 0));
        // A changed visible minimum cannot re-phase the grid. Interior common
        // world lines retain their actual integer coordinate on both axes.
        for line in original.iter().filter(|(_, value)| value.abs() <= 2) {
            assert!(panned.contains(line));
        }
    }
}

#[wasm_bindgen_test]
fn grid_packets_are_bounded_finite_after_overlay_normalization_and_focus_relative() {
    let whole = TileRect {
        min: TileCoord {
            x: i32::MIN,
            y: i32::MIN,
        },
        max: TileCoord {
            x: i32::MAX,
            y: i32::MAX,
        },
    };
    for zoom in [0.000001, 0.0001, 0.01, 1.0, 100.0, f64::MAX] {
        let c = camera(zoom);
        let lines = grid_lines(c, whole);
        assert!(lines.len() <= 256);
        let mut sprites = grid_sprites(c, whole);
        assert!(sprites.len() <= 512);
        assert_eq!(sprites.len() % 2, 0);
        for sprite in &sprites {
            assert_eq!(sprite.color[3], -2.0);
            assert_eq!(sprite.uv, GRID_COLOR);
            assert_eq!(sprite.depths, [f32::INFINITY; 4]);
            assert_eq!(sprite.pages, [0; 4]);
            assert!(
                sprite
                    .position
                    .iter()
                    .chain(&sprite.radius)
                    .chain(&sprite.color)
                    .chain(&sprite.uv)
                    .all(|value| value.is_finite())
            );
        }
        crate::web::normalize_depths(&mut sprites);
        for sprite in sprites {
            assert_eq!(sprite.depths, [0.0; 4]);
        }
    }
    let c = camera(0.25);
    let mut ground = c;
    ground.focus_elevation_meters = 0.0;
    assert_eq!(
        grid_lines(c, bounds()),
        grid_lines(ground, bounds()),
        "grid is a focus-relative plane, not terrain-following"
    );
}

#[wasm_bindgen_test]
fn invalid_grid_camera_bounds_and_degenerate_clips_are_empty() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for field in 0..6 {
            let mut c = camera(1.0);
            match field {
                0 => c.zoom = value,
                1 => c.center[0] = value,
                2 => c.center[1] = value,
                3 => c.viewport[0] = value,
                4 => c.viewport[1] = value,
                _ => c.focus_elevation_meters = value,
            }
            assert!(grid_sprites(c, bounds()).is_empty());
        }
    }
    for value in [0.0, -1.0] {
        let mut c = camera(value);
        assert!(grid_sprites(c, bounds()).is_empty());
        c = camera(1.0);
        c.viewport[0] = value;
        assert!(grid_sprites(c, bounds()).is_empty());
    }
    for b in [
        TileRect::from_xywh(0, 0, 0, 8),
        TileRect::from_xywh(0, 0, 8, 0),
        TileRect::from_xywh(8, 8, -4, -4),
    ] {
        assert!(grid_sprites(camera(1.0), b).is_empty());
    }
    assert!(
        clip_line(
            ScreenPoint { x: 1.0, y: 1.0 },
            ScreenPoint { x: 1.0, y: 1.0 },
            [128.0; 2]
        )
        .is_none()
    );
}
