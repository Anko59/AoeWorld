use super::buildings::building;
use crate::playground::game::village::garrison::shelter_controls;
use aoe_core::EntityId;
use aoe_protocol::{BuildingKind, ConstructionState, GarrisonUnitState, UnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn garrison_controls_require_a_completed_center_or_sheltered_villagers() {
    assert_eq!(shelter_controls(None), (false, false));
    let mut shelter = building(1);
    shelter.garrison.push(GarrisonUnitState {
        id: EntityId(2),
        kind: UnitKind::Militia,
        hp: 20,
        max_hp: 40,
        steward: None,
        vassal: None,
    });
    assert_eq!(shelter_controls(Some(&shelter)), (true, false));
    shelter.garrison[0].kind = UnitKind::Villager;
    assert_eq!(shelter_controls(Some(&shelter)), (true, true));
    shelter.kind = BuildingKind::WatchTower;
    assert_eq!(shelter_controls(Some(&shelter)), (false, true));
    shelter.kind = BuildingKind::TownCenter;
    shelter.construction = ConstructionState::UnderConstruction;
    shelter.garrison.clear();
    assert_eq!(shelter_controls(Some(&shelter)), (false, false));
}
