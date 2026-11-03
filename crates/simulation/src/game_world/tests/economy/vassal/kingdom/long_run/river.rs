//! Multi-domain measurement on the ordinary prepared-map river terrain.
//! Synthetic geography, real navigation, finite trees, building and income rules.
use super::*;
use std::collections::{BTreeMap, VecDeque};

fn fixture() -> (GameWorld, Vec<EntityId>) {
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
    for x in [48, 120] {
        let castle = place_near(&mut w, BuildingKind::Castle, TileCoord::new(x, 40));
        for y in [20, 25, 30, 35] {
            place_near(&mut w, BuildingKind::House, TileCoord::new(x - 15, y));
        }
        place_near(&mut w, BuildingKind::Mill, TileCoord::new(x, 55));
        place_near(&mut w, BuildingKind::LumberCamp, TileCoord::new(x, 65));
        if let Terrain::Map { generator, overlay } = &mut w.terrain {
            for y in 70..76 {
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
        let king = workers_near(&mut w, TileCoord::new(x - 1, 40), 1)[0];
        w.kill_unit(w.lookup[&king]);
        w.remove_unit(king);
        let king = w
            .spawn_kind(
                PlayerId(0),
                UnitKind::Vassal,
                position(TileCoord::new(x - 1, 40)),
            )
            .unwrap();
        w.issue_garrison(PlayerId(0), king, castle).unwrap();
        workers.extend(workers_near(&mut w, TileCoord::new(x, 62), 20));
    }
    let wolves: Vec<_> = w
        .animals()
        .filter(|a| a.kind == crate::AnimalKind::Wolf)
        .map(|a| a.id)
        .collect();
    for wolf in wolves {
        w.strike_animal(wolf, workers[0], u16::MAX);
    }
    reserves(&mut w, 1_800, 1_200, 600, 400);
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

fn run(threads: usize) -> Observation {
    rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .build()
        .unwrap()
        .install(|| {
            let (mut w, workers) = fixture();
            let hz = w.config().tick_hz;
            let mut out = Observation::default();
            let mut previous: Vec<_> = workers.iter().map(|id| job(&w, *id)).collect();
            for tick in 1..=hz * 600 {
                if tick > 1 && tick % (hz * 60) == 1 {
                    let stock = &mut w.economy.players.get_mut(&PlayerId(0)).unwrap().stockpile;
                    stock.food = stock.food.saturating_sub(600);
                    stock.wood = stock.wood.saturating_sub(400);
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
                let work = w.vassal_work();
                assert!(
                    work.orders <= 128 && work.resource_searches <= 16 && work.site_checks <= 64
                );
                assert!(work.unit_visits <= 2_048 && work.building_visits <= 4_096);
                assert!(work.resource_nodes <= 1_608 * work.resource_searches);
                assert!(work.resource_blocks <= 25 * work.resource_searches);
                if tick % (hz * 60) == 0 {
                    assert_eq!(work.domains, 2, "both Vassals must remain active");
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
                        if let Some(vassal) = w.vassal_controller(id) {
                            poaching += usize::from(
                                !w.economy.vassal_homes[&vassal].owns(position(target)),
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
fn ten_minute_river_vassals_measure_churn_and_match_one_and_four_workers() {
    let one = run(1);
    let four = run(4);
    assert_eq!(one, four, "river_vassal_one_and_four_worker_replay_matches");
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
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/vassal-stability");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("river.txt"), &report).unwrap();
    // Baseline 4272cf15583b, identical workload: 701 changes, 3,095 actual
    // deposits and 305,124 moving/planning worker ticks. Require a clear churn
    // reduction with the brief's 3% maximum income loss and bounded idle.
    assert!(
        reassignments * 5 <= 701 * 4,
        "at least 20% fewer job changes: {report}"
    );
    assert!(
        one.delivered.iter().sum::<u64>() * 100 >= 3_095 * 97,
        "income must stay within 3% of baseline: {report}"
    );
    assert!(one.walking < 305_124, "walking must also fall: {report}");
    assert!(
        one.idle_minutes.iter().all(|&n| n <= 2) && one.idle_minutes.last() == Some(&0),
        "idle stays bounded and ends at zero: {report}"
    );
    assert!(
        one.unreachable_minutes.iter().all(|&n| n == 0),
        "no disconnected target may appear in a minute sample: {report}"
    );
}
