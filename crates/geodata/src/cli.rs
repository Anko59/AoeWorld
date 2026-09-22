use aoe_geodata::{
    DEFAULT_CACHE_QUOTA_BYTES, DEFAULT_JOB_ACQUISITION_BUDGET_BYTES, DemResolution, DownloadPolicy,
    GeneratedMap, REQUIRED_OVERVIEW_SOURCE_IDS, SourceCache, WorkerRequest, WorkerResponse,
    execute, overview_sources,
};
use aoe_map::{MAP_SCHEMA_VERSION, MapPackage, MapRequest};
use serde::Serialize;
use std::{
    env,
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Instant,
};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;
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
        "map-generate-detailed" => map_generate_detailed(),
        "map-verify" => map_verify(),
        "map-perf" => map_perf(),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: aoe-map-worker [bootstrap|verify|map-estimate|map-generate|map-generate-detailed|map-verify|map-perf]"
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
    let sources = overview_sources().map_err(|error| error.to_string())?;
    print_acquisition_estimate(&cache, &sources)?;
    for source in sources {
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
    let cache = cache()?;
    let sources = overview_sources().map_err(|error| error.to_string())?;
    print_acquisition_estimate(&cache, &sources)?;
    let response = execute(WorkerRequest::PrepareOverviewDirectory {
        cache_root: cache_root(),
        output_directory: output,
        request,
        samples_per_axis: OVERVIEW_SAMPLES_PER_AXIS,
    })
    .map_err(|error| error.to_string())?;
    let WorkerResponse::PreparedDirectory { package } = response else {
        return Err("map worker returned an unexpected directory response".to_owned());
    };
    println!("generated {}", package.content_hash_hex());
    Ok(())
}

fn map_generate_detailed() -> Result<(), String> {
    let request = read_request()?;
    let output = package_path()?;
    let samples_per_axis = env::var("AOE_MAP_SAMPLES")
        .unwrap_or_else(|_| "128".to_owned())
        .parse::<u16>()
        .map_err(|_| "AOE_MAP_SAMPLES must be an integer".to_owned())?;
    let resolution = match env::var("AOE_MAP_DEM_RESOLUTION").as_deref() {
        Ok("glo90") | Err(_) => DemResolution::Glo90,
        Ok("glo30") | Ok("glo30_prefer_glo90") => DemResolution::Glo30PreferGlo90,
        Ok(_) => return Err("AOE_MAP_DEM_RESOLUTION must be glo90 or glo30".to_owned()),
    };
    let response = execute(WorkerRequest::PrepareDetailedDirectory {
        cache_root: cache_root(),
        output_directory: output,
        request,
        samples_per_axis,
        resolution,
        staging_root: None,
    })
    .map_err(|error| error.to_string())?;
    let WorkerResponse::PreparedDirectory { package } = response else {
        return Err("map worker returned an unexpected detailed response".to_owned());
    };
    println!("generated detailed {}", package.content_hash_hex());
    Ok(())
}

fn print_acquisition_estimate(
    cache: &SourceCache,
    sources: &[aoe_geodata::KnownSource],
) -> Result<(), String> {
    let estimate = cache
        .estimate_known_acquisition(sources)
        .map_err(|error| error.to_string())?;
    println!(
        "acquisition estimate: {} source(s), {} cached bytes, {} bytes to download",
        estimate.source_count, estimate.cached_bytes, estimate.download_bytes
    );
    Ok(())
}

fn map_verify() -> Result<(), String> {
    let (root, hash) = package_target()?;
    GeneratedMap::verify_directory(&root, &hash).map_err(|error| error.to_string())?;
    println!("verified {hash}");
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
        let chunk = generator.chunk(x, y).map_err(|error| error.to_string())?;
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

fn package_target() -> Result<(PathBuf, String), String> {
    let path = package_path()?;
    if path.is_file() {
        let hash = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("{} is not a canonical package manifest", path.display()))?;
        return Ok((
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_owned(),
            hash.to_owned(),
        ));
    }
    let mut manifest = None;
    for entry in fs::read_dir(&path).map_err(|error| format!("{}: {error}", path.display()))? {
        let candidate = entry
            .map_err(|error| format!("{}: {error}", path.display()))?
            .path();
        if candidate.extension().is_some_and(|ext| ext == "json")
            && manifest.replace(candidate).is_some()
        {
            return Err(format!(
                "{} must contain exactly one package manifest for map-verify",
                path.display()
            ));
        }
    }
    let Some(manifest) = manifest else {
        return Err(format!(
            "{} must contain exactly one package manifest for map-verify",
            path.display()
        ));
    };
    let hash = manifest
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("{} is not a canonical package manifest", manifest.display()))?;
    Ok((path, hash.to_owned()))
}

fn cache_root() -> PathBuf {
    env::var_os("AOE_GEODATA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".cache/geodata"))
}

fn env_path(name: &str) -> Result<PathBuf, String> {
    env::var_os(name)
        .map(PathBuf::from)
        .ok_or_else(|| format!("{name} must name a path"))
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
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{} exceeds the {} byte input limit",
            path.display(),
            limit
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", path.display()))
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
