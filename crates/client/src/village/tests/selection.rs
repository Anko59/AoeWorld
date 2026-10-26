use super::super::selection::{Selection, box_members};
use aoe_core::{EntityId, PlayerId, ScreenPoint};
use aoe_protocol::UnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn idle_finder_updates_jobs_removals_and_cycles_in_id_order() {
    use super::super::selection::IdleFinder;
    use aoe_protocol::UnitTask;
    let mut finder = IdleFinder::default();
    let mut unit = super::workers::villager(UnitTask::Idle);
    for id in [7, 2, 4] {
        unit.id = EntityId(id);
        finder.update(&unit, true);
    }
    assert_eq!(finder.len(), 3);
    for id in [2, 4, 7, 2] {
        assert_eq!(finder.next(), Some(EntityId(id)));
    }
    finder.clear();
    for id in [7, 2, 4] {
        unit.id = EntityId(id);
        finder.update(&unit, true);
    }
    assert_eq!(
        finder.next(),
        Some(EntityId(4)),
        "Camera snapshots keep the cursor"
    );
    unit.id = EntityId(4);
    unit.moving = true;
    finder.update(&unit, true);
    unit.id = EntityId(7);
    finder.remove(unit.id);
    assert_eq!(finder.next(), Some(EntityId(2)));
    unit.id = EntityId(2);
    // An owner change or an ineligible kind removes it from the index.
    finder.update(&unit, false);
    assert_eq!(finder.len(), 0);
    assert_eq!(finder.next(), None);
    unit.moving = false;
    for task in [
        UnitTask::Building,
        UnitTask::Gathering,
        UnitTask::Returning,
        UnitTask::Dying,
    ] {
        unit.task = task;
        finder.update(&unit, true);
        assert_eq!(finder.len(), 0, "{task:?}");
    }
    unit.task = UnitTask::Carrying;
    finder.update(&unit, true);
    assert_eq!(finder.len(), 1, "An idle worker can still carry resources");
    unit.auto_scout = true;
    finder.update(&unit, true);
    assert_eq!(finder.len(), 0, "Auto-scout waiting for a route is working");
}

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

#[wasm_bindgen_test]
fn resources_are_exclusive_with_units_and_buildings() {
    let mut selected = Selection::default();
    selected.building(EntityId(1));
    selected.resource(42);
    assert!(selected.units.is_empty());
    assert_eq!(selected.building_id, None);
    assert_eq!(selected.resource_id, Some(42));
    selected.units(vec![EntityId(2)]);
    assert_eq!(selected.resource_id, None);
}

#[wasm_bindgen_test]
fn building_group_selection_is_owned_on_screen_and_exclusive_with_units_and_resources() {
    use aoe_protocol::BuildingKind;
    let mut selected = Selection::default();
    selected.same_buildings(
        [
            (EntityId(1), PlayerId(0), BuildingKind::Barracks, true),
            (EntityId(2), PlayerId(0), BuildingKind::Barracks, true),
            (EntityId(3), PlayerId(1), BuildingKind::Barracks, true),
            (EntityId(4), PlayerId(0), BuildingKind::Barracks, false),
            (EntityId(5), PlayerId(0), BuildingKind::Stable, true),
        ],
        PlayerId(0),
        BuildingKind::Barracks,
        EntityId(2),
    );
    assert_eq!(selected.buildings, [EntityId(1), EntityId(2)]);
    assert_eq!(selected.building_id, Some(EntityId(2)));
    assert!(selected.units.is_empty());
    selected.units(vec![EntityId(9)]);
    assert!(selected.buildings.is_empty());
    selected.building(EntityId(8));
    assert_eq!(selected.buildings, [EntityId(8)]);
    selected.resource(42);
    assert!(selected.buildings.is_empty());
}
