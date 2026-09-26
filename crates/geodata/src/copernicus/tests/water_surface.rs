use super::*;

#[test]
fn detailed_water_surface_uses_acquired_dem_elevation() {
    let root = temporary_directory();
    fs::create_dir_all(&root).unwrap();
    let path = root.join("native-dem.tif");
    write_tile(&path, [1.0, 0.01, 0.0, 50.5, 0.0, -0.01], 123.45, None);
    let request = MapRequest::default();
    let mut sampler = Sampler::new(
        request,
        30_000,
        Bounds {
            min_latitude: 47,
            max_latitude: 49,
            min_longitude: 1,
            max_longitude: 3,
        },
        BTreeSet::new(),
        vec![test_tile(path, 48, 2)],
        vec![0; 128 * 128],
    )
    .unwrap();
    let hydrology_pages = vec![HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        kind: vec![HydrologyKind::Lake as u8; 4],
        method: vec![HydrologyEvidenceMethod::HydroLakesExtent as u8; 4],
        water_model: None,
    }];
    let cover = vec![ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        worldcover_class: vec![80; 4],
    }];
    let mut hydrology = crate::PreparedHydrology {
        samples_per_axis: 2,
        evidence_index: HydrologyEvidenceIndex {
            samples_per_axis: 2,
            page_samples: aoe_map::ENVIRONMENT_PAGE_SAMPLES,
            world_cover_year: aoe_map::WORLD_COVER_OBSERVATION_YEAR,
            policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: ordered_hydrology_page_root(&hydrology_pages).unwrap(),
            modern_land_cover_page_root: ordered_modern_land_cover_page_root(&cover).unwrap(),
            water_model: None,
        },
        source_locks: vec![],
        hydrology_pages,
        modern_land_cover_pages: cover,
        river_topology: None,
    };
    super::super::entry::apply_detailed_water_model(
        &mut sampler,
        &mut hydrology,
        request,
        aoe_map::WaterCorrectionDocument::empty(request, 2).unwrap(),
    )
    .unwrap();
    let model = hydrology.hydrology_pages[0].water_model.as_ref().unwrap();
    assert_eq!(model.surface_level_centimeters, vec![Some(12_345); 4]);
    assert!(
        model
            .provenance
            .iter()
            .all(|value| *value == aoe_map::WaterModelProvenance::ModelledLakeSurface as u8)
    );
    fs::remove_dir_all(root).unwrap();
}
