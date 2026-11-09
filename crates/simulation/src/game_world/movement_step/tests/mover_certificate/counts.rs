//! The counts the certificate must deliver. Two hooks do not exist yet, so this
//! file does not compile on the unchanged engine (the expected red):
//! `RouteMetrics::apply_resweeps`, the ordered apply loop's body re-sweeps of
//! ordinary proposals, and `FastStep::certified`, set when the proposal's padded
//! sweep proved the step clear whatever earlier movers of the tick do.
use super::invalidation::{self, Meeting};
use super::{centre, open_world, proposal_of, proposals, shifted, spawn, walk};
use crate::GameWorld;
use crate::UnitKind::{Militia, Villager};
use crate::game_world::movement_step::FastStep;

/// One per `collision_sweep` that `apply_step` runs for a non-group step.
pub(super) fn resweeps(world: &GameWorld) -> u64 {
    world.route_scheduler_stats().metrics.apply_resweeps
}

pub(super) fn certified(step: &FastStep) -> bool {
    step.certified
}

/// Today every accepted ordinary proposal is swept again in `apply_step`
/// (`mod.rs:387-399`): 64 times a tick here. Certified, never.
#[test]
fn uncrowded_movers_in_open_ground_cause_no_apply_resweeps() {
    let mut world = open_world();
    for n in 0..64 {
        let at = centre(20 + 2 * (n % 8), 20 + 2 * (n / 8));
        let id = spawn(&mut world, 1, Militia, at);
        // One long leg: no waypoint is crossed, so every tick is a proposal.
        walk(&mut world, id, shifted(at, 150 * 1024, 0));
    }
    let before = resweeps(&world);
    for tick in 0..200 {
        let steps = proposals(&world);
        let sure = steps.iter().filter(|(_, step)| certified(step)).count();
        assert_eq!(steps.len(), 64, "tick {tick}: every mover proposes");
        assert_eq!(sure, 64, "tick {tick}: two tiles apart is certain");
        world.advance();
        for (id, step) in &steps {
            let at = world.unit(*id).unwrap().position;
            assert_eq!(at, step.state.position, "tick {tick}: applied");
        }
        assert_eq!(resweeps(&world), before, "tick {tick}: no re-sweep");
    }
}

/// Two bodies abreast at exactly their radii sum: every step is clear, none
/// is certain, and each keeps today's one re-sweep: not two, and no detour
/// through the sequential path.
#[test]
fn a_mover_too_close_to_certify_keeps_its_one_resweep_a_tick() {
    let mut world = open_world();
    let at = centre(40, 40);
    for lane in [0, 410] {
        let id = spawn(&mut world, 1, Militia, shifted(at, 0, lane));
        walk(&mut world, id, shifted(at, 100 * 1024, lane));
    }
    let before = resweeps(&world);
    for tick in 1..=50 {
        let steps = proposals(&world);
        let sure = steps.iter().any(|(_, step)| certified(step));
        assert_eq!(steps.len(), 2, "tick {tick}: abreast is clear");
        assert!(!sure, "tick {tick}: touching bodies are not certain");
        world.advance();
        for (id, step) in &steps {
            let at = world.unit(*id).unwrap().position;
            assert_eq!(at, step.state.position, "tick {tick}: applied");
        }
        assert_eq!(resweeps(&world) - before, 2 * tick, "tick {tick}");
    }
}

/// The group precedent in `tests.rs` (a snapshot with overfull cells cannot
/// certify an ordered step) for ordinary movers, in both directions.
#[test]
fn overfull_cells_withhold_and_suspend_the_certificate() {
    let mut world = open_world();
    let mover = spawn(&mut world, 1, Militia, centre(20, 20));
    walk(&mut world, mover, centre(120, 20));
    let mut crowd = Vec::new();
    for _ in 0..33 {
        crowd.push(spawn(&mut world, 3, Villager, centre(100, 100)));
    }
    assert!(world.collisions.has_overfull_cells());
    let stale = proposal_of(&world, mover).unwrap();
    assert!(!certified(&stale), "a sampled snapshot proves nothing");
    // An earlier write can end the overflow: the old proposal still re-sweeps.
    world.remove_unit(crowd.pop().unwrap());
    assert!(!world.collisions.has_overfull_cells());
    let before = resweeps(&world);
    assert!(world.apply_step(stale, &mut Vec::new(), &mut Vec::new()));
    assert_eq!(resweeps(&world) - before, 1, "uncertified: re-swept");
    let fresh = proposal_of(&world, mover).unwrap();
    assert!(certified(&fresh), "open ground without overflow");
    // An earlier write can also start one: the certificate is not used.
    // (This step is applied outside the ordered loop, where no watch runs and
    // every step is swept again whatever the cells hold, and the spawn alone
    // would end a watch: `watch` fills a cell inside the loop, by a step.)
    crowd.push(spawn(&mut world, 3, Villager, centre(100, 100)));
    assert!(world.collisions.has_overfull_cells());
    let before = resweeps(&world);
    assert!(world.apply_step(fresh, &mut Vec::new(), &mut Vec::new()));
    assert_eq!(resweeps(&world) - before, 1, "overflow: re-swept");
}

/// The snapshot is clear and an earlier mover closes the step.
fn uncertain(name: &str, meeting: &Meeting) {
    let step = proposal_of(&meeting.world, meeting.walker).unwrap();
    assert!(!certified(&step), "{name}: an earlier mover closes it");
}

/// The `invalidation` meetings, one subunit of slack each.
#[test]
fn a_step_an_earlier_mover_can_close_is_not_certified() {
    uncertain("head on", &invalidation::head_on());
    uncertain("road", &invalidation::on_a_road());
    uncertain("carry", &invalidation::with_a_carry());
    uncertain("dash", &invalidation::dashing());
    uncertain("upgrade", &invalidation::upgraded());
    uncertain("sidestep", &invalidation::sidestep());
}
