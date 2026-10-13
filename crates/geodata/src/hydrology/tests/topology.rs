use super::*;

#[test]
fn endpoint_distance_handles_extreme_projected_coordinates_without_overflow() {
    let reach = RiverReachMetadata {
        next_down_id: 0,
        distance_to_sink_centimeters: 0,
        start_x_centimeters: i32::MAX,
        start_y_centimeters: i32::MAX,
        end_x_centimeters: i32::MAX,
        end_y_centimeters: i32::MIN,
        length_centimeters: 0,
    };
    let delta = i128::from(i32::MAX) - i128::from(i32::MIN);
    assert_eq!(
        endpoint_distance_squared(i32::MIN, i32::MIN, &reach),
        delta * delta
    );
}
