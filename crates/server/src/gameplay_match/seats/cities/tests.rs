use super::*;

#[test]
fn names_follow_real_start_coordinates_and_keep_local_spelling() {
    let atlas = read(
        "3\tÎle du Nord\tIle du Nord\t\t49.0\t2.0\tP\tPPL\n\
         5\tSud\tSud\t\t48.0\t2.0\tP\tPPL\n\
         2\tHill\tHill\t\t48.4\t2.0\tT\tHLL\n"
            .as_bytes(),
    )
    .unwrap();
    let request = MapRequest {
        center_latitude_e7: 485_000_000,
        center_longitude_e7: 20_000_000,
        ..MapRequest::default()
    };
    let map = MapCoordinates {
        request,
        side_meters: 130_000.0,
        side_tiles: 1000.0,
    };
    assert_eq!(
        nearest(&atlas, map.location(500.0, 110.0)),
        Some("Île du Nord")
    );
    assert_eq!(nearest(&atlas, map.location(500.0, 890.0)), Some("Sud"));
}

#[test]
fn equal_distance_prefers_the_stable_geonames_id() {
    let atlas = read(
        "9\tLater\tLater\t\t48.5\t2.0\tP\tPPL\n\
         3\tEarlier\tEarlier\t\t48.5\t2.0\tP\tPPL\n"
            .as_bytes(),
    )
    .unwrap();
    assert_eq!(
        nearest(&atlas, direction(48.5_f64.to_radians(), 2_f64.to_radians())),
        Some("Earlier")
    );
}
