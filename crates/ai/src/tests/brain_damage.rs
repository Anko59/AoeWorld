//! `Brain::note_damage`: a blow of ours landing (`ai/tests/test_review.py`).
use aoe_sdk::{BuildingKind, UnitKind};

use super::fake::{FakeWorld, base, enemy, foreign_building, soldier, with_context};
use crate::blackboard::Blackboard;
use crate::brain;
use crate::config::Config;
use crate::view::{Building, Unit};

fn armed(count: u32, tile: (i32, i32)) -> Vec<Unit> {
    (0..count).map(|i| soldier(300 + i, tile)).collect()
}

fn world(foe_buildings: Vec<Building>, foes: Vec<Unit>) -> FakeWorld {
    let (mut units, buildings) = base(10);
    units.extend(armed(12, (60, 60)));
    let mut world = FakeWorld::new(units, buildings);
    world.enemy_buildings = foe_buildings;
    world.enemies = foes;
    world
}

fn stat(board: &Blackboard, name: &str) -> Option<i64> {
    board
        .stats
        .get(name)
        .map(|v| i64::try_from(*v).unwrap_or(i64::MAX))
}

/// One decision's `note_damage` over `world`, with the board carried across.
fn observe(world: &FakeWorld, board: &mut Blackboard, attacking: bool) {
    with_context(world, &[], board, &Config::default(), |ctx| {
        ctx.board.attacking = attacking;
        brain::note_damage(ctx);
    });
}

#[test]
fn a_foundation_rising_in_health_is_not_a_landed_blow() {
    let mut foundation = foreign_building(901, BuildingKind::House, (64, 64));
    foundation.complete = false;
    foundation.hp = 100;
    let mut board = Blackboard::default();
    observe(&world(vec![foundation.clone()], vec![]), &mut board, true);
    foundation.hp = 400;
    observe(&world(vec![foundation], vec![]), &mut board, true);
    assert!(board.wounded.is_empty());
    assert_eq!(stat(&board, "attack_hits"), None);
    assert_eq!(
        board.enemy_hp.get(&901).copied(),
        Some(400),
        "the new health is remembered"
    );
}

#[test]
fn a_blow_counts_only_when_health_falls_with_our_soldiers_near() {
    let mut target = foreign_building(901, BuildingKind::Barracks, (64, 64));
    let mut board = Blackboard::default();
    observe(&world(vec![target.clone()], vec![]), &mut board, true);
    assert_eq!(
        stat(&board, "attack_hits"),
        None,
        "the first sight is a baseline, not a blow"
    );
    target.hp = 900;
    observe(&world(vec![target], vec![]), &mut board, true);
    assert_eq!(stat(&board, "attack_hits"), Some(1));
    assert_eq!(stat(&board, "enemies_wounded"), Some(1));
    assert!(board.wounded.contains(&901));

    let mut far = foreign_building(902, BuildingKind::Barracks, (250, 250));
    let mut board = Blackboard::default();
    observe(&world(vec![far.clone()], vec![]), &mut board, true);
    far.hp = 900;
    observe(&world(vec![far], vec![]), &mut board, true);
    assert_eq!(stat(&board, "attack_hits"), None);
    assert!(board.wounded.is_empty());
}

#[test]
fn a_wound_is_not_an_attack_hit_unless_an_attack_is_under_way() {
    let mut target = foreign_building(901, BuildingKind::Barracks, (64, 64));
    let mut board = Blackboard::default();
    observe(&world(vec![target.clone()], vec![]), &mut board, false);
    target.hp = 900;
    observe(&world(vec![target], vec![]), &mut board, false);
    assert_eq!(stat(&board, "enemies_wounded"), Some(1));
    assert_eq!(stat(&board, "attack_hits"), None);
}

#[test]
fn a_wounded_enemy_unit_counts_like_a_building() {
    let mut raider = enemy(800, UnitKind::Spearman, (62, 62));
    let mut board = Blackboard::default();
    observe(&world(vec![], vec![raider.clone()]), &mut board, true);
    raider.hp = 10;
    observe(&world(vec![], vec![raider]), &mut board, true);
    assert!(board.wounded.contains(&800));
    assert_eq!(stat(&board, "attack_hits"), Some(1));
}

#[test]
fn health_remembered_for_enemies_out_of_sight_is_dropped_when_the_table_grows_too_large() {
    let target = foreign_building(901, BuildingKind::Barracks, (64, 64));
    let mut board = Blackboard::default();
    for id in 10_000..14_100u32 {
        board.enemy_hp.insert(id, 10);
    }
    observe(&world(vec![target], vec![]), &mut board, false);
    assert_eq!(
        board.enemy_hp.keys().copied().collect::<Vec<_>>(),
        vec![901]
    );
}
