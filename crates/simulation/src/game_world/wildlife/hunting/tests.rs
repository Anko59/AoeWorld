use super::*;
use aoe_core::{Seed, TileCoord, WorldConfig, WorldPosition};

fn at(x: i32) -> WorldPosition {
    WorldPosition::from_tile_center(TileCoord::new(x, 20)).unwrap()
}

#[test]
fn hunting_chase_keeps_a_goal_that_still_reaches_the_moved_prey() {
    for (kind, within, beyond) in [
        (AnimalKind::Sheep, 1, 2),
        (AnimalKind::Wolf, 1, 2),
        (AnimalKind::Deer, 3, 5),
        (AnimalKind::Boar, 3, 5),
    ] {
        let mut w = GameWorld::new(WorldConfig::new(256, 256, Seed(7)).unwrap()).unwrap();
        w.set_fog_disabled(true);
        let worker = w.spawn_villager(PlayerId(0), at(20)).unwrap();
        let animal = w.spawn_animal(kind, at(30)).unwrap();
        if kind == AnimalKind::Sheep {
            w.wildlife.animals.get_mut(&animal).unwrap().state.owner = Some(PlayerId(0));
        }
        w.issue_hunt(PlayerId(0), &[worker], animal).unwrap();
        let mut job = w.wildlife.hunts.remove(&worker).unwrap();
        assert!(w.chase(worker, &mut job, at(30)));
        let old = w.movement_order(worker).unwrap();
        let index = w.lookup[&worker];
        assert!(!w.waiting_for_route(index));
        let since = w.units[index].planning_since;
        let orders = w.order_demand()[crate::OrderIssuer::Automation as usize].orders;
        w.wildlife.animals.get_mut(&animal).unwrap().state.position = at(30 + within);
        job.repath_tick = w.tick.0;
        assert!(w.chase(worker, &mut job, at(30 + within)));
        assert_eq!(
            w.order_demand()[crate::OrderIssuer::Automation as usize].orders,
            orders,
            "{kind:?}: the admitted endpoint still reaches the prey"
        );
        assert_eq!(
            w.movement_order(worker).unwrap().destination,
            old.destination
        );
        assert_eq!(w.units[index].planning_since, since);
        w.wildlife.animals.get_mut(&animal).unwrap().state.position = at(30 + beyond);
        job.repath_tick = w.tick.0;
        assert!(w.chase(worker, &mut job, at(30 + beyond)));
        assert_eq!(
            w.movement_order(worker).unwrap().destination,
            at(30 + beyond)
        );
        assert_eq!(
            w.order_demand()[crate::OrderIssuer::Automation as usize].orders,
            orders + 1,
            "prey outside the retained goal's reach still gets a new trip"
        );
    }
}
