use super::*;

#[test]
fn vassal_home_and_worker_management_follow_authoritative_updates() {
    let mut world = started();
    let mut vassal = unit(3, ME, UnitKind::Vassal);
    vassal.home_castle = Some(EntityId(20));
    let mut worker = unit(2, ME, UnitKind::Villager);
    worker.managed_by = Some(vassal.id);
    tick(&mut world, 1, |d| d.units.extend([vassal, worker]));
    assert_eq!(
        world.unit(vassal.id).unwrap().home_castle(),
        Some(EntityId(20))
    );
    assert_eq!(world.unit(worker.id).unwrap().managed_by(), Some(vassal.id));
    worker.managed_by = None;
    tick(&mut world, 2, |d| d.units.push(worker));
    assert_eq!(world.unit(worker.id).unwrap().managed_by(), None);
    assert_eq!(
        world.unit(vassal.id).unwrap().home_castle(),
        Some(EntityId(20))
    );
}

#[test]
fn general_summary_and_commander_follow_authoritative_updates() {
    let mut world = started();
    let mut general = unit(3, ME, UnitKind::General);
    general.general = Some(crate::GeneralStatus {
        army_size: 42,
        state: crate::GeneralState::Retreating,
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
