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
