use super::*;
use aoe_map::{
    HydrologyEvidenceIndex, HydrologyKind, HydrologyWaterPolicy, WaterCorrectionDocument,
};
use std::sync::{Arc, atomic::AtomicBool};

#[test]
fn offline_gdal_overview_composes_typed_vectors_and_roundtrips_all_roots() {
    let fixture = OverviewFixture::new();
    let request = MapRequest::default();
    let axes = OverviewFieldAxes::LANDSCAPE;
    let history = GeographicHistoricalCorrectionDocument::empty(
        request,
        1024,
        hyde::HYDE_AREA_PREPROCESSING_IDENTITY,
    )
    .unwrap();
    let vegetation = VegetationPatchDocument::empty(request, 128).unwrap();
    let prepared = prepare_overview_fields_from_verified_sources(
        request,
        axes,
        true,
        &history,
        &vegetation,
        fixture.sources.clone(),
    )
    .unwrap();
    let overview = GeneratedMap::from_prepared(request, prepared).unwrap();
    let original = overview.clone();
    let vectors = vector_fixture(request, &overview.elevation_pages, &overview.water_pages);
    assert!(
        vectors
            .modern_land_cover_preprocessing()
            .unwrap()
            .contains("0-nodata")
    );
    let composed = overview.with_overview_vector_hydrology(vectors).unwrap();
    composed.validate().unwrap();
    assert_eq!(composed.package.source_locks.len(), 9);
    assert!(
        composed
            .package
            .source_locks
            .iter()
            .all(|lock| !lock.id.contains("worldcover"))
    );
    assert_eq!(composed.elevation_pages, original.elevation_pages);
    assert_eq!(composed.water_pages, original.water_pages);
    assert_eq!(composed.vegetation_pages, original.vegetation_pages);
    assert_eq!(
        composed.historical_land_use_pages,
        original.historical_land_use_pages
    );
    assert_eq!(
        composed.package.environment.elevation,
        original.package.environment.elevation
    );
    assert_eq!(
        composed.package.environment.water_samples_per_axis(),
        Some(128)
    );
    assert_eq!(
        composed.package.environment.historical_samples_per_axis(),
        Some(1024)
    );
    assert_eq!(composed.package.request.year_ce, 600);
    let index = composed
        .package
        .environment
        .hydrology_evidence
        .as_ref()
        .unwrap();
    assert_eq!(index.samples_per_axis, 1024);
    assert_eq!(index.world_cover_year, 2021); // classification legend only
    assert_eq!(index.water_model.as_ref().unwrap().target_year_ce, 600);
    assert_eq!(
        composed
            .package
            .source_locks
            .iter()
            .filter(|lock| lock.preprocessing_version.contains("modern-class=0-nodata"))
            .count(),
        2
    );
    assert!(
        composed
            .modern_land_cover_pages
            .iter()
            .all(|page| page.worldcover_class.iter().all(|class| *class == 0))
    );
    let bytes = serde_json::to_vec(&composed).unwrap();
    let decoded: GeneratedMap = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, composed);
    decoded.validate().unwrap();
    let output = fixture.root.join("typed-output");
    composed.write_directory(&output).unwrap();
    GeneratedMap::verify_directory(&output, &composed.package.content_hash_hex()).unwrap();
    let loaded =
        GeneratedMap::read_directory(&output, &composed.package.content_hash_hex()).unwrap();
    assert_eq!(loaded, composed);
    let eager = composed
        .package
        .generator_with_page_provider(Arc::new(Pages(composed.clone())))
        .unwrap();
    let persisted = loaded
        .package
        .clone()
        .generator_with_page_provider(Arc::new(Pages(loaded)))
        .unwrap();
    for (x, y) in [(0, 0), (64, 64), (512, 512)] {
        let coordinate = serde_json::from_value(serde_json::json!({"x": x, "y": y})).unwrap();
        assert_eq!(
            eager.tile_at_with_cancel(coordinate, &|| false).unwrap(),
            persisted
                .tile_at_with_cancel(coordinate, &|| false)
                .unwrap()
        );
    }
    let mut corrupt = composed.clone();
    corrupt.modern_land_cover_pages[0].worldcover_class[0] = 40;
    assert!(corrupt.validate().is_err());
    let mut corrupt = composed;
    corrupt.hydrology_evidence_pages[0]
        .water_model
        .as_mut()
        .unwrap()
        .kind[0] = HydrologyKind::Lake as u8;
    assert!(corrupt.validate().is_err());
    original.validate().unwrap();
    assert!(original.package.environment.hydrology_evidence.is_none());
    assert_eq!(original.package.source_locks.len(), 7);
}

// GDAL samples synthetic native vector datasets and the overview DEM/ocean.
// Pinned lock identities are test metadata, not proof of real acquisition.
fn vector_fixture(
    request: MapRequest,
    elevation: &[aoe_map::ElevationPage],
    water: &[aoe_map::WaterPage],
) -> crate::PreparedHydrology {
    let (evidence, modern, river_topology) =
        crate::hydrology::offline_vector_pages(request, 1024, water);
    assert!(
        evidence
            .iter()
            .any(|page| page.kind.contains(&(HydrologyKind::Lake as u8)))
    );
    assert!(
        evidence
            .iter()
            .any(|page| page.kind.contains(&(HydrologyKind::River as u8)))
    );
    assert!(
        evidence
            .iter()
            .any(|page| page.kind.contains(&(HydrologyKind::Ocean as u8)))
    );
    let mut prepared = crate::PreparedHydrology { samples_per_axis: 1024,
        evidence_index: HydrologyEvidenceIndex { samples_per_axis: 1024,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES, world_cover_year: 2021,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: aoe_map::ordered_hydrology_page_root(&evidence).unwrap(),
            modern_land_cover_page_root: aoe_map::ordered_modern_land_cover_page_root(&modern).unwrap(), water_model: None },
        source_locks: crate::hydrology_vector_sources().iter().map(|source| source.cache_lock().unwrap()
            .to_map_source_lock("offline-fixture".into(), "modern-landcover=not-requested;modern-class=0-nodata;2021=classification-legend-only".into()).unwrap()).collect(),
        hydrology_pages: evidence, modern_land_cover_pages: modern, river_topology };
    let elevation_context = crate::hydrology::overview_elevation_context(elevation).unwrap();
    assert_eq!(elevation_context.len(), 16 * 16);
    assert!(elevation_context.iter().all(|page| page.level == 0));
    crate::hydrology::apply_water_model(
        &mut prepared,
        request,
        &elevation_context,
        WaterCorrectionDocument::empty(request, 1024).unwrap(),
    )
    .unwrap();
    prepared
}

#[derive(Debug)]
struct Pages(GeneratedMap);
impl aoe_map::EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: aoe_map::EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<aoe_map::EnvironmentPage>, aoe_map::EnvironmentPageError> {
        use aoe_map::{EnvironmentPage as E, PageLayer as L};
        if cancelled() {
            return Err(aoe_map::EnvironmentPageError::Cancelled);
        }
        macro_rules! lookup {
            ($pages:expr, $variant:ident) => {
                $pages
                    .iter()
                    .find(|page| page.level == key.level && page.x == key.x && page.y == key.y)
                    .cloned()
                    .map(E::$variant)
            };
        }
        let page = match key.layer {
            L::Elevation => lookup!(self.0.elevation_pages, Elevation),
            L::Water => lookup!(self.0.water_pages, Water),
            L::Vegetation => lookup!(self.0.vegetation_pages, Vegetation),
            L::HistoricalLandUse => lookup!(self.0.historical_land_use_pages, HistoricalLandUse),
            L::HydrologyEvidence => lookup!(self.0.hydrology_evidence_pages, HydrologyEvidence),
            L::ModernLandCover => lookup!(self.0.modern_land_cover_pages, ModernLandCover),
        };
        page.map(Arc::new)
            .ok_or(aoe_map::EnvironmentPageError::Missing)
    }
}

#[test]
fn vector_worker_options_reject_before_overview_acquisition_or_publication() {
    let fixture = OverviewFixture::new();
    let cache = fixture.root.join("no-cache");
    let output = fixture.root.join("no-output");
    let request = MapRequest::default();
    let valid = serde_json::json!({ "operation": "prepare_overview_directory", "cache_root": cache,
        "output_directory": output, "request": request, "samples_per_axis": 1024,
        "field_axes": OverviewFieldAxes::LANDSCAPE, "hydrology_mode": "vectors" });
    for change in 0..8 {
        let mut input = valid.clone();
        match change {
            0 => input["field_axes"]["historical"] = 128.into(),
            1 => input["field_axes"]["water"] = 64.into(),
            2 => input["samples_per_axis"] = 128.into(),
            3 => input["request"]["center_latitude_e7"] = 600_000_000.into(),
            4 => {
                input["water_corrections"] =
                    serde_json::to_value(WaterCorrectionDocument::empty(request, 128).unwrap())
                        .unwrap()
            }
            5 => input["hydrology_mode"] = "none".into(),
            6 => input["request"]["requested_side_meters"] = 3_000_000.into(),
            _ => {
                input["water_corrections"] =
                    serde_json::to_value(WaterCorrectionDocument::empty(request, 1024).unwrap())
                        .unwrap();
                input["water_corrections"]["target_year_ce"] = 2000.into();
            }
        }
        if change == 5 {
            input["water_corrections"] =
                serde_json::to_value(WaterCorrectionDocument::empty(request, 1024).unwrap())
                    .unwrap();
        }
        assert!(
            execute(serde_json::from_value(input).unwrap()).is_err(),
            "case {change}"
        );
        assert!(!cache.exists());
        assert!(!output.exists());
    }
    let mut malformed = valid.clone();
    malformed["hydrology_mode"] = "unknown".into();
    assert!(serde_json::from_value::<WorkerRequest>(malformed).is_err());
    assert!(matches!(
        crate::execute_with_cancellation(
            serde_json::from_value(valid).unwrap(),
            &AtomicBool::new(true)
        ),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
    assert!(!cache.exists());
    assert!(!output.exists());
}
