use super::*;

fn fixture() -> (GameWorld, EntityId, EntityId, TileCoord, TileCoord) {
    let mut world = coast();
    let (land, water) = bank(&world);
    let transport = ship(&mut world, UnitKind::TransportShip, water, 0);
    let passenger = world.spawn_militia(PlayerId(0), at(land)).unwrap();
    (world, transport, passenger, land, water)
}

#[test]
fn loaded_units_are_hidden_untargetable_and_cannot_act_until_unloaded() {
    let (mut world, transport, passenger, land, water) = fixture();
    let enemy = ship(&mut world, UnitKind::Galley, water, 1);
    world.issue_attack(PlayerId(0), passenger, enemy).unwrap();
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    assert_eq!(world.transport_cargo(transport), &[passenger]);
    assert!(!world.unit_visible_to(PlayerId(0), passenger));
    assert!(!world.unit_visible_to(PlayerId(1), passenger));
    assert!(
        !world
            .query(TileRect::from_xywh(land.x - 2, land.y - 2, 8, 8))
            .0
            .iter()
            .any(|u| u.id == passenger)
    );
    assert!(!world.unit(passenger).unwrap().moving);
    assert_eq!(world.unit(passenger).unwrap().attack_target, None);
    assert!(world.issue_move(passenger, at(land)).is_err());
    assert!(world.issue_attack(PlayerId(0), passenger, enemy).is_err());
    assert!(world.issue_attack(PlayerId(1), enemy, passenger).is_err());
    world.issue_unload(PlayerId(0), transport, land).unwrap();
    assert!(world.transport_cargo(transport).is_empty());
    let unit = world.unit(passenger).unwrap();
    assert!(unit.hp > 0);
    assert_eq!(unit.task, UnitTask::Idle);
    assert!(
        world
            .unit_tile_passable(unit.kind, unit.position.tile_floor(), &|| false)
            .unwrap()
    );
    assert!(world.unit_visible_to(PlayerId(0), passenger));
    assert!(
        world
            .query(TileRect::from_xywh(land.x - 2, land.y - 2, 8, 8))
            .0
            .iter()
            .any(|u| u.id == passenger)
    );
}

#[test]
fn embarked_formation_members_are_rejected_atomically_until_unloaded() {
    use aoe_core::Formation;
    fn run() -> [u8; 32] {
        let (mut world, transport, passenger, land, _) = fixture();
        let second = world.spawn_militia(PlayerId(0), at(land)).unwrap();
        let ashore = world.spawn_militia(PlayerId(0), at(land)).unwrap();
        world
            .issue_load(PlayerId(0), &[passenger, second], transport)
            .unwrap();
        for ids in [
            &[passenger][..],
            &[passenger, second][..],
            &[ashore, passenger][..],
        ] {
            for queued in [false, true] {
                for attack_move in [false, true] {
                    let before = world.canonical_hash();
                    assert_eq!(
                        world.issue_formation_move(
                            PlayerId(0),
                            ids,
                            Formation::Line,
                            at(land),
                            queued,
                            attack_move,
                        ),
                        Err(EconomyError::InvalidTarget)
                    );
                    assert_eq!(before, world.canonical_hash());
                }
            }
            let before = world.canonical_hash();
            assert_eq!(
                world.issue_formation(PlayerId(0), ids, Formation::Box),
                Err(EconomyError::InvalidTarget)
            );
            assert_eq!(before, world.canonical_hash());
        }
        let position = world.unit(passenger).unwrap().position;
        advance(&mut world, 40);
        assert_eq!(world.unit(passenger).unwrap().position, position);
        assert_eq!(world.transport_cargo(transport), &[passenger, second]);
        assert!(world.formations.is_empty());
        world.issue_unload(PlayerId(0), transport, land).unwrap();
        world
            .issue_formation(PlayerId(0), &[passenger, second], Formation::Line)
            .unwrap();
        world.canonical_hash()
    }
    assert_eq!(run(), run());
}

#[test]
fn transport_capacity_is_five_and_failed_loads_leave_state_unchanged() {
    let (mut world, transport, first, land, _) = fixture();
    let mut passengers = vec![first];
    for _ in 0..5 {
        passengers.push(world.spawn_villager(PlayerId(0), at(land)).unwrap());
    }
    world
        .issue_load(PlayerId(0), &passengers[..5], transport)
        .unwrap();
    assert_eq!(world.transport_cargo(transport).len(), 5);
    let before = world.canonical_hash();
    assert!(
        world
            .issue_load(PlayerId(0), &passengers[5..], transport)
            .is_err()
    );
    assert_eq!(world.canonical_hash(), before);
    world.issue_unload(PlayerId(0), transport, land).unwrap();
    assert!(world.transport_cargo(transport).is_empty());
    assert!(
        passengers
            .iter()
            .all(|&id| world.unit_visible_to(PlayerId(0), id))
    );
}

#[test]
fn load_validates_the_whole_group_ownership_and_kind_atomically() {
    let (mut world, transport, passenger, land, water) = fixture();
    let enemy = world.spawn_villager(PlayerId(1), at(land)).unwrap();
    let fishing = ship(&mut world, UnitKind::FishingShip, water, 0);
    // Distance is a rendezvous order, covered by the crossing journey tests.
    for group in [
        vec![passenger, enemy],
        vec![passenger, fishing],
        vec![passenger, EntityId(999_999)],
        vec![passenger, passenger],
        vec![],
    ] {
        let before = world.canonical_hash();
        assert!(world.issue_load(PlayerId(0), &group, transport).is_err());
        assert_eq!(world.canonical_hash(), before, "invalid group {group:?}");
    }
    assert_eq!(
        world.issue_load(PlayerId(1), &[enemy], transport),
        Err(EconomyError::NotOwner)
    );
    assert!(world.transport_cargo(transport).is_empty());
}

#[test]
fn unload_rejects_other_players_water_and_distant_banks_without_losing_cargo() {
    let (mut world, transport, passenger, land, water) = fixture();
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    assert_eq!(
        world.issue_unload(PlayerId(1), transport, land),
        Err(EconomyError::NotOwner)
    );
    for target in [water, TileCoord::new(land.x - 12, land.y)] {
        let before = world.canonical_hash();
        assert!(world.issue_unload(PlayerId(0), transport, target).is_err());
        assert_eq!(world.canonical_hash(), before);
        assert_eq!(world.transport_cargo(transport), &[passenger]);
    }
}

#[test]
fn sinking_a_transport_kills_its_passengers_and_clears_cargo() {
    let (mut world, transport, passenger, _, _) = fixture();
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    world.kill_unit(world.lookup[&transport]);
    assert_eq!(world.unit(passenger).unwrap().hp, 0);
    assert_eq!(
        world.unit(passenger).unwrap().death_tick,
        Some(world.tick())
    );
    assert!(world.transport_cargo(transport).is_empty());
    assert!(
        world
            .issue_unload(PlayerId(0), transport, TileCoord::new(1, 1))
            .is_err()
    );
}

#[test]
fn transport_loading_unloading_and_movement_replay_deterministically() {
    let run = || {
        let (mut world, transport, passenger, land, water) = fixture();
        let second = world.spawn_villager(PlayerId(0), at(land)).unwrap();
        world
            .issue_load(PlayerId(0), &[second, passenger], transport)
            .unwrap();
        assert_eq!(world.transport_cargo(transport), &[passenger, second]);
        let loaded_hash = world.canonical_hash();
        let destination = TileCoord::new(water.x + 4, water.y);
        world.issue_move(transport, at(destination)).unwrap();
        advance(&mut world, 200);
        assert_eq!(world.unit(transport).unwrap().position, at(destination));
        world.issue_move(transport, at(water)).unwrap();
        advance(&mut world, 200);
        world.issue_unload(PlayerId(0), transport, land).unwrap();
        (loaded_hash, world.canonical_hash())
    };
    assert_eq!(run(), run());
}

#[test]
fn wildlife_clears_a_passenger_target_after_boarding_and_cannot_damage_cargo() {
    use crate::AnimalKind;
    for kind in [AnimalKind::Wolf, AnimalKind::Boar] {
        let mut world = coast();
        let (land, water) = bank(&world);
        let transport = ship(&mut world, UnitKind::TransportShip, water, 0);
        let passenger = world.spawn_villager(PlayerId(0), at(land)).unwrap();
        let animal = world.spawn_animal(kind, at(land)).unwrap();
        if kind == AnimalKind::Boar {
            world.issue_hunt(PlayerId(0), &[passenger], animal).unwrap();
        }
        world.advance();
        assert_eq!(
            world.animal(animal).unwrap().target,
            Some(passenger),
            "{kind:?} acquires the passenger before boarding"
        );
        world
            .issue_load(PlayerId(0), &[passenger], transport)
            .unwrap();
        let before = world.unit(passenger).unwrap();
        assert!(before.hp > 0);
        world.advance();
        assert_eq!(world.animal(animal).unwrap().target, None, "{kind:?}");
        advance(&mut world, 60);
        let after = world.unit(passenger).unwrap();
        assert_eq!(
            (after.hp, after.last_hit),
            (before.hp, before.last_hit),
            "{kind:?}"
        );
        assert_eq!(world.transport_cargo(transport), &[passenger]);
    }
}

#[test]
fn embarked_units_vacate_bank_building_footprints_without_releasing_population() {
    use crate::BuildingKind;
    let mut world = coast();
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let land = (20..100)
        .find_map(|y| {
            (20..world.config().width_tiles - 16)
                .map(|x| TileCoord::new(x, y))
                .find(|tile| {
                    let origin = TileCoord::new(tile.x - 1, tile.y);
                    world
                        .validate_building_placement(BuildingKind::House, origin)
                        .is_ok()
                        && world
                            .validate_building_placement(BuildingKind::Dock, origin)
                            .is_ok()
                        && (1..=10).all(|dx| {
                            world
                                .unit_tile_passable(
                                    UnitKind::TransportShip,
                                    TileCoord::new(tile.x + dx, tile.y),
                                    &|| false,
                                )
                                .unwrap()
                        })
                })
        })
        .expect("bank needs clear House and Dock footprints");
    let water = TileCoord::new(land.x + 1, land.y);
    let transport = ship(&mut world, UnitKind::TransportShip, water, 0);
    let passenger = world.spawn_villager(PlayerId(0), at(land)).unwrap();
    let population = world.player_economy(PlayerId(0)).unwrap().population_used;
    assert!(population > 0);
    let origin = TileCoord::new(land.x - 1, land.y);
    assert!(
        world
            .validate_building_placement(BuildingKind::Dock, origin)
            .is_err()
    );
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    let destination = TileCoord::new(land.x + 9, land.y);
    world.issue_move(transport, at(destination)).unwrap();
    advance(&mut world, 300);
    assert_eq!(world.unit(transport).unwrap().position, at(destination));
    assert_eq!(world.unit(passenger).unwrap().position, at(land));
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        population
    );
    for kind in [BuildingKind::House, BuildingKind::Dock] {
        assert!(
            world.validate_building_placement(kind, origin).is_ok(),
            "{kind:?} must ignore the embarked passenger's old position"
        );
    }
}

#[test]
fn training_can_spawn_on_an_embarked_passengers_vacated_bank_tile() {
    use crate::BuildingKind;
    let (mut world, transport, passenger, land, _) = fixture();
    let footprint = TileRect::from_xywh(land.x - 4, land.y - 2, 4, 4);
    let center = world
        .insert_building(PlayerId(0), BuildingKind::TownCenter, footprint, true)
        .unwrap();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.stockpile.food = 100;
    player.population_cap = 10;
    // A deterministic occupied-perimeter fixture leaves exactly the bank tile
    // available. Embarking must release it for ordinary producer placement.
    for tile in crate::game_world::economy::placement::perimeter(footprint) {
        if tile != land {
            world.economy.blocked_tiles.insert(tile);
        }
    }
    assert!(world.tile_passable(land));
    world
        .issue_load(PlayerId(0), &[passenger], transport)
        .unwrap();
    let population = world.player_economy(PlayerId(0)).unwrap().population_used;
    world
        .issue_train(PlayerId(0), center, UnitKind::Villager)
        .unwrap();
    advance(&mut world, 25 * 20);
    assert!(world.building(center).unwrap().training_queue.is_empty());
    assert_eq!(
        world.player_economy(PlayerId(0)).unwrap().population_used,
        population + 1
    );
    let trained = world
        .units()
        .find(|unit| unit.kind == UnitKind::Villager)
        .unwrap();
    assert_eq!(trained.position, at(land));
    assert_eq!(world.transport_cargo(transport), &[passenger]);
}
