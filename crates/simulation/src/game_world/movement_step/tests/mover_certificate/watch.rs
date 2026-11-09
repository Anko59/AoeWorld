//! The padding of a certificate estimates how far the earlier movers of its
//! batch go. The collision index measures what they do while the ordered loop
//! runs (`CollisionIndex::watch_drift`) and stops vouching for certified steps
//! once a body went farther, or while a cell holds more centres than a sweep
//! reads: a certified mover then takes the re-sweep after all. One tick each,
//! inside the loop; `collision::index::tests` covers the watch alone.
use super::counts::{certified, resweeps};
use super::invalidation;
use super::{centre, open_world, proposal_of, proposals, shifted, spawn, squared, walk};
use crate::GameWorld;
use crate::UnitKind::{Militia, Scout, Villager};
use aoe_core::{EntityId, WorldPosition};

/// Puts a walking unit `aside` subunits beside its lane and keeps its order,
/// as `slide_toward` leaves a unit it pushed off a foundation. Its next step
/// returns to the lane: `interpolate` reads the order, not the position.
fn slide(world: &mut GameWorld, id: EntityId, aside: i32) {
    let index = world.lookup[&id];
    let from = world.units[index].state.position;
    world.units[index].state.position = shifted(from, 0, aside);
    let state = world.units[index].state;
    world.track_unit_step(id, from.tile_floor(), &state);
}

/// One tick of two movers that are both certain of their steps, `first` having
/// the earlier turn. Both take the proposed step; returns the re-sweeps the
/// ordered loop ran for them.
fn certified_tick(name: &str, world: &mut GameWorld, first: EntityId) -> u64 {
    let steps = proposals(world);
    assert_eq!(steps.len(), 2, "{name}: both movers propose");
    assert_eq!(steps[0].0, first, "{name}: turn order");
    let sure = steps.iter().all(|(_, step)| certified(step));
    assert!(sure, "{name}: both steps are certain in the snapshot");
    let before = resweeps(world);
    world.advance();
    for (id, step) in &steps {
        let at = world.unit(*id).unwrap().position;
        assert_eq!(at, step.state.position, "{name}: applied");
    }
    resweeps(world) - before
}

/// Two militia twenty tiles apart in open ground. On its lane a militia steps
/// 46 subunits, within the bound of 48; from 300 beside it, its step back is
/// 303 long. Ahead of the other's turn that ends the watch, and the other
/// sweeps again; behind it, no turn is left to pay for it.
#[test]
fn a_step_longer_than_the_bound_costs_a_later_certified_mover_its_resweep() {
    for (aside, moved, expected) in [(0, 0, 0), (300, 0, 1), (300, 1, 0)] {
        let mut world = open_world();
        let movers = [centre(40, 40), centre(60, 60)].map(|at| {
            let id = spawn(&mut world, 1, Militia, at);
            walk(&mut world, id, shifted(at, 100 * 1024, 0));
            id
        });
        slide(&mut world, movers[moved], aside);
        let name = format!("mover {moved}, {aside} aside");
        let swept = certified_tick(&name, &mut world, movers[0]);
        assert_eq!(swept, expected, "{name}");
    }
}

/// The `head_on` meeting with the rider 300 subunits beside its lane: far
/// enough from the walker's step for a certificate (660 against 591), and its
/// step back to the lane, 326 long against a bound of 130, ends one subunit
/// inside the radii sum of that step's end. Only the watch stands between the
/// certificate and a step into the rider.
#[test]
fn a_step_back_to_its_lane_that_closes_a_certified_step_gets_it_refused() {
    let meeting = invalidation::head_on();
    let (first, walker, lands) = (meeting.first, meeting.walker, meeting.lands);
    let mut world = meeting.world;
    slide(&mut world, first, 300);
    assert_eq!(world.active_movers.first(), Some(&first), "turn order");
    let own = proposal_of(&world, first).expect("the rider's way back is clear");
    let step = proposal_of(&world, walker).expect("a clear step in the snapshot");
    assert!(!certified(&own), "the walker stands near the rider's lane");
    assert!(certified(&step), "the rider stands beyond the padding");
    let before = resweeps(&world);
    world.advance();
    let (rider, after) = (world.unit(first).unwrap(), world.unit(walker).unwrap());
    assert_eq!(rider.position, lands, "the rider's step back");
    assert_eq!(resweeps(&world) - before, 2, "the rider's and the walker's");
    assert_ne!(after.position, step.state.position, "took a closed step");
    let reach = i64::from(rider.kind.collision_radius() + after.kind.collision_radius());
    let clear = squared(after.position, lands) >= reach * reach;
    assert!(clear, "walked into the rider: {after:?}");
}

/// `counts::overfull_cells_withhold_and_suspend_the_certificate` applies its
/// last step outside the ordered loop, where no watch runs and every step is
/// swept again whatever the cells hold. Here the loop itself fills the cell:
/// the joiner's first step, within the bound, brings tile (50, 50) one more
/// centre before the walker's turn, thirty tiles away. The 33rd suspends the
/// walker's certificate; the 32nd does not.
#[test]
fn a_cell_filled_in_the_ordered_loop_costs_a_later_certified_mover_its_resweep() {
    for (stack, expected) in [(31, 0), (32, 1)] {
        let mut world = open_world();
        let corner = WorldPosition::new(51_200, 51_200);
        for _ in 0..stack {
            spawn(&mut world, 3, Villager, shifted(corner, 60, 60));
        }
        let joiner = spawn(&mut world, 2, Scout, shifted(corner, -60, 900));
        let walker = spawn(&mut world, 1, Militia, centre(20, 20));
        walk(&mut world, joiner, shifted(corner, 20 * 1024, 900));
        walk(&mut world, walker, centre(120, 20));
        let name = format!("a stack of {stack}");
        assert!(!world.collisions.has_overfull_cells(), "{name}: snapshot");
        let swept = certified_tick(&name, &mut world, joiner);
        let filled = world.collisions.has_overfull_cells();
        assert_eq!(filled, stack == 32, "{name}: tile (50, 50) after the tick");
        assert_eq!(swept, expected, "{name}");
    }
}
