//! `match-bench --callgrind`: re-runs this binary under Callgrind (cache
//! simulation on, one rayon worker) for each `--scale`. The child builds the
//! match and plays `--warmup` ticks uninstrumented, then instruments
//! `--counted` ticks and collects only around each `counted_tick`, so world setup
//! and the bots are outside the counts. The profile is split into the movement
//! stages of `perf-stage-counters` plus the whole tick phases (bookkeeping:
//! index relocation, vision, building memory, collect_changed). Instructions
//! and simulated last-level misses only: this machine refuses perf counters.
use super::super::callgrind::{Profile, parse};
use super::super::stages::{costs, row};
use super::super::world::BoxError;
use super::Options;
use serde_json::{Map, Value, json};
use std::{fs, path::Path, process::Command};

/// Whole tick phases, by the function that runs each.
const PHASES: [(&str, &str); 12] = [
    ("phase_movement", "GameWorld>::advance_movement"),
    ("phase_transports", "GameWorld>::advance_transports"),
    ("phase_spacing", "GameWorld>::advance_spacing"),
    ("phase_wildlife", "GameWorld>::advance_wildlife"),
    ("phase_combat", "GameWorld>::advance_combat"),
    ("phase_construction", "GameWorld>::advance_construction"),
    ("phase_gathering", "GameWorld>::advance_gathering"),
    ("phase_training", "GameWorld>::advance_training"),
    ("phase_vassals", "GameWorld>::advance_vassals"),
    ("phase_generals", "GameWorld>::advance_generals"),
    ("phase_stewards", "GameWorld>::advance_stewards"),
    (
        "phase_building_memory",
        "GameWorld>::remember_buildings_this_tick",
    ),
];

/// Bookkeeping rows the plan's phase 1 gate reads (summed in `bookkeeping`).
const BOOKKEEPING: [&str; 7] = [
    "index_collision",
    "index_idle_spacing",
    "index_chunks",
    "index_wildlife_cells",
    "vision",
    "collect_changed",
    "phase_building_memory",
];

fn child_args(options: &Options, scale: f64, out: &Path) -> Vec<String> {
    vec![
        "match-bench".into(),
        "--seats".into(),
        options.seats.to_string(),
        "--scale".into(),
        scale.to_string(),
        "--side-tiles".into(),
        options.side_tiles.to_string(),
        "--seed".into(),
        options.seed.to_string(),
        "--warmup".into(),
        options.warmup.to_string(),
        "--counted".into(),
        options.counted.to_string(),
        "--counted-child".into(),
        out.display().to_string(),
    ]
}

/// The stage report of one profile; `movers` and `ticks` divide.
pub(super) fn split(
    profile: &Profile,
    movers: u64,
    ticks: u64,
) -> Result<Map<String, Value>, String> {
    let mut found = costs(profile)?;
    for (name, pattern) in PHASES {
        found.push((name, profile.inclusive(pattern)));
    }
    let mut book: Option<Vec<u64>> = None;
    for (name, cost) in &found {
        if BOOKKEEPING.contains(name)
            && let Some(cost) = cost
        {
            let sum = book.get_or_insert_with(|| vec![0; profile.events.len()]);
            sum.iter_mut().zip(cost).for_each(|(s, add)| *s += add);
        }
    }
    found.push(("bookkeeping", book));
    let mut stages = Map::new();
    for (name, cost) in &found {
        stages.insert(
            (*name).to_owned(),
            row(profile, cost.as_ref(), movers, ticks)?,
        );
    }
    Ok(stages)
}

fn ir(stages: &Map<String, Value>, name: &str) -> f64 {
    stages[name]["per_tick"]["ir"].as_f64().unwrap_or(0.0) / 1e6
}

fn count(
    options: &Options,
    scale: f64,
    exe: &Path,
    child: &Path,
    out: &Path,
) -> Result<(), BoxError> {
    let status = Command::new("valgrind")
        .arg("--tool=callgrind")
        .arg("--cache-sim=yes")
        .arg("--instr-atstart=no")
        .arg("--collect-atstart=no")
        .arg(format!("--callgrind-out-file={}", out.display()))
        .arg(exe)
        .args(child_args(options, scale, child))
        .status()?;
    if !status.success() {
        return Err(format!("callgrind child at scale {scale} failed: {status}").into());
    }
    Ok(())
}

/// Bind all CLI options and the actual executable, including uncommitted builds.
pub(super) fn cache_key(options: &Options, scale: f64, exe: &Path) -> Result<String, BoxError> {
    let mut hash = blake3::Hasher::new();
    hash.update(&serde_json::to_vec(&(options, scale))?);
    hash.update_reader(fs::File::open(exe)?)?;
    Ok(hash.finalize().to_hex().to_string())
}

pub(super) fn run(options: &Options) -> Result<(), BoxError> {
    let exe = std::env::current_exe()?;
    let dir = options
        .out
        .parent()
        .unwrap_or(Path::new("."))
        .join("callgrind");
    fs::create_dir_all(&dir)?;
    let mut scenarios = Vec::new();
    let mut text = String::from(
        "scale  units  movers  Mi/tick  movement  propose  apply  formations  route  vision  index  bookkeeping  combat  gathering  stewards  Ir/mover(movement)  DRAM MiB/tick\n",
    );
    for &scale in &options.scale {
        let key = cache_key(options, scale, &exe)?;
        let child = dir.join(format!("child-{key}.json"));
        let out = dir.join(format!("callgrind.scale-{key}.out"));
        let complete = dir.join(format!("complete-{key}"));
        // Child JSON is written before Valgrind finishes its profile. Reuse
        // only after the entire process succeeded, so interrupted runs retry.
        if complete.exists() && child.exists() && out.exists() {
            eprintln!("match-bench: reusing {}", out.display());
        } else {
            if complete.exists() {
                fs::remove_file(&complete)?;
            }
            count(options, scale, &exe, &child, &out)?;
            fs::write(&complete, b"")?;
        }
        let meta: Value = serde_json::from_str(&fs::read_to_string(&child)?)?;
        let profile = parse(&fs::read_to_string(&out)?)?;
        let movers = meta["movers_per_tick"].as_u64().unwrap_or(1).max(1);
        let stages = split(&profile, movers, options.counted.max(1))?;
        let index = [
            "index_collision",
            "index_idle_spacing",
            "index_chunks",
            "index_wildlife_cells",
        ]
        .iter()
        .map(|n| ir(&stages, n))
        .sum::<f64>();
        text.push_str(&format!(
            "{:>5} {:>6} {:>7} {:>8.0} {:>9.0} {:>8.0} {:>6.0} {:>11.0} {:>6.0} {:>7.0} {:>6.0} {:>12.0} {:>7.0} {:>10.0} {:>9.0} {:>19.0} {:>14.1}\n",
            scale, meta["units_per_tick"], movers, ir(&stages, "tick"), ir(&stages, "movement"),
            ir(&stages, "propose"), ir(&stages, "apply"), ir(&stages, "formations"),
            ir(&stages, "route_search"), ir(&stages, "vision"), index,
            ir(&stages, "bookkeeping"), ir(&stages, "phase_combat"),
            ir(&stages, "phase_gathering"), ir(&stages, "phase_stewards"),
            stages["movement"]["per_mover"]["ir"].as_f64().unwrap_or(0.0) / options.counted.max(1) as f64,
            stages["tick"]["per_tick"]["dram_bytes"].as_f64().unwrap_or(0.0) / 1048576.0,
        ));
        scenarios.push(
            json!({ "scale": scale, "cache_key": key, "child": meta, "caches": profile.caches,
            "profile": out.display().to_string(), "stages": stages }),
        );
    }
    eprint!("{text}");
    let report = json!({
        "version": 1,
        "source": "callgrind cache simulation (valgrind), one rayon worker, counted_tick only",
        "dram_bytes_per_miss": 64,
        "note": "instructions and simulated LL misses x 64 B, not hardware counters (perf_event_paranoid is 4 here); per_mover divides by the mean active movers over the counted ticks and per_tick by the counted ticks; inclusive stages overlap",
        "seats": options.seats, "warmup": options.warmup, "counted": options.counted,
        "scenarios": scenarios,
        "table": text,
    });
    let path = options.out.with_extension("callgrind.json");
    super::write(&path, &report)?;
    eprintln!("match-bench: wrote {}", path.display());
    Ok(())
}
