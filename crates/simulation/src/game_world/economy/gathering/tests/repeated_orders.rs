use super::*;

#[test]
fn repeating_a_live_gather_job_keeps_its_trip_phase_and_credits() {
    for phase in [
        GatherPhase::Approaching,
        GatherPhase::Gathering,
        GatherPhase::Returning,
    ] {
        let (mut w, node) = resource_mainland();
        let id = w
            .spawn_villager(
                PlayerId(0),
                position(TileCoord::new(node.tile.x - 4, node.tile.y)).unwrap(),
            )
            .unwrap();
        w.issue_gather(PlayerId(0), id, node.id).unwrap();
        let mut task = w.economy.gatherers.remove(&id).unwrap();
        task.phase = phase;
        task.credits = 7;
        if phase == GatherPhase::Returning {
            let to = TileCoord::new(node.tile.x - 8, node.tile.y);
            w.issue_task_move(id, position(to).unwrap()).unwrap();
            task.return_target = Some(to);
            w.units[w.lookup[&id]].state.carried = Some(CarriedResource {
                kind: node.kind,
                amount: 10,
            });
        }
        w.economy.gatherers.insert(id, task);
        let before = w.canonical_hash();
        let requests = w.order_demand()[crate::OrderIssuer::Automation as usize].orders;
        w.issue_gather(PlayerId(0), id, node.id).unwrap();
        assert_eq!(
            w.canonical_hash(),
            before,
            "{phase:?}: an unchanged job retains progress"
        );
        assert_eq!(
            w.order_demand()[crate::OrderIssuer::Automation as usize].orders,
            requests,
            "no new movement intent for the same job"
        );
    }
}

#[test]
fn repeating_a_failed_gather_job_can_start_a_fresh_approach() {
    let (mut w, node) = resource_mainland();
    let id = w
        .spawn_villager(
            PlayerId(0),
            position(TileCoord::new(node.tile.x - 4, node.tile.y)).unwrap(),
        )
        .unwrap();
    w.issue_gather(PlayerId(0), id, node.id).unwrap();
    let index = w.lookup[&id];
    w.stop_movement(index);
    w.set_movement_error(index, Some(GameWorldError::Unreachable));
    w.issue_gather(PlayerId(0), id, node.id).unwrap();
    assert!(w.unit(id).unwrap().moving);
    assert!(w.movement_failure(id).is_none());
}
