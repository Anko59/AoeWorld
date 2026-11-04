//! Opposing workers must clear a player's one-tile building lane.
use super::*;
use crate::game_world::terrain_fixtures::build;

#[test]
fn villagers_two_way_one_tile_building_lane_flows() {
    let mut f = build("forest");
    // Four contiguous three-tile buildings on each side, one tile between.
    for x in [44, 47, 50, 53] {
        for y in [47, 51] {
            f.world
                .insert_building(
                    PlayerId(0),
                    BuildingKind::Barracks,
                    TileRect::from_xywh(x, y, 3, 3),
                    true,
                )
                .unwrap();
        }
    }
    let mut traffic = Vec::new();
    for n in 0..4 {
        for (from, to) in [(40 - n, 60 + n), (60 + n, 40 - n)] {
            let id = f
                .world
                .spawn_villager(PlayerId(0), center(from, 50))
                .unwrap();
            let goal = center(to, 50);
            f.world.issue_move(id, goal).unwrap();
            traffic.push((id, goal));
        }
    }
    let mut arrived = 0;
    let mut last_tick = 0;
    for tick in 1..=6000 {
        f.world.advance();
        arrived = traffic
            .iter()
            .filter(|&&(id, goal)| {
                let u = f.world.unit(id).unwrap();
                u.position == goal && !u.moving && !u.planning
            })
            .count();
        last_tick = tick;
        if arrived == traffic.len() {
            break;
        }
    }
    eprintln!(
        "building lane: arrived={arrived}/{} last_tick={last_tick}",
        traffic.len()
    );
    for &(id, goal) in &traffic {
        let u = f.world.unit(id).unwrap();
        eprintln!(
            "worker {id:?} at {:?} goal={goal:?} moving={} planning={}",
            u.position, u.moving, u.planning
        );
    }
    assert_eq!(
        arrived,
        traffic.len(),
        "two-way traffic must leave the one-tile lane"
    );
}

#[test]
fn villagers_leave_a_village_lane_with_a_stationary_worker() {
    // Seed-1 arena's lane at (150,151..153): Barracks to the west,
    // the Town Center's solid annex to the east, and a hunter in the middle.
    let mut f = build("forest");
    for (kind, x, y) in [
        (BuildingKind::TownCenter, 151, 151),
        (BuildingKind::Barracks, 147, 151),
        (BuildingKind::House, 149, 148),
        (BuildingKind::LumberCamp, 150, 156),
    ] {
        let r = kind.rules();
        f.world
            .insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(x, y, r.side, r.depth),
                true,
            )
            .unwrap();
    }
    let parked = f
        .world
        .spawn_villager(PlayerId(0), center(150, 152))
        .unwrap();
    // Keep the observed stationary work body; this test isolates navigation
    // from the AI and from whether a particular carcass later runs out.
    f.world.units[f.world.lookup[&parked]].state.task = UnitTask::Hunting;
    let mut traffic = Vec::new();
    for (from, goal) in [
        (center(150, 151), center(149, 155)),
        (center(150, 153), center(148, 150)),
    ] {
        let id = f.world.spawn_villager(PlayerId(0), from).unwrap();
        f.world.issue_move(id, goal).unwrap();
        traffic.push((id, goal));
    }
    let mut arrived = 0;
    for _ in 0..6000 {
        f.world.advance();
        arrived = traffic
            .iter()
            .filter(|&&(id, goal)| f.world.unit(id).unwrap().position == goal)
            .count();
        if arrived == traffic.len() {
            break;
        }
    }
    for &(id, goal) in &traffic {
        eprintln!(
            "village lane {id:?}: {:?} -> {goal:?}, error={:?}",
            f.world.unit(id).unwrap().position,
            f.world.movement_failure(id)
        );
    }
    eprintln!("village lane arrived={arrived}/{}", traffic.len());
    assert_eq!(
        arrived,
        traffic.len(),
        "find the open route around a working body"
    );
    assert_eq!(f.world.unit(parked).unwrap().position, center(150, 152));
    assert_eq!(f.world.unit(parked).unwrap().task, UnitTask::Hunting);
}

#[test]
fn lane_recording_preserves_the_stationary_job() {
    let (mut world, primary) = GameWorld::movement_lane_demo(aoe_core::Seed(1)).unwrap();
    let mut villagers: Vec<_> = world
        .units()
        .filter(|u| u.kind == UnitKind::Villager)
        .map(|u| u.id)
        .collect();
    villagers.sort();
    assert_eq!(villagers.len(), 3);
    let parked = villagers[0];
    let second = villagers[2];
    assert_eq!(primary, villagers[1]);
    for (id, goal) in [(primary, center(127, 131)), (second, center(126, 126))] {
        world.issue_move(id, goal).unwrap();
    }
    for _ in 0..6000 {
        world.advance();
    }
    assert_eq!(world.unit(primary).unwrap().position, center(127, 131));
    assert_eq!(world.unit(second).unwrap().position, center(126, 126));
    assert_eq!(world.unit(parked).unwrap().position, center(128, 128));
    assert_eq!(world.unit(parked).unwrap().task, UnitTask::Hunting);
}

#[test]
fn gathering_workers_escape_the_village_lane_and_deposit() {
    let mut f = build("forest");
    for (kind, x, y) in [
        (BuildingKind::TownCenter, 151, 151),
        (BuildingKind::Barracks, 147, 151),
        (BuildingKind::House, 149, 148),
        (BuildingKind::LumberCamp, 150, 156),
    ] {
        let r = kind.rules();
        f.world
            .insert_building(
                PlayerId(0),
                kind,
                TileRect::from_xywh(x, y, r.side, r.depth),
                true,
            )
            .unwrap();
    }
    let parked = f
        .world
        .spawn_villager(PlayerId(0), center(150, 152))
        .unwrap();
    f.world.units[f.world.lookup[&parked]].state.task = UnitTask::Hunting;
    let before = f.world.player_economy(PlayerId(0)).unwrap().stockpile.wood;
    let mut workers = Vec::new();
    for (from, tile) in [
        (center(150, 151), TileCoord::new(144, 156)),
        (center(150, 153), TileCoord::new(144, 159)),
    ] {
        let node = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Wood).unwrap();
        if let Terrain::Map { generator, overlay } = &mut f.world.terrain {
            overlay.insert_starting_resource(generator, node).unwrap();
        }
        f.world
            .collision_closed(TileRect::from_xywh(tile.x, tile.y, 1, 1));
        let id = f.world.spawn_villager(PlayerId(0), from).unwrap();
        f.world.issue_gather(PlayerId(0), id, node.id).unwrap();
        workers.push(id);
    }
    let mut escaped = std::collections::BTreeSet::new();
    for _ in 0..2400 {
        f.world.advance();
        for &id in &workers {
            if f.world.unit(id).unwrap().position.x < 149 * 1024 {
                escaped.insert(id);
            }
        }
    }
    let deposited = f.world.player_economy(PlayerId(0)).unwrap().stockpile.wood - before;
    eprintln!("gather lane deposited={deposited}");
    for &id in &workers {
        eprintln!(
            "gather worker {id:?} {:?} obstruction={:?} order={:?}",
            f.world.unit(id).unwrap(),
            f.world.movement_obstruction_ticks(id),
            f.world.movement_order(id)
        );
    }
    assert!(
        deposited >= 20,
        "real gathering jobs must leave the lane and deposit"
    );
    assert_eq!(escaped.len(), workers.len(), "both workers must escape");
}

#[test]
fn opposing_movers_clear_the_village_lane_bends() {
    let (mut world, _) = GameWorld::movement_lane_demo(aoe_core::Seed(1)).unwrap();
    // Isolate moving traffic at both mouths, without a stationary work body.
    for id in world.units().map(|u| u.id).collect::<Vec<_>>() {
        world.remove_unit(id);
    }
    let mut traffic = Vec::new();
    for side in [-1, 1] {
        for (from, goal) in [
            (center(128, 127), center(122, 131)),
            (center(128, 129), center(136, 125)),
        ] {
            let id = world
                .spawn_villager(PlayerId(0), WorldPosition::new(from.x + side * 224, from.y))
                .unwrap();
            let goal = center(
                goal.tile_floor().x + i32::from(side > 0),
                goal.tile_floor().y,
            );
            world.issue_move(id, goal).unwrap();
            traffic.push((id, goal));
        }
    }
    let mut arrived = 0;
    for _ in 0..2400 {
        world.advance();
        arrived = traffic
            .iter()
            .filter(|&&(id, goal)| world.unit(id).unwrap().position == goal)
            .count();
        if arrived == traffic.len() {
            break;
        }
    }
    eprintln!("bent lane arrived={arrived}/{}", traffic.len());
    for &(id, goal) in &traffic {
        eprintln!(
            "bend {id:?} {:?} -> {goal:?} obstruction={:?}",
            world.unit(id).unwrap(),
            world.movement_obstruction_ticks(id)
        );
    }
    assert_eq!(arrived, traffic.len());
}

#[test]
fn a_worker_partly_overlapped_by_a_new_building_escapes_without_teleporting() {
    let mut f = build("forest");
    let start = WorldPosition::new(153756, 155126); // seed-1 minute-16 observation
    let id = f.world.spawn_villager(PlayerId(0), start).unwrap();
    assert!(f.world.body_position_clear(UnitKind::Villager, start));
    let footprint = TileRect::from_xywh(147, 151, 3, 3);
    assert!(!footprint.contains(start.tile_floor()));
    f.world
        .insert_building(PlayerId(0), BuildingKind::Barracks, footprint, true)
        .unwrap();
    assert!(!f.world.body_position_clear(UnitKind::Villager, start));
    let goal = center(159, 148);
    f.world.issue_move(id, goal).unwrap();
    let (speed, denominator) = f.world.movement_speed(UnitKind::Villager, PlayerId(0));
    let bound = speed.div_ceil(denominator) as i64;
    let mut previous = start;
    for _ in 0..2400 {
        let escaping = !f.world.body_position_clear(UnitKind::Villager, previous);
        f.world.advance();
        let at = f.world.unit(id).unwrap().position;
        let dx = i64::from(at.x) - i64::from(previous.x);
        let dy = i64::from(at.y) - i64::from(previous.y);
        // Match the established real-terrain metric's two-subunit integer
        // interpolation tolerance after recovery; escaping itself stays strict.
        assert!(dx * dx + dy * dy <= (bound + 2).pow(2));
        if escaping {
            assert!(
                dx * dx + dy * dy <= bound * bound,
                "escape must spend normal movement credit"
            );
        }
        previous = at;
        if at == goal {
            break;
        }
    }
    eprintln!(
        "overlapped worker: {:?} -> {goal:?}",
        f.world.unit(id).unwrap().position
    );
    assert_eq!(f.world.unit(id).unwrap().position, goal);
}

#[test]
fn static_overlap_recovery_never_deepens_or_enters_another_wall() {
    let mut f = build("forest");
    let start = WorldPosition::new(153756, 155126);
    f.world
        .insert_building(
            PlayerId(0),
            BuildingKind::Barracks,
            TileRect::from_xywh(147, 151, 3, 3),
            true,
        )
        .unwrap();
    let away = WorldPosition::new(start.x + 40, start.y);
    let into = WorldPosition::new(start.x - 40, start.y);
    assert!(!f.world.body_position_clear(UnitKind::Villager, away));
    assert!(f.world.body_step_clear(UnitKind::Villager, start, away));
    assert!(!f.world.body_step_clear(UnitKind::Villager, start, into));
    f.world
        .insert_building(
            PlayerId(0),
            BuildingKind::PalisadeWall,
            TileRect::from_xywh(151, 151, 1, 1),
            true,
        )
        .unwrap();
    assert!(!f.world.body_step_clear(
        UnitKind::Villager,
        start,
        WorldPosition::new(154500, start.y)
    ));
}
