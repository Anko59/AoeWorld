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
        &[
            UnitKind::Militia,
            UnitKind::Spearman,
            UnitKind::ManAtArms,
            UnitKind::LongSwordsman,
            UnitKind::Pikeman
        ]
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
            false,
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
    // Licensed DE: 50 food and 20 gold, from the combat table.
    assert_eq!(
        stock(&world),
        (before.0 - 50, before.1, before.2 - 20, before.3)
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

#[test]
fn dark_age_militia_research_in_feudal_and_train_men_at_arms() {
    use crate::{Age, Tech};
    let (mut w, _, center) = village();
    set_stock(&mut w, 10_000, 10_000, 10_000, 10_000);
    let worker = villager_at(&mut w, 99, 100);
    let barracks = w
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::Barracks,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    finish(&mut w, barracks, 1200);
    w.economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 30;
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::ManAtArms),
        Err(EconomyError::RequiresFeudalAge)
    );
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    advance(&mut w, 21 * 20);
    let militia = w.units().find(|u| u.kind == UnitKind::Militia).unwrap().id;
    w.units[w.lookup[&militia]].state.hp = 30;
    let builder = villager_at(&mut w, 119, 100);
    let mill = w
        .issue_build(
            PlayerId(0),
            &[builder],
            BuildingKind::Mill,
            TileCoord::new(120, 100),
            false,
        )
        .unwrap();
    finish(&mut w, mill, 1000);
    w.issue_research(PlayerId(0), center, Tech::FeudalAge)
        .unwrap();
    advance(&mut w, 130 * 20);
    assert_eq!(w.player_economy(PlayerId(0)).unwrap().age, Age::Feudal);
    w.economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 30;
    assert_eq!(
        w.issue_train(PlayerId(0), barracks, UnitKind::ManAtArms),
        Err(EconomyError::TechRequirementMissing)
    );
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::ManAtArms),
        Err(EconomyError::ResearchBusy)
    );
    w.issue_cancel_training(PlayerId(0), barracks).unwrap();
    set_stock(&mut w, 99, 0, 40, 0);
    assert_eq!(
        w.issue_research(PlayerId(0), barracks, Tech::ManAtArms),
        Err(EconomyError::InsufficientResources)
    );
    set_stock(&mut w, 500, 0, 200, 0);
    let before = stock(&w);
    w.issue_research(PlayerId(0), barracks, Tech::ManAtArms)
        .unwrap();
    assert_eq!(stock(&w), (before.0 - 100, 0, before.2 - 40, 0));
    advance(&mut w, Tech::ManAtArms.rules().seconds * 20 - 1);
    assert_eq!(w.unit(militia).unwrap().kind, UnitKind::Militia);
    w.advance();
    let upgraded = w.unit(militia).unwrap();
    assert_eq!(
        (upgraded.kind, upgraded.hp, upgraded.max_hp),
        (UnitKind::ManAtArms, 35, 45)
    );
    w.economy
        .players
        .get_mut(&PlayerId(0))
        .unwrap()
        .population_cap = 30;
    w.issue_train(PlayerId(0), barracks, UnitKind::Militia)
        .unwrap();
    assert_eq!(
        w.building(barracks)
            .unwrap()
            .training_queue
            .front()
            .unwrap()
            .kind,
        UnitKind::ManAtArms
    );
    advance(&mut w, 21 * 20);
    assert_eq!(
        w.units().filter(|u| u.kind == UnitKind::ManAtArms).count(),
        2
    );
    assert!(!w.units().any(|u| u.kind == UnitKind::Militia));
}
