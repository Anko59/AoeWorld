use super::*;
use std::collections::BTreeSet;
use wasm_bindgen_test::wasm_bindgen_test;

// Frozen pre-optimization selector; preserve its spatial arithmetic and order.
fn reference(
    camera: Camera,
    config: aoe_core::WorldConfig,
    bounds: (i16, i16),
    capacity: usize,
) -> Vec<(i32, i32)> {
    if capacity == 0 {
        return Vec::new();
    }
    let visible = visible_tiles_for_height_bounds(
        camera,
        config,
        camera.focus_elevation_meters,
        Some(bounds),
    );
    let horizontal = camera.viewport[0] / (aoe_core::ISO_TILE_WIDTH * camera.zoom) + 16.0;
    let difference = camera.center[0] - camera.center[1];
    let center = [
        camera.center[0].round() as i64,
        camera.center[1].round() as i64,
    ];
    let mut nearest = BTreeSet::new();
    for y in visible.min.y.div_euclid(CHUNK_TILES)
        ..=visible.max.y.saturating_sub(1).div_euclid(CHUNK_TILES)
    {
        let low = (f64::from(y * CHUNK_TILES) + difference - horizontal).floor() as i32;
        let high = (f64::from((y + 1) * CHUNK_TILES) + difference + horizontal).ceil() as i32;
        let minimum = low.max(visible.min.x).div_euclid(CHUNK_TILES);
        let maximum = high
            .min(visible.max.x.saturating_sub(1))
            .div_euclid(CHUNK_TILES);
        for x in minimum..=maximum {
            let dx = i64::from(x * CHUNK_TILES + CHUNK_TILES / 2) - center[0];
            let dy = i64::from(y * CHUNK_TILES + CHUNK_TILES / 2) - center[1];
            nearest.insert((dx * dx + dy * dy, x, y));
            if nearest.len() > capacity {
                nearest.pop_last();
            }
        }
    }
    nearest.into_iter().map(|(_, x, y)| (x, y)).collect()
}

#[wasm_bindgen_test]
fn heap_candidates_match_old_set_for_camera_height_border_and_capacity_sweeps() {
    for (width, height) in [(32, 32), (50, 70), (1024, 1536), (20_000, 20_000)] {
        let config = aoe_core::WorldConfig::new(width, height, aoe_core::Seed(9)).unwrap();
        for center in [
            [0.0, 0.0],
            [32.0, 32.0],
            [f64::from(width) * 0.5, f64::from(height) * 0.5],
            [f64::from(width) - 0.5, f64::from(height) - 0.5],
        ] {
            for (zoom, viewport, focus, bounds) in [
                (1.0, [800.0, 600.0], 0.0, (0, 0)),
                (0.25, [4096.0, 1024.0], 7.0, (-80, 80)),
                (2.0, [512.0, 256.0], 52.0, (0, 52)),
                (0.125, [1600.0, 900.0], -30.0, (-300, 500)),
            ] {
                let camera = Camera {
                    center,
                    zoom,
                    viewport,
                    focus_elevation_meters: focus,
                };
                for capacity in [0, 1, 64, 512] {
                    let expected = reference(camera, config, bounds, capacity);
                    let actual = candidate_chunks(camera, config, bounds, capacity);
                    assert_eq!(
                        actual, expected,
                        "{width}x{height} {center:?} {zoom} {bounds:?} cap={capacity}"
                    );
                    assert!(actual.len() <= capacity);
                    assert_eq!(
                        actual.iter().copied().collect::<BTreeSet<_>>().len(),
                        actual.len()
                    );
                }
            }
        }
    }
}

#[wasm_bindgen_test]
fn equal_distance_candidates_keep_ascending_coordinate_ties_and_spatial_counts() {
    let config = aoe_core::WorldConfig::new(1024, 1024, aoe_core::Seed(1)).unwrap();
    let camera = Camera {
        center: [32.0, 32.0],
        zoom: 1.0,
        viewport: [4096.0, 4096.0],
        focus_elevation_meters: 0.0,
    };
    assert_eq!(candidate_chunks(camera, config, (0, 0), 1), vec![(0, 0)]);
    let wide_camera = Camera {
        center: [512.0, 512.0],
        zoom: 0.125,
        ..camera
    };
    let all = reference(wide_camera, config, (-80, 80), 512);
    assert!(all.len() > 64);
    assert_eq!(
        candidate_chunks(wide_camera, config, (-80, 80), 64),
        all[..64]
    );
    assert_eq!(candidate_chunks(wide_camera, config, (-80, 80), 512), all);
}
