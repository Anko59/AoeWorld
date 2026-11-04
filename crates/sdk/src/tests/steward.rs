use super::*;

#[test]
fn roaming_steward_and_worker_management_follow_authoritative_updates() {
    let mut world = started();
    let steward = unit(3, ME, UnitKind::Steward);
    let mut worker = unit(2, ME, UnitKind::Villager);
    worker.managed_by = Some(steward.id);
    tick(&mut world, 1, |d| d.units.extend([steward, worker]));
    assert_eq!(world.unit(steward.id).unwrap().home_castle(), None);
    assert_eq!(
        world.unit(worker.id).unwrap().managed_by(),
        Some(steward.id)
    );
    worker.managed_by = None;
    tick(&mut world, 2, |d| d.units.push(worker));
    assert_eq!(world.unit(worker.id).unwrap().managed_by(), None);
    assert_eq!(world.unit(steward.id).unwrap().home_castle(), None);
}

#[test]
fn general_summary_and_commander_follow_authoritative_updates() {
    let mut world = started();
    let mut general = unit(3, ME, UnitKind::General);
    general.general = Some(crate::GeneralStatus {
        army_size: 42,
        state: crate::GeneralState::Retreating,
        training: false,
        queued: 0,
        mix: [35, 30, 20, 10, 5],
        counts: [10, 10, 10, 6, 6],
        squads: std::array::from_fn(|n| {
            let (kind, size) = *[
                (crate::SquadKind::Infantry, 10),
                (crate::SquadKind::Archers, 10),
                (crate::SquadKind::Cavalry, 10),
                (crate::SquadKind::Cavalry, 6),
                (crate::SquadKind::Siege, 5),
                (crate::SquadKind::Siege, 1),
            ]
            .get(n)?;
            Some(crate::SquadStatus {
                leader: EntityId(100 + n as u32),
                kind,
                size,
                state: crate::GeneralState::Retreating,
            })
        }),
    });
    let mut soldier = unit(4, ME, UnitKind::Knight);
    soldier.commanded_by = Some(general.id);
    tick(&mut world, 1, |d| d.units.extend([general, soldier]));
    assert_eq!(world.unit(general.id).unwrap().general(), general.general);
    assert_eq!(
        world.unit(soldier.id).unwrap().commanded_by(),
        Some(general.id)
    );
    assert_eq!(crate::rules::unit_cost(UnitKind::General).food, 800);
    assert_eq!(crate::rules::unit_train_seconds(UnitKind::General), 60);
}
