//! A Vassal on typed hydrology: its road to the far-bank Town Center spans the
//! river with a bridge that its builders raise and then cross.
use super::*;
use crate::game_world::terrain_fixtures::build;

#[test]
fn a_vassal_bridges_a_real_river_and_its_builders_cross() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let (v, span) = world.prepare_vassal_river_demo().unwrap();
    let standing: std::collections::BTreeSet<_> = world
        .player_buildings_of(PlayerId(0), BuildingKind::Bridge)
        .map(|b| b.id)
        .collect();
    let along_x = span.from.x != span.to.x;
    let beyond = |t: TileCoord| {
        if along_x {
            t.x > span.to.x
        } else {
            t.y > span.to.y
        }
    };
    let (mut built, mut crossed) = (None, None);
    for t in 0..60_000_u32 {
        world.advance();
        if built.is_none()
            && world.buildings().any(|b| {
                b.kind == BuildingKind::Bridge && b.is_complete() && !standing.contains(&b.id)
            })
        {
            built = Some(t);
        }
        if built.is_some()
            && world
                .units()
                .any(|u| u.kind == UnitKind::Villager && beyond(u.position.tile_floor()))
        {
            crossed = Some(t);
            break;
        }
    }
    eprintln!(
        "real river {span:?}: bridge complete at {built:?}, a builder beyond it at {crossed:?}; {:?}",
        world.vassal_status(v)
    );
    assert!(built.is_some(), "{:?}", world.vassal_status(v));
    assert!(crossed.is_some(), "builders must cross the bridge");
    let first_site = |kind| {
        world
            .player_buildings_of(PlayerId(0), kind)
            .map(|b| b.id)
            .filter(|id| !standing.contains(id))
            .min()
    };
    let bridge = first_site(BuildingKind::Bridge).expect("bridge was placed");
    let road = first_site(BuildingKind::Road).expect("roads were placed");
    let roads = world
        .player_buildings_of(PlayerId(0), BuildingKind::Road)
        .count();
    eprintln!(
        "first bridge {bridge:?}, first road {road:?}, {roads} road sites when the builder crossed"
    );
    assert!(bridge < road, "the crossing enters the shared queue first");
    let evidence = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/hierarchy");
    if evidence.is_dir() {
        std::fs::write(
            evidence.join("river-road.txt"),
            format!("span={span:?} bridge={bridge:?} first_road={road:?} built={built:?} crossed={crossed:?} roads={roads}\n"),
        )
        .unwrap();
    }
    // Re-planning keeps the span it laid: no parallel bridges.
    for _ in 0..6_000 {
        world.advance();
    }
    // Re-planning keeps what it laid: no two new bridges side by side.
    let banks: Vec<_> = world
        .buildings()
        .filter(|b| b.kind == BuildingKind::Bridge)
        .filter_map(|b| b.bridge_span.map(|s| s.from))
        .collect();
    // The river runs the whole height of the domain, so its roads may cross
    // it more than once, but never with spans side by side.
    assert!(banks.len() <= 4, "bridges after re-planning: {banks:?}");
    for (i, a) in banks.iter().enumerate() {
        for b in &banks[i + 1..] {
            assert!(
                (a.x - b.x).abs().max((a.y - b.y).abs()) > 30,
                "parallel bridges {a:?} {b:?}"
            );
        }
    }
}

/// With fog of war on (as in a played match) the Vassal still develops the
/// domain: it builds what it can see and keeps exploring outward.
#[test]
fn a_vassal_builds_on_a_real_river_map_with_fog_on() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let (v, _) = world.prepare_vassal_river_demo().unwrap();
    world.set_fog_disabled(false);
    for _ in 0..20_000_u32 {
        world.advance();
    }
    let count = |k: BuildingKind| world.buildings().filter(|b| b.kind == k).count();
    eprintln!(
        "fog on: walls {} roads {} {:?}",
        count(BuildingKind::StoneWall) + count(BuildingKind::PalisadeWall),
        count(BuildingKind::Road),
        world.vassal_status(v)
    );
    assert!(
        count(BuildingKind::Road) + count(BuildingKind::StoneWall) > 0,
        "{:?}",
        world.vassal_status(v)
    );
}

#[test]
fn a_vassal_calls_only_the_nearby_general_on_reachable_land() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let bridges: Vec<_> = world
        .player_buildings_of(PlayerId(0), BuildingKind::Bridge)
        .map(|b| b.id)
        .collect();
    for bridge in bridges {
        world.remove_building(bridge);
    }
    let (vassal, span) = world.prepare_vassal_river_demo().unwrap();
    world.advance();
    world.advance();
    world
        .issue_vassal_development(PlayerId(0), vassal, false)
        .unwrap();
    let castle = world
        .player_buildings_of(PlayerId(0), BuildingKind::Castle)
        .next()
        .unwrap()
        .footprint;
    let center = TileCoord::new(
        (castle.min.x + castle.max.x) / 2,
        (castle.min.y + castle.max.y) / 2,
    );
    let dx = (span.to.x - span.from.x).signum();
    let dy = (span.to.y - span.from.y).signum();
    let pick = |world: &mut GameWorld, at: TileCoord, player, kind| {
        (-3..=3)
            .flat_map(|y| (-3..=3).map(move |x| TileCoord::new(at.x + x, at.y + y)))
            .find_map(|tile| {
                let pos = WorldPosition::from_tile_center(tile).unwrap();
                world
                    .spawn_kind(player, kind, pos)
                    .ok()
                    .map(|id| (id, tile))
            })
            .expect("place a unit on the selected bank")
    };
    let (near, near_tile) = pick(
        world,
        TileCoord::new(span.from.x - dx * 6, span.from.y - dy * 6),
        PlayerId(0),
        UnitKind::General,
    );
    let (across, across_tile) = pick(
        world,
        TileCoord::new(span.to.x + dx * 3, span.to.y + dy * 3),
        PlayerId(0),
        UnitKind::General,
    );
    let (far, far_tile) = pick(
        world,
        TileCoord::new(center.x - dx * 45, center.y - dy * 45),
        PlayerId(0),
        UnitKind::General,
    );
    let (_, threat) = pick(
        world,
        TileCoord::new(span.from.x - dx * 10, span.from.y - dy * 10),
        PlayerId(1),
        UnitKind::Militia,
    );
    let within_call = |tile: TileCoord| {
        let x = i64::from(center.x * 1024 - tile.x * 1024 - 512);
        let y = i64::from(center.y * 1024 - tile.y * 1024 - 512);
        x * x + y * y <= (32 * 1024_i64).pow(2)
    };
    assert!(within_call(near_tile) && within_call(across_tile) && !within_call(far_tile));
    let bucket = aoe_map::LandBucket::for_radius(UnitKind::General.collision_radius()).unwrap();
    assert_eq!(
        world
            .land_reachable_from_each(
                PlayerId(0),
                bucket,
                &[near_tile, across_tile, far_tile],
                threat,
            )
            .unwrap(),
        [true, false, true],
        "the two rejections must exercise different rules"
    );

    world.tick.0 = 20;
    world.advance_vassals(&mut Vec::new());
    let status = world.vassal_status(vassal).unwrap();
    assert!(status.alert);
    let called =
        [near, across, far].map(|id| world.economy.generals.armies[&id].destination.is_some());
    eprintln!(
        "generated river recall: {called:?}, responding {}",
        status.responding
    );
    assert_eq!(called, [true, false, false]);
    assert_eq!(status.responding, 1);
}

#[test]
fn walking_zones_expand_after_a_river_crossing_opens() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let bridges: Vec<_> = world
        .player_buildings_of(PlayerId(0), BuildingKind::Bridge)
        .map(|b| b.id)
        .collect();
    for bridge in bridges {
        world.remove_building(bridge);
    }
    let (vassal, span) = world.prepare_vassal_river_demo().unwrap();
    world.advance();
    world.advance();
    world
        .issue_vassal_development(PlayerId(0), vassal, false)
        .unwrap();
    let (tc, rect) = world
        .player_buildings_of(PlayerId(0), BuildingKind::TownCenter)
        .map(|b| (b.id, b.footprint))
        .next()
        .unwrap();
    let at = |tile| WorldPosition::from_tile_center(tile).unwrap();
    let steward = crate::game_world::economy::placement::perimeter(rect)
        .into_iter()
        .find_map(|tile| {
            world
                .spawn_kind(PlayerId(0), UnitKind::Steward, at(tile))
                .ok()
        })
        .unwrap();
    world.issue_garrison(PlayerId(0), steward, tc).unwrap();
    let across = world
        .spawn_kind(PlayerId(0), UnitKind::Militia, at(span.to))
        .unwrap();
    for _ in 0..25 {
        world.advance();
    }
    assert_eq!(world.steward_town_center(steward), Some(tc));
    assert!(
        !world.economy.steward_homes[&steward].owns(at(span.from)),
        "the Steward cannot walk to the Castle bank"
    );
    assert_eq!(world.vassal_controller(across), None);

    let opened = world.tick().0;
    world.place_finished_bridge(span).unwrap();
    assert!(!world.economy.steward_homes[&steward].owns(at(span.from)));
    for _ in 0..world.config.tick_hz {
        world.advance();
    }
    assert!(
        world.economy.steward_homes[&steward].owns(at(span.from)),
        "the new walking zone is available within one game second"
    );
    eprintln!(
        "walking zone reached the other bank within {} ticks",
        world.tick().0 - opened
    );
    for _ in 0..20 {
        world.advance();
    }
    assert_eq!(world.vassal_controller(across), Some(vassal));
}

#[test]
fn an_entire_forest_patch_has_one_steward_zone() {
    let mut fixture = build("general-forest");
    let world = &mut fixture.world;
    let at = |x, y| WorldPosition::from_tile_center(TileCoord::new(x, y)).unwrap();
    let mut stewards = Vec::new();
    for x in [61, 97] {
        let tc = world
            .insert_building(
                PlayerId(0),
                BuildingKind::TownCenter,
                TileRect::from_xywh(x, 27, 4, 4),
                true,
            )
            .unwrap();
        let steward = world
            .spawn_kind(PlayerId(0), UnitKind::Steward, at(x - 1, 29))
            .unwrap();
        world.issue_garrison(PlayerId(0), steward, tc).unwrap();
        stewards.push(steward);
    }
    for _ in 0..25 {
        world.advance();
    }
    let mut counts = [0, 0];
    for y in 15..46 {
        for x in 75..87 {
            let held = [0, 1].map(|i| world.economy.steward_homes[&stewards[i]].owns(at(x, y)));
            assert_eq!(held.iter().filter(|&&owns| owns).count(), 1);
            counts[usize::from(held[1])] += 1;
        }
    }
    eprintln!("372 contiguous forest sites, ownership {counts:?}");
    assert!(counts.contains(&372));
}

#[test]
fn vassal_reports_the_current_tasks_of_twenty_defenders_by_squad() {
    let mut fixture = build("bridge");
    let world = &mut fixture.world;
    let (vassal, _) = world.prepare_vassal_river_demo().unwrap();
    world.advance();
    world.advance();
    world
        .issue_vassal_development(PlayerId(0), vassal, false)
        .unwrap();
    let castle = world
        .player_buildings_of(PlayerId(0), BuildingKind::Castle)
        .next()
        .unwrap()
        .footprint;
    let exits = crate::game_world::economy::placement::perimeter(castle);
    for kind in [UnitKind::Archer, UnitKind::Knight] {
        let spots: Vec<_> = exits
            .iter()
            .copied()
            .filter(|tile| world.unit_tile_passable(kind, *tile, &|| false).unwrap())
            .take(7)
            .collect();
        assert_eq!(spots.len(), 7);
        for tile in spots {
            world
                .spawn_kind(
                    PlayerId(0),
                    kind,
                    WorldPosition::from_tile_center(tile).unwrap(),
                )
                .unwrap();
        }
    }
    world.tick.0 = 19;
    world.advance();
    let holding = world.vassal_status(vassal).unwrap();
    assert_eq!(holding.military, 20);
    assert_eq!(holding.squads.map(|s| s.holding), [6, 7, 7, 0, 0]);

    let enemy = exits
        .into_iter()
        .find_map(|tile| {
            world
                .spawn_kind(
                    PlayerId(1),
                    UnitKind::Militia,
                    WorldPosition::from_tile_center(tile).unwrap(),
                )
                .ok()
        })
        .unwrap();
    world.tick.0 = 39;
    world.advance();
    let fighting = world.vassal_status(vassal).unwrap();
    assert!(fighting.alert);
    assert_eq!(fighting.military, 20);
    let attacking = fighting.squads.map(|s| s.attacking);
    assert!(attacking[..3].iter().all(|&count| count > 0));
    assert!(world.units().any(|u| {
        u.attack_target == Some(enemy) && world.vassal_controller(u.id) == Some(vassal)
    }));
    let total: u8 = fighting
        .squads
        .iter()
        .map(|s| s.holding + s.moving + s.attacking)
        .sum();
    assert_eq!(total, 20);
    eprintln!("Vassal defence held [6, 7, 7], attacking {attacking:?}, {total} defenders");
}
