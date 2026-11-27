//! A raster that is already there does not wait for one that is computed.
use super::super::minimap::{MAX_MINIMAP_BYTES, coarse_minimap};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid,
    MAP_SCHEMA_VERSION, MapPackage, MapRequest, PreparedEnvironment, ProjectionMetadata,
    PyramidLevel, ordered_page_root,
};
use axum::{
    body::{Bytes, to_bytes},
    extract::{Path, State},
};
use std::{path::Path as FsPath, time::Duration};
use tokio::time::timeout;

/// Only a failing test waits this long.
const WAIT: Duration = Duration::from_secs(10);

/// Stores a prepared map of 64 tiles and returns its hash.
fn stored_map(directory: &FsPath, height: i32) -> String {
    let pages = [(0, 2), (1, 1)].map(|(level, side)| ElevationPage {
        level,
        x: 0,
        y: 0,
        width: side,
        height: side,
        geographic_height_centimeters: vec![height; usize::from(side).pow(2)],
    });
    let levels = pages
        .iter()
        .map(|page| PyramidLevel {
            samples_per_axis: u16::from(page.width),
            ordered_page_root: ordered_page_root(std::slice::from_ref(page)).expect("root"),
        })
        .collect();
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        MapRequest {
            requested_side_meters: 3_840,
            ..MapRequest::default()
        },
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment {
            samples_per_axis: 2,
            geographic_millimeters_per_sample: 1_920_000,
            page_samples: ENVIRONMENT_PAGE_SAMPLES,
            elevation: FieldPyramid { levels },
            ..PreparedEnvironment::default()
        },
    )
    .expect("package");
    crate::map_store::persist_prepared(Some(directory), &package, &pages, &[], &[], &[])
        .expect("stored package");
    package.content_hash_hex()
}

/// A server that has no raster in memory yet.
fn started(directory: &FsPath) -> crate::AppState {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    crate::AppState::new(&config, "minimap-waiting-test").expect("state")
}

async fn coarse(hash: String, state: crate::AppState) -> Bytes {
    let response = coarse_minimap(Path(hash), State(state))
        .await
        .expect("coarse raster");
    to_bytes(response.into_body(), MAX_MINIMAP_BYTES)
        .await
        .expect("body")
}

/// The computing map waits for the page registry, which the test reads and so
/// keeps. A cached raster needs no page either, or it would wait there as well.
#[tokio::test]
async fn a_cached_raster_is_served_while_another_map_computes() {
    let directory = tempfile::tempdir().expect("directory");
    let cached = stored_map(directory.path(), 0);
    let missing = stored_map(directory.path(), 20_000);
    let state = started(directory.path());
    let expected = coarse(cached.clone(), state.clone()).await;

    let registry = state.page_residencies.clone();
    let reading = registry.read().await;
    let computing = tokio::spawn(coarse(missing, state.clone()));
    timeout(WAIT, async {
        // A queued writer keeps every later reader out.
        while registry.try_read().is_ok() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the other map reaches the page registry");

    let served = timeout(WAIT, coarse(cached, state)).await;
    assert!(
        served.is_ok_and(|raster| raster == expected),
        "a cached raster waits for another map's computation"
    );
    assert!(!computing.is_finished());
    drop(reading);
    let computed = timeout(WAIT, computing).await.expect("released");
    assert_ne!(computed.expect("task"), Bytes::new());
}
