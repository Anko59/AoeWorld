// Radius geometry, exact static sweeps and local rejoining intentionally
// change these current pins. Preserve workloads, floors and replay comparisons.
//! Identical deterministic orders for lazy and installed package navigation.
//! Synthetic terrain isolates navigation, not a finished France match.
use super::*;
use aoe_core::{PlayerId, TileCoord, WorldPosition};
use aoe_map::{NavigationPhase, NavigationProbe, NavigationSpan};
use aoe_simulation::{PlanningProbe, TickPhase};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
struct Probe {
    phases: Arc<Mutex<[(u64, u128); 8]>>,
    planning: Mutex<(Option<Instant>, u128)>,
}
struct Span(Arc<Mutex<[(u64, u128); 8]>>, usize, Instant);
impl NavigationSpan for Span {}
impl Drop for Span {
    fn drop(&mut self) {
        let mut phases = self.0.lock().unwrap();
        phases[self.1].0 += 1;
        phases[self.1].1 += self.2.elapsed().as_nanos();
    }
}
impl NavigationProbe for Probe {
    fn begin(&self, phase: NavigationPhase) -> Box<dyn NavigationSpan> {
        Box::new(Span(self.phases.clone(), phase as usize, Instant::now()))
    }
}
impl PlanningProbe for Probe {
    fn begin(&self) {
        self.planning.lock().unwrap().0 = Some(Instant::now());
    }
    fn end(&self) {
        let mut state = self.planning.lock().unwrap();
        if let Some(start) = state.0.take() {
            state.1 += start.elapsed().as_nanos();
        }
    }
}

fn summary(mut values: Vec<f64>) -> serde_json::Value {
    values.sort_by(f64::total_cmp);
    serde_json::json!({
        "p50_ms": values[values.len() / 2],
        "p95_ms": values[(values.len() * 95 / 100).min(values.len() - 1)],
        "mean_ms": values.iter().sum::<f64>() / values.len() as f64,
    })
}

fn measure(installed: bool) {
    let mut world = fixture::flat_world(2048);
    let Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    let directory = tempfile::tempdir().unwrap();
    if installed {
        prepare(generator, [91; 32], Some(directory.path()), &|| false).unwrap();
    }
    world.set_fog_disabled(true);
    world.prepare_navigation(&|| false).unwrap();
    let mut units = Vec::new();
    for player in 0..100 {
        let base = TileCoord::new(40 + player % 10 * 200, 40 + player / 10 * 200);
        for slot in 0..70 {
            let tile = TileCoord::new(base.x + slot % 10, base.y + slot / 10);
            let position = WorldPosition::from_tile_center(tile).unwrap();
            let id = world.spawn_unit(PlayerId(player as u16), position).unwrap();
            units.push((id, tile));
        }
    }
    let probe = Arc::new(Probe::default());
    if let Terrain::Map { generator, .. } = world.terrain() {
        generator.set_navigation_probe(probe.clone());
    }
    world.set_planning_probe(probe.clone());
    let (mut advance, mut movement, mut planning) = (Vec::new(), Vec::new(), Vec::new());
    let mut tick_phases = [(0u64, 0u128); 8];
    for tick in 0..160 {
        // Keep geographically dispersed cold routes in flight, with identical
        // work/order counts in both modes. Order admission is outside advance.
        for offset in 0..20 {
            let at = (tick * 43 + offset * 347) % units.len();
            let (id, tile) = units[at];
            let goal = TileCoord::new((tile.x + 701) % 2000 + 20, (tile.y + 503) % 2000 + 20);
            world
                .issue_move(id, WorldPosition::from_tile_center(goal).unwrap())
                .unwrap();
        }
        probe.planning.lock().unwrap().1 = 0;
        let phase_start = *probe.phases.lock().unwrap();
        let begin = Instant::now();
        world.advance_with_probe(&mut |_| true, &mut |phase| {
            if phase == TickPhase::Movement {
                movement.push(begin.elapsed().as_secs_f64() * 1000.0);
            }
        });
        advance.push(begin.elapsed().as_secs_f64() * 1000.0);
        planning.push(probe.planning.lock().unwrap().1 as f64 / 1e6);
        for ((total, end), start) in tick_phases
            .iter_mut()
            .zip(*probe.phases.lock().unwrap())
            .zip(phase_start)
        {
            total.0 += end.0 - start.0;
            total.1 += end.1 - start.1;
        }
    }
    let phases = *probe.phases.lock().unwrap();
    let Terrain::Map { generator, .. } = world.terrain() else {
        panic!()
    };
    // Pin the current authoritative result in both physical cache modes. Re-pinned
    // for the bundled economy reservations, Vassal and walking changes, which alter
    // this workload's gatherer and route outcomes; both cache modes still agree.
    // Swept avoidance adds current obstruction/arrival state; friendly
    // squeeze-through and worksite-only gather redirects change it again. Regenerated with
    // `make test-native-focused AOE_NATIVE_TEST_PACKAGE=aoe-server
    // AOE_NATIVE_TEST_FILTER='test(large_navigation_)'`; both cache modes retain
    // their identical workload, canonical state and no-construction assertions.
    assert_eq!(
        world.canonical_hash(),
        [
            // PR 7 removes follower waits/index hashing; both navigation modes reproduce this pin.
            137, 242, 73, 125, 23, 111, 44, 191, 216, 158, 22, 49, 30, 111, 140, 218, 148, 153, 179,
            7, 37, 62, 53, 106, 202, 74, 10, 198, 226, 8, 175, 60,
        ]
    );
    if installed {
        for phase in [0, 1, 3, 4] {
            assert_eq!(
                phases[phase].0, 0,
                "static navigation was constructed in the measured loop"
            );
        }
    }
    let revision = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let names = [
        "terrain",
        "terrain_point",
        "walk_cache",
        "portals",
        "portal_costs",
        "components",
        "hierarchy",
        "pocket",
    ];
    let advance_navigation: serde_json::Map<String, serde_json::Value> = names
        .iter()
        .zip(tick_phases)
        .map(|(name, (count, ns))| {
            (
                name.to_string(),
                serde_json::json!({"count": count, "total_ms": ns as f64 / 1e6}),
            )
        })
        .collect();
    let report = serde_json::json!({
        "revision": String::from_utf8_lossy(&revision.stdout).trim(),
        "navigation_format": NAVIGATION_FORMAT_VERSION, "debug_assertions": cfg!(debug_assertions),
        "installed": installed, "side_tiles": 2048, "players": 100, "units": world.unit_count(),
        "ticks": 160, "advance": summary(advance), "movement": summary(movement), "planning": summary(planning),
        "navigation_phase_count_and_ms": phases.map(|(n, ns)| (n, ns as f64 / 1e6)),
        "advance_navigation": advance_navigation,
        "static_usage": generator.static_navigation_usage(), "walk_cache": generator.walk_cache_usage(),
        "portal_cache": generator.portal_cache_usage(), "hash": format!("{:x?}", world.canonical_hash()),
        "process_status": fs::read_to_string("/proc/self/status").unwrap_or_default().lines()
            .filter(|line| line.starts_with("VmRSS:") || line.starts_with("VmHWM:")).collect::<Vec<_>>(),
    });
    let output = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../reports/navigation");
    fs::create_dir_all(&output).unwrap();
    let name = if installed { "static" } else { "lazy" };
    fs::write(
        output.join(format!("{name}.json")),
        serde_json::to_string_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("navigation perf {name}: {report}");
}

#[test]
fn large_navigation_lazy_tick_measurement() {
    measure(false);
}

#[test]
fn large_navigation_static_tick_measurement() {
    measure(true);
}
