use super::*;

#[test]
fn modeled_water_uses_recipe_six_while_model_free_recipe_five_identity_is_unchanged() {
    let request = MapRequest::default();
    let legacy_current = MapPackage::with_generation_recipe(
        crate::MAP_SCHEMA_VERSION,
        crate::GENERATION_RECIPE_VERSION,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        PreparedEnvironment::default(),
    )
    .expect("recipe-five package");
    let ordinary =
        MapPackage::new(crate::MAP_SCHEMA_VERSION, request, Vec::new()).expect("ordinary package");
    assert_eq!(
        ordinary.generation_recipe_version,
        crate::GENERATION_RECIPE_VERSION
    );
    assert_eq!(ordinary.content_hash, legacy_current.content_hash);
    assert_eq!(
        terrain_fingerprint(&ordinary.generator().chunk(0, 0).expect("ordinary chunk")),
        terrain_fingerprint(
            &legacy_current
                .generator()
                .chunk(0, 0)
                .expect("recipe-five chunk")
        )
    );

    let water_model = crate::HydrologyWaterModelIndex {
        model_version: crate::HYDROLOGY_WATER_MODEL_VERSION,
        samples_per_axis: 2,
        target_year_ce: crate::WATER_CORRECTION_TARGET_YEAR_CE,
        correction_document: crate::WaterCorrectionDocument::empty(request, 2)
            .expect("empty water corrections"),
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![crate::PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: [3; 32],
            }],
        },
        hydrology_evidence: Some(crate::HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
            policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [1; 32],
            modern_land_cover_page_root: [2; 32],
            water_model: Some(water_model),
        }),
        ..PreparedEnvironment::default()
    };
    let modeled = MapPackage::with_prepared_environment(
        crate::MAP_SCHEMA_VERSION,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment.clone(),
    )
    .expect("modeled package");
    assert_eq!(
        modeled.generation_recipe_version,
        crate::WATER_MODEL_GENERATION_RECIPE_VERSION
    );
    assert!(modeled.validate().is_ok());
    assert_ne!(modeled.content_hash, ordinary.content_hash);
    assert_eq!(
        MapPackage::with_generation_recipe(
            crate::MAP_SCHEMA_VERSION,
            crate::GENERATION_RECIPE_VERSION,
            request,
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            environment.clone(),
        ),
        Err(MapPackageError::InvalidGenerationRecipeVersion)
    );
    assert_eq!(
        MapPackage::with_generation_recipe(
            crate::MAP_SCHEMA_VERSION,
            crate::WATER_MODEL_GENERATION_RECIPE_VERSION,
            request,
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            PreparedEnvironment::default(),
        ),
        Err(MapPackageError::InvalidGenerationRecipeVersion)
    );
}
