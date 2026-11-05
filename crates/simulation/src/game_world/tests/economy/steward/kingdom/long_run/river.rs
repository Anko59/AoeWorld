//! Multi-domain measurement on the ordinary prepared-map river terrain.
//! Synthetic geography, real navigation, finite trees, building and income rules.
use super::*;
use std::collections::{BTreeMap, VecDeque};

fn fixture(domains: usize, unfinished_mills: bool) -> (GameWorld, Vec<EntityId>) {
    fixture_sized(domains, unfinished_mills, 20)
}

fn fixture_sized(
    domains: usize,
    unfinished_mills: bool,
    per_domain: usize,
) -> (GameWorld, Vec<EntityId>) {
    let mut w = crate::game_world::terrain_fixtures::build("river").world;
    let old: Vec<_> = w.units().map(|u| u.id).collect();
    for id in old {
        w.kill_unit(w.lookup[&id]);
        w.remove_unit(id);
    }
    w.initialize_village(PlayerId(0), TileCoord::new(40, 40))
        .unwrap();
    let old: Vec<_> = w.units().map(|u| u.id).collect();
    for id in old {
        w.kill_unit(w.lookup[&id]);
        w.remove_unit(id);
    }
    let old: Vec<_> = w.buildings().map(|b| b.id).collect();
    for id in old {
        w.remove_building(id);
    }
    w.economy.players.get_mut(&PlayerId(0)).unwrap().age = Age::Castle;
    reserves(&mut w, 20_000, 20_000, 20_000, 20_000);
    let mut workers = Vec::new();
    for &(x, base_y) in &[(48, 40), (120, 40), (48, 160), (120, 160)][..domains] {
        let town_center = place_near(&mut w, BuildingKind::TownCenter, TileCoord::new(x, base_y));
        for y in [20, 25, 30, 35] {
            place_near(
                &mut w,
                BuildingKind::House,
                TileCoord::new(x - 15, base_y + y - 40),
            );
        }
        let mill = place_near(&mut w, BuildingKind::Mill, TileCoord::new(x, base_y + 15));
        if unfinished_mills {
            let footprint = w.building(mill).unwrap().footprint;
            w.remove_building(mill);
            let pending = w
                .insert_building(PlayerId(0), BuildingKind::Mill, footprint, false)
                .unwrap();
            assert!(!w.building(pending).unwrap().is_complete());
        }
        place_near(
            &mut w,
            BuildingKind::LumberCamp,
            TileCoord::new(x, base_y + 25),
        );
        if let Terrain::Map { generator, overlay } = &mut w.terrain {
            for y in base_y + 30..base_y + 36 {
                for dx in -8..8 {
                    overlay
                        .insert_starting_resource(
                            generator,
                            ResourceNode::starting(TileCoord::new(x + dx, y), ResourceKind::Wood)
                                .unwrap(),
                        )
                        .unwrap();
                }
            }
        }
        w.resource_collision_changed();
        let king = workers_near(&mut w, TileCoord::new(x - 1, base_y), 1)[0];
        w.kill_unit(w.lookup[&king]);
        w.remove_unit(king);
        let king = w
            .spawn_kind(
                PlayerId(0),
                UnitKind::Steward,
                position(TileCoord::new(x - 1, base_y)),
            )
            .unwrap();
        w.issue_garrison(PlayerId(0), king, town_center).unwrap();
        workers.extend(workers_near(
            &mut w,
            TileCoord::new(x, base_y + 22),
            per_domain,
        ));
    }
    let wolves: Vec<_> = w
        .animals()
        .filter(|a| a.kind == crate::AnimalKind::Wolf)
        .map(|a| a.id)
        .collect();
    for wolf in wolves {
        w.strike_animal(wolf, workers[0], u16::MAX);
    }
    let scale = domains as u32;
    reserves(&mut w, 900 * scale, 600 * scale, 300 * scale, 200 * scale);
    assert_eq!(
        w.player_economy(PlayerId(0)).unwrap().population_used as usize,
        w.units().count(),
        "measurement setup must not retain removed actors' population"
    );
    (w, workers)
}

#[derive(Clone, Copy, Default)]
struct Job {
    kind: Option<ResourceKind>,
    tile: Option<TileCoord>,
    builder: Option<EntityId>,
}

fn job(w: &GameWorld, id: EntityId) -> Job {
    Job {
        kind: w.gathering_kind(id),
        tile: w.gather_tile(id),
        builder: w.economy.builders.get(&id).map(|t| t.building),
    }
}

// Independent static tile-component oracle. Includes finite trees, foundations,
// rivers and diagonal edge rules, excludes transient unit traffic. It is an
// observation, never used to issue or admit the measured orders.
fn components(w: &GameWorld) -> BTreeMap<TileCoord, usize> {
    let mut seen = BTreeMap::new();
    let mut component = 0;
    for y in 0..w.config().height_tiles {
        for x in 0..w.config().width_tiles {
            let at = TileCoord::new(x, y);
            if seen.contains_key(&at) || !w.tile_passable(at) {
                continue;
            }
            component += 1;
            seen.insert(at, component);
            let mut queue = VecDeque::from([at]);
            while let Some(from) = queue.pop_front() {
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let to = TileCoord::new(from.x + dx, from.y + dy);
                        if !seen.contains_key(&to)
                            && w.tile_passable(to)
                            && w.terrain().crossable(from, to, w.config())
                        {
                            seen.insert(to, component);
                            queue.push_back(to);
                        }
                    }
                }
            }
        }
    }
    seen
}

#[derive(Default, Debug, PartialEq)]
struct Observation {
    resource_changes: u64,
    node_changes: u64,
    build_starts: u64,
    starts_from_idle: u64,
    walking: u64,
    idle_ticks: u64,
    idle_minutes: Vec<usize>,
    unreachable_minutes: Vec<usize>,
    poaching_minutes: Vec<usize>,
    delivered: [u64; 4],
    hashes: Vec<String>,
}

fn run(threads: usize, domains: usize, unfinished_mills: bool) -> Observation {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(|| {
            let (mut w, workers) = fixture(domains, unfinished_mills);
            let hz = w.config().tick_hz;
            let mut out = Observation::default();
            let mut previous: Vec<_> = workers.iter().map(|id| job(&w, *id)).collect();
            for tick in 1..=hz * 600 {
                if tick > 1 && tick % (hz * 60) == 1 {
                    let stock = &mut w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile;
                    stock.food = stock.food.saturating_sub(300 * domains as u32);
                    stock.wood = stock.wood.saturating_sub(200 * domains as u32);
                }
                let before = w
                    .economy
                    .income
                    .get(&PlayerId(0))
                    .map_or([0; 4], |i| i.recent(w.tick.0 + 1, hz));
                w.advance();
                let after = w
                    .economy
                    .income
                    .get(&PlayerId(0))
                    .map_or([0; 4], |i| i.recent(w.tick.0, hz));
                for ((sum, new), old) in out.delivered.iter_mut().zip(after).zip(before) {
                    *sum += new.saturating_sub(old);
                }
                for (index, &id) in workers.iter().enumerate() {
                    let u = w.unit(id).expect("all fixture workers survive");
                    let now = job(&w, id);
                    let old = previous[index];
                    out.resource_changes +=
                        u64::from(old.kind.is_some() && now.kind.is_some() && old.kind != now.kind);
                    out.node_changes += u64::from(
                        old.kind.is_some() && now.kind == old.kind && now.tile != old.tile,
                    );
                    out.build_starts +=
                        u64::from(now.builder.is_some() && now.builder != old.builder);
                    out.starts_from_idle += u64::from(
                        old.kind.is_none()
                            && old.builder.is_none()
                            && (now.kind.is_some() || now.builder.is_some()),
                    );
                    out.walking += u64::from(u.moving || u.planning);
                    out.idle_ticks += u64::from(idle(&w, id));
                    previous[index] = now;
                }
                let work = w.steward_work();
                assert!(
                    work.orders <= 64 * domains
                        && work.resource_searches <= 8 * domains
                        && work.site_checks <= 32 * domains
                );
                assert!(
                    work.unit_visits <= 1_024 * domains && work.building_visits <= 2_048 * domains
                );
                assert!(work.resource_nodes <= 1_608 * work.resource_searches);
                assert!(work.resource_blocks <= 25 * work.resource_searches);
                if tick % (hz * 60) == 0 {
                    assert_eq!(work.domains, domains, "both Stewards must remain active");
                    let seen = components(&w);
                    let mut unreachable = 0;
                    let mut poaching = 0;
                    for &id in &workers {
                        let Some(target) = w.gather_tile(id) else {
                            continue;
                        };
                        let from = w.unit(id).unwrap().position.tile_floor();
                        let reachable = (-1..=1).any(|dy| {
                            (-1..=1).any(|dx| {
                                let to = TileCoord::new(target.x + dx, target.y + dy);
                                seen.get(&from).is_some_and(|c| seen.get(&to) == Some(c))
                            })
                        });
                        unreachable += usize::from(!reachable);
                        if let Some(steward) = w.steward_controller(id) {
                            poaching += usize::from(
                                !w.economy.steward_homes[&steward].owns(position(target)),
                            );
                        }
                    }
                    out.idle_minutes
                        .push(workers.iter().filter(|id| idle(&w, **id)).count());
                    out.unreachable_minutes.push(unreachable);
                    out.poaching_minutes.push(poaching);
                    out.hashes.push(w.canonical_hash_hex());
                }
            }
            out
        })
}

#[test]
fn ten_minute_river_stewards_measure_churn_and_match_one_and_four_workers() {
    let one = run(1, 2, false);
    let four = run(4, 2, false);
    assert_eq!(
        one, four,
        "river_steward_one_and_four_worker_replay_matches"
    );
    let reassignments = one.resource_changes + one.node_changes + one.build_starts;
    let report = format!(
        "workers=40 minutes=10 reassignments/villager/min={:.4} walking-share={:.4} resources/min={:?} mean-idle={:.3}\n{one:#?}\n",
        reassignments as f64 / 400.0,
        one.walking as f64 / (40.0 * 12_000.0),
        one.delivered.map(|n| n as f64 / 10.0),
        one.idle_ticks as f64 / 12_000.0
    );
    eprintln!("{report}");
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/steward-stability");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("river.txt"), &report).unwrap();
    // Current mature-work baseline 541b7a3f1a67: 421 changes, 3,910
    // deposits and 261,613 walking ticks. Preserve it alongside the original
    // 701-change churn floor; helper improvements are measured at Mill startup.
    assert!(
        reassignments * 5 <= 701 * 4,
        "at least 20% fewer job changes: {report}"
    );
    assert!(
        reassignments <= 421,
        "mature-work churn must not rise: {report}"
    );
    assert!(
        one.delivered.iter().sum::<u64>() * 100 >= 3_910 * 97,
        "income must stay within 3% of baseline: {report}"
    );
    assert!(
        one.walking <= 261_613,
        "mature-work walking must not rise: {report}"
    );
    assert!(
        one.idle_minutes.iter().all(|&n| n <= 2) && one.idle_minutes.last() == Some(&0),
        "idle stays bounded and ends at zero: {report}"
    );
    assert!(
        one.unreachable_minutes.iter().all(|&n| n == 0),
        "no disconnected target may appear in a minute sample: {report}"
    );
    assert!(
        one.poaching_minutes.iter().all(|&n| n == 0),
        "a controller must not adopt a job serving another domain: {report}"
    );
}

#[test]
fn ten_minute_four_river_stewards_match_one_and_four_workers() {
    let one = run(1, 4, false);
    assert_eq!(
        one,
        run(4, 4, false),
        "four_domain_river_one_and_four_worker_replay_matches"
    );
    let changes = one.resource_changes + one.node_changes + one.build_starts;
    let report = format!(
        "workers=80 minutes=10 job-changes={changes} resources/min={} mean-idle={}\n{one:#?}\n",
        one.delivered.iter().sum::<u64>() as f64 / 10.0,
        one.idle_ticks as f64 / 12_000.0
    );
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/steward-stability");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("four-river.txt"), &report).unwrap();
    assert!(changes <= 736, "mature-work churn must not rise: {report}");
    assert!(
        one.delivered.iter().sum::<u64>() * 100 >= 7_300 * 97,
        "income must stay within 3%: {report}"
    );
    assert_eq!(one.idle_minutes.last(), Some(&0), "{report}");
    assert!(one.idle_minutes.iter().all(|&n| n <= 4), "{report}");
    assert!(one.unreachable_minutes.iter().all(|&n| n == 0), "{report}");
    assert!(one.poaching_minutes.iter().all(|&n| n == 0), "{report}");
}

#[test]
fn ten_minute_four_river_stewards_finish_mills_without_helper_swarms() {
    let one = run(1, 4, true);
    assert_eq!(
        one,
        run(4, 4, true),
        "four_domain_mill_startup_one_and_four_worker_replay_matches"
    );
    let changes = one.resource_changes + one.node_changes + one.build_starts;
    let report = format!(
        "workers=80 minutes=10 job-changes={changes} resources/min={} mean-idle={}\n{one:#?}\n",
        one.delivered.iter().sum::<u64>() as f64 / 10.0,
        one.idle_ticks as f64 / 12_000.0
    );
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/steward-stability");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("mill-startup.txt"), &report).unwrap();
    // Same fixture on 541b7a3f1a67, before helper limits: 895 changes,
    // 159 construction starts, 5,515 deposits, 628,919 walking worker ticks.
    assert!(
        changes < 895,
        "helper teams must reduce job changes: {report}"
    );
    assert!(
        one.build_starts < 159,
        "construction must stop recruiting swarms: {report}"
    );
    assert!(
        one.delivered.iter().sum::<u64>() * 100 >= 5_515 * 97,
        "income must stay within 3%: {report}"
    );
    assert!(
        one.walking < 628_919 && one.idle_ticks <= 2_508,
        "walking and mean idle must not rise: {report}"
    );
    assert_eq!(one.idle_minutes.last(), Some(&0), "{report}");
    assert!(one.idle_minutes.iter().all(|&n| n <= 4), "{report}");
    assert!(one.unreachable_minutes.iter().all(|&n| n == 0), "{report}");
    assert!(one.poaching_minutes.iter().all(|&n| n == 0), "{report}");
}

/// Continuity keeps busy woodcutters on their trees, so a river domain whose
/// Food ran dry used to keep one or two farmers for the whole ten minutes.
/// Stated bound: with the 300-Food-per-minute drain, three or more farmers
/// and a Food stock of at least 100 by minute 7 (stock was ~50 for ten
/// minutes before), (the Food workers forage and farm; Farm count stayed 1 in the fixture).
#[test]
fn a_dry_river_domain_gets_farms_and_food_within_seven_minutes() {
    let (mut w, workers) = fixture_sized(1, false, 20);
    // Only Farms may feed this kingdom: natural forage and fish are gone.
    let mut natural = Vec::new();
    if let Terrain::Map { generator, .. } = &w.terrain {
        for cy in 0..16 {
            for cx in 0..16 {
                if let Ok(chunk) = generator.chunk(cx, cy) {
                    natural.extend(
                        chunk
                            .resources
                            .into_iter()
                            .filter(|n| n.kind == ResourceKind::Food),
                    );
                }
            }
        }
    }
    for node in natural {
        let _ = w.deplete_resource(node.id, node.initial_amount);
    }
    let hz = w.config().tick_hz;
    let mut farms_over_time = Vec::new();
    for tick in 1..=hz * 600 {
        if tick > 1 && tick % (hz * 60) == 1 {
            let stock = &mut w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile;
            stock.food = stock.food.saturating_sub(300);
            stock.wood = stock.wood.saturating_sub(200);
        }
        w.advance();
        if tick % (hz * 60) == 0 {
            let farms = w
                .buildings()
                .filter(|b| b.kind == BuildingKind::Farm)
                .count();
            let food = workers
                .iter()
                .filter(|id| w.gathering_kind(**id) == Some(ResourceKind::Food))
                .count();
            let s = w.player_economy(PlayerId(0)).unwrap().stockpile;
            farms_over_time.push((farms, food, s.food, s.wood));
        }
    }
    eprintln!("(Farm buildings, Food workers, food, wood) per minute: {farms_over_time:?}");
    let (_, food_workers, food, _) = farms_over_time[6];
    assert!(
        food_workers >= 3,
        "too few farmers at minute 7: {farms_over_time:?}"
    );
    assert!(
        food >= 100,
        "Food still dry at minute 7: {farms_over_time:?}"
    );
}
