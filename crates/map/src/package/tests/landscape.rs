use super::*;
use crate::DetailProfile;
use aoe_core::TileCoord;

fn request(profile: DetailProfile) -> MapRequest {
    MapRequest {
        detail_profile: profile,
        ..MapRequest::default()
    }
}
fn modeled_environment(request: MapRequest) -> PreparedEnvironment {
    PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: crate::FieldPyramid {
            levels: vec![
                crate::PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: [3; 32],
                },
                crate::PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: [4; 32],
                },
            ],
        },
        hydrology_evidence: Some(crate::HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
            policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [1; 32],
            modern_land_cover_page_root: [2; 32],
            water_model: Some(crate::HydrologyWaterModelIndex {
                model_version: crate::HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: 2,
                target_year_ce: crate::WATER_CORRECTION_TARGET_YEAR_CE,
                correction_document: crate::WaterCorrectionDocument::empty(request, 2).unwrap(),
            }),
        }),
        ..PreparedEnvironment::default()
    }
}
fn package(profile: DetailProfile, modeled: bool) -> MapPackage {
    let request = request(profile);
    MapPackage::with_prepared_environment(
        9,
        request,
        vec![source("elevation")],
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        if modeled {
            modeled_environment(request)
        } else {
            PreparedEnvironment::default()
        },
    )
    .unwrap()
}
fn geography(package: &MapPackage) -> [u8; 32] {
    hash_package(
        package.generator_version,
        package.request,
        &package.source_locks,
        &package.projection,
        &package.provenance,
        &package.environment,
        HashMode::Geography,
    )
}

#[test]
fn landscape_profile_is_appended_explicitly_without_changing_standard_defaults() {
    assert_eq!(DetailProfile::StandardV1 as u8, 0);
    assert_eq!(DetailProfile::LandscapeV2 as u8, 1);
    assert_eq!(
        serde_json::to_string(&DetailProfile::LandscapeV2).unwrap(),
        "\"landscape_v2\""
    );
    let mut value = serde_json::to_value(MapRequest::default()).unwrap();
    value.as_object_mut().unwrap().remove("detail_profile");
    let decoded: MapRequest = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.detail_profile, DetailProfile::StandardV1);
    assert_eq!(decoded.schema_version, 1);
    let standard = package(DetailProfile::StandardV1, false);
    let landscape = package(DetailProfile::LandscapeV2, false);
    assert_eq!(
        (
            standard.schema_version,
            standard.generator_version,
            standard.generation_recipe_version
        ),
        (9, 9, 8)
    );
    assert_eq!(
        (
            landscape.schema_version,
            landscape.generator_version,
            landscape.generation_recipe_version
        ),
        (10, 9, 9)
    );
    standard.validate().unwrap();
    landscape.validate().unwrap();
}

#[test]
fn profile_schema_recipe_matrix_rejects_mixed_contracts() {
    for detail in [DetailProfile::StandardV1, DetailProfile::LandscapeV2] {
        for schema in [7, 8, 9, 10, 11] {
            for recipe in 2..=10 {
                for evidence in [false, true] {
                    let expected = match detail {
                        DetailProfile::StandardV1 => {
                            (3..=8).contains(&recipe) && (schema == 9 || (schema == 8 && !evidence))
                        }
                        DetailProfile::LandscapeV2 => schema == 10 && recipe == 9,
                    };
                    assert_eq!(
                        profile::validate(schema, detail, recipe, evidence).is_ok(),
                        expected,
                        "{detail:?} schema{schema}/recipe{recipe}/typed{evidence}"
                    );
                }
            }
        }
    }
    for detail in [DetailProfile::StandardV1, DetailProfile::LandscapeV2] {
        let mut value = serde_json::to_value(package(detail, false)).unwrap();
        value["request"]["detail_profile"] =
            serde_json::to_value(if detail == DetailProfile::StandardV1 {
                DetailProfile::LandscapeV2
            } else {
                DetailProfile::StandardV1
            })
            .unwrap();
        assert!(match serde_json::from_value::<MapPackage>(value) {
            Err(_) => true,
            Ok(parsed) => parsed.validate().is_err(),
        });
    }
}

#[test]
fn detail_changes_content_identity_but_not_geography_or_published_source_heights() {
    for modeled in [false, true] {
        let standard = package(DetailProfile::StandardV1, modeled);
        let landscape = package(DetailProfile::LandscapeV2, modeled);
        assert_ne!(standard.content_hash, landscape.content_hash);
        assert_eq!(geography(&standard), geography(&landscape));
        assert_eq!(
            landscape.environment,
            if modeled {
                modeled_environment(landscape.request)
            } else {
                PreparedEnvironment::default()
            }
        );
        let decoded: MapPackage =
            serde_json::from_slice(&serde_json::to_vec(&landscape).unwrap()).unwrap();
        assert_eq!(decoded, landscape);
        decoded.validate().unwrap();
        // Source-free defaults still use the same frozen geography hash. Richer
        // provider geometry is qualified separately with prepared page fixtures.
        if !modeled {
            let old = standard.generator();
            let new = landscape.generator();
            for tile in [
                TileCoord::new(0, 0),
                TileCoord::new(50, 70),
                TileCoord::new(100, 150),
            ] {
                let old = old.tile_at(tile).unwrap();
                let new = new.tile_at(tile).unwrap();
                assert_eq!(
                    new.geographic_height_centimeters,
                    old.geographic_height_centimeters
                );
                assert_eq!(new.game_height_level, old.game_height_level);
                assert_eq!(new.surface, old.surface);
                assert_eq!(new.elevation_provenance, old.elevation_provenance);
            }
        }
    }
    let old = package(DetailProfile::StandardV1, false);
    let mut altered = old.request;
    altered.seed += 1;
    let other = MapPackage::new(9, altered, old.source_locks.clone()).unwrap();
    assert_ne!(old.content_hash, other.content_hash);
    assert_eq!(geography(&old), geography(&other));
}

#[test]
fn landscape_modeled_water_binds_actual_request_without_reusing_old_document_identity() {
    let standard = package(DetailProfile::StandardV1, true);
    let landscape = package(DetailProfile::LandscapeV2, true);
    assert_eq!(landscape.generation_recipe_version, 9);
    assert_eq!(standard.generation_recipe_version, 8);
    assert_ne!(
        standard
            .environment
            .hydrology_evidence
            .as_ref()
            .unwrap()
            .water_model
            .as_ref()
            .unwrap()
            .correction_document
            .digest()
            .unwrap(),
        landscape
            .environment
            .hydrology_evidence
            .as_ref()
            .unwrap()
            .water_model
            .as_ref()
            .unwrap()
            .correction_document
            .digest()
            .unwrap()
    );
    assert_eq!(geography(&standard), geography(&landscape));
    assert!(
        MapPackage::with_prepared_environment(
            9,
            landscape.request,
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            standard.environment
        )
        .is_err()
    );
    assert!(
        MapPackage::with_generation_recipe(
            9,
            6,
            request(DetailProfile::StandardV1),
            Vec::new(),
            ProjectionMetadata::default(),
            EnvironmentalProvenance::default(),
            PreparedEnvironment::default()
        )
        .is_err()
    );
}

#[path = "landscape/serialization.rs"]
mod serialization;
