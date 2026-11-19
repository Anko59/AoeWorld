use super::*;

#[test]
fn back_to_work_resumes_the_previous_farm_and_leaves_soldiers_inside() {
    let player = PlayerId(0);
    let (mut world, center) = shelter(BuildingKind::TownCenter);
    let farm = world
        .insert_building(
            player,
            BuildingKind::Farm,
            TileRect::from_xywh(94, 100, 3, 3),
            true,
        )
        .unwrap();
    let farmer = world.spawn_villager(player, at(98, 100)).unwrap();
    world.issue_farm(player, farmer, farm).unwrap();
    world.issue_garrison(player, farmer, center).unwrap();
    ticks_until(&mut world, 300, |w| w.unit(farmer).is_none());
    let soldier = enter(&mut world, center, UnitKind::Militia);

    assert_eq!(
        world.issue_back_to_work(PlayerId(1), center),
        Err(EconomyError::NotOwner)
    );
    let save = world.snapshot([0; 32]).unwrap();
    let mut world = super::world();
    world.restore_snapshot(&save, [0; 32], &|| false).unwrap();
    world.issue_back_to_work(player, center).unwrap();
    assert!(
        world
            .economy
            .gatherers
            .assignments()
            .iter()
            .any(|&(id, _, target)| id == farmer && target == Some(farm))
    );
    assert!(world.unit(soldier).is_none());
}

#[test]
fn town_bell_sends_nearby_owned_villagers_to_available_shelters_and_releases_them() {
    let player = PlayerId(0);
    let (mut world, center) = shelter(BuildingKind::TownCenter);
    let tower = world
        .insert_building(
            player,
            BuildingKind::WatchTower,
            TileRect::from_xywh(110, 100, 1, 1),
            true,
        )
        .unwrap();
    let central = world.spawn_villager(player, at(99, 100)).unwrap();
    let near_tower = world.spawn_villager(player, at(111, 100)).unwrap();
    let far = world.spawn_villager(player, at(140, 100)).unwrap();
    let foreign = world.spawn_villager(PlayerId(1), at(99, 101)).unwrap();
    let soldier = world
        .spawn_kind(player, UnitKind::Militia, at(99, 102))
        .unwrap();

    assert_eq!(
        world.issue_town_bell(PlayerId(1), center),
        Err(EconomyError::NotOwner)
    );
    world.issue_town_bell(player, center).unwrap();
    ticks_until(&mut world, 300, |w| {
        w.unit(central).is_none() && w.unit(near_tower).is_none()
    });
    assert_eq!(world.building(center).unwrap().garrison[0].unit.id, central);
    assert_eq!(
        world.building(tower).unwrap().garrison[0].unit.id,
        near_tower
    );
    for id in [far, foreign, soldier] {
        assert!(world.unit(id).is_some());
    }

    let save = world.snapshot([0; 32]).unwrap();
    let mut world = super::world();
    world.restore_snapshot(&save, [0; 32], &|| false).unwrap();
    world.issue_town_bell(player, center).unwrap();
    assert!(world.unit(central).is_some());
    assert!(world.unit(near_tower).is_some());
}

#[test]
fn ringing_again_before_arrival_cancels_only_bell_orders() {
    let player = PlayerId(0);
    let (mut world, center) = shelter(BuildingKind::TownCenter);
    let bell_worker = world.spawn_villager(player, at(94, 100)).unwrap();
    let manual_worker = world.spawn_villager(player, at(94, 101)).unwrap();
    world.issue_garrison(player, manual_worker, center).unwrap();
    world.issue_town_bell(player, center).unwrap();
    assert_eq!(
        world.economy.garrison_orders.get(&bell_worker),
        Some(&center)
    );
    world.issue_town_bell(player, center).unwrap();
    assert!(!world.economy.garrison_orders.contains_key(&bell_worker));
    assert_eq!(
        world.economy.garrison_orders.get(&manual_worker),
        Some(&center)
    );
    advance(&mut world, 350);
    assert!(world.unit(bell_worker).is_some());
    assert!(world.unit(manual_worker).is_none());
}
