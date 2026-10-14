use crate::playground::game::capture::{Capture, ChunkCounts, Fragment};
use aoe_core::{Camera, WorldConfig};
use std::collections::BTreeSet;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn camera_fragment_clamps_world_and_shared_zoom_limits() {
    let config = WorldConfig::default();
    let camera = Camera::new([100.0, 200.0], [1280.0, 720.0]);
    let edge = Fragment([-500.0, 9999999.0, 3.0, 0.0]).apply(camera, config);
    assert_eq!(edge.zoom, 3.0);
    let expected = Camera {
        center: [0.0, f64::from(config.height_tiles) - 1.0],
        zoom: 3.0,
        ..camera
    }
    .clamp_center(config);
    assert_eq!(edge, expected);
    assert_eq!(
        Fragment([f64::NAN, f64::NAN, 0.25, 0.0])
            .apply(camera, config)
            .zoom,
        0.25
    );
    assert_eq!(
        Fragment([f64::NAN, f64::NAN, 2.0, 0.0])
            .apply(camera, config)
            .center,
        camera.center
    );
    assert_eq!(Fragment([f64::NAN; 4]).apply(camera, config), camera);
}

#[wasm_bindgen_test]
fn settle_attributes_preserve_full_frame_integer_and_write_only_on_change() {
    use crate::playground::game::controls::navigation::capture_update;
    let document = web_sys::window()
        .expect("browser window")
        .document()
        .expect("browser document");
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .expect("canvas element")
        .dyn_into()
        .expect("HTML canvas");
    capture_update(&canvas, 2, 1, u64::MAX);
    assert_eq!(
        canvas.get_attribute("data-chunks-pending").as_deref(),
        Some("2")
    );
    assert_eq!(
        canvas.get_attribute("data-chunks-failed").as_deref(),
        Some("1")
    );
    assert_eq!(
        canvas.get_attribute("data-frame").as_deref(),
        Some("18446744073709551615")
    );
    // An unchanged update must leave the attributes alone.
    canvas
        .set_attribute("data-frame", "probe")
        .expect("frame probe");
    canvas
        .set_attribute("data-chunks-pending", "probe")
        .expect("pending probe");
    capture_update(&canvas, 2, 1, u64::MAX);
    assert_eq!(canvas.get_attribute("data-frame").as_deref(), Some("probe"));
    assert_eq!(
        canvas.get_attribute("data-chunks-pending").as_deref(),
        Some("probe")
    );
    capture_update(&canvas, 0, 0, 42);
    assert_eq!(
        canvas.get_attribute("data-chunks-pending").as_deref(),
        Some("0")
    );
    assert_eq!(
        canvas.get_attribute("data-chunks-failed").as_deref(),
        Some("0")
    );
    assert_eq!(canvas.get_attribute("data-frame").as_deref(), Some("42"));
}

#[wasm_bindgen_test]
fn capture_failures_deduplicate_retries_and_remove_recovered_chunks() {
    let mut capture = Capture::default();
    capture.chunk_received((1, 0), true);
    capture.chunk_received((1, 0), true);
    capture.chunk_received((8, 8), true);
    assert_eq!(capture.failures, [(1, 0), (8, 8)]);
    capture.chunk_received((1, 0), false);
    capture.chunk_received((1, 0), false);
    assert_eq!(capture.failures, [(8, 8)]);
}

#[wasm_bindgen_test]
fn settle_counts_include_queued_demand_and_only_visible_failures() {
    let visible = [(0, 0), (1, 0), (2, 0)];
    let resident = BTreeSet::from([(0, 0), (9, 9)]);
    let failures = [(1, 0), (8, 8)];
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
