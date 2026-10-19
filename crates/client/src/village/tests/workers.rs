use crate::playground::game::village::hud::counts_as_gatherer;
use aoe_protocol::UnitTask;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn farmers_count_as_food_gatherers_and_idle_or_building_villagers_do_not() {
    for task in [UnitTask::Gathering, UnitTask::Returning, UnitTask::Farming] {
        assert!(counts_as_gatherer(task), "{task:?}");
    }
    for task in [
        UnitTask::Idle,
        UnitTask::Moving,
        UnitTask::Carrying,
        UnitTask::Building,
    ] {
        assert!(!counts_as_gatherer(task), "{task:?}");
    }
}
