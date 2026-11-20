use super::*;

#[test]
fn secondary_castle_arrows_carry_their_own_anti_ship_bonus() {
    for (victim, expected) in [
        (UnitKind::Galley, 916), // 8 primary + four (8 + 11 Ships): pierce armour 3.
        (UnitKind::FishingShip, 906), // 10 primary + four (10 + 11 Fishing Ships).
        (UnitKind::FireShip, 921), // Pierce armour 4: 7 + four (7 + 11).
        (UnitKind::DemolitionShip, 901), // Pierce armour 0: 11 + four (11 + 11).
    ] {
        let mut w = coast();
        let (land, water) = bank(&w);
        let castle = w
            .insert_building(
                PlayerId(0),
                crate::BuildingKind::Castle,
                TileRect::from_xywh(land.x - 4, land.y - 1, 4, 4),
                true,
            )
            .unwrap();
        let target = ship(&mut w, victim, water, 1);
        let unit = &mut w.units[w.lookup[&target]].state;
        unit.hp = 1000;
        unit.max_hp = 1000;
        if victim != UnitKind::FishingShip {
            w.issue_stance(PlayerId(1), &[target], aoe_core::CombatStance::NoAttack)
                .unwrap();
        }
        w.issue_attack(PlayerId(0), castle, target).unwrap();
        advance(&mut w, 25);
        assert_eq!(w.unit(target).unwrap().hp, expected, "{victim:?}");
    }
}

#[test]
fn naval_class_bonuses_follow_the_overhaul_single_warship_class() {
    use UnitKind::*;
    // After update 169123 galleys, galleons and cannon ships share one armour
    // class (Long Ranged Ship); fire ships and the Hulk line have their own
    // bonus; nothing else carries a bonus against other ships.
    for (attacker, victim, expected) in [
        (Galley, Galley, 5),
        (WarGalley, WarGalley, 5),
        (Galleon, Galleon, 9),
        (Galley, FishingShip, 0),
        (Galley, FireShip, 0),
        (Galley, DemolitionShip, 0),
        (Galley, TransportShip, 0),
        (Galley, Hulk, 0),
        (FireGalley, Galleon, 1),
        (FireShip, WarGalley, 1),
        (FastFireShip, Galley, 1),
        (FireShip, FishingShip, 0),
        (FireShip, Hulk, 0),
        (Hulk, FireShip, 1),
        (WarHulk, FastFireShip, 1),
        (Carrack, FireGalley, 1),
        (Hulk, Galley, 0),
        (CannonGalleon, CatapultGalleon, 0),
    ] {
        let mut w = coast();
        let (_, water) = bank(&w);
        let target = ship(&mut w, victim, water, 1);
        assert_eq!(
            w.unit_damage_bonus(attacker, target),
            expected,
            "{attacker:?} -> {victim:?}"
        );
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
fn fire_ship_uses_the_dat_range_of_two_and_a_half_tiles() {
    for (distance, in_range) in [(2560, true), (2561, false)] {
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
fn demolition_ship_blast_reaches_three_tiles_but_not_beyond() {
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
            WorldPosition::new(origin.x + 3072, origin.y),
        )
        .unwrap();
    let outside = world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Galley,
            WorldPosition::new(origin.x + 3073, origin.y),
        )
        .unwrap();
    let hp = world.unit(edge).unwrap().hp;
    world.issue_attack(PlayerId(0), demo, target).unwrap();
    world.advance();
    assert_eq!(world.unit(demo).unwrap().hp, 0);
    assert!(world.unit(edge).unwrap().hp < hp);
    assert_eq!(world.unit(outside).unwrap().hp, hp);
}

#[test]
fn a_galley_attack_move_fights_and_resumes_its_water_route() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let galley = ship(&mut world, UnitKind::Galley, water, 0);
    let target = ship(
        &mut world,
        UnitKind::FishingShip,
        TileCoord::new(water.x + 4, water.y + 2),
        1,
    );
    let end = at(TileCoord::new(water.x + 12, water.y));
    world.issue_attack_move(galley, end).unwrap();
    let mut attacked = false;
    for _ in 0..1_300 {
        world.advance();
        attacked |= world
            .unit(galley)
            .is_some_and(|u| u.attack_target == Some(target));
    }
    assert!(attacked, "the Galley obeys the engage part of the order");
    assert_eq!(world.unit(galley).unwrap().position, end);
}

#[test]
fn galley_attack_move_keeps_its_marker_and_queue_through_combat() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let galley = ship(&mut world, UnitKind::Galley, water, 0);
    let target = ship(
        &mut world,
        UnitKind::FishingShip,
        TileCoord::new(water.x + 4, water.y + 2),
        1,
    );
    let end = at(TileCoord::new(water.x + 12, water.y));
    let next = at(TileCoord::new(water.x + 14, water.y));
    world.issue_attack_move(galley, end).unwrap();
    world.issue_queued_move(galley, next).unwrap();
    let expected = vec![(end, true), (next, false)];
    assert_eq!(world.move_waypoints(galley), expected);
    let mut attacked = false;
    for _ in 0..1_500 {
        world.advance();
        if world.unit(galley).unwrap().attack_target == Some(target) {
            attacked = true;
            assert_eq!(world.move_waypoints(galley), expected);
        }
    }
    assert!(attacked);
    assert_eq!(world.unit(galley).unwrap().position, next);
}

#[test]
fn galley_patrol_turns_back_and_stop_clears_the_route() {
    let mut world = coast();
    let (_, water) = bank(&world);
    let galley = ship(&mut world, UnitKind::Galley, water, 0);
    let start = world.unit(galley).unwrap().position;
    let end = at(TileCoord::new(water.x + 5, water.y));
    world.issue_patrol(PlayerId(0), galley, end).unwrap();
    let mut furthest = start.x;
    let mut returned = false;
    for _ in 0..300 {
        world.advance();
        let unit = world.unit(galley).unwrap();
        furthest = furthest.max(unit.position.x);
        returned |= furthest >= end.x - 512 && unit.position.x < start.x + 512;
    }
    assert!(returned, "patrol reaches the endpoint and returns");
    world.issue_stop(PlayerId(0), galley).unwrap();
    let stopped = world.unit(galley).unwrap().position;
    advance(&mut world, 200);
    assert_eq!(world.unit(galley).unwrap().position, stopped);
}

#[test]
fn galley_line_fires_arrows_then_bolts() {
    use crate::ProjectileKind::{Arrow, Bolt};
    for (kind, expected) in [
        (UnitKind::Galley, Arrow),
        (UnitKind::WarGalley, Bolt),
        (UnitKind::Galleon, Bolt),
    ] {
        let mut world = coast();
        let (_, water) = bank(&world);
        let attacker = ship(&mut world, kind, water, 0);
        let victim = ship(
            &mut world,
            UnitKind::FishingShip,
            TileCoord::new(water.x + 3, water.y),
            1,
        );
        world.issue_attack(PlayerId(0), attacker, victim).unwrap();
        let mut kinds = std::collections::BTreeSet::new();
        for _ in 0..60 {
            advance(&mut world, 1);
            kinds.extend(world.projectiles().map(|p| p.kind as u8));
        }
        assert_eq!(
            kinds,
            std::collections::BTreeSet::from([expected as u8]),
            "{kind:?}"
        );
    }
}

#[test]
fn naval_battle_demo_builds_and_a_war_galley_fires_a_bolt() {
    let (mut world, _) = GameWorld::naval_battle_demo(aoe_core::Seed(1)).unwrap();
    let galleys = |p| {
        world
            .units
            .iter()
            .filter(|u| u.state.player == PlayerId(p) && u.state.kind == UnitKind::WarGalley)
            .count()
    };
    assert_eq!((galleys(0), galleys(1)), (3, 3));
    let mut bolt = false;
    for _ in 0..200 {
        world.advance();
        bolt |= world
            .projectiles()
            .any(|p| p.kind == crate::ProjectileKind::Bolt);
    }
    assert!(bolt, "a War Galley fires a bolt");
}
