use aoe_geodata::{
    DEFAULT_CACHE_QUOTA_BYTES, DEFAULT_JOB_ACQUISITION_BUDGET_BYTES, DownloadPolicy, GeneratedMap,
    REQUIRED_OVERVIEW_SOURCE_IDS, SourceCache, overview_sources, prepare_overview,
};
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_PACKAGE_BYTES: u64 = 2 * 1024 * 1024;
const OVERVIEW_SAMPLES_PER_AXIS: u16 = 128;

pub fn run(arguments: &[OsString]) -> Result<(), String> {
    let Some(command) = arguments.first().and_then(|argument| argument.to_str()) else {
        return Err(usage());
    };
    if arguments.len() != 1 {
        return Err(usage());
    }
    match command {
        "bootstrap" => bootstrap(),
        "verify" => verify_sources(),
        "map-estimate" => map_estimate(),
        "map-generate" => map_generate(),
        "map-verify" => map_verify(),
        "map-perf" => map_perf(),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: aoe-map-worker [bootstrap|verify|map-estimate|map-generate|map-verify|map-perf]"
        .to_owned()
}

fn cache() -> Result<SourceCache, String> {
    SourceCache::new(
        env::var_os("AOE_GEODATA_CACHE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".cache/geodata")),
        DownloadPolicy {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: DEFAULT_JOB_ACQUISITION_BUDGET_BYTES,
        },
    )
    .map_err(|error| error.to_string())
}

fn bootstrap() -> Result<(), String> {
    let cache = cache()?;
    let cancelled = AtomicBool::new(false);
    for source in overview_sources().map_err(|error| error.to_string())? {
        let lock = cache
            .acquire_known(&source, &cancelled)
            .map_err(|error| error.to_string())?;
        println!("verified {} ({})", lock.id, lock.sha256);
    }
    Ok(())
}

fn verify_sources() -> Result<(), String> {
    let cache = cache()?;
    let mut missing = Vec::new();
    for id in REQUIRED_OVERVIEW_SOURCE_IDS {
        let Some(lock) = cache.known_lock(id).map_err(|error| error.to_string())? else {
            missing.push((*id).to_owned());
            continue;
        };
        if !cache
            .is_verified(&lock)
            .map_err(|error| error.to_string())?
        {
            missing.push((*id).to_owned());
        }
    }
    if missing.is_empty() {
        println!(
            "verified {} source-backed overview inputs",
            REQUIRED_OVERVIEW_SOURCE_IDS.len()
        );
        Ok(())
    } else {
        Err(format!(
            "offline source verification is incomplete; missing or corrupt: {}",
            missing.join(", ")
        ))
    }
}

fn map_estimate() -> Result<(), String> {
    let request = read_request()?;
    let estimate = request.estimate().map_err(|error| error.to_string())?;
    print_json(&estimate)
}

fn map_generate() -> Result<(), String> {
    let request = read_request()?;
    let output = package_path()?;
    let prepared = prepare_overview(cache_root(), request, OVERVIEW_SAMPLES_PER_AXIS)
        .map_err(|error| error.to_string())?;
    let generated =
        GeneratedMap::from_prepared(request, prepared).map_err(|error| error.to_string())?;
    generated.validate().map_err(|error| error.to_string())?;
    atomic_write_json(&output, &generated)?;
    println!("generated {}", generated.package.content_hash_hex());
    Ok(())
}

fn map_verify() -> Result<(), String> {
    let path = package_path()?;
    let generated = read_json::<GeneratedMap>(&path, MAX_PACKAGE_BYTES)?;
    generated.validate().map_err(|error| error.to_string())?;
    println!("verified {}", generated.package.content_hash_hex());
    Ok(())
}

fn map_perf() -> Result<(), String> {
    let request = read_request()?;
    let package = MapPackage::new(MAP_SCHEMA_VERSION, request, Vec::new())
        .map_err(|error| error.to_string())?;
    let chunks = package.chunk_count_per_side();
    let coordinates = [
        (0, 0),
        (chunks.saturating_sub(1), 0),
        (0, chunks.saturating_sub(1)),
        (chunks.saturating_sub(1), chunks.saturating_sub(1)),
        (chunks / 2, chunks / 2),
    ];
    let generator = package.generator();
    let started = Instant::now();
    let mut tiles = 0_usize;
    let mut resources = 0_usize;
    for (x, y) in coordinates {
        let x = i32::try_from(x).map_err(|_| "chunk coordinate exceeds i32")?;
        let y = i32::try_from(y).map_err(|_| "chunk coordinate exceeds i32")?;
        let chunk = generator.chunk(x, y);
        tiles = tiles.saturating_add(chunk.tiles.len());
        resources = resources.saturating_add(chunk.resources.len());
    }
    print_json(&MapPerformance {
        evidence_class: "synthetic_fallback_sampling",
        content_hash: package.content_hash_hex(),
        chunks_sampled: coordinates.len(),
        tiles_sampled: tiles,
        resources_sampled: resources,
        elapsed_milliseconds: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn read_request() -> Result<MapRequest, String> {
    let path = env_path("AOE_MAP_REQUEST")?;
    read_json::<MapRequest>(&path, MAX_REQUEST_BYTES)?
        .normalized()
        .map_err(|error| error.to_string())
}

fn package_path() -> Result<PathBuf, String> {
    env_path("AOE_MAP_PACKAGE")
}

fn cache_root() -> PathBuf {
    env::var_os("AOE_GEODATA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".cache/geodata"))
}

fn env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} must name a file"))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path, limit: u64) -> Result<T, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if metadata.len() > limit {
        return Err(format!(
            "{} exceeds the {} byte input limit",
            path.display(),
            limit
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
}

fn atomic_write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    if bytes.len() > usize::try_from(MAX_PACKAGE_BYTES).unwrap_or(usize::MAX) {
        return Err(format!(
            "generated package exceeds the {MAX_PACKAGE_BYTES} byte limit"
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no parent directory", path.display()))?;
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("map"),
        std::process::id()
    ));
    fs::write(&temporary, bytes).map_err(|error| format!("{}: {error}", temporary.display()))?;
    fs::rename(&temporary, path).map_err(|error| format!("{}: {error}", path.display()))
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

#[derive(Serialize)]
struct MapPerformance {
    evidence_class: &'static str,
    content_hash: String,
    chunks_sampled: usize,
    tiles_sampled: usize,
    resources_sampled: usize,
    elapsed_milliseconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_fallback_package_is_self_contained() {
        let generated = GeneratedMap {
            package: MapPackage::new(MAP_SCHEMA_VERSION, MapRequest::default(), Vec::new())
                .expect("package"),
            elevation_pages: Vec::new(),
            water_pages: Vec::new(),
            vegetation_pages: Vec::new(),
            historical_land_use_pages: Vec::new(),
        };
        generated.validate().expect("self-contained package");
    }
}
