//! The management cases of `ai/tests/scaling/test_kingdom.py`: the kingdom
//! grows with its living settlements, and its managers are paid and seated.
use aoe_sdk::{BuildingKind, Order, UnitKind};

use super::{Sent, kingdom, seated};
use crate::brain::Brain;
use crate::config::Config;
use crate::play::buildorder::{self, Stage};
use crate::play::management::ManagementManager;
use crate::roster::price;
use crate::tests::fake::{Recorder, enemy, foreign_building, unit_of};

fn town_center_builds(tape: &super::Tape) -> Vec<(BuildingKind, (i32, i32))> {
    tape.orders()
        .builds()
        .into_iter()
        .filter(|(kind, _)| *kind == BuildingKind::TownCenter)
        .collect()
}

#[test]
fn test_the_next_settlement_is_wanted_even_after_twelve_town_centers() {
    let mut realm = kingdom(600, 12);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    assert!(!town_center_builds(&tape).is_empty());
}

#[test]
fn test_a_settlement_is_placed_outside_existing_town_center_neighborhoods() {
    let mut realm = kingdom(180, 4);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    let centers: Vec<_> = ctx
        .buildings
        .iter()
        .filter(|b| b.kind == BuildingKind::TownCenter)
        .map(|b| b.center())
        .collect();
    let placed = town_center_builds(&tape);
    assert!(!placed.is_empty());
    let (_, (x, y)) = placed[0];
    let nearest = centers
        .iter()
        .map(|(cx, cy)| (x - cx).abs().max((y - cy).abs()))
        .min()
        .expect("town centers");
    assert!(nearest >= 24, "{nearest}");
}

#[test]
fn test_the_last_town_center_is_rebuilt_with_a_small_surviving_workforce() {
    let mut realm = kingdom(3, 1);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ctx.buildings.retain(|b| b.kind != BuildingKind::TownCenter);
    ctx.center = None;
    ManagementManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
    assert!(!town_center_builds(&tape).is_empty());
}

#[test]
fn test_default_management_uses_a_steward_and_builds_a_castle() {
    let mut realm = kingdom(40, 1);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
    let orders = tape.orders();
    assert!(
        orders
            .trains()
            .iter()
            .any(|(_, kind)| *kind == UnitKind::Steward)
    );
    assert!(
        orders
            .builds()
            .iter()
            .any(|(kind, _)| *kind == BuildingKind::Castle)
    );
}

#[test]
fn test_a_seated_steward_gets_its_training_switch_turned_on_without_order_spam() {
    let mut realm = kingdom(40, 1);
    realm.world.buildings[0].garrison = vec![(900, UnitKind::Steward, 50, 50)];
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    let mut manager = ManagementManager::default();
    manager.step(&mut ctx).expect("the order budget holds");
    let first = tape.orders().steward_settings();
    assert!(!first.is_empty());
    assert!(first[0].3, "training is switched on");
    ctx.tick += 20;
    manager.step(&mut ctx).expect("the order budget holds");
    assert_eq!(tape.orders().steward_settings().len(), first.len());
}

#[test]
fn test_management_training_pauses_while_the_town_center_drains_for_research() {
    let mut realm = kingdom(40, 1);
    realm.world.buildings[0].garrison = vec![(900, UnitKind::Steward, 50, 50)];
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ctx.hold_town_center = true;
    ManagementManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
    assert!(tape.orders().steward_settings().iter().any(|s| !s.3));
}

#[test]
fn test_large_armies_keep_commands_bounded_and_leave_the_economy_a_turn() {
    let mut realm = kingdom(60, 2);
    realm
        .economy()
        .researched
        .extend([aoe_sdk::Tech::Crossbowman, aoe_sdk::Tech::Pikeman]);
    realm
        .world
        .units
        .extend((0..1000).map(|i| unit_of(3000 + i, UnitKind::Spearman, (60, 60))));
    realm.world.enemies = vec![enemy(9000, UnitKind::Militia, (65, 65))];
    let mut seat = Recorder::default();
    Brain::new(Config::default()).decide(&realm.world, &[], &mut seat);
    assert!(
        seat.orders.len() <= 40,
        "two decisions/s must stay below the 100 orders/s server budget: {}",
        seat.orders.len()
    );
    assert!(
        seat.orders.iter().any(|o| matches!(
            o,
            Order::Train { .. } | Order::Gather { .. } | Order::Build { .. }
        )),
        "combat cannot starve development"
    );
}

#[test]
fn test_a_vassal_is_enabled_and_given_workers_for_ordinary_fortification() {
    let mut realm = kingdom(60, 1);
    realm.world.enemy_buildings = vec![foreign_building(999, BuildingKind::Barracks, (130, 80))];
    realm.push_building(seated(
        950,
        BuildingKind::Castle,
        (65, 65),
        (951, UnitKind::Vassal),
    ));
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .step(&mut ctx)
        .expect("the order budget holds");
    let orders = tape.orders();
    assert!(orders.vassal_development().contains(&(951, true)));
    let reserved = ctx
        .board
        .vassal_workers
        .get(&950)
        .cloned()
        .unwrap_or_default();
    assert!(!reserved.is_empty());
    assert!(
        orders
            .gathers()
            .iter()
            .all(|(unit, _)| !reserved.contains(unit))
    );
}

#[test]
fn test_the_second_town_center_is_requested_at_thirty_villagers() {
    let mut realm = kingdom(30, 1);
    let tape = realm.tape();
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    assert!(!town_center_builds(&tape).is_empty());
}

#[test]
fn test_town_center_saving_leaves_wood_for_essential_work() {
    let mut realm = kingdom(40, 1);
    realm.economy().wood = 100;
    let mut ctx = realm.ctx();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    assert!(
        ctx.can_afford([0, 100, 0, 0]),
        "a remote woodcutter still needs a camp"
    );
    assert!(!ctx.can_afford([0, 275, 0, 100]));
}

#[test]
fn test_an_established_army_waits_for_the_next_town_center_price() {
    let mut realm = kingdom(60, 2);
    realm.economy().wood = 100;
    let mut ctx = realm.ctx();
    ctx.army = (0..ctx.config.imperial_army_min)
        .map(|i| unit_of(3000 + i, UnitKind::Spearman, (60, 60)))
        .collect();
    ManagementManager::default()
        .expand(&mut ctx)
        .expect("the order budget holds");
    assert!(
        ctx.saving_castle,
        "ongoing army spending must not consume the expansion float"
    );
}

#[test]
fn test_saving_the_first_steward_can_begin_before_imperial_buildings_stand() {
    let mut realm = kingdom(50, 1);
    {
        let eco = realm.economy();
        eco.food = 800;
        eco.gold = 600;
    }
    realm.board.stats.insert(
        "army_peak".to_owned(),
        u64::from(realm.config.imperial_army_min),
    );
    let mut ctx = realm.ctx();
    assert!(buildorder::wants_imperial_food(&ctx));
    assert_eq!(buildorder::stage(&ctx), Stage::Castle);
    ManagementManager::default().save(&mut ctx, UnitKind::Steward, price(UnitKind::Steward));
    assert_eq!(ctx.spent[2], 600);
    assert_eq!(ctx.spent[0], 800);
}
