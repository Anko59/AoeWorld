use super::{
    CHUNK_CACHE_CAPACITY, CHUNK_TILES, CaptureCase, ElevationPage, EvictionTarget,
    MAX_PACKAGE_PAGE_BYTES, MAX_PACKAGE_PAGES, Result, StagedPackages, copy_tree, measure_tree,
    read_elevation_range, read_historical_coverage, read_water_evidence,
};
use aoe_map::{MapPackage, MapRequest, Ratio};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const EVICTION_COMPRESSION: u32 = 20;
const EVICTION_TILES_PER_SIDE: u32 = 750;
const MIN_PACKAGE_RELIEF_CENTIMETERS: i32 = 150_000;
const MIN_CHUNK_SOURCE_RELIEF_CENTIMETERS: i32 = 15_000;

pub(super) fn prepare(
    root: &Path,
    staged: &StagedPackages,
    alpine: &CaptureCase,
) -> Result<CaptureCase> {
    let mut request = alpine.request;
    request.compression = Ratio::new(EVICTION_COMPRESSION, 1)?;
    request = request.normalized()?;
    let tiles_per_side = u32::try_from(request.estimate()?.tiles_per_side)?;
    if tiles_per_side != EVICTION_TILES_PER_SIDE {
        return Err(format!(
            "Alpine eviction request estimates {tiles_per_side} tiles per side; expected {EVICTION_TILES_PER_SIDE}"
        )
        .into());
    }
    let chunk_axis = tiles_per_side.div_ceil(CHUNK_TILES);
    let package_chunk_count_bound = chunk_axis.saturating_mul(chunk_axis);
    if package_chunk_count_bound <= CHUNK_CACHE_CAPACITY {
        return Err("Alpine eviction package does not exceed the bounded chunk cache".into());
    }

    let generated = StagedPackages(make_isolated_directory(root, "visual-eviction-package")?);
    let request_file = RequestFile::create(root, request)?;
    let cache = env::var_os("AOE_GEODATA_CACHE")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join(".cache/geodata"));
    let worker = root.join("target/release/aoe-map-worker");
    if !worker.is_file() {
        return Err(format!(
            "release map worker is missing: {}; build it with `cargo build --release -p aoe-geodata --bin aoe-map-worker`",
            worker.display()
        )
        .into());
    }
    let worker = worker
        .to_str()
        .ok_or("release map worker path is not valid UTF-8")?;
    let cache = cache
        .to_str()
        .ok_or("geodata cache path is not valid UTF-8")?;
    let request_path = request_file
        .0
        .to_str()
        .ok_or("generated request path is not valid UTF-8")?;
    let output = generated
        .0
        .to_str()
        .ok_or("generated package path is not valid UTF-8")?;
    let environment = [
        ("AOE_GEODATA_CACHE", cache),
        ("AOE_MAP_REQUEST", request_path),
        ("AOE_MAP_PACKAGE", output),
    ];
    super::super::process::run_with_env(
        worker,
        &["map-generate"],
        &environment,
        Duration::from_secs(1_200),
    )?;
    super::super::process::run_with_env(
        worker,
        &["map-verify"],
        &environment,
        Duration::from_secs(120),
    )?;

    let manifest = only_manifest(&generated.0)?;
    let package: MapPackage = serde_json::from_slice(&fs::read(&manifest)?)?;
    package.validate()?;
    let content_hash = package.content_hash_hex();
    if manifest
        .file_stem()
        .and_then(|name| name.to_str())
        .is_none_or(|name| name != content_hash)
        || package.request != request
        || package.source_locks.is_empty()
    {
        return Err("generated Alpine eviction package is not canonical and source-locked".into());
    }

    let source = generated.0.join("pages").join(&content_hash);
    let (page_count, page_bytes) = measure_tree(&source)?;
    if page_count > MAX_PACKAGE_PAGES || page_bytes > MAX_PACKAGE_PAGE_BYTES {
        return Err(format!(
            "generated Alpine eviction pages exceed the fixed budget: pages={page_count}/{MAX_PACKAGE_PAGES}, bytes={page_bytes}/{MAX_PACKAGE_PAGE_BYTES}"
        )
        .into());
    }
    let elevation = read_elevation_range(&generated.0, &content_hash)?;
    if elevation
        .maximum_centimeters
        .saturating_sub(elevation.minimum_centimeters)
        < MIN_PACKAGE_RELIEF_CENTIMETERS
    {
        return Err(
            "generated Alpine eviction package lacks 1.5 km of source elevation relief".into(),
        );
    }
    let water = package
        .environment
        .water
        .is_some()
        .then(|| read_water_evidence(&generated.0, &content_hash))
        .transpose()?;
    let historical_land_use = read_historical_coverage(&generated.0, &content_hash)?;
    let (minimum_elevation_centimeters, maximum_elevation_centimeters) =
        read_chunk_elevation_range(&source, chunk_axis - 1, chunk_axis - 1)?;
    if maximum_elevation_centimeters.saturating_sub(minimum_elevation_centimeters)
        < MIN_CHUNK_SOURCE_RELIEF_CENTIMETERS
    {
        return Err(format!(
            "Alpine eviction southeast chunk source relief is only {} cm; expected at least {} cm",
            maximum_elevation_centimeters.saturating_sub(minimum_elevation_centimeters),
            MIN_CHUNK_SOURCE_RELIEF_CENTIMETERS
        )
        .into());
    }

    fs::copy(&manifest, staged.0.join(format!("{content_hash}.json")))?;
    copy_tree(&source, &staged.0.join("pages").join(&content_hash))?;
    let manifest_path = format!("generated-source-backed/alpine_eviction/{content_hash}.json");
    Ok(CaptureCase {
        id: "alpine_eviction".to_owned(),
        geometry: "relief".to_owned(),
        content_hash,
        manifest_path,
        location: "Central Alps, southeast relief corner".to_owned(),
        request,
        tiles_per_side,
        package_chunk_count_bound,
        schema_version: package.schema_version,
        generator_version: package.generator_version,
        generation_recipe_version: package.generation_recipe_version,
        source_locks: package.source_locks,
        preparation_elapsed_milliseconds: 0,
        page_count,
        page_bytes,
        water_pages: water,
        historical_land_use,
        elevation_range_centimeters: elevation,
        eviction_target: Some(EvictionTarget {
            chunk_x: chunk_axis - 1,
            chunk_y: chunk_axis - 1,
            minimum_elevation_centimeters,
            maximum_elevation_centimeters,
        }),
    })
}

fn make_isolated_directory(root: &Path, label: &str) -> Result<PathBuf> {
    let target = root.join("target");
    fs::create_dir_all(&target)?;
    for attempt in 0..4 {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = target.join(format!("{label}-{}-{nonce}-{attempt}", std::process::id()));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("could not create an isolated generated-package directory".into())
}

fn only_manifest(directory: &Path) -> Result<PathBuf> {
    let entries = fs::read_dir(directory)?.collect::<std::io::Result<Vec<_>>>()?;
    let mut manifests = entries
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        });
    let manifest = manifests
        .next()
        .ok_or("map worker generated no package manifest")?;
    if manifests.next().is_some() {
        return Err("map worker generated more than one package manifest".into());
    }
    Ok(manifest)
}

fn read_chunk_elevation_range(pages: &Path, chunk_x: u32, chunk_y: u32) -> Result<(i32, i32)> {
    let start_tile_x = chunk_x.saturating_mul(CHUNK_TILES);
    let start_tile_y = chunk_y.saturating_mul(CHUNK_TILES);
    let end_tile_x = start_tile_x
        .saturating_add(CHUNK_TILES)
        .min(EVICTION_TILES_PER_SIDE);
    let end_tile_y = start_tile_y
        .saturating_add(CHUNK_TILES)
        .min(EVICTION_TILES_PER_SIDE);
    let sample_start_x =
        usize::try_from(u64::from(start_tile_x) * 128 / u64::from(EVICTION_TILES_PER_SIDE))?;
    let sample_start_y =
        usize::try_from(u64::from(start_tile_y) * 128 / u64::from(EVICTION_TILES_PER_SIDE))?;
    let sample_end_x = usize::try_from(
        (u64::from(end_tile_x) * 128).div_ceil(u64::from(EVICTION_TILES_PER_SIDE)),
    )?
    .min(127);
    let sample_end_y = usize::try_from(
        (u64::from(end_tile_y) * 128).div_ceil(u64::from(EVICTION_TILES_PER_SIDE)),
    )?
    .min(127);
    let mut minimum = i32::MAX;
    let mut maximum = i32::MIN;
    for page_y in 0..2_u32 {
        for page_x in 0..2_u32 {
            let path = pages.join(format!("elevation/0-{page_x}-{page_y}.json"));
            let page: ElevationPage = serde_json::from_slice(&fs::read(path)?)?;
            page.validate()?;
            if page.level != 0 || u32::from(page.x) != page_x || u32::from(page.y) != page_y {
                return Err("generated elevation page has unexpected source coordinates".into());
            }
            let origin_x = usize::from(page.x) * usize::from(page.width);
            let origin_y = usize::from(page.y) * usize::from(page.height);
            if origin_x > sample_end_x
                || origin_y > sample_end_y
                || origin_x + usize::from(page.width) <= sample_start_x
                || origin_y + usize::from(page.height) <= sample_start_y
            {
                continue;
            }
            let local_start_x = sample_start_x.saturating_sub(origin_x);
            let local_start_y = sample_start_y.saturating_sub(origin_y);
            let local_end_x = sample_end_x
                .saturating_sub(origin_x)
                .min(usize::from(page.width) - 1);
            let local_end_y = sample_end_y
                .saturating_sub(origin_y)
                .min(usize::from(page.height) - 1);
            for y in local_start_y..=local_end_y.min(usize::from(page.height) - 1) {
                for x in local_start_x..=local_end_x.min(usize::from(page.width) - 1) {
                    let index = y * usize::from(page.width) + x;
                    let height = page.geographic_height_centimeters[index];
                    minimum = minimum.min(height);
                    maximum = maximum.max(height);
                }
            }
        }
    }
    if minimum == i32::MAX || maximum == i32::MIN {
        return Err("generated southeast chunk has no source elevation samples".into());
    }
    Ok((minimum, maximum))
}

struct RequestFile(PathBuf);

impl RequestFile {
    fn create(root: &Path, request: MapRequest) -> Result<Self> {
        let target = root.join("target");
        fs::create_dir_all(&target)?;
        for attempt in 0..4 {
            let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let path = target.join(format!(
                "visual-eviction-request-{}-{nonce}-{attempt}.json",
                std::process::id()
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    serde_json::to_writer(&mut file, &request)?;
                    file.sync_all()?;
                    return Ok(Self(path));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("could not create a unique generated map request file".into())
    }
}

impl Drop for RequestFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
