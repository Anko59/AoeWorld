use super::*;

#[test]
fn typed_evidence_pages_evict_and_reload_with_bounded_residency() {
    let directory = tempfile::tempdir().expect("package directory");
    let (base, elevation, water, vegetation, land_use) = prepared();
    let axis = 576_u16;
    let page_count = axis / u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let mut hydrology = Vec::new();
    let mut land_cover = Vec::new();
    for y in 0..page_count {
        for x in 0..page_count {
            let len = usize::from(ENVIRONMENT_PAGE_SAMPLES).pow(2);
            hydrology.push(HydrologyEvidencePage {
                level: 0,
                x,
                y,
                width: ENVIRONMENT_PAGE_SAMPLES,
                height: ENVIRONMENT_PAGE_SAMPLES,
                kind: vec![HydrologyKind::Land as u8; len],
                method: vec![HydrologyEvidenceMethod::WorldCoverClass as u8; len],
                water_model: None,
            });
            land_cover.push(ModernLandCoverPage {
                level: 0,
                x,
                y,
                width: ENVIRONMENT_PAGE_SAMPLES,
                height: ENVIRONMENT_PAGE_SAMPLES,
                worldcover_class: vec![40; len],
            });
        }
    }
    let mut environment = base.environment.clone();
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: axis,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: ordered_hydrology_page_root(&hydrology).expect("hydrology root"),
        modern_land_cover_page_root: ordered_modern_land_cover_page_root(&land_cover)
            .expect("land-cover root"),
        water_model: None,
    });
    let package = MapPackage::with_prepared_environment(
        base.generator_version,
        base.request,
        base.source_locks.clone(),
        base.projection.clone(),
        base.provenance.clone(),
        environment,
    )
    .expect("typed package");
    let page_root = directory
        .path()
        .join("pages")
        .join(package.content_hash_hex());
    let hydrology_root = page_root.join("hydrology-evidence");
    let land_cover_root = page_root.join("modern-land-cover");
    fs::create_dir_all(&hydrology_root).expect("hydrology directory");
    fs::create_dir_all(&land_cover_root).expect("land-cover directory");
    for page in &hydrology {
        fs::write(
            hydrology_root.join(format!("0-{}-{}.json", page.x, page.y)),
            serde_json::to_vec(page).expect("hydrology JSON"),
        )
        .expect("write hydrology page");
    }
    for page in &land_cover {
        fs::write(
            land_cover_root.join(format!("0-{}-{}.json", page.x, page.y)),
            serde_json::to_vec(page).expect("land-cover JSON"),
        )
        .expect("write land-cover page");
    }
    persist_prepared(
        Some(directory.path()),
        &package,
        &elevation,
        &water,
        &vegetation,
        &land_use,
    )
    .expect("publish typed package");

    let residency =
        PageResidency::open(directory.path(), &package, &|| false).expect("verify typed residency");
    let first_key = EnvironmentPageKey {
        layer: PageLayer::HydrologyEvidence,
        level: 0,
        x: 0,
        y: 0,
    };
    let original = residency
        .page(first_key, &|| false)
        .expect("first evidence page");
    let original_bytes = fs::read(hydrology_root.join("0-0-0.json")).expect("page bytes");
    for (layer, count) in [
        (PageLayer::HydrologyEvidence, hydrology.len()),
        (PageLayer::ModernLandCover, land_cover.len()),
    ] {
        for index in 0..count {
            if layer == PageLayer::HydrologyEvidence && index == 0 {
                continue;
            }
            let x = (index % usize::from(page_count)) as u16;
            let y = (index / usize::from(page_count)) as u16;
            residency
                .page(
                    EnvironmentPageKey {
                        layer,
                        level: 0,
                        x,
                        y,
                    },
                    &|| false,
                )
                .expect("load indexed evidence page");
        }
    }
    assert_eq!(
        residency.resident_pages(),
        crate::map_store::residency::MAX_RESIDENT_ENVIRONMENT_PAGES
    );
    fs::remove_file(hydrology_root.join("0-0-0.json")).expect("remove evicted page");
    assert_eq!(
        residency.page(first_key, &|| false),
        Err(EnvironmentPageError::Missing)
    );
    fs::write(hydrology_root.join("0-0-0.json"), original_bytes).expect("restore evicted page");
    assert_eq!(
        *residency.page(first_key, &|| false).expect("reload page"),
        *original
    );
    assert_eq!(
        residency.resident_pages(),
        crate::map_store::residency::MAX_RESIDENT_ENVIRONMENT_PAGES
    );
}
