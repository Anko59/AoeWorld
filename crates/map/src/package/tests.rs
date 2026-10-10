use super::*;

#[path = "tests/strict.rs"]
mod strict;

fn environment() -> PreparedEnvironment {
    PreparedEnvironment {
        samples_per_axis: 4,
        geographic_millimeters_per_sample: 30_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 4,
                    ordered_page_root: [1; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [2; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [3; 32],
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    }
}
fn source(id: &str) -> SourceLock {
    SourceLock {
        id: id.to_owned(),
        provider: "fixture".to_owned(),
        release: "test".to_owned(),
        url: "https://example.invalid/test".to_owned(),
        sha256: [7; 32],
        acquired_at: "2026-09-18T00:00:00Z".to_owned(),
        native_resolution: "30 meters".to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "EGM2008".to_owned(),
        license: "test-only".to_owned(),
        preprocessing_version: "test-v1".to_owned(),
    }
}

fn terrain_fingerprint(chunk: &crate::Chunk) -> [u8; 32] {
    let mut hash = blake3::Hasher::new();
    for tile in &chunk.tiles {
        hash.update(&tile.geographic_height_centimeters.to_le_bytes());
        hash.update(&tile.game_height_level.to_le_bytes());
        for level in tile.surface.corner_game_height_levels {
            hash.update(&level.to_le_bytes());
        }
        hash.update(&[
            tile.surface.kind as u8,
            tile.surface.triangulation as u8,
            tile.biome as u8,
            tile.vegetation_provenance as u8,
            tile.water as u8,
            tile.elevation_provenance as u8,
            tile.water_provenance as u8,
            u8::from(tile.passable),
        ]);
    }
    *hash.finalize().as_bytes()
}

fn fingerprint_hex(chunk: &crate::Chunk) -> String {
    terrain_fingerprint(chunk)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
#[test]
fn packages_have_canonical_source_order_and_stable_identity() {
    let first =
        MapPackage::new(1, MapRequest::default(), vec![source("b"), source("a")]).expect("package");
    let second =
        MapPackage::new(1, MapRequest::default(), vec![source("a"), source("b")]).expect("package");
    assert_eq!(first, second);
    assert_eq!(first.chunk_count_per_side(), 16);
}
#[test]
fn seed_changes_detail_identity_but_not_geographic_elevation() {
    let first = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    let second = MapPackage::new(
        1,
        MapRequest {
            seed: 2,
            ..MapRequest::default()
        },
        vec![],
    )
    .expect("package");
    assert_ne!(first.content_hash, second.content_hash);
    assert_eq!(
        first.generator().chunk(0, 0).expect("fixture chunk").tiles[0]
            .geographic_height_centimeters,
        second.generator().chunk(0, 0).expect("fixture chunk").tiles[0]
            .geographic_height_centimeters
    );
}
#[test]
fn duplicate_source_locks_are_rejected() {
    assert!(matches!(
        MapPackage::new(
            1,
            MapRequest::default(),
            vec![source("same"), source("same")]
        ),
        Err(MapPackageError::InvalidSourceLocks)
    ));
}
#[test]
fn validation_rejects_tampering_and_other_generation_recipe_identity() {
    let mut package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    package.estimate.tiles_per_side += 1;
    assert_eq!(package.validate(), Err(MapPackageError::NonCanonicalFields));
    let current = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    package = current.clone();
    package.content_hash = hash_package(
        package.generator_version,
        package.request,
        &package.source_locks,
        &package.projection,
        &package.provenance,
        &package.environment,
        HashMode::Content(crate::GENERATION_RECIPE_VERSION + 1),
    );
    assert_eq!(package.validate(), Err(MapPackageError::NonCanonicalFields));
    assert_eq!(
        package.generator().chunk(0, 0),
        current.generator().chunk(0, 0)
    );
    package = current.clone();
    package.generation_recipe_version += 1;
    assert_eq!(
        package.validate(),
        Err(MapPackageError::InvalidGenerationRecipeVersion)
    );
}

/// Regression guard on the current version, not a compatibility fixture: a
/// deliberate generation or identity change updates these digests.
#[test]
fn default_package_identity_and_terrain_are_pinned() {
    let package = MapPackage::new(1, MapRequest::default(), vec![]).expect("package");
    assert_eq!(package.schema_version, crate::MAP_SCHEMA_VERSION);
    assert_eq!(
        package.generation_recipe_version,
        crate::GENERATION_RECIPE_VERSION
    );
    assert_eq!(package.content_hash_hex(), DEFAULT_PACKAGE_HASH);
    assert_eq!(
        fingerprint_hex(&package.generator().chunk(0, 0).expect("chunk")),
        DEFAULT_TERRAIN_FINGERPRINT
    );
}

const DEFAULT_PACKAGE_HASH: &str =
    "9f736ce35dc8498c6e33256586a9cdb6c4ea7844d474dbb691ee684ee96068b5";
const DEFAULT_TERRAIN_FINGERPRINT: &str =
    "5216ecbe42545ee3958222a22b78957a274aad41029b38797e926ac8ac46aa29";

#[test]
fn packages_require_and_hash_projection_metadata() {
    assert!(matches!(
        MapPackage::with_projection(
            1,
            MapRequest::default(),
            Vec::new(),
            ProjectionMetadata {
                horizontal_crs: " ".to_owned(),
                vertical_datum: VerticalDatum::UnspecifiedFallback,
                tool_version: "test".to_owned(),
            },
        ),
        Err(MapPackageError::InvalidProjection)
    ));
    let first = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let second = MapPackage::with_projection(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata {
            horizontal_crs: "EPSG:3857".to_owned(),
            vertical_datum: VerticalDatum::Egm2008Orthometric,
            tool_version: "GDAL 3.6.2 / PROJ 9.1.1".to_owned(),
        },
    )
    .expect("package");
    assert_ne!(first.content_hash, second.content_hash);
}

#[test]
fn packages_hash_environmental_provenance() {
    let fallback = MapPackage::new(1, MapRequest::default(), Vec::new()).expect("package");
    let sourced = MapPackage::with_environment(
        1,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: LayerProvenance::HistoricallyCorrected,
            vegetation: LayerProvenance::ModelDerived,
            historical_land_use: LayerProvenance::SourceDerived,
        },
    )
    .expect("package");
    assert_ne!(fallback.content_hash, sourced.content_hash);
}

#[test]
fn packages_hash_prepared_environment_roots() {
    let package = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        vec![source("elevation")],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment(),
    )
    .expect("prepared package");
    let mut changed_environment = environment();
    changed_environment.elevation.levels[0].ordered_page_root = [4; 32];
    let changed = MapPackage::with_prepared_environment(
        1,
        MapRequest::default(),
        vec![source("elevation")],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        changed_environment,
    )
    .expect("changed package");
    assert_ne!(package.content_hash, changed.content_hash);
    assert!(package.validate().is_ok());
}

#[test]
fn acquisition_time_is_not_a_content_input_but_preprocessing_is() {
    let first =
        MapPackage::new(1, MapRequest::default(), vec![source("elevation")]).expect("package");
    let mut later_source = source("elevation");
    later_source.acquired_at = "2026-09-19T00:00:00Z".to_owned();
    let later = MapPackage::new(1, MapRequest::default(), vec![later_source]).expect("package");
    assert_eq!(first.content_hash, later.content_hash);
    let mut altered_source = source("elevation");
    altered_source.preprocessing_version = "test-v2".to_owned();
    let altered = MapPackage::new(1, MapRequest::default(), vec![altered_source]).expect("package");
    assert_ne!(first.content_hash, altered.content_hash);
}

#[test]
fn unsupported_schemas_and_vector_access_without_typed_pages_are_rejected() {
    let mut environment = environment();
    environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
        samples_per_axis: 2,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
        policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [4; 32],
        modern_land_cover_page_root: [5; 32],
        water_model: None,
    });
    let package = MapPackage::with_prepared_environment(
        crate::MAP_SCHEMA_VERSION,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )
    .expect("typed-evidence package");
    assert!(matches!(
        package.generator_with_elevation(Vec::new()),
        Err(MapPackageError::InvalidEnvironment)
    ));
    assert!(matches!(
        package.generator_with_environment(Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        Err(MapPackageError::InvalidEnvironment)
    ));
    for schema_version in [0, crate::MAP_SCHEMA_VERSION + 1] {
        let mut unsupported_schema = package.clone();
        unsupported_schema.schema_version = schema_version;
        assert_eq!(
            unsupported_schema.validate(),
            Err(MapPackageError::NonCanonicalFields)
        );
    }
}
