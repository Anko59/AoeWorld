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
