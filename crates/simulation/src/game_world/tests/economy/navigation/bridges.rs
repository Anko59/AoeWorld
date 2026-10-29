use super::*;
use aoe_map::*;

#[path = "bridge_fixture.rs"]
mod river_fixture;
use river_fixture::{clear_nodes, river_lane, river_width};

#[path = "bridges/angles.rs"]
mod angles;
#[path = "bridges/bridge_queue_tests.rs"]
mod bridge_queue_tests;
#[path = "bridges/head_on.rs"]
mod head_on;

fn setup_axis(horizontal: bool) -> (GameWorld, EntityId) {
    setup_world(river_width(horizontal, 30..34), horizontal)
}

fn setup_world(mut world: GameWorld, horizontal: bool) -> (GameWorld, EntityId) {
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.age = crate::Age::Feudal;
    player.stockpile = Stockpile {
        wood: 2_000,
        stone: 2_000,
        ..Default::default()
    };
    fixture::survey(
        &mut world,
        PlayerId(0),
        if horizontal {
            TileCoord::new(32, 20)
        } else {
            TileCoord::new(20, 32)
        },
        10,
    );
    let worker = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(if horizontal {
                TileCoord::new(28, 20)
            } else {
                TileCoord::new(20, 28)
            })
            .unwrap(),
        )
        .unwrap();
    world.prepare_navigation(&|| false).unwrap();
    (world, worker)
}

fn setup() -> (GameWorld, EntityId) {
    setup_axis(true)
}

#[test]
fn army_attack_orders_use_live_bridge_components_instead_of_static_island_proofs() {
    let (mut world, worker) = setup();
    world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    let attacker = world
        .spawn_kind(
            PlayerId(0),
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(28, 21)).unwrap(),
        )
        .unwrap();
    let victim = world
        .spawn_villager(
            PlayerId(1),
            WorldPosition::from_tile_center(TileCoord::new(36, 20)).unwrap(),
        )
        .unwrap();
    world.issue_attack(PlayerId(0), attacker, victim).unwrap();
    advance(&mut world, 20 * 25);
    assert!(world.unit(attacker).unwrap().position.tile_floor().x >= 34);
    let victim = world.unit(victim).unwrap();
    assert!(victim.hp < victim.max_hp);
}

#[test]
fn forty_tile_bridge_crosses_chunk_boundaries_and_continues_its_route() {
    let mut world = river_width(true, 12..52);
    world
        .place_town_center(PlayerId(0), TileCoord::new(1, 1))
        .unwrap();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.age = crate::Age::Feudal;
    player.stockpile = Stockpile {
        wood: 2000,
        stone: 2000,
        ..Default::default()
    };
    // The long lane may contain generated fish: exhaust those resources so
    // this fixture tests the maximum span rather than occupied placement.
    if let crate::Terrain::Map { generator, overlay } = &mut world.terrain {
        for x in 11..=52 {
            let tile = TileCoord::new(x, 20);
            let (sample, node) = generator
                .tile_and_node_with_cancel(tile, &|| false)
                .unwrap()
                .unwrap();
            assert_eq!(
                sample.water == WaterKind::None,
                x == 11 || x == 52,
                "bank at {tile:?}"
            );
            if let Some(node) = node {
                overlay
                    .deplete(generator, node.id, node.initial_amount)
                    .unwrap();
            }
        }
    }
    fixture::survey(&mut world, PlayerId(0), TileCoord::new(31, 20), 25);
    let worker = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(10, 20)).unwrap(),
        )
        .unwrap();
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(11, 20),
            TileCoord::new(52, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 330);
    assert!(world.building(bridge).unwrap().is_complete());
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(TileCoord::new(54, 20)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 20 * 65);
    assert_eq!(
        world.unit(worker).unwrap().position.tile_floor(),
        TileCoord::new(54, 20)
    );
}

#[test]
fn bridge_geometry_caps_forty_water_tiles_and_canonicalizes_reversed_drags() {
    let from = TileCoord::new(5, 9);
    let to = TileCoord::new(46, 9);
    assert_eq!(
        GameWorld::bridge_footprint(from, to),
        Some(TileRect::from_xywh(5, 9, 42, 1))
    );
    assert_eq!(
        GameWorld::bridge_footprint(to, from),
        GameWorld::bridge_footprint(from, to)
    );
    assert!(GameWorld::bridge_footprint(from, TileCoord::new(47, 9)).is_none());
    assert!(GameWorld::bridge_footprint(from, TileCoord::new(6, 9)).is_none());
    assert!(GameWorld::bridge_footprint(from, TileCoord::new(46, 10)).is_some());
}

#[test]
fn vertical_bridge_crosses_only_its_axis() {
    let (mut world, worker) = setup_axis(false);
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(20, 34),
            TileCoord::new(20, 29),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    assert!(world.building(bridge).unwrap().is_complete());
    let deck = TileCoord::new(20, 31);
    assert!(
        world
            .unit_step_clear(UnitKind::Villager, TileCoord::new(20, 30), deck)
            .unwrap()
    );
    assert!(
        !world
            .unit_step_clear(UnitKind::Villager, TileCoord::new(19, 31), deck)
            .unwrap()
    );
    world
        .issue_move(
            worker,
            WorldPosition::from_tile_center(TileCoord::new(20, 36)).unwrap(),
        )
        .unwrap();
    advance(&mut world, 20 * 25);
    assert_eq!(
        world.unit(worker).unwrap().position.tile_floor(),
        TileCoord::new(20, 36)
    );
}

#[test]
fn bridge_combat_collapse_kills_deck_occupants_and_notifies_the_tick() {
    let (mut world, worker) = setup();
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    let victim = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(31, 20)).unwrap(),
        )
        .unwrap();
    let bank = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(34, 20)).unwrap(),
        )
        .unwrap();
    let attacker = world
        .spawn_kind(
            PlayerId(1),
            UnitKind::Militia,
            WorldPosition::from_tile_center(TileCoord::new(28, 20)).unwrap(),
        )
        .unwrap();
    world.economy.buildings.get_mut(&bridge).unwrap().hp = 1;
    world.issue_attack(PlayerId(1), attacker, bridge).unwrap();
    let mut notified = false;
    for _ in 0..80 {
        notified |= world.advance().iter().any(|u| u.id == victim && u.hp == 0);
        if world.building(bridge).is_none() {
            break;
        }
    }
    assert!(world.building(bridge).is_none());
    assert_eq!(world.unit(victim).unwrap().hp, 0);
    assert!(world.unit(bank).unwrap().hp > 0);
    assert!(
        notified,
        "collapsed occupants must be included in the death delta"
    );
    assert!(!world.tile_passable(TileCoord::new(31, 20)));
}

#[test]
fn bridge_placement_is_atomic_feudal_bounded_and_requires_water_between_land_banks() {
    let (mut world, worker) = setup();
    let from = TileCoord::new(29, 20);
    let to = TileCoord::new(34, 20);
    world.economy.players.get_mut(&PlayerId(0)).unwrap().age = crate::Age::Dark;
    assert_eq!(
        world.issue_build_bridge(PlayerId(0), &[worker], from, to, false),
        Err(EconomyError::RequiresFeudalAge)
    );
    world.economy.players.get_mut(&PlayerId(0)).unwrap().age = crate::Age::Feudal;
    for end in [
        TileCoord::new(34, i32::MAX),
        TileCoord::new(33, 20),
        TileCoord::new(35, 20),
        TileCoord::new(29, 20),
        TileCoord::new(i32::MAX, 20),
    ] {
        let before = world.canonical_hash();
        assert!(
            world
                .issue_build_bridge(PlayerId(0), &[worker], from, end, false)
                .is_err()
        );
        assert_eq!(
            world.canonical_hash(),
            before,
            "refused bridge must not charge or place anything"
        );
    }
    let stock = world.player_economy(PlayerId(0)).unwrap().stockpile;
    let bridge = world
        .issue_build_bridge(PlayerId(0), &[worker], from, to, false)
        .unwrap();
    assert_eq!(world.building(bridge).unwrap().kind, BuildingKind::Bridge);
    let after = world.player_economy(PlayerId(0)).unwrap().stockpile;
    assert_eq!(
        (stock.wood - after.wood, stock.stone - after.stone),
        (100, 20)
    );
    assert!(!world.tile_passable(TileCoord::new(31, 20)));
    world
        .issue_cancel_construction(PlayerId(0), bridge)
        .unwrap();
    assert_eq!(world.player_economy(PlayerId(0)).unwrap().stockpile, stock);
}

#[test]
fn pinned_bridge_completion_connects_axis_only_and_destruction_disconnects_routes() {
    let run = || {
        let (mut world, worker) = setup();
        let goal = TileCoord::new(36, 20);
        let bridge = world
            .issue_build_bridge(
                PlayerId(0),
                &[worker],
                TileCoord::new(29, 20),
                TileCoord::new(34, 20),
                false,
            )
            .unwrap();
        advance(&mut world, 20 * 100);
        assert!(world.building(bridge).unwrap().is_complete());
        let deck = TileCoord::new(31, 20);
        assert!(world.tile_passable(deck));
        assert!(
            world
                .unit_step_clear(UnitKind::Villager, TileCoord::new(30, 20), deck)
                .unwrap()
        );
        for from in [
            TileCoord::new(31, 19),
            TileCoord::new(30, 19),
            TileCoord::new(32, 21),
        ] {
            assert!(
                !world
                    .unit_step_clear(UnitKind::Villager, from, deck)
                    .unwrap()
            );
        }
        assert!(
            !world
                .unit_tile_passable(UnitKind::FishingShip, deck, &|| false)
                .unwrap()
        );
        world
            .issue_move(worker, WorldPosition::from_tile_center(goal).unwrap())
            .unwrap();
        advance(&mut world, 20 * 25);
        assert_eq!(world.unit(worker).unwrap().position.tile_floor(), goal);
        world.remove_building(bridge);
        assert!(!world.tile_passable(deck));
        let Terrain::Map { generator, overlay } = world.terrain() else {
            panic!()
        };
        generator.prepare_navigation(overlay, &|| false).unwrap();
        assert_ne!(
            generator
                .navigation_destination(overlay, goal, TileCoord::new(28, 20), &|| false)
                .unwrap(),
            Some(TileCoord::new(28, 20))
        );
        world.canonical_hash()
    };
    assert_eq!(
        run(),
        run(),
        "bridge lifecycle and movement replay deterministically"
    );
}

#[test]
fn bridge_crosses_shallow_coastal_water() {
    let mut world = river_lane(true, 30..34, 30, Some(20));
    clear_nodes(&mut world, TileRect::from_xywh(10, 10, 4, 4));
    clear_nodes(&mut world, TileRect::from_xywh(28, 19, 7, 2));
    world
        .place_town_center(PlayerId(0), TileCoord::new(10, 10))
        .unwrap();
    let player = world.economy.players.get_mut(&PlayerId(0)).unwrap();
    player.age = crate::Age::Feudal;
    player.stockpile.wood = 200;
    player.stockpile.stone = 50;
    fixture::survey(&mut world, PlayerId(0), TileCoord::new(32, 20), 10);
    let worker = world
        .spawn_villager(
            PlayerId(0),
            WorldPosition::from_tile_center(TileCoord::new(28, 20)).unwrap(),
        )
        .unwrap();
    if let Terrain::Map { generator, .. } = world.terrain() {
        assert_eq!(
            generator
                .tile_at_with_cancel(TileCoord::new(31, 20), &|| false)
                .unwrap()
                .unwrap()
                .water,
            WaterKind::Shallow
        );
    }
    let bridge = world
        .issue_build_bridge(
            PlayerId(0),
            &[worker],
            TileCoord::new(29, 20),
            TileCoord::new(34, 20),
            false,
        )
        .unwrap();
    advance(&mut world, 20 * 100);
    assert!(world.building(bridge).unwrap().is_complete());
    let deck = TileCoord::new(31, 20);
    let side = TileCoord::new(31, 19);
    assert!(world.tile_passable(deck));
    assert!(
        world.tile_passable(side),
        "the side is walkable land, not water"
    );
    assert!(
        !world
            .unit_step_clear(UnitKind::Villager, side, deck)
            .unwrap()
    );
    assert!(
        !world
            .unit_step_clear(UnitKind::Villager, deck, side)
            .unwrap()
    );
    let walker = world
        .spawn_villager(PlayerId(0), WorldPosition::from_tile_center(side).unwrap())
        .unwrap();
    world
        .issue_move(walker, WorldPosition::from_tile_center(deck).unwrap())
        .unwrap();
    let mut first_deck = None;
    for _ in 0..20 * 25 {
        world.advance();
        let tile = world.unit(walker).unwrap().position.tile_floor();
        if tile.y == 20 && (30..34).contains(&tile.x) {
            first_deck.get_or_insert(tile);
        }
    }
    assert!(
        matches!(first_deck, Some(TileCoord { x: 30 | 33, y: 20 })),
        "enter through a bank: {first_deck:?}"
    );
    assert_eq!(world.unit(walker).unwrap().position.tile_floor(), deck);
}
