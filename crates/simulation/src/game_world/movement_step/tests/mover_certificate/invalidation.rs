//! One tick each: a mover with an earlier turn ends inside the step that a
//! later mover's snapshot proposal was clear for. Today the ordered re-sweep
//! refuses that step; a certificate whose padding is smaller than the earlier
//! mover's displacement would take it. Every scene leaves one subunit of slack.
use super::{centre, open_world, pave, proposal_of, shifted, spawn, squared, walk};
use crate::UnitKind::{Militia, Scout, Villager};
use crate::{GameWorld, UnitKind};
use aoe_core::{EntityId, PlayerId, WorldPosition};

/// `first` has the earlier turn and lands on `lands`, closing `walker`'s step.
pub(super) struct Meeting {
    pub(super) world: GameWorld,
    pub(super) first: EntityId,
    pub(super) lands: WorldPosition,
    pub(super) walker: EntityId,
}

/// A rider and a walker of two players face each other on one row. `steps`
/// are their steps this tick: the rider's leaves it one subunit inside the
/// radii sum of the walker's step end.
fn facing(mut world: GameWorld, kinds: (UnitKind, UnitKind), steps: (i32, i32)) -> Meeting {
    let at = centre(100, 100);
    let reach = kinds.0.collision_radius() + kinds.1.collision_radius();
    let east = shifted(at, reach + steps.0 + steps.1 - 1, 0);
    let rider = spawn(&mut world, 2, kinds.0, east);
    let walker = spawn(&mut world, 1, kinds.1, at);
    world.issue_move(rider, centre(60, 100)).unwrap();
    world.issue_move(walker, centre(140, 100)).unwrap();
    Meeting {
        world,
        first: rider,
        lands: shifted(at, reach + steps.1 - 1, 0),
        walker,
    }
}

pub(super) fn head_on() -> Meeting {
    // A scout walks 128 subunits a tick, militia 46.
    facing(open_world(), (Scout, Militia), (128, 46))
}

pub(super) fn on_a_road() -> Meeting {
    let mut world = open_world();
    for x in 96..106 {
        pave(&mut world, x, 100);
    }
    // A quarter faster on the road: scout 160, villager 51.
    facing(world, (Scout, Villager), (160, 51))
}

pub(super) fn with_a_carry() -> Meeting {
    // 384/5 a tick is 76 subunits, and 77 once four fifths are carried over.
    let world = super::world_with_speed(384, 5);
    let mut meeting = facing(world, (Scout, Militia), (77, 46));
    let index = meeting.world.lookup[&meeting.first];
    if let Some(order) = &mut meeting.world.units[index].order {
        order.speed_carry = 4;
    }
    meeting
}

pub(super) fn dashing() -> Meeting {
    // 700 a tick is no proposal: two sequential sweeps of 512 and 188.
    let world = super::world_with_speed(700, 1);
    facing(world, (Scout, Militia), (700, 46))
}

pub(super) fn upgraded() -> Meeting {
    // Only its Feudal Age makes this scout the fastest body: 155 hundredths of
    // a tile a second is 79 subunits a tick, while the configured speed is 16.
    let mut world = super::world_with_speed(16, 1);
    world.economy.players.insert(
        PlayerId(2),
        crate::PlayerEconomy {
            player: PlayerId(2),
            stockpile: crate::Stockpile::default(),
            population_used: 0,
            population_cap: 200,
            age: crate::Age::Feudal,
            researched: crate::Tech::FeudalAge.bit(),
            market_prices: crate::MarketPrices::default(),
        },
    );
    facing(world, (Scout, Militia), (79, 46))
}

pub(super) fn sidestep() -> Meeting {
    let mut world = open_world();
    let at = centre(100, 100);
    let rider = spawn(&mut world, 2, Scout, at);
    // A body in the rider's way: its only clear step is 128 to its right.
    spawn(&mut world, 3, Villager, shifted(at, 500, 0));
    // The walker's lane ends one subunit inside the radii sum of that spot.
    let lane = 128 + 461 - 1;
    let walker = spawn(&mut world, 1, Villager, shifted(at, -40, lane));
    world.issue_move(rider, centre(140, 100)).unwrap();
    walk(&mut world, walker, shifted(at, 20 * 1024 - 40, lane));
    Meeting {
        world,
        first: rider,
        lands: shifted(at, 0, 128),
        walker,
    }
}

fn assert_refused(name: &str, meeting: Meeting) {
    let mut world = meeting.world;
    let (first, walker, lands) = (meeting.first, meeting.walker, meeting.lands);
    let turn = world.active_movers.first().copied();
    assert_eq!(turn, Some(first), "{name}: turn order");
    let step = proposal_of(&world, walker).expect("a clear step in the snapshot");
    let proposed = step.state.position;
    world.advance();
    let (mover, after) = (world.unit(first).unwrap(), world.unit(walker).unwrap());
    assert_eq!(mover.position, lands, "{name}: the earlier mover's step");
    assert_ne!(after.position, proposed, "{name}: took a closed step");
    let reach = i64::from(mover.kind.collision_radius() + after.kind.collision_radius());
    let clear = squared(after.position, lands) >= reach * reach;
    assert!(clear, "{name}: walked into the earlier mover: {after:?}");
}

#[test]
fn a_rider_closing_a_step_head_on_gets_it_refused() {
    assert_refused("head on", head_on());
}

#[test]
fn a_rider_on_a_road_closes_a_step_from_a_quarter_further() {
    assert_refused("road", on_a_road());
}

#[test]
fn a_carried_fraction_lengthens_the_step_that_closes_another() {
    assert_refused("carry", with_a_carry());
}

#[test]
fn a_rider_too_fast_for_a_proposal_closes_a_step_from_700_subunits() {
    assert_refused("dash", dashing());
}

#[test]
fn an_age_upgrade_lengthens_the_step_that_closes_another() {
    assert_refused("upgrade", upgraded());
}

#[test]
fn a_sidestep_closes_a_step_beside_the_sidestepping_unit() {
    assert_refused("sidestep", sidestep());
}

/// Tile (50, 50): 33 centres with the rider that leaves on its first step.
/// The walker's sample (salt 0 + id 33, 32 of 33 slots from slot 0) leaves
/// out slot 32, the one body its step runs into.
#[test]
fn a_cell_that_stops_being_overfull_exposes_the_body_the_sample_missed() {
    let mut world = open_world();
    let corner = WorldPosition::new(51_200, 51_200);
    for _ in 0..31 {
        spawn(&mut world, 3, Villager, shifted(corner, 60, 100));
    }
    let leaver = spawn(&mut world, 2, Scout, shifted(corner, 10, 600));
    let missed = spawn(&mut world, 3, Villager, shifted(corner, 1020, 512));
    let walker = spawn(&mut world, 1, Villager, shifted(corner, 1324, 812));
    world.issue_move(leaver, centre(40, 50)).unwrap();
    walk(&mut world, walker, shifted(corner, 1724, -3188));
    assert!(world.collisions.has_overfull_cells());
    let step = proposal_of(&world, walker).expect("the sampled sweep is clear");
    world.advance();
    assert!(!world.collisions.has_overfull_cells(), "the cell drained");
    let (after, body) = (world.unit(walker).unwrap(), world.unit(missed).unwrap());
    assert_ne!(after.position, step.state.position, "took a closed step");
    let clear = squared(after.position, body.position) >= 410 * 410;
    assert!(clear, "walked into the unsampled body: {after:?}");
}

/// Tile (50, 50) holds 32 centres; a rider's first step brings the 33rd.
/// Every body is far from the walker, yet an overfull cell admits no approach.
#[test]
fn a_cell_that_becomes_overfull_closes_the_approach_to_it() {
    let mut world = open_world();
    let corner = WorldPosition::new(51_200, 51_200);
    for _ in 0..32 {
        spawn(&mut world, 3, Villager, shifted(corner, 60, 60));
    }
    let joiner = spawn(&mut world, 2, Scout, shifted(corner, -60, 900));
    let walker = spawn(&mut world, 1, Villager, shifted(corner, 1624, 512));
    world.issue_move(joiner, centre(60, 50)).unwrap();
    world.issue_move(walker, centre(40, 50)).unwrap();
    assert!(!world.collisions.has_overfull_cells());
    let step = proposal_of(&world, walker).expect("open ground in the snapshot");
    // Certified, even: only the index's watch still sends it to the sweep.
    assert!(super::counts::certified(&step), "every body is far");
    world.advance();
    assert!(world.collisions.has_overfull_cells(), "the cell filled");
    let after = world.unit(walker).unwrap().position;
    assert_ne!(after, step.state.position, "approached an overfull cell");
}
