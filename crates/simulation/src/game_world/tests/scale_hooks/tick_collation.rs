//! Result collation is optional work: a caller that reads the world after the
//! tick (the server's fan-out) steps it without the changed-unit list.
use super::*;

/// Civilians of two players walking and nothing else: no job, fight, building
/// or animal, so every tick takes the early return after the wildlife phase.
fn walkers() -> GameWorld {
    let mut w = world();
    for n in 0..6_u16 {
        let id = w
            .spawn_villager(PlayerId(n % 2), at(100 + 3 * i32::from(n), 100))
            .unwrap();
        w.issue_move(id, at(140, 90 + 4 * i32::from(n))).unwrap();
    }
    w
}

/// Every phase but result collation, which an uncollated tick has no work for.
fn work(phases: &[TickPhase]) -> Vec<TickPhase> {
    phases
        .iter()
        .copied()
        .filter(|phase| *phase != TickPhase::Changed)
        .collect()
}

/// The early return collates too. Without the list it must still refill the
/// route budgets and leave the world the collating tick leaves.
#[test]
fn idle_ticks_leave_the_same_world_without_result_collation() {
    let (mut listing, mut silent) = (walkers(), walkers());
    assert_eq!(silent.canonical_hash(), listing.canonical_hash());
    let mut listed = 0;
    for tick in 1..=120 {
        if tick == 40 {
            // New orders mid-run: both worlds turn their walkers around.
            for world in [&mut listing, &mut silent] {
                let ids = world.units().map(|unit| unit.id).collect::<Vec<_>>();
                for (n, id) in ids.into_iter().enumerate() {
                    world.issue_move(id, at(100 + 2 * n as i32, 120)).unwrap();
                }
            }
        }
        let (mut listing_phases, mut silent_phases) = (Vec::new(), Vec::new());
        listed += listing
            .advance_with_probe(&mut |_| true, &mut |phase| listing_phases.push(phase))
            .len();
        silent.advance_uncollated(&mut |_| true, &mut |phase| silent_phases.push(phase));
        assert_eq!(
            listing_phases,
            [TickPhase::Movement, TickPhase::Spacing, TickPhase::Wildlife],
            "tick {tick}: the fixture left the idle path"
        );
        assert_eq!(work(&silent_phases), listing_phases, "tick {tick}");
        assert_eq!(
            silent.canonical_hash(),
            listing.canonical_hash(),
            "tick {tick}"
        );
    }
    assert!(
        listed >= 120,
        "the walkers must move: {listed} unit records listed in 120 ticks"
    );
}
