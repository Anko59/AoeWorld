use super::*;

#[test]
fn hidden_naval_class_damage_includes_ship_armour_and_separate_fire_melee() {
    for (victim, galley, war_galley, fire) in [
        (UnitKind::FishingShip, 8, 9, 3),
        (UnitKind::Galley, 8, 9, 4),
        (UnitKind::WarGalley, 8, 9, 4),
        (UnitKind::FireShip, 2, 3, 1),
        (UnitKind::DemolitionShip, 5, 6, 1),
        (UnitKind::TransportShip, 8, 9, 3),
    ] {
        let mut w = coast();
        let (_, water) = bank(&w);
        let target = ship(&mut w, victim, water, 1);
        for (attacker, expected) in [
            (UnitKind::Galley, galley),
            (UnitKind::WarGalley, war_galley),
            (UnitKind::FireShip, fire),
        ] {
            assert_eq!(
                w.unit_damage_bonus(attacker, target),
                expected,
                "{attacker:?} -> {victim:?}"
            );
        }
    }
}

#[test]
fn galleys_and_fire_ships_damage_enemy_ships_on_water() {
    for kind in [UnitKind::Galley, UnitKind::WarGalley, UnitKind::FireShip] {
        let mut world = coast();
        let (_, water) = bank(&world);
        let attacker = ship(&mut world, kind, water, 0);
        let victim = ship(
            &mut world,
            UnitKind::FishingShip,
            TileCoord::new(water.x + 1, water.y),
            1,
        );
        let before = world.unit(victim).unwrap().hp;
        world.issue_attack(PlayerId(0), attacker, victim).unwrap();
        advance(&mut world, 40);
        assert!(world.unit(victim).unwrap().hp < before, "{kind:?}");
        assert!(
            world
                .unit_tile_passable(
                    UnitKind::FishingShip,
                    world.unit(attacker).unwrap().position.tile_floor(),
                    &|| false
                )
                .unwrap()
        );
    }
}

#[test]
fn galley_hits_a_shore_unit_without_beaching_or_chasing_onto_land() {
    let mut world = coast();
    let (land, water) = bank(&world);
    let start = TileCoord::new(water.x + 3, water.y);
    let galley = ship(&mut world, UnitKind::Galley, start, 0);
    let victim = world.spawn_villager(PlayerId(1), at(land)).unwrap();
    let before = world.unit(victim).unwrap().hp;
    world.issue_attack(PlayerId(0), galley, victim).unwrap();
    for _ in 0..40 {
        // The ranged-hit escape reaction must not turn this firing-position
        // regression into a chase; the owner explicitly keeps this target here.
        world.issue_stop(PlayerId(1), victim).unwrap();
        world.advance();
    }
    assert!(world.unit(victim).unwrap().hp < before);
    assert_eq!(world.unit(galley).unwrap().position, at(start));
    assert!(world.movement_order(galley).is_none());
}

#[test]
fn demolition_detonation_damages_nearby_enemy_ships_and_consumes_the_attacker() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let demo = ship(&mut world, UnitKind::DemolitionShip, water, 0);
    let near = TileCoord::new(water.x + 1, water.y);
    let first = ship(&mut world, UnitKind::Galley, near, 1);
    let second = ship(
        &mut world,
        UnitKind::Galley,
        TileCoord::new(near.x, near.y + 1),
        1,
    );
    let distant = ship(
        &mut world,
        UnitKind::Galley,
        TileCoord::new(water.x + 10, water.y),
        1,
    );
    let initial_hp = world.unit(first).unwrap().hp;
    let distant_hp = world.unit(distant).unwrap().hp;
    world.issue_attack(PlayerId(0), demo, first).unwrap();
    advance(&mut world, 40);
    assert_eq!(world.unit(demo).unwrap().hp, 0);
    assert!(world.unit(first).unwrap().hp < initial_hp);
    assert!(world.unit(second).unwrap().hp < initial_hp);
    assert_eq!(world.unit(distant).unwrap().hp, distant_hp);
}

#[test]
fn transport_has_no_attack_and_enemy_naval_orders_require_ownership() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let transport = ship(&mut world, UnitKind::TransportShip, water, 0);
    let galley = ship(
        &mut world,
        UnitKind::Galley,
        TileCoord::new(water.x + 1, water.y),
        1,
    );
    assert_eq!(
        world.issue_attack(PlayerId(0), transport, galley),
        Err(EconomyError::WrongUnitKind)
    );
    assert_eq!(
        world.issue_attack(PlayerId(0), galley, transport),
        Err(EconomyError::NotOwner)
    );
}

#[test]
fn fire_ship_uses_the_original_fractional_range_of_two_point_four_nine_tiles() {
    for (distance, in_range) in [(2549, true), (2560, false)] {
        let mut world = coast();
        let (_, water) = bank(&world);
        let origin = at(TileCoord::new(water.x + 3, water.y));
        let attacker = world
            .spawn_kind(PlayerId(0), UnitKind::FireShip, origin)
            .unwrap();
        let target = world
            .spawn_kind(
                PlayerId(1),
                UnitKind::FishingShip,
                WorldPosition::new(origin.x + distance, origin.y),
            )
            .unwrap();
        world.issue_attack(PlayerId(0), attacker, target).unwrap();
        assert_eq!(
            world.movement_order(attacker).is_none(),
            in_range,
            "distance {distance}"
        );
    }
}

#[test]
fn demolition_blast_reaches_two_point_five_tiles_but_not_beyond() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let origin = at(TileCoord::new(water.x + 3, water.y));
    let demo = world
        .spawn_kind(PlayerId(0), UnitKind::DemolitionShip, origin)
        .unwrap();
    let target = world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Galley,
            WorldPosition::new(origin.x + 1024, origin.y),
        )
        .unwrap();
    let edge = world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Galley,
            WorldPosition::new(origin.x + 2560, origin.y),
        )
        .unwrap();
    let outside = world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Galley,
            WorldPosition::new(origin.x + 2561, origin.y),
        )
        .unwrap();
    let hp = world.unit(edge).unwrap().hp;
    world.issue_attack(PlayerId(0), demo, target).unwrap();
    world.advance();
    assert_eq!(world.unit(demo).unwrap().hp, 0);
    assert!(world.unit(edge).unwrap().hp < hp);
    assert_eq!(world.unit(outside).unwrap().hp, hp);
}
