use super::*;

#[test]
fn geographic_route_contract_has_five_20km_orders_and_100km_total() {
    let start = TileCoord::new(24_999, 24_999);
    let waypoints = planned_waypoints(start).expect("in-bounds route");
    assert_eq!(waypoints.len(), 5);
    assert_eq!(waypoints[0], TileCoord::new(34_999, 24_999));
    assert_eq!(waypoints[1], start);
    assert_eq!(waypoints[2], waypoints[0]);
    assert_eq!(waypoints[3], start);
    assert_eq!(waypoints[4], waypoints[0]);
    let distance = std::iter::once(start)
        .chain(waypoints.iter().copied())
        .zip(waypoints.iter().copied())
        .map(|(from, to)| {
            f64::from(from.x.abs_diff(to.x) + from.y.abs_diff(to.y)) * METERS_PER_TILE
        })
        .sum::<f64>();
    assert_eq!(distance, REQUIRED_DISTANCE_METERS);
}

#[test]
fn geographic_route_contract_rejects_waypoints_outside_map() {
    assert!(matches!(
        planned_waypoints(TileCoord::new(49_999, 49_999)),
        Err(SourceQualificationError::GeographicWaypointOutsideMap { .. })
    ));
}
