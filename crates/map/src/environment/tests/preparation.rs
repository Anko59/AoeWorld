use super::*;

#[test]
fn shared_worker_options_keep_their_wire_shape() {
    assert_eq!(
        serde_json::to_value(OverviewFieldAxes::LANDSCAPE).unwrap(),
        serde_json::json!({"elevation": 1024, "vegetation": 1024, "water": 1024, "historical": 1024})
    );
    assert_eq!(
        serde_json::to_value(OverviewHydrologyMode::Vectors).unwrap(),
        "vectors"
    );
    assert!(OverviewHydrologyMode::default().is_none());
}

#[test]
fn landscape_axes_sample_every_field_at_the_elevation_axis() {
    let axes = OverviewFieldAxes::LANDSCAPE;
    assert_eq!(axes.water, axes.elevation, "coasts coarser than relief");
    assert_eq!(
        axes.vegetation, axes.elevation,
        "stands coarser than relief"
    );
    assert_eq!(axes.historical, axes.elevation);
}
