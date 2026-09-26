//! Fixed-matrix validation and isolated package staging for visual captures.
use aoe_map::{ElevationPage, MapPackage, WaterPage};
mod historical;
pub(super) use historical::HistoricalLandUseEvidence;
use historical::read_historical_coverage;
mod eviction;
#[cfg(test)]
mod tests;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const VISUAL_CASES: [(&str, &str); 3] = [
    ("river_lake", "inland"),
    ("nile_delta_coast", "ocean"),
    ("alpine_relief", "relief"),
];
const CHUNK_TILES: u32 = 32;
const CHUNK_CACHE_CAPACITY: u32 = 512;
const MAX_PACKAGE_PAGES: usize = 64;
const MAX_PACKAGE_PAGE_BYTES: u64 = 1_048_576;

#[derive(Deserialize)]
struct MatrixReport {
    result: String,
    revision: String,
    cases: Vec<MatrixOutcome>,
}

#[derive(Deserialize)]
struct MatrixOutcome {
    id: String,
    location: String,
    status: String,
    request: aoe_map::MapRequest,
    content_hash: Option<String>,
    elapsed_milliseconds: u64,
}

#[derive(Clone, Serialize)]
pub(super) struct CaptureInputs {
    pub(super) version: u8,
    pub(super) prepared_revision: String,
    pub(super) capture_revision: String,
    pub(super) case_corrections: serde_json::Value,
    pub(super) activation_cases: Vec<CaptureCase>,
    pub(super) cases: Vec<CaptureCase>,
    pub(super) eviction_case: CaptureCase,
}

#[derive(Clone, Serialize)]
pub(super) struct CaptureCase {
    pub(super) id: String,
    pub(super) geometry: String,
    pub(super) content_hash: String,
    pub(super) manifest_path: String,
    pub(super) location: String,
    pub(super) request: aoe_map::MapRequest,
    pub(super) tiles_per_side: u32,
    pub(super) package_chunk_count_bound: u32,
    pub(super) schema_version: u16,
    pub(super) generator_version: u16,
    pub(super) generation_recipe_version: u16,
    pub(super) source_locks: Vec<aoe_map::SourceLock>,
    pub(super) preparation_elapsed_milliseconds: u64,
    pub(super) page_count: usize,
    pub(super) page_bytes: u64,
    pub(super) water_pages: Option<WaterEvidence>,
    pub(super) historical_land_use: HistoricalLandUseEvidence,
    pub(super) elevation_range_centimeters: ElevationRange,
    pub(super) eviction_target: Option<EvictionTarget>,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct EvictionTarget {
    pub(super) chunk_x: u32,
    pub(super) chunk_y: u32,
    pub(super) minimum_elevation_centimeters: i32,
    pub(super) maximum_elevation_centimeters: i32,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct WaterEvidence {
    pub(super) sample_count: usize,
    pub(super) ocean_nonzero_samples: usize,
    pub(super) inland_nonzero_samples: usize,
    pub(super) ocean_coverage_percent_sum: u64,
    pub(super) inland_coverage_percent_sum: u64,
}

#[derive(Clone, Debug, Serialize)]
pub(super) struct ElevationRange {
    pub(super) minimum_centimeters: i32,
    pub(super) maximum_centimeters: i32,
    pub(super) level_zero_samples: usize,
}

pub(super) struct StagedPackages(pub(super) PathBuf);

impl Drop for StagedPackages {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) fn prepare(
    root: &Path,
    capture_revision: String,
) -> Result<(StagedPackages, CaptureInputs)> {
    let matrix_path = root.join("reports/geodata/matrix.json");
    if !matrix_path.is_file() {
        return Err(format!(
            "source-backed visual capture needs {}. Run `AOE_GEODATA_CACHE=/absolute/cache make test-geographic-matrix` first.",
            matrix_path.display()
        )
        .into());
    }
    let matrix: MatrixReport = serde_json::from_slice(&fs::read(matrix_path)?)?;
    if matrix.result != "PASS" {
        return Err("geographic matrix report is not PASS; visual capture requires the verified fixed matrix".into());
    }
    if matrix.cases.len() != 11 {
        return Err(format!(
            "fixed geographic matrix report contains {} cases; expected all 11",
            matrix.cases.len()
        )
        .into());
    }
    let case_corrections: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("docs/geodata/coast-correction.json"))?)?;
    let matrix_definition: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("docs/geodata/reference-matrix.json"))?)?;
    let nile_request = matrix_definition
        .get("cases")
        .and_then(serde_json::Value::as_array)
        .and_then(|cases| {
            cases.iter().find(|case| {
                case.get("id").and_then(serde_json::Value::as_str) == Some("nile_delta_coast")
            })
        })
        .and_then(|case| case.get("request"))
        .ok_or("fixed geographic matrix definition has no Nile coast request")?;
    let fixed_nile_request: aoe_map::MapRequest = serde_json::from_value(nile_request.clone())?;
    let packages = StagedPackages(make_staging_directory(root)?);
    let mut activation_cases = Vec::with_capacity(matrix.cases.len());
    let mut visual_cases = Vec::with_capacity(VISUAL_CASES.len());
    for prepared in &matrix.cases {
        let id = prepared.id.as_str();
        if prepared.status != "PASS" {
            return Err(format!("fixed geographic case {id} is not PASS").into());
        }
        if id == "nile_delta_coast" && prepared.request != fixed_nile_request {
            return Err(
                "nile_delta_coast matrix request differs from the fixed reference matrix".into(),
            );
        }
        let hash = prepared
            .content_hash
            .as_deref()
            .filter(|hash| valid_hash(hash))
            .ok_or_else(|| format!("fixed geographic case {id} has no canonical content hash"))?;
        let source = root.join("local-assets/maps-matrix").join(id);
        let manifest = source.join(format!("{hash}.json"));
        let package: MapPackage = serde_json::from_slice(&fs::read(&manifest)?)?;
        package.validate()?;
        if package.content_hash_hex() != hash
            || package.request != prepared.request
            || package.source_locks.is_empty()
        {
            return Err(format!(
                "fixed geographic case {id} is not a matching source-locked package"
            )
            .into());
        }
        let water = package
            .environment
            .water
            .is_some()
            .then(|| read_water_evidence(&source, hash))
            .transpose()?;
        let elevation = read_elevation_range(&source, hash)?;
        let historical_land_use = read_historical_coverage(&source, hash)?;
        let (page_count, page_bytes) = measure_tree(&source.join("pages").join(hash))?;
        let tiles_per_side = u32::try_from(package.estimate.tiles_per_side)?;
        let chunk_axis = tiles_per_side.div_ceil(CHUNK_TILES);
        let package_chunk_count_bound = chunk_axis.saturating_mul(chunk_axis);
        if page_count > MAX_PACKAGE_PAGES
            || page_bytes > MAX_PACKAGE_PAGE_BYTES
            || package_chunk_count_bound > CHUNK_CACHE_CAPACITY
        {
            return Err(format!(
                "fixed package {id} exceeds its page/work budget: pages={page_count}/{MAX_PACKAGE_PAGES}, bytes={page_bytes}/{MAX_PACKAGE_PAGE_BYTES}, chunks={package_chunk_count_bound}/{CHUNK_CACHE_CAPACITY}"
            )
            .into());
        }
        fs::copy(&manifest, packages.0.join(format!("{hash}.json")))?;
        copy_tree(
            &source.join("pages").join(hash),
            &packages.0.join("pages").join(hash),
        )?;

        let geometry = VISUAL_CASES
            .iter()
            .find_map(|(case_id, geometry)| (*case_id == id).then_some(*geometry));
        if let Some(geometry) = geometry {
            qualify_case(id, geometry, water.as_ref(), &elevation)?;
        }
        let case = CaptureCase {
            id: id.to_owned(),
            geometry: geometry.unwrap_or("overview").to_owned(),
            content_hash: hash.to_owned(),
            manifest_path: format!("local-assets/maps-matrix/{id}/{hash}.json"),
            location: prepared.location.clone(),
            request: package.request,
            tiles_per_side,
            package_chunk_count_bound,
            schema_version: package.schema_version,
            generator_version: package.generator_version,
            generation_recipe_version: package.generation_recipe_version,
            source_locks: package.source_locks,
            preparation_elapsed_milliseconds: prepared.elapsed_milliseconds,
            page_count,
            page_bytes,
            water_pages: water,
            historical_land_use,
            elevation_range_centimeters: elevation,
            eviction_target: None,
        };
        if geometry.is_some() {
            visual_cases.push(case.clone());
        }
        activation_cases.push(case);
    }
    let alpine = activation_cases
        .iter()
        .find(|case| case.id == "alpine_relief")
        .ok_or("fixed geographic matrix omitted Alpine relief")?;
    let eviction_case = eviction::prepare(root, &packages, alpine)?;
    Ok((
        packages,
        CaptureInputs {
            version: 2,
            prepared_revision: matrix.revision,
            capture_revision,
            case_corrections,
            activation_cases,
            cases: visual_cases,
            eviction_case,
        },
    ))
}

fn qualify_case(
    id: &str,
    geometry: &str,
    water: Option<&WaterEvidence>,
    elevation: &ElevationRange,
) -> Result<()> {
    match geometry {
        "inland" if water.is_some_and(|pages| pages.inland_coverage_percent_sum > 0) => Ok(()),
        "ocean" if water.is_some_and(|pages| pages.ocean_coverage_percent_sum > 0) => Ok(()),
        "relief"
            if elevation.maximum_centimeters.saturating_sub(elevation.minimum_centimeters)
                >= 150_000 =>
        {
            Ok(())
        }
        "inland" => Err(format!(
            "source-backed visual case {id} has zero inland-water page coverage; no lake capture was made"
        )
        .into()),
        "ocean" => Err(format!(
            "source-backed visual case {id} has zero ocean page coverage; no coast capture was made"
        )
        .into()),
        "relief" => Err(format!(
            "source-backed visual case {id} relief range is {} cm; at least 150000 cm is required for the Alps capture",
            elevation.maximum_centimeters.saturating_sub(elevation.minimum_centimeters)
        )
        .into()),
        _ => Err(format!("unknown visual qualification geometry {geometry}").into()),
    }
}

fn read_water_evidence(root: &Path, hash: &str) -> Result<WaterEvidence> {
    let directory = root.join("pages").join(hash).join("water");
    let mut evidence = WaterEvidence {
        sample_count: 0,
        ocean_nonzero_samples: 0,
        inland_nonzero_samples: 0,
        ocean_coverage_percent_sum: 0,
        inland_coverage_percent_sum: 0,
    };
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let page: WaterPage = serde_json::from_slice(&fs::read(path)?)?;
        page.validate()?;
        if page.level != 0 {
            continue;
        }
        if page.ocean_coverage_percent.len() != page.inland_coverage_percent.len() {
            return Err("water page has mismatched coverage arrays".into());
        }
        evidence.sample_count += page.ocean_coverage_percent.len();
        evidence.ocean_nonzero_samples += page
            .ocean_coverage_percent
            .iter()
            .filter(|v| **v > 0)
            .count();
        evidence.inland_nonzero_samples += page
            .inland_coverage_percent
            .iter()
            .filter(|v| **v > 0)
            .count();
        evidence.ocean_coverage_percent_sum += page
            .ocean_coverage_percent
            .iter()
            .map(|v| u64::from(*v))
            .sum::<u64>();
        evidence.inland_coverage_percent_sum += page
            .inland_coverage_percent
            .iter()
            .map(|v| u64::from(*v))
            .sum::<u64>();
    }
    if evidence.sample_count == 0 {
        return Err(format!("source-backed package {hash} has no level-zero water samples").into());
    }
    Ok(evidence)
}

fn read_elevation_range(root: &Path, hash: &str) -> Result<ElevationRange> {
    let directory = root.join("pages").join(hash).join("elevation");
    let mut minimum = i32::MAX;
    let mut maximum = i32::MIN;
    let mut samples = 0_usize;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let page: ElevationPage = serde_json::from_slice(&fs::read(path)?)?;
        page.validate()?;
        if page.level != 0 {
            continue;
        }
        for value in page.geographic_height_centimeters {
            minimum = minimum.min(value);
            maximum = maximum.max(value);
            samples += 1;
        }
    }
    if samples == 0 {
        return Err(
            format!("source-backed package {hash} has no level-zero elevation samples").into(),
        );
    }
    Ok(ElevationRange {
        minimum_centimeters: minimum,
        maximum_centimeters: maximum,
        level_zero_samples: samples,
    })
}

fn make_staging_directory(root: &Path) -> Result<PathBuf> {
    let parent = root.join("target");
    fs::create_dir_all(&parent)?;
    for attempt in 0..4 {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = parent.join(format!(
            "geographic-visuals-{}-{now}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err("could not create a unique visual package staging directory".into())
}

fn measure_tree(root: &Path) -> Result<(usize, u64)> {
    let mut count = 0_usize;
    let mut bytes = 0_u64;
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                count = count.saturating_add(1);
                bytes = bytes.saturating_add(entry.metadata()?.len());
            } else {
                return Err("package page tree contains a non-regular entry".into());
            }
        }
    }
    Ok((count, bytes))
}

fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source = entry.path();
        let target = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_file() {
            fs::copy(source, target)?;
        } else {
            return Err("package page tree contains a non-regular entry".into());
        }
    }
    Ok(())
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
