use super::super::{PREVIEW_SAMPLES_PER_AXIS, preview};
use super::modeled_source_package;
use aoe_map::{GroundMaterial, MAP_SCHEMA_VERSION, MapPackage, MapRequest, SourceLock, WaterKind};
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use std::{fs, path::Path as FsPath};

fn config(directory: Option<&FsPath>) -> crate::Config {
    crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: directory.map(FsPath::to_owned),
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    }
}

fn app_state(directory: Option<&FsPath>) -> crate::AppState {
    crate::AppState::new(&config(directory), "map-preview-test").expect("app state")
}

fn source_lock() -> SourceLock {
    SourceLock {
        id: "preview-fixture".to_owned(),
        provider: "test-only".to_owned(),
        release: "fixture".to_owned(),
        url: "https://example.invalid/preview-fixture".to_owned(),
        sha256: [5; 32],
        acquired_at: "2026-09-27T00:00:00Z".to_owned(),
        native_resolution: "fixture".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "fixture".to_owned(),
        license: "test-only".to_owned(),
        preprocessing_version: "fixture-v1".to_owned(),
    }
}

fn persist_source_fixture(directory: &FsPath) -> MapPackage {
    let (prepared, _, elevation, hydrology, land_cover) = modeled_source_package();
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        prepared.request,
        vec![source_lock()],
        prepared.projection,
        prepared.provenance,
        prepared.environment,
    )
    .expect("source-backed prepared package");
    let page_root = directory.join("pages").join(package.content_hash_hex());
    let hydrology_root = page_root.join("hydrology-evidence");
    let land_cover_root = page_root.join("modern-land-cover");
    fs::create_dir_all(&hydrology_root).expect("hydrology directory");
    fs::create_dir_all(&land_cover_root).expect("land-cover directory");
    fs::write(
        hydrology_root.join("0-0-0.json"),
        serde_json::to_vec(&hydrology).expect("hydrology JSON"),
    )
    .expect("write hydrology page");
    fs::write(
        land_cover_root.join("0-0-0.json"),
        serde_json::to_vec(&land_cover).expect("land-cover JSON"),
    )
    .expect("write modern land-cover page");
    crate::map_store::persist_prepared(Some(directory), &package, &[elevation], &[], &[], &[])
        .expect("persist source package and elevation page");
    package
}

fn elevation_page_path(directory: &FsPath, package: &MapPackage) -> std::path::PathBuf {
    directory
        .join("pages")
        .join(package.content_hash_hex())
        .join("elevation/0-0-0.json")
}

#[tokio::test]
async fn preview_endpoint_returns_source_backed_terrain_samples() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = persist_source_fixture(directory.path());
    let state = app_state(Some(directory.path()));
    let hash = package.content_hash_hex();
    let response = preview(Path(hash), State(state))
        .await
        .expect("source-backed preview");
    let preview = response.0;

    assert_eq!(preview.samples_per_axis, PREVIEW_SAMPLES_PER_AXIS);
    assert_eq!(
        preview.cells.len(),
        usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2)
    );
    assert!(preview.source_backed);
    assert_eq!(
        preview.minimum_height_centimeters,
        preview
            .cells
            .iter()
            .map(|cell| cell.geographic_height_centimeters)
            .min()
            .expect("preview samples")
    );
    assert_eq!(
        preview.maximum_height_centimeters,
        preview
            .cells
            .iter()
            .map(|cell| cell.geographic_height_centimeters)
            .max()
            .expect("preview samples")
    );
    assert!(preview.minimum_height_centimeters < 0);
    assert!(preview.maximum_height_centimeters > 0);
    assert!(preview.cells.iter().any(|cell| {
        cell.water == WaterKind::Lake as u8 && cell.material == GroundMaterial::Water as u8
    }));
}

#[tokio::test]
async fn preview_endpoint_returns_not_found_for_unknown_hash() {
    let state = app_state(None);
    let result = preview(Path("0".repeat(64)), State(state)).await;
    assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
}

#[tokio::test]
async fn preview_endpoint_fails_when_a_verified_source_page_is_missing_or_corrupt() {
    for corrupt in [false, true] {
        let directory = tempfile::tempdir().expect("package directory");
        let package = persist_source_fixture(directory.path());
        let state = app_state(Some(directory.path()));
        let page = elevation_page_path(directory.path(), &package);
        if corrupt {
            fs::write(&page, b"not a verified elevation page").expect("corrupt page");
        } else {
            fs::remove_file(&page).expect("remove elevation page");
        }

        let result = preview(Path(package.content_hash_hex()), State(state)).await;
        assert!(matches!(result, Err(StatusCode::NOT_FOUND)));
    }
}

#[tokio::test]
async fn fallback_preview_endpoint_marks_samples_as_not_source_backed() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
        .expect("fallback package");
    crate::map_store::persist(Some(directory.path()), &package).expect("persist fallback package");
    let state = app_state(Some(directory.path()));
    let response = preview(Path(package.content_hash_hex()), State(state))
        .await
        .expect("fallback preview");

    assert_eq!(response.0.samples_per_axis, PREVIEW_SAMPLES_PER_AXIS);
    assert_eq!(
        response.0.cells.len(),
        usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2)
    );
    assert!(!response.0.source_backed);
}
