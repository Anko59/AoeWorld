use crate::playground::game::capture::{ChunkCounts, parse_fragment};
use aoe_core::{Camera, WorldConfig};
use std::collections::BTreeSet;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn camera_fragment_decimals_and_independent_invalid_parts() {
    let config = WorldConfig::default();
    let camera = Camera::new([100.0, 200.0], [1280.0, 720.0]);
    let parsed = parse_fragment("#x=300.25&y=400.5&zoom=1.75&renderer=canvas");
    let applied = parsed.apply(camera, config);
    assert_eq!(applied.center, [300.25, 400.5]);
    assert_eq!(applied.zoom, 1.75);
    assert_eq!(parsed.renderer, Some("canvas"));
    for value in ["", "nope", "NaN", "inf", "1e999"] {
        let applied = parse_fragment(&format!("#x={value}&y=400.5&zoom=2")).apply(camera, config);
        assert_eq!(applied.center, [100.0, 400.5]);
        assert_eq!(applied.zoom, 2.0);
        let applied =
            parse_fragment(&format!("#x=300.25&y={value}&zoom={value}")).apply(camera, config);
        assert_eq!(applied.center, [300.25, 200.0]);
        assert_eq!(applied.zoom, 1.0);
    }
    assert_eq!(
        parse_fragment("#zoom=2").apply(camera, config).center,
        camera.center
    );
    assert_eq!(parse_fragment("#renderer=invalid").renderer, None);
    assert_eq!(parse_fragment("#renderer=webgpu").renderer, Some("webgpu"));
    assert_eq!(parse_fragment("#renderer=webgl2").renderer, Some("webgl2"));
}

#[wasm_bindgen_test]
fn camera_fragment_clamps_world_and_shared_zoom_limits() {
    let config = WorldConfig::default();
    let camera = Camera::new([100.0, 200.0], [1280.0, 720.0]);
    let edge = parse_fragment("#x=-500&y=9999999&zoom=99").apply(camera, config);
    assert_eq!(edge.zoom, 3.0);
    let expected = Camera {
        center: [0.0, f64::from(config.height_tiles) - 1.0],
        zoom: 3.0,
        ..camera
    }
    .clamp_center(config);
    assert_eq!(edge, expected);
    assert_eq!(parse_fragment("#zoom=-5").apply(camera, config).zoom, 0.25);
    assert_eq!(
        parse_fragment("#unrelated=value").apply(camera, config),
        camera
    );
}

#[wasm_bindgen_test]
fn settle_counts_include_queued_demand_and_only_visible_failures() {
    let visible = [(0, 0), (1, 0), (2, 0)];
    let resident = BTreeSet::from([(0, 0), (9, 9)]);
    let failures = BTreeSet::from([(1, 0), (8, 8)]);
    assert_eq!(
        ChunkCounts::for_visible(&visible, |key| resident.contains(key), &failures),
        ChunkCounts {
            pending: 2,
            failed: 1
        }
    );
    let loaded = BTreeSet::from(visible);
    assert_eq!(
        ChunkCounts::for_visible(&visible, |key| loaded.contains(key), &failures),
        ChunkCounts {
            pending: 0,
            failed: 0
        }
    );
    assert_eq!(
        ChunkCounts::for_visible(&[], |_| false, &failures),
        ChunkCounts {
            pending: 0,
            failed: 0
        }
    );
}
