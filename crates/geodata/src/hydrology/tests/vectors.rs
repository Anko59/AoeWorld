use super::*;

#[test]
fn france_footprint_uses_projected_cells_without_legacy_degree_inflation() {
    let request = MapRequest {
        requested_side_meters: 1_200_000,
        detail_profile: aoe_map::DetailProfile::LandscapeV2,
        ..MapRequest::default()
    };
    let cancelled = AtomicBool::new(false);
    for axis in [2, 15, 16, 65] {
        assert!(validate_vector_footprint(request, axis, &cancelled).is_ok());
    }
    assert!(!supports_hydrorivers(
        request_bounds(request).expect("legacy bounds")
    ));
    for request in [
        MapRequest {
            center_latitude_e7: 360_000_000,
            ..request
        },
        MapRequest {
            center_latitude_e7: 600_000_000,
            ..request
        },
        MapRequest {
            center_longitude_e7: -120_000_000,
            ..request
        },
        MapRequest {
            center_longitude_e7: 250_000_000,
            ..request
        },
    ] {
        assert!(matches!(
            validate_vector_footprint(request, 16, &cancelled),
            Err(GeodataError::Preparation(
                "vector hydrology footprint is outside the western Europe pilot"
            ))
        ));
    }
}

#[test]
fn pinned_vector_selection_has_no_worldcover_or_catalog_inputs() {
    let sources = hydrology_vector_sources();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0].id, "hydrolakes-v1.0-global-gdb");
    assert_eq!(sources[1].id, "hydrorivers-v1.0-eu-shp");
    assert!(
        sources
            .iter()
            .all(|source| !source.id.contains("worldcover"))
    );
    assert!(
        sources
            .iter()
            .all(|source| matches!(source.expected_checksum, crate::ExpectedChecksum::Sha256(_)))
    );
    assert!(
        sources.iter().map(|source| source.bytes).sum::<u64>()
            <= crate::MAX_HYDROLOGY_DOWNLOAD_BYTES
    );
}

#[test]
fn overview_dem_context_accepts_full_128_and_1024_pyramids_without_relaxing_grid_validation() {
    for axis in [128_u16, 1024] {
        let mut pages = Vec::new();
        for y in 0..axis / PAGE {
            for x in 0..axis / PAGE {
                pages.push(aoe_map::ElevationPage {
                    level: 0,
                    x,
                    y,
                    width: PAGE as u8,
                    height: PAGE as u8,
                    geographic_height_centimeters: vec![4200; usize::from(PAGE).pow(2)],
                });
            }
        }
        let level_zero = pages.clone();
        pages.push(aoe_map::ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![4200],
        });
        assert!(water_model::ElevationGrid::new(&pages).is_err());
        assert_eq!(
            super::super::overview_elevation_context(&pages).unwrap(),
            level_zero
        );
        pages.push(pages[0].clone());
        assert!(super::super::overview_elevation_context(&pages).is_err());
    }
    assert!(super::super::overview_elevation_context(&[]).is_err());
}

#[test]
fn vector_api_cancel_and_invalid_axis_fail_before_context_cache_or_network() {
    let root = std::env::temp_dir().join(format!("aoe-vector-no-cache-{}", std::process::id()));
    assert!(!root.exists(), "test root must not already exist");
    let request = MapRequest::default();
    let corrections = WaterCorrectionDocument::empty(request, 2).expect("corrections");
    assert!(matches!(
        PreparedHydrology::prepare_vectors(
            root.clone(),
            request,
            2,
            &[],
            &[],
            corrections.clone(),
            &AtomicBool::new(true),
        ),
        Err(GeodataError::Cache(crate::CacheError::Cancelled))
    ));
    assert!(!root.exists());
    assert!(matches!(
        prepare_hydrology_vectors(
            root.clone(),
            request,
            1,
            &[],
            &[],
            corrections.clone(),
            &AtomicBool::new(false),
        ),
        Err(GeodataError::Preparation(
            "hydrology preparation supports 2 through 1024 samples per axis"
        ))
    ));
    assert!(
        prepare_hydrology_vectors(
            root.clone(),
            MapRequest {
                schema_version: 2,
                ..request
            },
            2,
            &[],
            &[],
            corrections,
            &AtomicBool::new(false),
        )
        .is_err()
    );
    assert!(!root.exists());
}
