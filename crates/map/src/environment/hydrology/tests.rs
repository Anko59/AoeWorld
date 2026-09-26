use super::*;

fn pages_for_axis(axis: u16) -> Vec<HydrologyEvidencePage> {
    let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let count = axis.div_ceil(side);
    (0..count)
        .flat_map(|y| (0..count).map(move |x| (x, y)))
        .map(|(x, y)| {
            let width = (axis - x * side).min(side) as u8;
            let height = (axis - y * side).min(side) as u8;
            let len = usize::from(width) * usize::from(height);
            HydrologyEvidencePage {
                level: 0,
                x,
                y,
                width,
                height,
                kind: vec![HydrologyKind::Land as u8; len],
                method: vec![HydrologyEvidenceMethod::WorldCoverClass as u8; len],
                water_model: None,
            }
        })
        .collect()
}

fn pages_for_cover_axis(axis: u16) -> Vec<ModernLandCoverPage> {
    let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let count = axis.div_ceil(side);
    (0..count)
        .flat_map(|y| (0..count).map(move |x| (x, y)))
        .map(|(x, y)| {
            let width = (axis - x * side).min(side) as u8;
            let height = (axis - y * side).min(side) as u8;
            ModernLandCoverPage {
                level: 0,
                x,
                y,
                width,
                height,
                worldcover_class: vec![40; usize::from(width) * usize::from(height)],
            }
        })
        .collect()
}

#[test]
fn independent_evidence_index_is_bounded_and_worldcover_year_is_explicit() {
    let pages = pages_for_axis(65);
    let index = HydrologyEvidenceIndex {
        samples_per_axis: 65,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(&pages).expect("hydrology root"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(&pages_for_cover_axis(65))
            .expect("cover root"),
        water_model: None,
    };
    assert!(index.validate().is_ok());
    let mut invalid = index.clone();
    invalid.samples_per_axis = MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS + 1;
    assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidIndex));
    let mut invalid = index;
    invalid.world_cover_year += 1;
    assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidIndex));
}

#[test]
fn odd_source_axis_has_exact_edge_pages_and_canonical_roots() {
    let pages = pages_for_axis(65);
    assert_eq!(pages.len(), 4);
    assert_eq!(
        pages
            .iter()
            .map(|page| (page.x, page.y, page.width, page.height))
            .collect::<Vec<_>>(),
        [(0, 0, 64, 64), (1, 0, 1, 64), (0, 1, 64, 1), (1, 1, 1, 1)]
    );
    let root = ordered_hydrology_page_root(&pages).expect("root");
    let mut reversed = pages.clone();
    reversed.reverse();
    assert_eq!(root, ordered_hydrology_page_root(&reversed).expect("root"));
    reversed.push(pages[0].clone());
    assert_eq!(
        ordered_hydrology_page_root(&reversed),
        Err(EnvironmentError::InvalidPyramid)
    );
}

#[test]
fn land_cover_tile_lookup_checks_only_the_selected_class() {
    let mut page = ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 1,
        worldcover_class: vec![10, 255],
    };
    assert_eq!(page.class_at(0), Ok(10));
    assert_eq!(page.class_at(1), Err(EnvironmentError::InvalidPage));
    page.worldcover_class.pop();
    assert_eq!(page.class_at(0), Err(EnvironmentError::InvalidPage));
}

#[test]
fn invalid_kind_method_pairs_cover_classes_and_unknown_policy_are_rejected() {
    let mut page = pages_for_axis(2).remove(0);
    page.kind[0] = HydrologyKind::Lake as u8;
    page.method[0] = HydrologyEvidenceMethod::HydroRiversBufferedCorridor as u8;
    assert_eq!(page.validate(), Err(EnvironmentError::InvalidPage));
    page.method[0] = 255;
    assert_eq!(page.validate(), Err(EnvironmentError::InvalidPage));

    let mut cover = pages_for_cover_axis(2).remove(0);
    cover.worldcover_class[0] = 11;
    assert_eq!(cover.validate(), Err(EnvironmentError::InvalidPage));
    assert!(serde_json::from_str::<HydrologyWaterPolicy>("\"future_policy\"").is_err());
}

#[test]
fn geographic_water_correction_document_is_bounded_canonical_and_request_bound() {
    let request = crate::MapRequest::default();
    let polygon = vec![
        WaterCorrectionVertex {
            longitude_e7: 20_000_000,
            latitude_e7: 488_000_000,
        },
        WaterCorrectionVertex {
            longitude_e7: 24_000_000,
            latitude_e7: 488_000_000,
        },
        WaterCorrectionVertex {
            longitude_e7: 24_000_000,
            latitude_e7: 489_000_000,
        },
        WaterCorrectionVertex {
            longitude_e7: 20_000_000,
            latitude_e7: 489_000_000,
        },
    ];
    let document = WaterCorrectionDocument::new(
        request,
        128,
        vec![GeographicWaterPatch {
            id: "lake-001".to_owned(),
            precedence: 3,
            applies_from_year_ce: 500,
            applies_through_year_ce: 700,
            source_citation: "Regional survey, table 4".to_owned(),
            operation: WaterCorrectionOperation::SetNaturalLake,
            polygon: polygon.clone(),
        }],
    )
    .expect("valid correction document");
    let bytes = document.serialize().expect("serialized correction");
    let decoded = WaterCorrectionDocument::deserialize(&bytes).expect("decoded correction");
    assert_eq!(decoded, document);
    assert_eq!(
        decoded.digest().expect("digest"),
        document.digest().expect("digest")
    );
    assert!(document.validate_for(request, 128).is_ok());
    assert!(document.validate_for(request, 64).is_err());
    assert!(
        document
            .validate_for(
                crate::MapRequest {
                    seed: request.seed + 1,
                    ..request
                },
                128
            )
            .is_err()
    );

    let crossing = vec![polygon[0], polygon[2], polygon[1], polygon[3]];
    assert!(
        WaterCorrectionDocument::new(
            request,
            128,
            vec![GeographicWaterPatch {
                polygon: crossing,
                ..document.patches[0].clone()
            }]
        )
        .is_err()
    );
}

#[test]
fn modeled_water_pages_validate_categories_levels_and_flow_and_affect_roots() {
    let mut page = pages_for_axis(2).remove(0);
    page.kind[0] = HydrologyKind::Lake as u8;
    page.method[0] = HydrologyEvidenceMethod::HydroLakesExtent as u8;
    let evidence_root =
        ordered_hydrology_page_root(std::slice::from_ref(&page)).expect("evidence-only root");
    page.water_model = Some(HydrologyWaterModelPage {
        kind: vec![
            HydrologyKind::Lake as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
            HydrologyKind::Land as u8,
        ],
        surface_level_centimeters: vec![Some(950), None, None, None],
        flow_direction: vec![WaterFlowDirection::Unknown as u8; 4],
        provenance: vec![
            WaterModelProvenance::ModelledLakeSurface as u8,
            WaterModelProvenance::EvidenceOnly as u8,
            WaterModelProvenance::EvidenceOnly as u8,
            WaterModelProvenance::EvidenceOnly as u8,
        ],
    });
    page.validate().expect("valid model page");
    let model_root = ordered_hydrology_page_root(std::slice::from_ref(&page)).expect("model root");
    assert_ne!(evidence_root, model_root);

    let mut invalid = page.clone();
    invalid.water_model.as_mut().expect("model").flow_direction[1] =
        WaterFlowDirection::North as u8;
    assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidPage));

    let mut invalid = page;
    invalid
        .water_model
        .as_mut()
        .expect("model")
        .surface_level_centimeters[1] = Some(950);
    assert_eq!(invalid.validate(), Err(EnvironmentError::InvalidPage));
}
