use aoe_core::{EntityId, PlayerId, Seed};
use aoe_simulation::{GameWorld, UnitKind};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn cancellation_refunds_food_releases_population_and_checks_owner() {
    let (mut world, _) = GameWorld::default_with_village(Seed(1));
    let center = world.buildings().next().unwrap().id;
    let food = world.player_economy(PlayerId(0)).unwrap().stockpile.food;
    world.issue_train(PlayerId(0), center, UnitKind::Villager).unwrap();
    assert!(world.issue_cancel_training(PlayerId(1), center).is_err());
    assert!(world.issue_cancel_training(PlayerId(0), EntityId(u32::MAX)).is_err());
    world.issue_cancel_training(PlayerId(0), center).unwrap();
    assert!(world.building(center).unwrap().training_queue.is_empty());
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().stockpile.food, food);
    world.issue_train(PlayerId(0), center, UnitKind::Villager).unwrap();
}
