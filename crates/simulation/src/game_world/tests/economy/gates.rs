//! DE's Dark Age palisade gate uses a four by one footprint.
use super::buildings::{finish, set_stock, villager_at};
use super::*;

#[test]
fn palisade_gate_is_dark_age_buildable_and_opens_for_its_owner() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 1000, 1000, 0, 1000);
    let worker = villager_at(&mut world, 99, 100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::PalisadeGate,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    let gate = world.building(id).unwrap();
    assert_eq!(
        (gate.footprint.width(), gate.footprint.height(), gate.max_hp),
        (4, 1, 240)
    );
    assert!(!world.tile_passable(TileCoord::new(101, 100)));
    finish(&mut world, id, 700);
    world.advance();
    assert!(
        world.tile_passable(TileCoord::new(101, 100)),
        "completed gate opens for its nearby builder"
    );
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(TileCoord::new(90, 100)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 400);
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "gate closes once the owner leaves"
    );
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(99, 100)).unwrap(),
        )
        .unwrap();
    world.advance();
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "enemy does not open it"
    );
    assert!(
        world
            .issue_move(
                enemy,
                WorldPosition::from_tile_center(TileCoord::new(101, 100)).unwrap()
            )
            .is_err()
    );
}

fn feudal_world(stone: u32) -> (GameWorld, EntityId) {
    let (mut world, _, _) = village();
    set_stock(&mut world, 1000, 1000, 0, stone);
    world.economy.players.get_mut(&PlayerId(0)).unwrap().age = crate::Age::Feudal;
    fixture::survey(&mut world, PlayerId(0), TileCoord::new(104, 104), 12);
    let worker = villager_at(&mut world, 99, 100);
    (world, worker)
}

#[test]
fn stone_walls_need_feudal_age_and_stone_and_lay_a_sturdy_line() {
    let (mut world, _, _) = village();
    set_stock(&mut world, 1000, 1000, 0, 1000);
    let worker = villager_at(&mut world, 99, 100);
    let from = TileCoord::new(100, 100);
    let to = TileCoord::new(104, 100);
    let before = world.canonical_hash();
    assert_eq!(
        world.issue_build_wall(
            PlayerId(0),
            &[worker],
            BuildingKind::StoneWall,
            from,
            to,
            false
        ),
        Err(EconomyError::RequiresFeudalAge)
    );
    assert_eq!(world.canonical_hash(), before, "a refusal changes nothing");
    let (mut world, worker) = feudal_world(24);
    assert!(
        world
            .issue_build_wall(
                PlayerId(0),
                &[worker],
                BuildingKind::StoneWall,
                from,
                to,
                false
            )
            .is_err(),
        "five segments cost 25 stone"
    );
    let (mut world, worker) = feudal_world(100);
    let ids = world
        .issue_build_wall(
            PlayerId(0),
            &[worker],
            BuildingKind::StoneWall,
            from,
            to,
            false,
        )
        .unwrap();
    assert_eq!(ids.len(), 5);
    assert_eq!(super::buildings::stock(&world), (1000, 1000, 0, 75));
    let wall = world.building(ids[0]).unwrap();
    assert_eq!(
        (
            wall.kind,
            wall.max_hp,
            wall.footprint.width(),
            wall.footprint.height()
        ),
        (BuildingKind::StoneWall, 1800, 1, 1)
    );
    assert!(BuildingKind::StoneWall.rules().max_hp > BuildingKind::PalisadeWall.rules().max_hp);
    assert!(
        !world.tile_passable(TileCoord::new(102, 100)),
        "a foundation blocks"
    );
    for id in &ids {
        finish(&mut world, *id, 3000);
    }
    assert!(!world.tile_passable(TileCoord::new(102, 100)));
    assert!(
        world
            .issue_build_wall(PlayerId(0), &[worker], BuildingKind::House, from, to, false)
            .is_err(),
        "only wall kinds may be dragged"
    );
}

#[test]
fn stone_gate_opens_for_its_owner_and_stays_shut_for_an_enemy() {
    let (mut world, worker) = feudal_world(100);
    let id = world
        .issue_build(
            PlayerId(0),
            &[worker],
            BuildingKind::StoneGate,
            TileCoord::new(100, 100),
            false,
        )
        .unwrap();
    assert_eq!(
        super::buildings::stock(&world).3,
        70,
        "a gate costs 30 stone"
    );
    let gate = world.building(id).unwrap();
    assert_eq!(
        (gate.footprint.width(), gate.footprint.height(), gate.max_hp),
        (4, 1, 2750)
    );
    assert!(!world.tile_passable(TileCoord::new(101, 100)));
    finish(&mut world, id, 7000);
    world.advance();
    assert!(world.building(id).unwrap().gate_open);
    assert!(
        world.tile_passable(TileCoord::new(101, 100)),
        "owner's unit opens it"
    );
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(TileCoord::new(90, 100)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 400);
    assert!(!world.building(id).unwrap().gate_open);
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "closes once the owner leaves"
    );
    let enemy = world
        .spawn_militia(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(99, 100)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 20);
    assert!(
        !world.tile_passable(TileCoord::new(101, 100)),
        "an enemy never opens it"
    );
    assert!(
        world
            .issue_move(
                enemy,
                WorldPosition::from_tile_center(TileCoord::new(101, 100)).unwrap()
            )
            .is_err()
    );
}

#[test]
fn stone_walls_and_gates_are_hard_to_damage_and_skip_masonry() {
    use crate::building_combat;
    let wall = building_combat(BuildingKind::StoneWall);
    let palisade = building_combat(BuildingKind::PalisadeWall);
    assert!(wall.melee_armour > palisade.melee_armour);
    assert!(wall.pierce_armour > palisade.pierce_armour);
    assert!(BuildingKind::StoneGate.is_barrier() && BuildingKind::StoneWall.is_barrier());
    assert!(BuildingKind::StoneGate.rules().max_hp > BuildingKind::PalisadeGate.rules().max_hp);
}

#[test]
fn a_stone_fort_lets_its_owner_through_the_gate_and_keeps_the_enemy_out() {
    let (mut world, worker) = feudal_world(500);
    let tile = |x, y| TileCoord::new(x, y);
    let builders = [worker];
    let mut ids = Vec::new();
    // A 9x7 fort, wall corners included, with the gate in the south side.
    for (from, to) in [
        (tile(100, 100), tile(108, 100)),
        (tile(100, 101), tile(100, 106)),
        (tile(108, 101), tile(108, 106)),
        (tile(100, 107), tile(102, 107)),
        (tile(107, 107), tile(108, 107)),
    ] {
        ids.extend(
            world
                .issue_build_wall(
                    PlayerId(0),
                    &builders,
                    BuildingKind::StoneWall,
                    from,
                    to,
                    true,
                )
                .unwrap(),
        );
    }
    ids.push(
        world
            .issue_build(
                PlayerId(0),
                &builders,
                BuildingKind::StoneGate,
                tile(103, 107),
                true,
            )
            .unwrap(),
    );
    for id in &ids {
        finish(&mut world, *id, 12_000);
    }
    let gate = *ids.last().unwrap();
    let away = WorldPosition::from_tile_center(tile(90, 90)).unwrap();
    world.issue_move(worker, away).unwrap();
    advance(&mut world, 600);
    let inside = WorldPosition::from_tile_center(tile(104, 103)).unwrap();
    let own = villager_at(&mut world, 105, 112);
    // A closed gate is an obstacle to every route, so the owner first walks up
    // to it; it opens within two tiles and the way inside is then open.
    let front = WorldPosition::from_tile_center(tile(105, 109)).unwrap();
    world.issue_move(own, front).unwrap();
    advance(&mut world, 200);
    assert!(
        world.building(gate).unwrap().gate_open,
        "it opened for its owner"
    );
    world.issue_move(own, inside).unwrap();
    advance(&mut world, 600);
    let own_at = world.unit(own).unwrap().position.tile_floor();
    assert!(
        own_at.y < 107,
        "the owner walked in through the gate: {own_at:?}"
    );
    assert!(!world.building(gate).unwrap().gate_open, "it shut behind");
    let outside = WorldPosition::from_tile_center(tile(110, 112)).unwrap();
    let enemy = world.spawn_militia(PlayerId(1), outside).unwrap();
    world.issue_move(enemy, front).unwrap();
    advance(&mut world, 400);
    assert!(
        world.issue_move(enemy, inside).is_err(),
        "no route leads the enemy through a closed gate"
    );
    let enemy_at = world.unit(enemy).unwrap().position.tile_floor();
    assert!(enemy_at.y >= 108, "the enemy stayed outside: {enemy_at:?}");
}
