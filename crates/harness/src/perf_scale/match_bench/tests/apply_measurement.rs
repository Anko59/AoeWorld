//! Bounded in-process census; run explicitly with the focused harness target.
use super::{bots, build, run};
use aoe_simulation::{PlanningProbe, TickPhase};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Default)]
struct Probe(Mutex<BTreeMap<&'static str, (u64, u64)>>);
struct Span(Arc<Probe>, &'static str, Instant);
impl aoe_map::NavigationSpan for Span {}
impl Drop for Span {
    fn drop(&mut self) {
        let mut values = self.0.0.lock().unwrap();
        let value = values.entry(self.1).or_default();
        value.0 += 1;
        value.1 += self.2.elapsed().as_nanos() as u64;
    }
}
#[derive(Debug)]
struct Observer(Arc<Probe>);
impl PlanningProbe for Observer {
    fn begin(&self) {}
    fn end(&self) {}
    fn movement_count(&self, name: &'static str, count: usize) {
        self.0.0.lock().unwrap().entry(name).or_default().0 += count as u64;
    }
    fn movement_span(&self, name: &'static str) -> Option<Box<dyn aoe_map::NavigationSpan>> {
        matches!(
            name,
            "body_sweep" | "sidestep" | "settling" | "apply_loop" | "group_apply"
        )
        .then(|| Box::new(Span(self.0.clone(), name, Instant::now())) as Box<_>)
    }
}

fn output_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.cache/apply-less")
}

fn progress(scale: f64, workers: usize, tick: u64) {
    std::fs::create_dir_all(output_dir()).unwrap();
    std::fs::write(
        output_dir().join("progress.json"),
        json!({"scale": scale, "workers": workers, "tick": tick}).to_string(),
    )
    .unwrap();
}

#[test]
#[ignore = "explicit 100-seat profiling; run late_game_apply_census explicitly"]
fn late_game_apply_census() {
    let baseline_path = output_dir().join("before.json");
    let baseline: Option<Vec<serde_json::Value>> = match std::fs::read(baseline_path) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).unwrap()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("baseline read failed: {error}"),
    };
    if let Some(before) = &baseline {
        assert_eq!(before.len(), 4);
    }
    let mut reports = Vec::new();
    for scale in [1.0, 5.0] {
        for workers in [1, 4] {
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let mut report = pool.install(|| {
                let load = std::fs::read_to_string("/proc/loadavg").unwrap_or_default();
                progress(scale, workers, 0);
                let setup = Instant::now();
                let mut built = build::build(&build::Shape {
                    seats: 100,
                    scale,
                    side_tiles: 0,
                    seed: 1,
                })
                .unwrap();
                let setup_s = setup.elapsed().as_secs_f64();
                let mut counts = bots::BotCounts::default();
                let probe = Arc::new(Probe::default());
                let mut hashes = Vec::new();
                let mut movement = Vec::new();
                for tick in 1..=150 {
                    // Plain wall window first; profiling is deliberately separate.
                    if tick == 126 {
                        built
                            .world
                            .set_planning_probe(Arc::new(Observer(probe.clone())));
                    }
                    let mut last = Instant::now();
                    built.world.advance_with_probe(&mut |_| true, &mut |phase| {
                        let now = Instant::now();
                        if phase == TickPhase::Movement && (101..=125).contains(&tick) {
                            movement.push((now - last).as_nanos() as f64 / 1e6);
                        }
                        last = now;
                    });
                    bots::step(&mut built.world, &built.seats, &built.generals, &mut counts);
                    if tick % 10 == 0 {
                        progress(scale, workers, tick);
                        hashes.push(run::hex(&built.world));
                    }
                }
                let average = movement.iter().sum::<f64>() / movement.len() as f64;
                assert_eq!(movement.len(), 25);
                movement.sort_by(f64::total_cmp);
                json!({ "scale": scale, "workers": workers,
                    "units": built.world.unit_count(), "hashes": hashes, "load": load, "setup_s": setup_s,
                    "movement_mean_ms": average, "movement_p50_ms": movement[12],
                    "profile_ticks": 25, "profile": *probe.0.lock().unwrap() })
            });
            let baseline_equal = baseline.as_ref().map(|before| {
                let matching: Vec<_> = before
                    .iter()
                    .filter(|run| run["scale"] == scale && run["workers"] == workers)
                    .collect();
                assert_eq!(
                    matching.len(),
                    1,
                    "baseline must name this scale/worker once"
                );
                assert_eq!(
                    matching[0]["hashes"], report["hashes"],
                    "baseline divergence at scale {scale}, workers {workers}"
                );
                true
            });
            report["baseline_hashes_equal"] = json!(baseline_equal);
            eprintln!("apply census: {report}");
            if workers == 4 {
                let previous: &serde_json::Value = reports.last().unwrap();
                assert_eq!(previous["hashes"], report["hashes"]);
            }
            reports.push(report);
            std::fs::create_dir_all(output_dir()).unwrap();
            std::fs::write(
                output_dir().join("latest.json"),
                serde_json::to_vec_pretty(&reports).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn census_counts_a_multi_segment_fallback_once_without_changing_the_tick() {
    use aoe_core::{PlayerId, Seed, WorldConfig, WorldPosition};
    use aoe_simulation::GameWorld;
    let build = || {
        let mut config = WorldConfig::new(256, 256, Seed(1)).unwrap();
        config.move_speed_subunits_per_tick = 600;
        config.move_speed_subunits_per_tick_denominator = 1;
        let mut world = GameWorld::new(config).unwrap();
        let id = world
            .spawn_unit(PlayerId(0), WorldPosition::new(20500, 20500))
            .unwrap();
        world
            .issue_move(id, WorldPosition::new(80500, 20500))
            .unwrap();
        world
    };
    let mut reference = build();
    let mut measured = build();
    let probe = Arc::new(Probe::default());
    measured.set_planning_probe(Arc::new(Observer(probe.clone())));
    for _ in 0..4 {
        assert_eq!(reference.advance(), measured.advance());
        assert_eq!(reference.canonical_hash(), measured.canonical_hash());
    }
    assert_eq!(probe.0.lock().unwrap()["applied"].0, 4);
}
