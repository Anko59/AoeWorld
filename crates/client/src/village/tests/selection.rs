use super::super::selection::{Selection, box_members};
use aoe_core::{EntityId, PlayerId, ScreenPoint};
use aoe_protocol::UnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn box_selects_every_owned_unit_inside_in_either_drag_direction() {
    let candidates = [
        (EntityId(1), PlayerId(0), ScreenPoint { x: 20.0, y: 30.0 }),
        (EntityId(2), PlayerId(0), ScreenPoint { x: 40.0, y: 50.0 }),
        (EntityId(3), PlayerId(1), ScreenPoint { x: 30.0, y: 40.0 }),
        (EntityId(4), PlayerId(0), ScreenPoint { x: 90.0, y: 10.0 }),
    ];
    let a = ScreenPoint { x: 10.0, y: 20.0 };
    let b = ScreenPoint { x: 50.0, y: 60.0 };
    for (start, end) in [(a, b), (b, a)] {
        assert_eq!(
            box_members(candidates, PlayerId(0), start, end),
            vec![EntityId(1), EntityId(2)]
        );
    }
}

#[wasm_bindgen_test]
fn building_and_units_are_exclusive_and_type_selection_is_visible_and_owned() {
    let mut selected = Selection::default();
    selected.units = vec![EntityId(1), EntityId(2)];
    selected.building(EntityId(7));
    assert!(selected.units.is_empty());
    assert_eq!(selected.building_id, Some(EntityId(7)));
    selected.same_kind(
        [
            (EntityId(1), PlayerId(0), UnitKind::Villager, true),
            (EntityId(2), PlayerId(0), UnitKind::Villager, false),
            (EntityId(3), PlayerId(0), UnitKind::Scout, true),
            (EntityId(4), PlayerId(1), UnitKind::Villager, true),
        ],
        PlayerId(0),
        UnitKind::Villager,
    );
    assert_eq!(selected.units, vec![EntityId(1)]);
    assert_eq!(selected.building_id, None);
}
