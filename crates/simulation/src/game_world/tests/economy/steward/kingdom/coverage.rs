//! Every walking villager in an active territory ends up managed or Idle.
use super::*;

/// A "Moving" villager that does not move is invisible to the idle pass
/// (its task is not Idle) and to the stall watchdog (it is not yet managed).
#[test]
fn a_frozen_moving_villager_in_the_territory_is_recovered_and_employed() {
    let (mut w, steward, workers, node) = wood_realm(4);
    reserves(&mut w, 30_000, 0, 30_000, 30_000);
    let at = w.unit(workers[0]).unwrap().position.tile_floor();
    let strays = workers_near(&mut w, at, 4);
    for &id in &strays {
        let index = w.lookup[&id];
        w.units[index].state.task = UnitTask::Moving;
        let u = w.unit(id).unwrap();
        assert!(!u.moving && !u.planning, "the fixture must be frozen");
    }
    let everyone: Vec<_> = workers.iter().chain(&strays).copied().collect();
    advance(&mut w, 20 * 40);
    let managed = everyone
        .iter()
        .filter(|id| w.steward_controller(**id) == Some(steward))
        .count();
    let frozen = strays
        .iter()
        .filter(|id| {
            let u = w.unit(**id).unwrap();
            u.task == UnitTask::Moving && !u.moving && !u.planning
        })
        .count();
    eprintln!(
        "coverage: managed {managed}/{} frozen {frozen}/4 near {:?}",
        everyone.len(),
        node.tile
    );
    assert_eq!(frozen, 0, "frozen Moving villagers stayed frozen");
    assert_eq!(managed, everyone.len(), "every villager must be managed");
}
