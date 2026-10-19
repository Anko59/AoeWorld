use super::*;
use aoe_core::{EntityId, PlayerId, WorldPosition};
use aoe_protocol::{GameplayUnitState, ResourceKind, UnitKind, UnitTask};
use aoe_rendering::SceneUnitKind;
use wasm_bindgen_test::wasm_bindgen_test;

fn villager(task: UnitTask, gathering: Option<ResourceKind>) -> GameplayUnitState {
    GameplayUnitState {
        id: EntityId(1),
        player: PlayerId(0),
        position: WorldPosition::new(0, 0),
        moving: false,
        planning: false,
        facing: 0,
        kind: UnitKind::Villager,
        task,
        carried: None,
        gathering,
        hp: 25,
        max_hp: 25,
        attack_target: None,
        last_hit: None,
    }
}

// The catalog has no farmer or builder art: a farmer uses the foraging
// (food-gathering) art, and a builder the plain villager.
#[wasm_bindgen_test]
fn farmers_use_the_foraging_art_and_builders_the_plain_villager() {
    let farmer = villager(UnitTask::Farming, Some(ResourceKind::Food));
    assert_eq!(worker_kind(&farmer), SceneUnitKind::VillagerForaging);
    let builder = villager(UnitTask::Building, None);
    assert_eq!(worker_kind(&builder), SceneUnitKind::Villager);
}
