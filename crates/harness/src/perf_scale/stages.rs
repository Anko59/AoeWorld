//! `perf-stage-counters`: splits the Callgrind profiles of the group
//! benchmark (`crates/simulation/benches/group_instructions.rs`) into movement
//! stages: instructions and last-level cache misses per stage, per mover and
//! per tick. Misses come from Callgrind's cache simulation (`ILmr + DLmr +
//! DLmw`), so the bytes are simulated DRAM traffic at 64 B a miss, never a
//! hardware counter. Synthetic: one workload, one thread, a simulated cache.
use super::callgrind::{Cost, Profile, parse};
use super::world::BoxError;
use clap::Args;
use serde_json::{Map, Value, json};
use std::{fs, path::PathBuf};

const LINE_BYTES: u64 = 64;
const LAST_LEVEL_MISSES: [&str; 3] = ["ILmr", "DLmr", "DLmw"];

#[derive(Args, Debug, Clone)]
pub struct Options {
    /// The benchmark group's output directory (one subdirectory per benchmark).
    #[arg(
        long,
        default_value = "target/gungraun/aoe-simulation/group_instructions/simulation"
    )]
    dir: PathBuf,
    #[arg(long, default_value = "reports/perf/group-stages.json")]
    out: PathBuf,
}

/// How a stage's cost is found in a profile.
enum Source {
    /// Everything the whole counted window ran.
    Window,
    /// Inclusive cost of the functions with this trailing path.
    Function(&'static str),
    /// Inclusive cost of the calls from one function to another.
    Calls(&'static str, &'static str),
    /// A function's inclusive cost less the named stages it contains.
    Remainder(&'static str, &'static [&'static str]),
    /// The named stages, which do not overlap.
    Sum(&'static [&'static str]),
}
use Source::{Calls, Function, Remainder, Sum, Window};

/// Stages in dependency order. Inclusive stages overlap their callees: the
/// sum of all rows is not the tick. `movement` is the whole `advance_movement`.
const STAGES: [(&str, Source, &str); 25] = [
    ("tick", Window, "everything the counted tick ran"),
    (
        "movement",
        Function("GameWorld>::advance_movement"),
        "the movement phase",
    ),
    (
        "propose",
        Calls(
            "GameWorld>::advance_movement",
            "GameWorld>::propose_steps_into",
        ),
        "parallel step proposals of the individual movers",
    ),
    (
        "group_prepare",
        Function("GameWorld>::prepare_group_targets"),
        "formation targets before the group proposals",
    ),
    (
        "group_propose",
        Calls(
            "GameWorld>::advance_formations",
            "GameWorld>::propose_steps_into",
        ),
        "parallel step proposals of formation members",
    ),
    (
        "group_apply",
        Calls("GameWorld>::advance_formations", "GameWorld>::apply_step"),
        "ordered apply of formation members",
    ),
    (
        "formations",
        Function("GameWorld>::advance_formations"),
        "all formation work, group stages included",
    ),
    (
        "route_serve",
        Function("GameWorld>::serve_route_searches"),
        "fair route searches",
    ),
    (
        "route_ahead",
        Function("GameWorld>::plan_routes_ahead"),
        "lookahead route searches",
    ),
    (
        "route_search",
        Sum(&["route_serve", "route_ahead"]),
        "route search in the movement phase",
    ),
    (
        "yields",
        Function("GameWorld>::advance_yields"),
        "yield requests",
    ),
    (
        "engage",
        Function("GameWorld>::engage_formations"),
        "attack-move looks",
    ),
    (
        "rejoin",
        Function("GameWorld>::rejoin_detached_members"),
        "detached members rejoining",
    ),
    (
        "resume",
        Function("GameWorld>::resume_queued_moves"),
        "queued moves resumed",
    ),
    (
        "vision_update",
        Function("GameWorld>::update_unit_vision"),
        "per-step observer updates",
    ),
    (
        "vision_flush",
        Function("GameWorld>::flush_vision_batch"),
        "batched sight changes, per player",
    ),
    (
        "vision",
        Sum(&["vision_update", "vision_flush"]),
        "vision updates",
    ),
    (
        "apply",
        Remainder(
            "GameWorld>::advance_movement",
            &[
                "propose",
                "formations",
                "route_search",
                "yields",
                "engage",
                "rejoin",
                "resume",
                "vision_flush",
            ],
        ),
        "the ordered apply loop of the individual movers, slow paths included",
    ),
    (
        "sidestep",
        Function("GameWorld>::avoid_unit_step"),
        "sidestepping around bodies (inside propose and apply)",
    ),
    (
        "refinement",
        Function("GameWorld>::local_search"),
        "17x17 local refinement searches (inside propose and apply)",
    ),
    (
        "index_collision",
        Function("CollisionIndex>::relocate"),
        "collision cell index relocation",
    ),
    (
        "index_idle_spacing",
        Function("IdleSpacing>::relocate"),
        "idle spacing index relocation",
    ),
    (
        "index_chunks",
        Function("GameWorld>::move_bucket"),
        "chunk bucket index relocation",
    ),
    (
        "index_wildlife_cells",
        Remainder(
            "GameWorld>::track_unit_step",
            &["index_collision", "index_idle_spacing"],
        ),
        "wildlife unit-cell mirror relocation",
    ),
    (
        "collect_changed",
        Function("GameWorld>::collect_changed"),
        "changed-unit collation and sort",
    ),
];

/// Benchmarks the report covers: directory and callgrind file name.
const SCENARIOS: [&str; 2] = ["stages_steady", "stages_start"];

fn describe(scenario: &str) -> String {
    let (movers, warmup) = (
        super::groups_workload::DEFAULT_MOVERS,
        super::groups_workload::STEADY_WARMUP_TICKS,
    );
    match scenario {
        "stages_steady" => format!(
            "tick {} with {movers} movers under way: no route search, no planning",
            warmup + 1
        ),
        _ => format!(
            "the first tick after the orders of {movers} movers: planning and route searches"
        ),
    }
}

fn misses(profile: &Profile, cost: &[u64]) -> Result<u64, String> {
    let mut sum = 0;
    for event in LAST_LEVEL_MISSES {
        sum += cost.get(profile.event(event)?).copied().unwrap_or(0);
    }
    Ok(sum)
}

fn instructions(profile: &Profile, cost: &[u64]) -> Result<u64, String> {
    Ok(cost.get(profile.event("Ir")?).copied().unwrap_or(0))
}

/// Stage costs by name; `None` is a stage no function of the window ran.
fn costs(profile: &Profile) -> Result<Vec<(&'static str, Option<Cost>)>, String> {
    let mut found: Vec<(&'static str, Option<Cost>)> = Vec::new();
    let value = |found: &[(&str, Option<Cost>)], name: &str| {
        found
            .iter()
            .find(|(stage, _)| *stage == name)
            .and_then(|(_, cost)| cost.clone())
    };
    for (name, source, _) in &STAGES {
        let cost = match source {
            Window => Some(profile.totals.clone()),
            Function(pattern) => profile.inclusive(pattern),
            Calls(caller, callee) => profile.edge(caller, callee),
            Remainder(pattern, inner) => match profile.inclusive(pattern) {
                Some(mut cost) => {
                    for stage in *inner {
                        for (left, taken) in cost
                            .iter_mut()
                            .zip(value(&found, stage).unwrap_or_default())
                        {
                            *left = left.checked_sub(taken).ok_or_else(|| {
                                format!("stage {name}: {stage} exceeds its parent")
                            })?;
                        }
                    }
                    Some(cost)
                }
                None => None,
            },
            Sum(parts) => {
                let mut sum = vec![0; profile.events.len()];
                let mut any = false;
                for part in *parts {
                    if let Some(cost) = value(&found, part) {
                        any = true;
                        for (total, add) in sum.iter_mut().zip(cost) {
                            *total += add;
                        }
                    }
                }
                any.then_some(sum)
            }
        };
        found.push((name, cost));
    }
    Ok(found)
}

fn row(profile: &Profile, cost: Option<&Cost>, movers: u64, ticks: u64) -> Result<Value, String> {
    let Some(cost) = cost else {
        return Ok(json!({ "present": false, "ir": 0, "ll_misses": 0, "dram_bytes": 0 }));
    };
    let (ir, ll) = (instructions(profile, cost)?, misses(profile, cost)?);
    let div = |value: u64, by: u64| (value as f64 / by as f64 * 1000.0).round() / 1000.0;
    Ok(json!({
        "present": true,
        "ir": ir,
        "ll_misses": ll,
        "dram_bytes": ll * LINE_BYTES,
        "per_mover": { "ir": div(ir, movers), "ll_misses": div(ll, movers),
            "dram_bytes": div(ll * LINE_BYTES, movers) },
        "per_tick": { "ir": div(ir, ticks), "ll_misses": div(ll, ticks),
            "dram_bytes": div(ll * LINE_BYTES, ticks) },
    }))
}

/// The report for profiles read from `(benchmark, description, file text)`.
pub(super) fn report(profiles: &[(&str, String, String)]) -> Result<Value, String> {
    let (movers, ticks) = (u64::from(super::groups_workload::DEFAULT_MOVERS), 1);
    let mut scenarios = Vec::new();
    let mut caches = Vec::new();
    for (name, description, text) in profiles {
        let profile = parse(text).map_err(|error| format!("{name}: {error}"))?;
        let mut stages = Map::new();
        for (stage, cost) in costs(&profile)? {
            stages.insert(
                stage.to_owned(),
                row(&profile, cost.as_ref(), movers, ticks)?,
            );
        }
        caches = profile.caches.clone();
        scenarios.push(json!({ "name": name, "description": description,
            "ticks": ticks, "movers": movers, "stages": stages }));
    }
    let described = STAGES
        .iter()
        .map(|(name, _, what)| ((*name).to_owned(), json!(what)))
        .collect::<Map<_, _>>();
    Ok(json!({
        "version": 1,
        "source": "callgrind cache simulation (valgrind), one rayon worker",
        "ll_miss_events": LAST_LEVEL_MISSES,
        "dram_bytes_per_miss": LINE_BYTES,
        "simulated_caches": caches,
        "stage_descriptions": described,
        "note": "inclusive stages overlap their callees; a stage with present=false ran no function of that name in the window (0 measured, not skipped). Simulated misses, not hardware counters: perf_event_paranoid refuses those here.",
        "scenarios": scenarios,
    }))
}

pub fn run(options: Options) -> Result<(), BoxError> {
    let mut profiles = Vec::new();
    for name in SCENARIOS {
        let path = options.dir.join(name).join(format!("callgrind.{name}.out"));
        let text = fs::read_to_string(&path).map_err(|error| {
            format!(
                "{}: {error}; run make perf-groups-instructions",
                path.display()
            )
        })?;
        profiles.push((name, describe(name), text));
    }
    let report = report(&profiles)?;
    if let Some(parent) = options.out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&options.out, serde_json::to_string_pretty(&report)? + "\n")?;
    eprintln!("perf-stage-counters: wrote {}", options.out.display());
    Ok(())
}

#[cfg(test)]
mod tests;
