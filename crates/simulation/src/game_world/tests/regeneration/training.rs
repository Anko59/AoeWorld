use super::*;
use crate::game_world::economy::placement::perimeter;
use crate::{BuildingKind, UnitKind, VILLAGE_RULES};

fn center_with_one_exit(w: &mut GameWorld, exit: TileCoord) -> EntityId {
    let player = PlayerId(0);
    let footprint = TileRect::from_xywh(exit.x - 4, exit.y - 4, 4, 4);
    let center = w
        .insert_building(player, BuildingKind::TownCenter, footprint, true)
        .unwrap();
    for tile in perimeter(footprint)
        .into_iter()
        .filter(|&tile| tile != exit)
    {
        w.economy.blocked_tiles.insert(tile);
    }
    w.economy.players.get_mut(&player).unwrap().stockpile.food = VILLAGE_RULES.villager_food_cost;
    center
}

fn train_at(w: &mut GameWorld, center: EntityId, exit: TileCoord) {
    w.issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(w, VILLAGE_RULES.villager_training_seconds);
    assert!(w.building(center).unwrap().training_queue.is_empty());
    let unit = w.units().next().unwrap();
    assert_eq!(unit.kind, UnitKind::Villager);
    assert_eq!(unit.position.tile_floor(), exit);
}

#[test]
fn tree_recovery_leaves_the_last_training_exit_open() {
    let (mut w, [_, _, tree]) = tree_line();
    w.deplete_resource(tree.id, tree.initial_amount).unwrap();
    let center = center_with_one_exit(&mut w, tree.tile);
    w.tick.0 = 120 * u64::from(w.config().tick_hz);
    w.advance_resource_recovery(&mut |_| true);
    assert_eq!(w.resource_remaining(tree.id), Some(0));
    train_at(&mut w, center, tree.tile);
}

#[test]
fn herd_recovery_skips_the_training_exit_and_birth_uses_open_ground() {
    let mut w = world();
    let exit = TileCoord::new(101, 99);
    let center = center_with_one_exit(&mut w, exit);
    let [lost, survivor] = herd(&mut w, 100);
    assert!(w.strike_animal(lost, EntityId(u32::MAX), u16::MAX));
    w.tick.0 = 240 * u64::from(w.config().tick_hz);
    w.advance_herd_recovery();
    let newborn = w.animals().find(|a| a.hp > 0 && a.id != survivor).unwrap();
    assert_eq!(newborn.position.tile_floor(), TileCoord::new(100, 100));
    train_at(&mut w, center, exit);
}

#[test]
fn a_planned_town_center_keeps_its_exit_until_cancelled() {
    let (mut w, [_, _, tree]) = tree_line();
    w.deplete_resource(tree.id, tree.initial_amount).unwrap();
    let footprint = TileRect::from_xywh(tree.tile.x + 1, tree.tile.y + 1, 4, 4);
    let center = w
        .insert_building(PlayerId(0), BuildingKind::TownCenter, footprint, false)
        .unwrap();
    let interval = 120 * u64::from(w.config().tick_hz);
    w.tick.0 = interval;
    w.advance_resource_recovery(&mut |_| true);
    assert_eq!(w.resource_remaining(tree.id), Some(0));

    w.remove_building(center);
    w.tick.0 += interval;
    w.advance_resource_recovery(&mut |_| true);
    assert_eq!(w.resource_remaining(tree.id), Some(tree.initial_amount));
}
