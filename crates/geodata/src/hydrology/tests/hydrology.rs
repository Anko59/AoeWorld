use super::*;

fn prepared(kinds: Vec<u8>) -> PreparedHydrology {
    PreparedHydrology {
        samples_per_axis: 2,
        source_year: 2021,
        source_locks: Vec::new(),
        hydrology_pages: vec![HydrologyPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            kind: kinds,
            surface_height_centimeters: vec![0; 4],
            surface_height_known: vec![0],
            barrier_edges: vec![0; 4],
        }],
        modern_land_cover_pages: Vec::new(),
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
