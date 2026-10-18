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

#[test]
fn elevation_tile_locks_do_not_count_toward_the_overview_stack() {
    let lock = |id: &str| crate::SourceLock {
        id: id.to_owned(),
        provider: String::new(),
        release: String::new(),
        url: String::new(),
        sha256: [0; 32],
        acquired_at: String::new(),
        native_resolution: String::new(),
        crs: String::new(),
        vertical_datum: String::new(),
        license: String::new(),
        preprocessing_version: String::new(),
    };
    let locks = [
        lock("etopo-2022-v1-60s-surface"),
        lock("copernicus-dem-3 arc-seconds-Copernicus_DSM_COG_30_N48_00_E002_00_DEM"),
        lock("copernicus-dem-3 arc-seconds-Copernicus_DSM_COG_30_N48_00_E003_00_DEM"),
        lock("hydrorivers-v1.0-eu-shp"),
    ];
    assert_eq!(overview_base_lock_count(&locks), 2);
}
