use super::super::channels::segments;
use super::*;

#[test]
fn channel_width_uses_discharge_and_scale_and_short_reaches_have_a_ford() {
    let directory = TestDirectory::new();
    let mut rivers = river_dataset(&directory);
    let definition =
        crate::local_aeqd_definition(request().center_latitude_e7, request().center_longitude_e7);
    let mut features =
        vector_features(&mut rivers, (-180.0, -90.0, 180.0, 90.0), &definition, true)
            .expect("source lines");
    let bounds = [-15000.0, -15000.0, 15000.0, 15000.0];
    let ordinary = segments(&features, 30000.0, 500, bounds).expect("ordinary scale");
    assert!(
        ordinary
            .iter()
            .all(|s| (8 * 256..9 * 256).contains(&s.width_units))
    );
    let compressed = segments(&features, 30000.0, 50, bounds).expect("compressed scale");
    assert!(compressed.iter().all(|s| s.width_units == 3 * 256));
    for feature in &mut features {
        let reach = feature.river_reach.as_mut().expect("reach");
        reach.discharge = 3600.0;
    }
    let large = segments(&features, 30000.0, 500, bounds).expect("large river");
    assert!(large.iter().all(|s| s.width_units == 10 * 256));
    for feature in &mut features {
        let reach = feature.river_reach.as_mut().expect("reach");
        reach.line_points = vec![(-30.0, 0.0), (30.0, 0.0)];
    }
    let short = segments(&features, 30000.0, 500, bounds).expect("inter-lake reach");
    assert!(!short.is_empty());
    assert!(short.iter().all(|s| {
        s.sample([250 * 256, 250 * 256], 0)
            .is_some_and(|(_, ford)| ford)
    }));
}

#[test]
fn wide_corridor_clipping_cell_corner_keeps_reach_projection() {
    let directory = TestDirectory::new();
    let mut rivers = river_dataset(&directory);
    let definition =
        crate::local_aeqd_definition(request().center_latitude_e7, request().center_longitude_e7);
    let mut features =
        vector_features(&mut rivers, (-180.0, -90.0, 180.0, 90.0), &definition, true)
            .expect("lines");
    features.truncate(1);
    let reach = features[0].river_reach.as_mut().expect("reach");
    reach.discharge = 3600.0;
    let offset = 1500.0 / 2.0_f64.sqrt();
    reach.line_points = vec![(offset, offset), (offset + 1000.0, offset - 1000.0)];
    let hit = super::super::topology::nearest_river_reach(&features, 0.0, 0.0, 1875.0);
    assert!(
        hit.is_some(),
        "wide corridor intersects the northeast cell corner"
    );
    assert!(
        hit.unwrap().1.abs() < 0.001,
        "projection retains the nearest source station"
    );
    assert!(
        super::super::topology::nearest_river_reach(&features, -1000.0, -1000.0, 1875.0).is_none()
    );
}

#[test]
fn written_segments_are_split_at_page_half_width_margin() {
    let directory = TestDirectory::new();
    let mut rivers = river_dataset(&directory);
    let definition =
        crate::local_aeqd_definition(request().center_latitude_e7, request().center_longitude_e7);
    let mut features =
        vector_features(&mut rivers, (-180.0, -90.0, 180.0, 90.0), &definition, true)
            .expect("lines");
    features.truncate(1);
    features[0].river_reach.as_mut().expect("reach").line_points =
        vec![(-10000.0, 0.0), (10000.0, 0.0)];
    let lines = segments(&features, 30000.0, 500, [-1000.0, -1000.0, 1000.0, 1000.0])
        .expect("clipped lines");
    assert_eq!(lines.len(), 1);
    let lower = ((14000.0 / 60.0) * 256.0_f64).round() as i32 - 5 * 256;
    let upper = ((16000.0 / 60.0) * 256.0_f64).round() as i32 + 5 * 256;
    assert_eq!(lines[0].start[0], lower);
    assert_eq!(lines[0].end[0], upper);
    assert!(lines[0].station_start > lines[0].station_end);
    let encoded = serde_json::to_value(lines[0]).expect("segment");
    assert_eq!(
        encoded[7], 85334,
        "unclipped source length survives page clipping"
    );
}
