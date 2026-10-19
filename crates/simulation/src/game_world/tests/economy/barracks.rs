//! The Barracks trains Militia through the generic training queue.
use super::buildings::{finish, set_stock, stock, villager_at};
use super::*;
use crate::{BuildingKind, EconomyError, UnitKind, trainable_units};

#[test]
fn a_barracks_trains_a_militia_end_to_end() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 500, 500, 100, 0);
    assert_eq!(
        trainable_units(BuildingKind::Barracks),
        &[UnitKind::Militia]
    );
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 20;
    let worker = villager_at(&mut world, 99, 100);
    let barracks = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Barracks,
            TileCoord::new(100, 100),
        )
        .unwrap();
    assert_eq!(
        world.issue_train(PlayerId(0), barracks, UnitKind::Militia),
        Err(EconomyError::UnderConstruction)
    );
    finish(&mut world, barracks, 1200);
    // Completion recomputes the cap from housing; leave room for the militia.
    world
        .economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 20;
    assert_eq!(
        world.issue_train(PlayerId(0), barracks, UnitKind::Villager),
        Err(EconomyError::WrongUnitKind)
    );
    let before = stock(&world);
    let used = world.player_economy(PlayerId(0)).unwrap().population_used;
    world
        .issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    // 60 food and 20 gold, from the combat table.
    assert_eq!(
        stock(&world),
        (before.0 - 60, before.1, before.2 - 20, before.3)
    );
    // Cancelling refunds exactly and queues nothing.
    world.issue_cancel_training(PlayerId(0), barracks).unwrap();
    assert_eq!(stock(&world), before);
    world
        .issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    let ticks = 21 * world.config().tick_hz;
    advance(&mut world, ticks - 1);
    assert_eq!(world.building(barracks).unwrap().training_queue.len(), 1);
    advance(&mut world, 1);
    assert!(world.building(barracks).unwrap().training_queue.is_empty());
    let militia = world
        .units()
        .find(|unit| unit.kind == UnitKind::Militia)
        .expect("the finished militia exists");
    assert_eq!(
        (militia.player, militia.hp, militia.max_hp),
        (PlayerId(0), 40, 40)
    );
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        used + 1
    );
}
