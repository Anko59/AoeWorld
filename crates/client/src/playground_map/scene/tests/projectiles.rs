use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn unpack(packed: u8) -> (u8, u8) {
    (packed & 7, packed >> 3)
}

#[wasm_bindgen_test]
fn arrows_face_the_screen_direction_of_their_ground_path() {
    // A tile step along +x then -y reads as screen east; (dx - dy, dx + dy).
    let east = flight_facing([10.0, 10.0], [14.0, 6.0]);
    assert_eq!(
        unpack(east).0,
        2,
        "mirrored west row for an eastward flight"
    );
    let west = flight_facing([10.0, 10.0], [6.0, 14.0]);
    assert_eq!(unpack(west).0, 6);
    let south = flight_facing([10.0, 10.0], [14.0, 14.0]);
    assert_eq!(unpack(south).0, 0);
    let north = flight_facing([10.0, 10.0], [6.0, 6.0]);
    assert_eq!(unpack(north).0, 4);
    // A flight along the tile axis points between the cardinals.
    let along_x = flight_facing([10.0, 10.0], [14.0, 10.0]);
    assert_eq!(unpack(along_x).0, 1, "south-east, mirrored");
    let along_y = flight_facing([10.0, 10.0], [10.0, 14.0]);
    assert_eq!(unpack(along_y).0, 7, "south-west");
}

#[wasm_bindgen_test]
fn a_zero_length_flight_is_drawn_level_facing_south() {
    let packed = flight_facing([3.0, 3.0], [3.0, 3.0]);
    assert_eq!(unpack(packed).0, 0);
}
