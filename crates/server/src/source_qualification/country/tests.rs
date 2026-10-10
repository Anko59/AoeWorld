use super::*;

#[test]
fn bounded_start_outcomes_remain_distinct_and_unqualified() {
    for (outcome, label) in [
        (StartSearchResult::Unavailable, "unavailable"),
        (StartSearchResult::LimitReached, "limit_reached"),
        (StartSearchResult::Cancelled, "cancelled"),
    ] {
        assert_eq!(start_result(outcome), (label, None));
    }
    assert_eq!(
        start_result(StartSearchResult::Found(TileCoord::new(12, 34))),
        ("found", Some([12, 34]))
    );
    let report = SourceCountryProbe {
        policy: "test-only",
        content_hash: "test-only".into(),
        source_lock_count: 0,
        indexed_pages: 0,
        tiles_per_side: 20_000,
        typed_hydrology: false,
        ordinary_start: "limit_reached",
        start: None,
        routes: Vec::new(),
        live_activation: false,
        hardware_qualified: false,
    };
    let value = serde_json::to_value(report).expect("diagnostic report");
    assert_eq!(value["live_activation"], false);
    assert_eq!(value["hardware_qualified"], false);
    assert!(value["start"].is_null());
    assert_eq!(value["routes"], serde_json::json!([]));
}

#[test]
fn procedural_package_cannot_be_country_source_evidence() {
    let request = aoe_map::MapRequest {
        requested_side_meters: 1_200_000,
        compression: aoe_map::Ratio::new(30, 1).expect("compression"),
        ..aoe_map::MapRequest::default()
    };
    let package = MapPackage::new(aoe_map::GENERATION_RECIPE_VERSION, request, Vec::new())
        .expect("procedural package");
    assert!(!supported_country(&package));
}
