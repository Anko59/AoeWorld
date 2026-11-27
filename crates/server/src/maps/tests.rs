use super::*;
use aoe_map::{
    ElevationPage, EnvironmentPage, EnvironmentPageError, EnvironmentPageKey,
    EnvironmentPageProvider, EnvironmentalProvenance, FieldPyramid, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyWaterModelIndex,
    HydrologyWaterModelPage, HydrologyWaterPolicy, MapRequest, ModernLandCoverPage, PageLayer,
    PreparedEnvironment, ProjectionMetadata, PyramidLevel, WORLD_COVER_OBSERVATION_YEAR,
    WaterFlowDirection, WaterModelProvenance, ordered_hydrology_page_root,
    ordered_modern_land_cover_page_root, ordered_page_root,
};
use axum::http::{HeaderValue, StatusCode};
use std::{collections::BTreeMap, sync::Arc};

mod bounds;
mod landscape;
mod minimap;
mod minimap_waiting;
mod preview;
mod stored;

#[derive(Debug, Default)]
struct MemoryPageProvider {
    pages: BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
}

impl EnvironmentPageProvider for MemoryPageProvider {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.pages
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

pub(super) fn elevation_overview() -> ElevationPage {
    ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![12_345],
    }
}

fn modeled_source_package() -> (
    MapPackage,
    MemoryPageProvider,
    ElevationPage,
    HydrologyEvidencePage,
    ModernLandCoverPage,
) {
    let request = MapRequest {
        compression: aoe_map::Ratio::new(1, 1).expect("compression"),
        ..MapRequest::default()
    };
    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![-90_000, 250_000, 12_345, -100],
    };
    let water_model = HydrologyWaterModelPage {
        kind: vec![HydrologyKind::Lake as u8; 4],
        surface_level_centimeters: vec![Some(450_000); 4],
        flow_direction: vec![WaterFlowDirection::Unknown as u8; 4],
        provenance: vec![WaterModelProvenance::ModelledLakeSurface as u8; 4],
    };
    let hydrology = HydrologyEvidencePage {
        river_segments: Vec::new(),
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::Lake as u8; 4],
        method: vec![HydrologyEvidenceMethod::HydroLakesExtent as u8; 4],
        water_model: Some(water_model),
    };
    let land_cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![40; 4],
    };
    let document = aoe_map::WaterCorrectionDocument::empty(request, 2).expect("correction doc");
    let mut provider = MemoryPageProvider::default();
    provider.pages.insert(
        EnvironmentPageKey {
            layer: PageLayer::Elevation,
            level: 0,
            x: 0,
            y: 0,
        },
        Arc::new(EnvironmentPage::Elevation(elevation.clone())),
    );
    provider.pages.insert(
        EnvironmentPageKey {
            layer: PageLayer::HydrologyEvidence,
            level: 0,
            x: 0,
            y: 0,
        },
        Arc::new(EnvironmentPage::HydrologyEvidence(hydrology.clone())),
    );
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: 64,
        // The canonical pyramid ends at one sample; its overview lies within
        // the level-zero height range.
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                        .expect("elevation root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(&[elevation_overview()])
                        .expect("overview root"),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: Some(HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: 64,
            world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&hydrology))
                .expect("hydrology root"),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(std::slice::from_ref(
                &land_cover,
            ))
            .expect("land-cover root"),
            water_model: Some(HydrologyWaterModelIndex {
                model_version: aoe_map::HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: 2,
                target_year_ce: 600,
                correction_document: document,
            }),
        }),
    };
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("prepared source package");
    (package, provider, elevation, hydrology, land_cover)
}

#[tokio::test]
async fn source_height_bounds_include_all_elevation_and_modeled_water_samples() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, pages) = stored::modeled(MAP_SCHEMA_VERSION);
    stored::publish(directory.path(), &package, &pages);
    let state = stored::started(directory.path());
    assert_eq!(
        stored::served_bounds(&state, &package).await,
        Ok((-900, 4_500))
    );
}

#[tokio::test]
async fn fallback_height_bounds_cover_the_procedural_relief_range() {
    let directory = tempfile::tempdir().expect("package directory");
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
        .expect("fallback package");
    stored::publish(directory.path(), &package, &[]);
    let state = stored::started(directory.path());
    assert_eq!(
        stored::served_bounds(&state, &package).await,
        Ok((-502, 502))
    );
    assert_eq!(state.page_residencies.write().await.len(), 0);
}

#[tokio::test]
async fn height_bounds_fail_closed_for_missing_or_mistyped_source_pages() {
    let directory = tempfile::tempdir().expect("package directory");
    let (package, pages) = stored::modeled(MAP_SCHEMA_VERSION);
    stored::publish(directory.path(), &package, &pages);
    let state = stored::started(directory.path());
    let path = |page: &EnvironmentPage| stored::page_path(directory.path(), &package, page.key());
    let EnvironmentPage::HydrologyEvidence(hydrology) = &pages[2] else {
        panic!("the third page is hydrology evidence");
    };
    let unmodeled = EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
        water_model: None,
        ..hydrology.clone()
    });
    // The pages the bounds come from: level-zero elevation, then the water model.
    for (page, wrong, reason) in [
        (&pages[0], &pages[2], "page JSON is invalid"),
        (&pages[2], &pages[0], "page JSON is invalid"),
        (&pages[2], &unmodeled, "water-model presence does not match"),
    ] {
        let whole = std::fs::read(path(page)).expect("page bytes");
        std::fs::remove_file(path(page)).expect("missing page");
        let missing = stored::refused_opening(&state, directory.path(), &package).await;
        assert!(
            matches!(&missing, crate::map_store::MapStoreError::Io(error)
                if error.kind() == std::io::ErrorKind::NotFound),
            "{missing}"
        );
        std::fs::write(path(page), stored::json(wrong)).expect("wrong page");
        let wrong = stored::refused_opening(&state, directory.path(), &package).await;
        assert!(wrong.to_string().contains(reason), "{wrong}");
        std::fs::write(path(page), whole).expect("restore page");
    }
    // A cancelled scan leaves neither bounds nor an opened package behind.
    let polls = std::cell::Cell::new(0);
    for allowed in [0, 1, 3] {
        polls.set(0);
        let cancelled = || polls.replace(polls.get() + 1) >= allowed;
        assert!(matches!(
            crate::map_store::PageResidency::open(directory.path(), &package, &cancelled),
            Err(crate::map_store::MapStoreError::Cancelled)
        ));
    }
    let stopped = page_residency(&state, &package, Arc::new(AtomicBool::new(true))).await;
    assert!(stopped.is_err());
    assert_eq!(state.page_residencies.write().await.len(), 0);
    assert_eq!(
        stored::served_bounds(&state, &package).await,
        Ok((-900, 4_500))
    );
}

#[tokio::test]
async fn source_height_bounds_http_payload_uses_verified_pages_and_is_cached() {
    let (package, _, elevation, hydrology, land_cover) = modeled_source_package();
    let directory = tempfile::tempdir().expect("package directory");
    let page_root = directory
        .path()
        .join("pages")
        .join(package.content_hash_hex());
    let hydrology_root = page_root.join("hydrology-evidence");
    let land_cover_root = page_root.join("modern-land-cover");
    std::fs::create_dir_all(&hydrology_root).expect("hydrology directory");
    std::fs::create_dir_all(&land_cover_root).expect("land-cover directory");
    std::fs::write(
        hydrology_root.join("0-0-0.json"),
        serde_json::to_vec(&hydrology).expect("hydrology JSON"),
    )
    .expect("write hydrology page");
    std::fs::write(
        land_cover_root.join("0-0-0.json"),
        serde_json::to_vec(&land_cover).expect("land-cover JSON"),
    )
    .expect("write land-cover page");
    crate::map_store::persist_prepared(
        Some(directory.path()),
        &package,
        &[elevation, elevation_overview()],
        &[],
        &[],
        &[],
    )
    .expect("persist prepared package");
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: Some(directory.path().to_owned()),
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = crate::AppState::new(&config, "height-bounds-test").expect("state");
    let hash = package.content_hash_hex();
    let first = height_bounds(Path(hash.clone()), State(state.clone()))
        .await
        .expect("height bounds response");
    let payload = serde_json::to_value(first.0).expect("JSON response");
    assert_eq!(payload["content_hash"], hash);
    assert_eq!(payload["minimum_height_level"], -900);
    assert_eq!(payload["maximum_height_level"], 4_500);
    let provider = state
        .page_residencies
        .write()
        .await
        .get(&hash)
        .expect("height bounds residency");
    // The opening verified and decoded every page: nothing is read again.
    assert_eq!(provider.indexed_pages(), 4);
    assert_eq!(provider.verified_page_loads(), 0);
    assert_eq!(provider.resident_pages(), 0);
    let opened = provider;

    let second = height_bounds(Path(hash.clone()), State(state.clone()))
        .await
        .expect("cached height bounds response");
    assert_eq!(
        serde_json::to_value(second.0).expect("JSON response"),
        payload
    );
    let provider = state
        .page_residencies
        .write()
        .await
        .get(&hash)
        .expect("cached residency");
    assert!(Arc::ptr_eq(&provider, &opened), "opened a second time");
    assert_eq!(provider.verified_page_loads(), 0);
}

#[tokio::test]
async fn height_bounds_endpoint_returns_not_found_for_unknown_hash() {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().expect("bind"),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = crate::AppState::new(&config, "height-bounds-test").expect("state");
    let error = height_bounds(Path("0".repeat(64)), State(state))
        .await
        .expect_err("unknown map hash");
    assert_eq!(error.0, StatusCode::NOT_FOUND);
}

#[test]
fn fallback_preview_is_bounded_and_does_not_require_activation() {
    let package = MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
        .expect("fallback package");
    let preview = preview_package(package, None, &|| false).expect("fallback preview");
    assert_eq!(preview.samples_per_axis, PREVIEW_SAMPLES_PER_AXIS);
    assert_eq!(
        preview.cells.len(),
        usize::from(PREVIEW_SAMPLES_PER_AXIS).pow(2)
    );
    assert!(!preview.source_backed);
    assert!(preview.minimum_height_centimeters <= preview.maximum_height_centimeters);
}

#[test]
fn preview_coordinates_stay_inside_tiny_and_large_maps() {
    assert_eq!(preview_coordinate(1, 0).expect("coordinate"), 0);
    assert_eq!(
        preview_coordinate(500, PREVIEW_SAMPLES_PER_AXIS - 1).expect("coordinate"),
        484
    );
}

#[test]
fn controller_header_parses_only_complete_hex_resume_tokens() {
    let mut headers = HeaderMap::new();
    assert_eq!(controller_token(&headers), None);

    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("000000000000000000000000000000000000000000000000"),
    );
    assert_eq!(controller_token(&headers), Some(ResumeToken([0; 24])));

    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("00000000000000000000000000000000000000000000000z"),
    );
    assert_eq!(controller_token(&headers), None);
    headers.insert(
        CONTROLLER_TOKEN_HEADER,
        HeaderValue::from_static("0000000000000000000000000000000000000000000000"),
    );
    assert_eq!(controller_token(&headers), None);
}

#[test]
fn preview_coordinate_rejects_arithmetic_and_i32_overflow() {
    assert_eq!(preview_coordinate(0, 0).expect("zero map"), 0);
    assert!(preview_coordinate(u64::MAX, u16::MAX).is_err());
    assert!(preview_coordinate(i32::MAX as u64 * 2, 15).is_err());
}

mod rivers;

#[tokio::test]
async fn content_roster_reset_builds_the_coastal_fixture() {
    let config = crate::Config {
        bind: "127.0.0.1:0".parse().unwrap(),
        scenario: aoe_scenario::SMOKE,
        tick_hz: 20,
        asset_pack: None,
        map_package_directory: None,
        open_map_package: None,
        map_worker: None,
        geodata_cache_directory: ".cache/geodata".into(),
    };
    let state = crate::AppState::new(&config, "roster-reset-test").unwrap();
    assert_eq!(
        reset(
            State(state),
            Query::try_from_uri(&"/maps/reset?scenario=content-roster".parse().unwrap()).unwrap(),
        )
        .await,
        StatusCode::NO_CONTENT
    );
}
