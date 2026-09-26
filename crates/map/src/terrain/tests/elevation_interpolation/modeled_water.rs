use super::*;

#[test]
fn verified_modeled_lake_surface_is_the_surface_returned_for_terrain_queries() {
    use crate::{
        HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyWaterModelIndex,
        HydrologyWaterModelPage, HydrologyWaterPolicy, ModernLandCoverPage,
        WaterCorrectionDocument, WaterFlowDirection, WaterModelProvenance,
        ordered_hydrology_page_root, ordered_modern_land_cover_page_root, ordered_water_page_root,
    };

    let elevation = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0, 5_000, 6_000, 9_000],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        ocean_coverage_percent: vec![0, 100, 0, 0],
        inland_coverage_percent: vec![100, 0, 0, 0],
    };
    let hydrology = HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![
            HydrologyKind::Lake as u8,
            HydrologyKind::Ocean as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
        ],
        method: vec![
            HydrologyEvidenceMethod::HydroLakesExtent as u8,
            HydrologyEvidenceMethod::OverviewOcean as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
            HydrologyEvidenceMethod::WorldCoverClass as u8,
        ],
        water_model: Some(HydrologyWaterModelPage {
            kind: vec![
                HydrologyKind::Lake as u8,
                HydrologyKind::Ocean as u8,
                HydrologyKind::Land as u8,
                HydrologyKind::Land as u8,
            ],
            surface_level_centimeters: vec![Some(1_234), Some(0), None, None],
            flow_direction: vec![WaterFlowDirection::Unknown as u8; 4],
            provenance: vec![
                WaterModelProvenance::ModelledLakeSurface as u8,
                WaterModelProvenance::ModelledOceanSurface as u8,
                WaterModelProvenance::EvidenceOnly as u8,
                WaterModelProvenance::EvidenceOnly as u8,
            ],
        }),
    };
    hydrology.validate().expect("hydrology model page");
    let cover = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![40; 4],
    };
    let request = crate::MapRequest::default();
    let environment = crate::PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_page_root(std::slice::from_ref(&elevation))
                    .expect("elevation root"),
            }],
        },
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 2,
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water))
                    .expect("water root"),
            }],
        }),
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: Some(crate::HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: crate::WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(std::slice::from_ref(&hydrology))
                .expect("hydrology root"),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(std::slice::from_ref(
                &cover,
            ))
            .expect("cover root"),
            water_model: Some(HydrologyWaterModelIndex {
                model_version: crate::HYDROLOGY_WATER_MODEL_VERSION,
                samples_per_axis: 2,
                target_year_ce: crate::WATER_CORRECTION_TARGET_YEAR_CE,
                correction_document: WaterCorrectionDocument::empty(request, 2)
                    .expect("correction document"),
            }),
        }),
    };
    let provider = Pages(BTreeMap::from([
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::Elevation,
                level: 0,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::Elevation(elevation)),
        ),
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::Water,
                level: 0,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::Water(water)),
        ),
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::HydrologyEvidence,
                level: 0,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::HydrologyEvidence(hydrology)),
        ),
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::ModernLandCover,
                level: 0,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::ModernLandCover(cover)),
        ),
    ]));
    let generator = MapChunkGenerator::new([8; 32], 3, 128)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(provider),
        )
        .expect("verified environment provider");
    let tile = generator
        .tile_at_with_cancel(TileCoord::new(0, 0), &|| false)
        .expect("tile query")
        .expect("lake tile");
    assert_eq!(tile.water, WaterKind::Lake);
    assert_eq!(tile.water_provenance, Provenance::ModelDerived);
    assert_eq!(tile.game_height_level, 12);
    assert_eq!(tile.surface.corner_game_height_levels, [12; 4]);
    assert_eq!(tile.surface.kind, SurfaceKind::Plateau);
    assert_eq!(tile.material, GroundMaterial::Water);
    assert!(!tile.passable);

    let coast = generator
        .tile_at_with_cancel(TileCoord::new(127, 0), &|| false)
        .expect("coast tile query")
        .expect("ocean tile");
    assert_eq!(coast.water, WaterKind::Ocean);
    assert_eq!(coast.water_provenance, Provenance::ModelDerived);
    assert_eq!(coast.surface.corner_game_height_levels, [0; 4]);
}
