//! Packages stored on disk and a server over them, for the tests of their opening.
use super::super::height_bounds;
use super::{elevation_overview, modeled_source_package};
use crate::{
    AppState,
    map_store::{MapStoreError, PageResidency},
};
use aoe_map::{EnvironmentPage, EnvironmentPageKey, MapPackage};
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use std::{
    fs,
    path::{Path as FsPath, PathBuf},
};

pub(super) fn config(directory: &FsPath) -> crate::Config {
    crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    }
}

/// The server over `directory`. A test whose server does not start fails here.
pub(super) fn started(directory: &FsPath) -> AppState {
    AppState::new(&config(directory), "stored-package-test")
        .unwrap_or_else(|error| panic!("the server did not start: {error}"))
}

pub(super) fn json(page: &EnvironmentPage) -> Vec<u8> {
    match page {
        EnvironmentPage::Elevation(page) => serde_json::to_vec(page),
        EnvironmentPage::Water(page) => serde_json::to_vec(page),
        EnvironmentPage::Vegetation(page) => serde_json::to_vec(page),
        EnvironmentPage::HistoricalLandUse(page) => serde_json::to_vec(page),
        EnvironmentPage::HydrologyEvidence(page) => serde_json::to_vec(page),
        EnvironmentPage::ModernLandCover(page) => serde_json::to_vec(page),
    }
    .expect("page JSON")
}

pub(super) fn page_path(
    directory: &FsPath,
    package: &MapPackage,
    key: EnvironmentPageKey,
) -> PathBuf {
    directory
        .join("pages")
        .join(package.content_hash_hex())
        .join(key.layer.directory_name())
        .join(format!("{}-{}-{}.json", key.level, key.x, key.y))
}

/// Writes the manifest and the pages as they are, without the writer's check.
pub(super) fn publish(directory: &FsPath, package: &MapPackage, pages: &[EnvironmentPage]) {
    for page in pages {
        let path = page_path(directory, package, page.key());
        fs::create_dir_all(path.parent().expect("layer")).expect("layer directory");
        fs::write(path, json(page)).expect("page");
    }
    fs::write(
        directory.join(format!("{}.json", package.content_hash_hex())),
        serde_json::to_vec(package).expect("manifest JSON"),
    )
    .expect("manifest");
}

/// The modeled source package and each of its four pages. `variant` gives each
/// copy its own identity, so that one directory can hold several.
pub(super) fn modeled(variant: u16) -> (MapPackage, Vec<EnvironmentPage>) {
    let (base, _, elevation, hydrology, land_cover) = modeled_source_package();
    let package = MapPackage::with_prepared_environment(
        variant,
        base.request,
        base.source_locks,
        base.projection,
        base.provenance,
        base.environment,
    )
    .expect("modeled package");
    let pages = vec![
        EnvironmentPage::Elevation(elevation),
        EnvironmentPage::Elevation(elevation_overview()),
        EnvironmentPage::HydrologyEvidence(hydrology),
        EnvironmentPage::ModernLandCover(land_cover),
    ];
    (package, pages)
}

/// The bounds the endpoint serves, with the map they are of.
pub(super) async fn served_bounds(
    state: &AppState,
    package: &MapPackage,
) -> Result<(i64, i64), (StatusCode, String)> {
    let hash = package.content_hash_hex();
    let served = height_bounds(Path(hash.clone()), State(state.clone())).await?;
    let payload = serde_json::to_value(served.0).expect("bounds JSON");
    assert_eq!(payload["content_hash"], hash);
    let level = |name: &str| payload[name].as_i64().expect("height level");
    Ok((level("minimum_height_level"), level("maximum_height_level")))
}

/// Why the package does not open, after checking that the endpoint refuses its
/// bounds for the same reason and that nothing stays registered.
pub(super) async fn refused_opening(
    state: &AppState,
    directory: &FsPath,
    package: &MapPackage,
) -> MapStoreError {
    let error = PageResidency::open(directory, package, &|| false).expect_err("refused opening");
    assert_eq!(
        served_bounds(state, package).await,
        Err((StatusCode::NOT_FOUND, error.to_string()))
    );
    assert_eq!(state.page_residencies.write().await.len(), 0);
    error
}
