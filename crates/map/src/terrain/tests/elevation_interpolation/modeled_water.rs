use super::*;

#[test]
fn mapped_lake_on_dry_overview_uses_its_modeled_surface_for_terrain_queries() {
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
        inland_coverage_percent: vec![0, 0, 100, 100],
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
                WaterModelProvenance::GeographicCorrection as u8,
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
    let correction_document = WaterCorrectionDocument::new(
        request,
        2,
        vec![crate::GeographicWaterPatch {
            id: "historical-dry-cell".to_owned(),
            precedence: 1,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "curated historical source".to_owned(),
            operation: crate::WaterCorrectionOperation::SetLand,
            polygon: vec![
                crate::WaterCorrectionVertex {
                    longitude_e7: 235_800_000,
                    latitude_e7: 487_600_000,
                },
                crate::WaterCorrectionVertex {
                    longitude_e7: 236_200_000,
                    latitude_e7: 487_600_000,
                },
                crate::WaterCorrectionVertex {
                    longitude_e7: 236_200_000,
                    latitude_e7: 488_000_000,
                },
                crate::WaterCorrectionVertex {
                    longitude_e7: 235_800_000,
                    latitude_e7: 488_000_000,
                },
            ],
        }],
    )
    .expect("same-kind dry correction");
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
                correction_document,
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

    let corrected_land = generator
        .tile_at_with_cancel(TileCoord::new(127, 127), &|| false)
        .expect("corrected land query")
        .expect("corrected land tile");
    assert_eq!(corrected_land.water, WaterKind::None);
    assert_eq!(
        corrected_land.water_provenance,
        Provenance::HistoricallyCorrected
    );

    let modern_land = generator
        .tile_at_with_cancel(TileCoord::new(0, 127), &|| false)
        .expect("modern land query")
        .expect("historical fallback tile");
    assert_eq!(modern_land.water, WaterKind::Lake);

    let coast = generator
        .tile_at_with_cancel(TileCoord::new(127, 0), &|| false)
        .expect("coast tile query")
        .expect("ocean tile");
    assert_eq!(coast.water, WaterKind::Ocean);
    assert_eq!(coast.water_provenance, Provenance::ModelDerived);
    assert_eq!(coast.surface.corner_game_height_levels, [0; 4]);
}
