//! A generated, task-validated economy. Elapsed time never gates shared CI.
use super::{Verdict, WorkloadResult};
use aoe_core::TileRect;
use aoe_scenario::LATE_GAME;
use aoe_simulation::GameWorld;
use serde::Serialize;
use std::{collections::BTreeMap, error::Error, time::Instant};
mod census;
mod server;
pub(super) use census::Census;

pub(super) const PHASES: [&str; 10] = [
    "movement",
    "spacing",
    "wildlife",
    "combat",
    "construction",
    "gathering",
    "research_training",
    "vassal_economy",
    "building_memory",
    "changed",
];
const RUNS: usize = 3;
const WARMUP: usize = 384;
const TICKS: usize = 128;

#[derive(Default, Debug)]
pub(super) struct Series(pub(super) Vec<u64>);
impl Series {
    pub(super) fn percentile(&self, percent: usize) -> u64 {
        let mut sorted = self.0.clone();
        sorted.sort_unstable();
        sorted
            .get((sorted.len() * percent).div_ceil(100).saturating_sub(1))
            .copied()
            .unwrap_or(0)
    }
}

impl Serialize for Series {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("Series", 3)?;
        value.serialize_field("samples_ns", &self.0)?;
        value.serialize_field("p50_ns", &self.percentile(50))?;
        value.serialize_field("p95_ns", &self.percentile(95))?;
        value.end()
    }
}

#[derive(Debug, Serialize)]
pub(super) struct Run {
    initial_hash: String,
    final_hash: String,
    before: Census,
    after: Census,
    phases: BTreeMap<String, Series>,
    activity: Vec<Census>,
    server: server::Evidence,
}

fn hash(world: &GameWorld) -> String {
    world
        .canonical_hash()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect()
}
fn sample(phases: &mut BTreeMap<String, Series>, name: &str, nanos: u128) {
    phases
        .entry(name.to_owned())
        .or_default()
        .0
        .push(nanos as u64);
}

fn replenish(world: &mut GameWorld) -> Result<(), Box<dyn Error>> {
    let queues: Vec<_> = world
        .buildings()
        .filter_map(|b| {
            let kind = b.training_queue.front()?.kind;
            (b.training_queue.len() < 5).then_some((
                b.player,
                b.id,
                kind,
                5 - b.training_queue.len(),
            ))
        })
        .collect();
    for (player, building, kind, count) in queues {
        for _ in 0..count {
            world.issue_train(player, building, kind)?;
        }
    }
    Ok(())
}

pub(super) fn measure() -> Result<WorkloadResult, Box<dyn Error>> {
    let started = Instant::now();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut runs = Vec::new();
    let mut phases = BTreeMap::<String, Series>::new();
    let mut connection = super::ClientResult::default();
    let mut loaded_chunks = 0;
    let mut query = (0, 0);
    for run in 0..RUNS {
        eprintln!(
            "late-game: building generated world, run {}/{RUNS}",
            run + 1
        );
        let setup = Instant::now();
        let mut world = GameWorld::late_game_workload()?;
        eprintln!(
            "late-game: generated {} units in {:.3}s",
            world.unit_count(),
            setup.elapsed().as_secs_f64()
        );
        let initial_bound = world.entity_id_bound();
        let wood = world.players().map(|p| p.stockpile.wood).sum();
        let warmup = Instant::now();
        for _ in 0..WARMUP {
            world.advance();
        }
        eprintln!("late-game: warmup {:.3}s", warmup.elapsed().as_secs_f64());
        let before = Census::take(&world, wood, initial_bound);
        before.validate()?;
        if before.research != 100 {
            return Err("late-game research workload missing".into());
        }
        let initial_hash = hash(&world);
        let mut observed = BTreeMap::<String, Series>::new();
        let mut activity = Vec::new();
        for _ in 0..TICKS {
            let ordered = Instant::now();
            replenish(&mut world)?;
            let start = Instant::now();
            sample(&mut observed, "orders", (start - ordered).as_nanos());
            let mut last = start;
            world.advance_with_probe(&mut |_| true, &mut |phase| {
                let now = Instant::now();
                sample(
                    &mut observed,
                    PHASES[phase as usize],
                    (now - last).as_nanos(),
                );
                last = now;
            });
            sample(&mut observed, "simulation_tick", start.elapsed().as_nanos());
            sample(
                &mut observed,
                "with_orders_tick",
                ordered.elapsed().as_nanos(),
            );
            let census = Census::take(&world, wood, initial_bound);
            census.validate_activity()?;
            activity.push(census);
        }
        let after = Census::take(&world, wood, initial_bound);
        after.validate_after(&before)?;
        let final_hash = hash(&world);
        if let Some(first) = runs.first() {
            let first: &Run = first;
            if first.initial_hash != initial_hash || first.final_hash != final_hash {
                return Err("late-game independent runs diverged".into());
            }
        }
        loaded_chunks = world.occupied_chunk_count();
        let tile = world
            .units()
            .next()
            .ok_or("late-game population missing")?
            .position
            .tile_floor();
        let (_, stats) = world.query(TileRect::from_xywh(tile.x - 40, tile.y - 40, 96, 96));
        query = (stats.visited_chunks, stats.candidate_units);
        let (traffic, server) =
            runtime.block_on(server::measure(world, &mut observed, wood, initial_bound))?;
        if traffic.snapshots != 500 {
            return Err("late-game snapshots missing".into());
        }
        connection.snapshots += traffic.snapshots;
        connection.deltas += traffic.deltas;
        connection.bytes += traffic.bytes;
        connection.replicated += traffic.replicated;
        connection.visible = connection.visible.max(traffic.visible);
        for (name, series) in &observed {
            phases.entry(name.clone()).or_default().0.extend(&series.0);
        }
        runs.push(Run {
            initial_hash,
            final_hash,
            before,
            after,
            phases: observed,
            activity,
            server,
        });
    }
    let tick_duration_ns = phases["server_tick"].0.clone();
    let result = WorkloadResult {
        scenario: LATE_GAME.name.into(),
        seed: LATE_GAME.seed.0,
        workload_hash: LATE_GAME.workload_hash(),
        total_entities: LATE_GAME.entities,
        active_entities: LATE_GAME.entities,
        resident_entities: runs.last().ok_or("late-game runs missing")?.after.units as u32,
        replicated_entities: connection.replicated,
        max_visible_entities: connection.visible,
        loaded_chunks,
        query_visited_chunks: query.0,
        query_candidates: query.1,
        clients_expected: 100,
        clients_connected: 100,
        snapshots: connection.snapshots,
        deltas: connection.deltas,
        encoded_bytes: connection.bytes,
        tick_duration_ns,
        phases,
        late_game_runs: runs,
        elapsed_ms: started.elapsed().as_millis() as u64,
        verdict: Verdict::Pass,
    };
    print!("{}", summary(&result));
    Ok(result)
}

pub(super) fn summary(result: &WorkloadResult) -> String {
    if result.phases.is_empty() {
        return String::new();
    }
    let mut text = format!(
        "\n{}: per-phase milliseconds ({} independent runs)\n\n| Phase | p50 | p95 | Median run p50 | Median run p95 |\n|---|---:|---:|---:|---:|\n",
        result.scenario,
        result.late_game_runs.len()
    );
    for (name, samples) in &result.phases {
        let median = |percent| {
            Series(
                result
                    .late_game_runs
                    .iter()
                    .map(|run| run.phases[name].percentile(percent))
                    .collect(),
            )
            .percentile(50) as f64
                / 1e6
        };
        text.push_str(&format!(
            "| {name} | {:.3} | {:.3} | {:.3} | {:.3} |\n",
            samples.percentile(50) as f64 / 1e6,
            samples.percentile(95) as f64 / 1e6,
            median(50),
            median(95)
        ));
    }
    text
}
