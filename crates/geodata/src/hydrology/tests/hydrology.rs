use super::*;

fn prepared(kinds: Vec<u8>) -> PreparedHydrology {
    let methods = kinds
        .iter()
        .map(|kind| match HydrologyKind::try_from(*kind).expect("kind") {
            HydrologyKind::Ocean => aoe_map::HydrologyEvidenceMethod::OverviewOcean as u8,
            HydrologyKind::Lake
            | HydrologyKind::Reservoir
            | HydrologyKind::UnknownWater
            | HydrologyKind::RegulatedLake => {
                aoe_map::HydrologyEvidenceMethod::HydroLakesExtent as u8
            }
            HydrologyKind::River => {
                aoe_map::HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8
            }
            HydrologyKind::NoEvidence => aoe_map::HydrologyEvidenceMethod::None as u8,
            HydrologyKind::Land | HydrologyKind::Shallow => {
                aoe_map::HydrologyEvidenceMethod::WorldCoverClass as u8
            }
        })
        .collect::<Vec<_>>();
    let hydrology_pages = vec![HydrologyPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: kinds,
        method: methods,
        water_model: None,
    }];
    let modern_land_cover_pages = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![40; 4],
    }];
    let evidence_index = aoe_map::HydrologyEvidenceIndex {
        samples_per_axis: 2,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
        policy: aoe_map::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: aoe_map::ordered_hydrology_page_root(&hydrology_pages)
            .expect("hydrology root"),
        modern_land_cover_page_root: aoe_map::ordered_modern_land_cover_page_root(
            &modern_land_cover_pages,
        )
        .expect("cover root"),
        water_model: None,
    };
    PreparedHydrology {
        samples_per_axis: 2,
        evidence_index,
        source_locks: Vec::new(),
        hydrology_pages,
        modern_land_cover_pages,
        river_topology: None,
    }
}

#[test]
fn modern_water_is_consumable_without_rewriting_historical_land_use() {
    let modern = prepared(vec![
        HydrologyKind::Ocean as u8,
        HydrologyKind::Lake as u8,
        HydrologyKind::River as u8,
        HydrologyKind::Land as u8,
    ]);
    assert_eq!(
        modern.modern_water_override_at(4, 0, 0).expect("water"),
        Some((100, 0))
    );
    assert_eq!(
        modern.modern_water_override_at(4, 2, 0).expect("water"),
        Some((0, 100))
    );
    assert_eq!(
        modern.modern_water_override_at(4, 0, 2).expect("water"),
        Some((0, 100))
    );
    assert_eq!(
        modern.modern_water_override_at(4, 2, 2).expect("water"),
        None
    );
    assert!(modern.modern_water_override_at(4, 4, 0).is_err());
    let evidence_only = prepared(vec![
        HydrologyKind::Reservoir as u8,
        HydrologyKind::Shallow as u8,
        HydrologyKind::UnknownWater as u8,
        HydrologyKind::NoEvidence as u8,
    ]);
    assert_eq!(
        evidence_only
            .modern_water_override_at(2, 0, 0)
            .expect("water"),
        None
    );
    assert_eq!(
        evidence_only
            .modern_water_override_at(2, 1, 1)
            .expect("no-evidence fallback"),
        None
    );
}

#[test]
fn modeled_water_correction_overrides_the_shared_coverage_without_promoting_reservoirs() {
    let mut prepared = prepared(vec![
        HydrologyKind::Lake as u8,
        HydrologyKind::Reservoir as u8,
        HydrologyKind::River as u8,
        HydrologyKind::Land as u8,
    ]);
    prepared.hydrology_pages[0].water_model = Some(aoe_map::HydrologyWaterModelPage {
        kind: vec![
            HydrologyKind::Land as u8,
            HydrologyKind::Reservoir as u8,
            HydrologyKind::River as u8,
            HydrologyKind::Land as u8,
        ],
        surface_level_centimeters: vec![None; 4],
        flow_direction: vec![aoe_map::WaterFlowDirection::Unknown as u8; 4],
        provenance: vec![
            aoe_map::WaterModelProvenance::GeographicCorrection as u8,
            aoe_map::WaterModelProvenance::EvidenceOnly as u8,
            aoe_map::WaterModelProvenance::EvidenceOnly as u8,
            aoe_map::WaterModelProvenance::EvidenceOnly as u8,
        ],
    });
    let corrections = aoe_map::WaterCorrectionDocument::new(
        MapRequest::default(),
        2,
        vec![aoe_map::GeographicWaterPatch {
            id: "fixture-set-land".to_owned(),
            precedence: 1,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "test fixture".to_owned(),
            operation: aoe_map::WaterCorrectionOperation::SetLand,
            polygon: vec![
                aoe_map::WaterCorrectionVertex {
                    longitude_e7: 21_000_000,
                    latitude_e7: 488_500_000,
                },
                aoe_map::WaterCorrectionVertex {
                    longitude_e7: 23_500_000,
                    latitude_e7: 488_500_000,
                },
                aoe_map::WaterCorrectionVertex {
                    longitude_e7: 23_500_000,
                    latitude_e7: 490_500_000,
                },
                aoe_map::WaterCorrectionVertex {
                    longitude_e7: 21_000_000,
                    latitude_e7: 490_500_000,
                },
            ],
        }],
    )
    .expect("correction document");
    prepared.evidence_index.water_model = Some(aoe_map::HydrologyWaterModelIndex {
        model_version: aoe_map::HYDROLOGY_WATER_MODEL_VERSION,
        samples_per_axis: 2,
        target_year_ce: aoe_map::WATER_CORRECTION_TARGET_YEAR_CE,
        correction_document: corrections,
    });
    prepared.evidence_index.hydrology_page_root =
        ordered_hydrology_page_root(&prepared.hydrology_pages).expect("modeled root");
    prepared
        .evidence_index
        .validate_pages(&prepared.hydrology_pages, &prepared.modern_land_cover_pages)
        .expect("verified modeled evidence");

    assert_eq!(
        prepared
            .modern_water_override_at(2, 0, 0)
            .expect("corrected land"),
        Some((0, 0))
    );
    assert_eq!(
        prepared
            .modern_water_override_at(2, 1, 0)
            .expect("reservoir"),
        None
    );
    assert_eq!(
        prepared.modern_water_override_at(2, 0, 1).expect("river"),
        Some((0, 100))
    );
}

#[test]
fn evidence_page_lookup_reaches_the_last_row_major_page_directly() {
    let axis = 65_u16;
    let mut hydrology_pages = Vec::new();
    let mut cover_pages = Vec::new();
    for y in 0..2_u16 {
        for x in 0..2_u16 {
            let width = (axis - x * 64).min(64) as u8;
            let height = (axis - y * 64).min(64) as u8;
            let len = usize::from(width) * usize::from(height);
            let last_page = x == 1 && y == 1;
            hydrology_pages.push(HydrologyPage {
                level: 0,
                x,
                y,
                width,
                height,
                kind: vec![
                    if last_page {
                        HydrologyKind::Lake as u8
                    } else {
                        HydrologyKind::Land as u8
                    };
                    len
                ],
                method: vec![
                    if last_page {
                        aoe_map::HydrologyEvidenceMethod::HydroLakesExtent as u8
                    } else {
                        aoe_map::HydrologyEvidenceMethod::WorldCoverClass as u8
                    };
                    len
                ],
                water_model: None,
            });
            cover_pages.push(ModernLandCoverPage {
                level: 0,
                x,
                y,
                width,
                height,
                worldcover_class: vec![40; len],
            });
        }
    }
    let evidence_index = aoe_map::HydrologyEvidenceIndex {
        samples_per_axis: axis,
        page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
        policy: aoe_map::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: aoe_map::ordered_hydrology_page_root(&hydrology_pages)
            .expect("hydrology root"),
        modern_land_cover_page_root: aoe_map::ordered_modern_land_cover_page_root(&cover_pages)
            .expect("cover root"),
        water_model: None,
    };
    evidence_index
        .validate_pages(&hydrology_pages, &cover_pages)
        .expect("complete typed pages");
    let prepared = PreparedHydrology {
        samples_per_axis: axis,
        evidence_index,
        source_locks: Vec::new(),
        hydrology_pages,
        modern_land_cover_pages: cover_pages,
        river_topology: None,
    };
    assert_eq!(
        prepared
            .modern_water_override_at(axis, 64, 64)
            .expect("last page"),
        Some((0, 100))
    );
}

#[test]
fn invalid_extent_and_resolution_fail_before_source_or_cache_work() {
    let root = std::env::temp_dir().join(format!("aoe-hydrology-preflight-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let oversized = MapRequest {
        requested_side_meters: 10_000_001,
        ..MapRequest::default()
    };
    assert!(prepare_hydrology(root.clone(), oversized, 16).is_err());
    assert!(!root.exists());
    assert!(prepare_hydrology(root.clone(), MapRequest::default(), 1).is_err());
    assert!(!root.exists());
}

#[test]
fn river_evidence_is_limited_to_the_documented_western_europe_window() {
    let paris = request_bounds(MapRequest::default()).expect("Paris bounds");
    assert!(supports_hydrorivers(paris));
    let outside = MapRequest {
        center_longitude_e7: -750_000_000,
        ..MapRequest::default()
    };
    let outside = request_bounds(outside).expect("outside bounds");
    assert!(!supports_hydrorivers(outside));
}

#[test]
fn hydrology_plan_rejects_invalid_requests_before_creating_a_cache() {
    let root =
        std::env::temp_dir().join(format!("aoe-hydrology-plan-invalid-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let invalid = MapRequest {
        schema_version: 2,
        ..MapRequest::default()
    };
    assert!(matches!(
        preflight_hydrology(&root, invalid),
        Err(GeodataError::Preparation("invalid map request"))
    ));
    assert!(!root.exists());
}

#[test]
fn prepared_hydrology_fails_closed_for_bad_axis_or_missing_vector_sources() {
    let root =
        std::env::temp_dir().join(format!("aoe-hydrology-plan-prepare-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let plan = HydrologySourcePlan {
        worldcover: Vec::new(),
        sources: Vec::new(),
        rivers_available: false,
    };
    assert!(matches!(
        prepare_hydrology_with_plan(
            root.clone(),
            MapRequest::default(),
            1,
            HydrologySourcePlan {
                worldcover: Vec::new(),
                sources: Vec::new(),
                rivers_available: false,
            },
            &[],
        ),
        Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis"
        ))
    ));
    assert!(matches!(
        prepare_hydrology_with_plan(
            root.clone(),
            MapRequest {
                schema_version: 2,
                ..MapRequest::default()
            },
            2,
            HydrologySourcePlan {
                worldcover: Vec::new(),
                sources: Vec::new(),
                rivers_available: false,
            },
            &[],
        ),
        Err(GeodataError::Preparation("invalid map request"))
    ));
    assert!(matches!(
        prepare_hydrology_with_plan(root.clone(), MapRequest::default(), 2, plan, &[]),
        Err(GeodataError::Preparation(
            "HydroLAKES cache path is missing"
        ))
    ));
    let _ = std::fs::remove_dir_all(root);
}
