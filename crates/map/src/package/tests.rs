use super::*;

#[path = "tests/water_model.rs"]
mod water_model;

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
fn validation_rejects_tampering_and_old_generation_recipe_identity() {
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
        HashMode::Content(None),
    );
    assert_eq!(package.validate(), Err(MapPackageError::NonCanonicalFields));
    assert_eq!(
        package.generator().chunk(0, 0),
        current.generator().chunk(0, 0)
    );
    package.content_hash = hash_package(
        package.generator_version,
        package.request,
        &package.source_locks,
        &package.projection,
        &package.provenance,
        &package.environment,
        HashMode::Content(Some(2)),
    );
    assert_eq!(package.validate(), Err(MapPackageError::NonCanonicalFields));
    assert_eq!(
        terrain_fingerprint(&current.generator().chunk(0, 0).expect("fixture chunk")),
        [
            0x38, 0xfc, 0x44, 0x7f, 0x20, 0x1c, 0xca, 0xc7, 0x13, 0x84, 0x04, 0x8a, 0xe9, 0xee,
            0x2e, 0x4b, 0x03, 0xa1, 0xb7, 0xca, 0x3a, 0x89, 0x97, 0x93, 0x74, 0xa8, 0xa2, 0x6d,
            0x83, 0x1b, 0xd0, 0x6e,
        ]
    );
}

fn schema_nine_package_for_recipe(generation_recipe_version: u16) -> MapPackage {
    MapPackage::with_generation_recipe(
        crate::MAP_SCHEMA_VERSION,
        generation_recipe_version,
        MapRequest::default(),
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment::default(),
    )
    .expect("package")
}

#[test]
fn supported_generation_recipes_preserve_serialization_and_identity() {
    let legacy = schema_nine_package_for_recipe(crate::LEGACY_GENERATION_RECIPE_VERSION);
    let prior = schema_nine_package_for_recipe(crate::PRIOR_GENERATION_RECIPE_VERSION);
    let current = schema_nine_package_for_recipe(crate::GENERATION_RECIPE_VERSION);

    assert_eq!(
        legacy.generation_recipe_version,
        crate::LEGACY_GENERATION_RECIPE_VERSION
    );
    assert_eq!(
        legacy.content_hash_hex(),
        "d8aa78b19092802ca080ce67b34389196eb20dcb33bc8dc117f2de9815c9228d"
    );
    assert!(legacy.validate().is_ok());
    let serialized = serde_json::to_value(&legacy).expect("legacy JSON");
    assert!(
        !serialized
            .as_object()
            .expect("package object")
            .contains_key("generation_recipe_version")
    );
    let decoded: MapPackage = serde_json::from_value(serialized).expect("legacy decode");
    assert_eq!(decoded, legacy);
    assert!(decoded.validate().is_ok());

    assert_eq!(
        prior.generation_recipe_version,
        crate::PRIOR_GENERATION_RECIPE_VERSION
    );
    assert_eq!(
        prior.content_hash_hex(),
        "1eb4f33086e24910842a52b65c8a70ec2a40c7c9aba7db5f7b3c566f4f5f8c0c"
    );
    assert_eq!(
        current.generation_recipe_version,
        crate::GENERATION_RECIPE_VERSION
    );
    assert_eq!(
        MapPackage::new(crate::MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
            .expect("current package"),
        current
    );
    assert_ne!(current.content_hash, legacy.content_hash);
    assert_ne!(current.content_hash, prior.content_hash);
    for package in [&prior, &current] {
        let serialized = serde_json::to_value(package).expect("versioned package JSON");
        assert_eq!(
            serialized["generation_recipe_version"],
            package.generation_recipe_version
        );
        let decoded: MapPackage = serde_json::from_value(serialized).expect("versioned decode");
        assert_eq!(decoded, *package);
        assert!(decoded.validate().is_ok());
    }

    let mut unknown = current;
    unknown.generation_recipe_version = 99;
    assert_eq!(
        unknown.validate(),
        Err(MapPackageError::InvalidGenerationRecipeVersion)
    );
}

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
fn schema_eight_manifest_and_hash_remain_readable_after_schema_nine() {
    const LEGACY_HASH: &str = "1c82962e62bdfad02d3e0cb5f50aa85f1d72a0e778437c899284f5dfa6e95b18";
    const LEGACY_TERRAIN_FINGERPRINT: &str =
        "ef9ad686d6c588cacb21be148a4ec6291ea49997a77f47a825e77f30cd4c99aa";
    let fixture = include_str!("../../tests/fixtures/schema8-default-package.json");
    let legacy: MapPackage = serde_json::from_str(fixture).expect("legacy manifest parses");
    assert_eq!(legacy.schema_version, crate::LEGACY_MAP_SCHEMA_VERSION);
    assert_eq!(legacy.generator_version, 8);
    assert_eq!(legacy.content_hash_hex(), LEGACY_HASH);
    assert!(legacy.validate().is_ok());
    assert_eq!(
        serde_json::to_vec_pretty(&legacy).unwrap(),
        fixture.as_bytes()
    );

    let legacy_chunk = legacy
        .generator()
        .chunk(0, 0)
        .expect("legacy terrain chunk");
    assert_eq!(fingerprint_hex(&legacy_chunk), LEGACY_TERRAIN_FINGERPRINT);

    let current = MapPackage::new(crate::MAP_SCHEMA_VERSION, MapRequest::default(), vec![])
        .expect("current schema package");
    // The published recipe-4 identity uses the schema-9 generator input; it
    // is neither the generator-version-1 fixture nor the recipe-5 default.
    let prior = schema_nine_package_for_recipe(crate::PRIOR_GENERATION_RECIPE_VERSION);
    assert_eq!(current.schema_version, 9);
    assert_eq!(current.generator_version, 9);
    assert_eq!(
        current.generation_recipe_version,
        crate::GENERATION_RECIPE_VERSION
    );
    assert_eq!(
        prior.content_hash_hex(),
        "1eb4f33086e24910842a52b65c8a70ec2a40c7c9aba7db5f7b3c566f4f5f8c0c"
    );
    assert_ne!(current.content_hash, prior.content_hash);
    let current_chunk = current
        .generator()
        .chunk(0, 0)
        .expect("current terrain chunk");
    assert_ne!(fingerprint_hex(&current_chunk), LEGACY_TERRAIN_FINGERPRINT);
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
    .expect("schema-nine package");
    assert!(matches!(
        package.generator_with_elevation(Vec::new()),
        Err(MapPackageError::InvalidEnvironment)
    ));
    assert!(matches!(
        package.generator_with_environment(Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        Err(MapPackageError::InvalidEnvironment)
    ));

    let mut schema_eight_with_evidence = package.clone();
    schema_eight_with_evidence.schema_version = crate::LEGACY_MAP_SCHEMA_VERSION;
    assert_eq!(
        schema_eight_with_evidence.validate(),
        Err(MapPackageError::NonCanonicalFields)
    );
    let mut unsupported_schema =
        MapPackage::new(crate::MAP_SCHEMA_VERSION, MapRequest::default(), vec![])
            .expect("schema-nine package");
    unsupported_schema.schema_version += 1;
    assert_eq!(
        unsupported_schema.validate(),
        Err(MapPackageError::NonCanonicalFields)
    );
}
