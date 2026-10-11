//! Repeated production chunk generation, excluding package loading and warmup.
use super::*;
use aoe_map::MapChunkGenerator;
use std::{collections::BTreeMap, hint::black_box, process::Command};

const BATCH: usize = 16;

#[derive(Serialize)]
struct Report {
    schema_version: u8,
    profile: &'static str,
    cpu: String,
    revision: String,
    dirty: bool,
    toolchain: String,
    evidence_class: &'static str,
    content_hash: Option<String>,
    coordinate: [i32; 2],
    tiles_per_chunk: usize,
    materials: BTreeMap<String, usize>,
    chunks_per_sample: usize,
    sample_nanoseconds: Vec<u64>,
    median_nanoseconds_per_chunk: u64,
}

fn command(args: &[&str]) -> Result<String, String> {
    let output = Command::new(args[0])
        .args(&args[1..])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!("{} failed", args[0]));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}

pub(super) fn run() -> Result<(), String> {
    let samples = env::var("AOE_MAP_SAMPLES")
        .map_err(|e| e.to_string())?
        .parse::<usize>()
        .map_err(|_| "AOE_MAP_SAMPLES must be 3..=200")?;
    if !(3..=200).contains(&samples) {
        return Err("AOE_MAP_SAMPLES must be 3..=200".into());
    }
    let (generator, coordinate, class, hash) = if env::var_os("AOE_MAP_PACKAGE").is_some() {
        let (root, hash) = package_target()?;
        let map = GeneratedMap::read_directory(&root, &hash).map_err(|e| e.to_string())?;
        let center =
            i32::try_from(map.package.chunk_count_per_side() / 2).map_err(|e| e.to_string())?;
        let generator = map
            .package
            .generator_with_environment(
                map.elevation_pages,
                map.water_pages,
                map.vegetation_pages,
                map.historical_land_use_pages,
            )
            .map_err(|e| e.to_string())?;
        (
            generator,
            [center, center],
            "prepared_resident_chunk",
            Some(hash),
        )
    } else {
        (
            MapChunkGenerator::new([17; 32], 1, 128),
            [1, 1],
            "synthetic_chunk_fixture",
            None,
        )
    };
    let generate = || {
        generator
            .landscape_chunk_with_cancel(coordinate[0], coordinate[1], &|| false)
            .map_err(|e| e.to_string())
    };
    let first = generate()?;
    let mut materials = BTreeMap::new();
    for tile in &first.tiles {
        *materials
            .entry(format!("{:?}", tile.terrain.material))
            .or_default() += 1;
    }
    for _ in 0..BATCH {
        black_box(generate()?);
    }
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        for _ in 0..BATCH {
            black_box(generate()?);
        }
        times.push(u64::try_from(start.elapsed().as_nanos()).map_err(|e| e.to_string())?);
    }
    let mut sorted = times.clone();
    sorted.sort_unstable();
    let report = Report {
        schema_version: 1,
        profile: if cfg!(debug_assertions) {
            "dev"
        } else {
            "release"
        },
        cpu: fs::read_to_string("/proc/cpuinfo")
            .map_err(|e| e.to_string())?
            .lines()
            .find(|line| line.starts_with("model name"))
            .unwrap_or("unavailable")
            .to_owned(),
        revision: command(&["git", "rev-parse", "HEAD"])?,
        dirty: !command(&["git", "status", "--porcelain"])?.is_empty(),
        toolchain: command(&["rustc", "--version"])?,
        evidence_class: class,
        content_hash: hash,
        coordinate,
        tiles_per_chunk: first.tiles.len(),
        materials,
        chunks_per_sample: BATCH,
        sample_nanoseconds: times,
        median_nanoseconds_per_chunk: sorted[samples / 2] / BATCH as u64,
    };
    fs::create_dir_all("reports/perf").map_err(|e| e.to_string())?;
    fs::write(
        "reports/perf/map-chunk.json",
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::write("reports/perf/map-chunk.md", format!("# Chunk generation timing\n\nRevision: `{}`; dirty: {}; profile: {}; toolchain: {}.\n\n{} at {:?}: {} tiles, {} samples of {} chunks; median {} ns/chunk.\n\nPackage loading and warmup are excluded. Resident source pages and a synthetic fixture do not qualify server paging, finished RTS performance or dedicated hardware. Raw samples: `map-chunk.json`.\n", report.revision, report.dirty, report.profile, report.toolchain, report.evidence_class, report.coordinate, report.tiles_per_chunk, report.sample_nanoseconds.len(), BATCH, report.median_nanoseconds_per_chunk)).map_err(|e| e.to_string())?;
    print_json(&report)
}
