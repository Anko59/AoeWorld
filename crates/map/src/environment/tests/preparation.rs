use super::*;

#[test]
fn shared_worker_options_keep_their_wire_shape() {
    assert_eq!(
        serde_json::to_value(OverviewFieldAxes::LANDSCAPE).unwrap(),
        serde_json::json!({"elevation": 1024, "vegetation": 128, "water": 128, "historical": 1024})
    );
    assert_eq!(
        serde_json::to_value(OverviewHydrologyMode::Vectors).unwrap(),
        "vectors"
    );
    assert!(OverviewHydrologyMode::default().is_none());
}
