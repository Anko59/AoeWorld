use super::super::navy::boarding_cursor;
use aoe_protocol::UnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn board_cursor_requires_land_passengers_and_a_living_allied_transport_with_room() {
    assert_eq!(
        boarding_cursor(UnitKind::TransportShip, true, true, 0, true),
        Some(12.0)
    );
    assert_eq!(
        boarding_cursor(UnitKind::TransportShip, true, true, 5, true),
        Some(3.0)
    );
    assert_eq!(
        boarding_cursor(UnitKind::TransportShip, false, true, 0, true),
        None
    );
    assert_eq!(
        boarding_cursor(UnitKind::TransportShip, true, false, 0, true),
        None
    );
    assert_eq!(
        boarding_cursor(UnitKind::TransportShip, true, true, 0, false),
        None
    );
    for kind in [UnitKind::Villager, UnitKind::Galley, UnitKind::FishingShip] {
        assert_eq!(boarding_cursor(kind, true, true, 0, true), None);
    }
}
