use super::*;

#[test]
fn a_seeded_flat_match_scout_clears_the_starting_traffic() {
    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 500,
        seed: 3,
    })
    .unwrap();
    let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![]).unwrap();
    world.prepare_navigation(&|| false).unwrap();
    let placed = std::sync::atomic::AtomicUsize::new(0);
    let scouts = place_villages(&mut world, 2, Some(3), &placed, &|| false).unwrap();
    let id = scouts[0];
    let rect = world
        .buildings()
        .find(|b| b.player == PlayerId(0) && b.kind == aoe_simulation::BuildingKind::TownCenter)
        .unwrap()
        .footprint;
    let goal = aoe_core::WorldPosition::from_tile_center(TileCoord::new(
        (rect.min.x + rect.max.x) / 2 + 10,
        (rect.min.y + rect.max.y) / 2,
    ))
    .unwrap();
    world.issue_move(id, goal).unwrap();
    let destination = world.movement_order(id).unwrap().destination;
    for _ in 0..1500 {
        world.advance();
    }
    assert_eq!(
        world.unit(id).unwrap().position,
        destination,
        "unit={:?}, order={:?}, obstruction={:?}, rect={rect:?}, resources={:?}, units={:?}",
        world.unit(id),
        world.movement_order(id),
        world.movement_obstruction_ticks(id),
        world
            .starting_resources()
            .iter()
            .filter(|n| {
                (n.tile.x - rect.min.x)
                    .abs()
                    .max((n.tile.y - rect.min.y).abs())
                    <= 5
            })
            .collect::<Vec<_>>(),
        world
            .units()
            .filter(|u| {
                let tile = u.position.tile_floor();
                (tile.x - rect.min.x).abs().max((tile.y - rect.min.y).abs()) <= 5
            })
            .collect::<Vec<_>>()
    );
}

#[test]
fn ideal_starts_spread_over_the_whole_map() {
    let two = ideal_starts(400, 2);
    let gap = (two[0].x - two[1].x).abs().max((two[0].y - two[1].y).abs());
    assert!(gap > 150, "two players start on opposite sides: {gap}");
    let many = ideal_starts(800, 100);
    assert_eq!(many.len(), 100);
    let distinct: std::collections::BTreeSet<_> = many.iter().map(|t| (t.x, t.y)).collect();
    assert_eq!(distinct.len(), 100);
    assert!(
        many.iter()
            .all(|t| t.x > 0 && t.x < 800 && t.y > 0 && t.y < 800)
    );
    let (min_x, max_x) = (
        many.iter().map(|t| t.x).min().unwrap(),
        many.iter().map(|t| t.x).max().unwrap(),
    );
    assert!(max_x - min_x > 500, "grid spans the map");
}

#[test]
fn ring_search_covers_every_second_tile_once() {
    let ring = ring_tiles(TileCoord::new(50, 50), 4);
    let distinct: std::collections::BTreeSet<_> = ring.iter().map(|t| (t.x, t.y)).collect();
    assert_eq!(ring.len(), distinct.len());
    assert!(
        ring.iter()
            .all(|t| (t.x - 50).abs().max((t.y - 50).abs()) == 4)
    );
}

/// Per village, what the standard Arabia-like start holds near its centre.
fn kit_near(world: &GameWorld, scout: aoe_core::EntityId) -> Vec<(String, usize)> {
    let at = world
        .units()
        .find(|unit| unit.id == scout)
        .expect("scout")
        .position
        .tile_floor();
    let near = |tile: TileCoord| (tile.x - at.x).abs().max((tile.y - at.y).abs()) <= 24;
    let mut counts = std::collections::BTreeMap::new();
    for node in world.starting_resources().iter().filter(|n| near(n.tile)) {
        *counts.entry(format!("{:?}", node.kind)).or_insert(0) += 1;
    }
    // Natural nodes of the map count too (the start tops them up, never doubles).
    if let aoe_simulation::Terrain::Map { generator, .. } = world.terrain() {
        for y in at.y - 24..=at.y + 24 {
            for x in at.x - 24..=at.x + 24 {
                let node = generator
                    .object_at_with_cancel(TileCoord::new(x, y), &|| false)
                    .expect("object lookup");
                if let Some(node) = node {
                    *counts.entry(format!("{:?}", node.kind)).or_insert(0) += 1;
                }
            }
        }
    }
    for animal in world.animals().filter(|a| near(a.position.tile_floor())) {
        *counts.entry(format!("{:?}", animal.kind)).or_insert(0) += 1;
    }
    counts.into_iter().collect()
}

#[test]
fn every_flat_match_village_gets_the_full_standard_start() {
    for (tiles, seed, villages) in [(500, 3, 4), (500, 5, 4), (300, 1, 2), (900, 2, 9)] {
        let (package, pages) = flat::package(flat::FlatMap {
            tiles_per_side: tiles,
            seed,
        })
        .expect("flat package");
        let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![])
            .expect("flat world");
        let placed = std::sync::atomic::AtomicUsize::new(0);
        let scouts =
            place_villages(&mut world, villages, None, &placed, &|| false).expect("villages");
        assert_eq!(placed.load(Ordering::Acquire), villages);
        let mut short = Vec::new();
        for (slot, scout) in scouts.into_iter().enumerate() {
            let have: std::collections::BTreeMap<_, _> =
                kit_near(&world, scout).into_iter().collect();
            for (kind, need) in [
                ("Food", 6),
                ("Gold", 5),
                ("Stone", 5),
                ("Wood", 12),
                ("Sheep", 6),
                ("Deer", 2),
                ("Boar", 1),
            ] {
                if have.get(kind).copied().unwrap_or(0) < need {
                    short.push(format!("{tiles}/{seed} slot {slot}: {kind} {have:?}"));
                }
            }
        }
        assert!(short.is_empty(), "{short:#?}");
    }
}

#[test]
fn a_scout_survey_finds_nearby_deer_in_each_seeded_flat_village() {
    use aoe_core::WorldPosition;
    use aoe_simulation::{AnimalKind, BuildingKind, TileVisibility};

    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 500,
        seed: 3,
    })
    .unwrap();
    let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![]).unwrap();
    world.set_tick_hz(crate::Config::DEFAULT_SIM_HZ);
    world.prepare_navigation(&|| false).unwrap();
    let placed = AtomicUsize::new(0);
    let scouts = place_villages(&mut world, 2, Some(3), &placed, &|| false).unwrap();
    world.prepare_navigation(&|| false).unwrap();

    let mut surveys: Vec<_> = scouts
        .into_iter()
        .enumerate()
        .map(|(slot, scout)| {
            let player = PlayerId(slot as u16);
            let rect = world
                .buildings()
                .find(|building| {
                    building.player == player && building.kind == BuildingKind::TownCenter
                })
                .unwrap()
                .footprint;
            let home = TileCoord::new((rect.min.x + rect.max.x) / 2, (rect.min.y + rect.max.y) / 2);
            let near = move |tile: TileCoord| {
                (tile.x - home.x).pow(2) + (tile.y - home.y).pow(2) <= 18 * 18
            };
            let herd: Vec<_> = world
                .animals()
                .filter(|animal| {
                    animal.kind == AnimalKind::Deer && near(animal.position.tile_floor())
                })
                .collect();
            assert!(
                herd.len() >= 2,
                "{player:?} at {home:?} is missing its herd"
            );
            assert!(
                herd.iter()
                    .all(|animal| world.tile_passable(animal.position.tile_floor()))
            );
            // The public SDK survey samples two rings, roughly seven tiles apart.
            let points = [(8, 8), (14, 12)]
                .into_iter()
                .flat_map(move |(radius, count)| {
                    (0..count).map(move |i| {
                        let angle = std::f64::consts::TAU * f64::from(i) / f64::from(count);
                        let dx = (f64::from(radius) * angle.cos()).round() as i32;
                        let dy = (f64::from(radius) * angle.sin()).round() as i32;
                        WorldPosition::from_tile_center(TileCoord::new(home.x + dx, home.y + dy))
                            .unwrap()
                    })
                });
            (player, scout, near, points, false)
        })
        .collect();
    for tick in 0..4_000 {
        if tick % (crate::Config::DEFAULT_SIM_HZ / 2) == 0 {
            for (player, scout, near, points, saw_deer) in &mut surveys {
                *saw_deer |= world.animals().any(|animal| {
                    let tile = animal.position.tile_floor();
                    animal.kind == AnimalKind::Deer
                        && near(tile)
                        && world.visibility(*player, tile) == TileVisibility::Visible
                });
                if world.movement_order(*scout).is_none()
                    && let Some(goal) = points.next()
                {
                    let _ = world.issue_move(*scout, goal);
                }
            }
            if surveys.iter().all(|survey| survey.4) {
                return;
            }
        }
        world.advance();
    }
    for (player, _, _, _, saw_deer) in surveys {
        assert!(saw_deer, "{player:?} did not discover its starting deer");
    }
}

#[path = "tests/ai_seats.rs"]
mod ai_seats;
#[path = "tests/headless.rs"]
mod headless;
#[path = "tests/passes.rs"]
mod passes;
#[path = "tests/real_package.rs"]
mod real_package;
#[path = "tests/seats.rs"]
mod seats;

#[test]
fn explicit_benchmark_starts_remain_local_and_never_fall_back_to_mainland() {
    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 300,
        seed: 3,
    })
    .unwrap();
    let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![]).unwrap();
    let starts = [TileCoord::new(60, 60), TileCoord::new(200, 200)];
    let units =
        super::seats::place_explicit(&mut world, 2, &starts, &AtomicUsize::new(0), &|| false)
            .unwrap();
    assert_eq!(units.len(), 2);
    for (i, at) in starts.iter().enumerate() {
        assert!(world.buildings().any(|b| {
            b.player == PlayerId(i as u16)
                && b.kind == aoe_simulation::BuildingKind::TownCenter
                && (b.footprint.min.x - at.x)
                    .abs()
                    .max((b.footprint.min.y - at.y).abs())
                    <= 32
        }));
    }
}

#[test]
fn explicit_benchmark_starts_reject_count_bounds_and_overlap() {
    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 300,
        seed: 3,
    })
    .unwrap();
    for starts in [
        vec![],
        vec![TileCoord::new(-1, 50), TileCoord::new(200, 200)],
        vec![TileCoord::new(60, 60), TileCoord::new(61, 61)],
    ] {
        let mut world =
            GameWorld::from_prepared_map(package.clone(), pages.clone(), vec![], vec![], vec![])
                .unwrap();
        assert!(
            super::seats::place_explicit(&mut world, 2, &starts, &AtomicUsize::new(0), &|| false)
                .is_err()
        );
    }
}

#[test]
fn benchmark_forest_measurement_excludes_the_standard_kit_trees() {
    let (package, pages) = flat::package(flat::FlatMap {
        tiles_per_side: 300,
        seed: 3,
    })
    .unwrap();
    let mut world = GameWorld::from_prepared_map(package, pages, vec![], vec![], vec![]).unwrap();
    let anchor = TileCoord::new(100, 100);
    let before = super::seats::natural_wood_near(&world, anchor).unwrap();
    world.initialize_village(PlayerId(0), anchor).unwrap();
    assert!(!world.starting_resources().is_empty());
    assert_eq!(
        super::seats::natural_wood_near(&world, anchor).unwrap(),
        before
    );
}
