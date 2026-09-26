use crate::{
    DownloadPolicy, ExpectedChecksum, GeodataError, KnownSource, Provider, SourceCache, SourceLock,
    acquisition_marker, directory::publish_streaming_manifest,
};
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, EnvironmentalProvenance, LayerProvenance, MAP_SCHEMA_VERSION,
    MapPackage, MapRequest, PreparedEnvironment, ProjectionMetadata, VerticalDatum,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub const MAX_DETAILED_SAMPLES_PER_AXIS: u16 = 4_096;
pub const MAX_DETAILED_TILES: usize = 64;
pub const MAX_DETAILED_INPUT_BYTES: u64 = 4 * 1024 * 1024 * 1024;
pub const MAX_DETAILED_STAGING_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const PAGE: u16 = ENVIRONMENT_PAGE_SAMPLES as u16;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DemResolution {
    Glo90,
    Glo30PreferGlo90,
}

impl DemResolution {
    fn suffix(self) -> &'static str {
        match self {
            Self::Glo90 => "30",
            Self::Glo30PreferGlo90 => "10",
        }
    }

    fn fallback(self) -> Option<Self> {
        matches!(self, Self::Glo30PreferGlo90).then_some(Self::Glo90)
    }

    fn native_resolution(self) -> &'static str {
        match self {
            Self::Glo90 => "3 arc-seconds",
            Self::Glo30PreferGlo90 => "1 arc-second",
        }
    }
}

struct Tile {
    latitude: i32,
    longitude: i32,
    path: PathBuf,
    lock: SourceLock,
}

struct TileCoverage {
    tiles: Vec<Tile>,
    absent_tiles: BTreeSet<(i32, i32)>,
}

fn tool_version() -> String {
    let gdal = gdal::version::VersionInfo::release_name();
    let proj = gdal::version::VersionInfo::build_info()
        .get("PROJ_RUNTIME_VERSION")
        .cloned()
        .unwrap_or_else(|| "unknown".to_owned());
    format!("GDAL {gdal} / PROJ {proj}")
}

pub(crate) fn prepare_with_staging(
    cache_root: PathBuf,
    output_directory: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    resolution: DemResolution,
    staging_root: Option<PathBuf>,
    water_corrections: Option<aoe_map::WaterCorrectionDocument>,
) -> Result<MapPackage, GeodataError> {
    let _lease = staging_root.as_deref().map(Stage::lease).transpose()?;
    if !(2..=MAX_DETAILED_SAMPLES_PER_AXIS).contains(&samples_per_axis) {
        return Err(GeodataError::Preparation(
            "detailed preparation supports 2 through 4096 samples per axis",
        ));
    }
    let request = request
        .normalized()
        .map_err(|_| GeodataError::Preparation("invalid map request"))?;
    let hydrology_axis = samples_per_axis.min(crate::MAX_HYDROLOGY_SAMPLES_PER_AXIS);
    let water_corrections = water_corrections
        .map(Ok)
        .unwrap_or_else(|| aoe_map::WaterCorrectionDocument::empty(request, hydrology_axis))
        .map_err(|_| GeodataError::Preparation("invalid water correction document"))?;
    water_corrections
        .validate_for(request, hydrology_axis)
        .map_err(|_| GeodataError::Preparation("water corrections do not match request grid"))?;
    let estimate = request
        .estimate()
        .map_err(|_| GeodataError::Preparation("invalid map request estimate"))?;
    let bounds = geographic_bounds(request, estimate.effective_side_meters)?;
    validate_tile_budget(bounds)?;
    let hydrology_plan = crate::hydrology::preflight_hydrology(&cache_root, request)?;
    let overview = crate::prepare_overview(cache_root.clone(), request, 128)?;
    let mut hydrology = crate::hydrology::prepare_hydrology_with_plan(
        cache_root.clone(),
        request,
        hydrology_axis,
        hydrology_plan,
        &overview.water_pages,
    )?;
    crate::hydrology::apply_water_model(
        &mut hydrology,
        request,
        &overview.pages,
        water_corrections,
    )?;
    let cache = SourceCache::new(
        cache_root.clone(),
        DownloadPolicy {
            cache_quota_bytes: crate::DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: MAX_DETAILED_INPUT_BYTES,
        },
    )?;
    let cancelled = AtomicBool::new(false);
    let coverage = acquire_tiles(&cache, bounds, resolution, &cancelled)?;
    let stage = Stage::new(staging_root.as_deref().unwrap_or(&cache_root))?;
    pyramid::store_hydrology_evidence(&stage, &hydrology)?;
    let overview_ocean = source_backed_overview_ocean(&overview)?;
    let mut sampler = Sampler::new(
        request,
        estimate.effective_side_meters,
        bounds,
        coverage.absent_tiles,
        coverage.tiles,
        overview_ocean,
    )?;
    let fields = build_pyramids(
        &mut sampler,
        &stage,
        samples_per_axis,
        &overview,
        &hydrology,
    )?;
    let mut sources = vec![
        overview.source_lock,
        overview.water_source_lock,
        overview.vegetation_source_lock,
        overview.vegetation_classes_source_lock,
        overview.hyde_baseline_source_lock,
        overview.hyde_supplementary_source_lock,
        overview.hyde_readme_source_lock,
    ];
    sources.extend(
        sampler
            .tiles
            .iter()
            .map(|tile| {
                tile.lock
                    .to_map_source_lock(acquisition_marker(), "copernicus-cog-page-v1".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    sources.extend(hydrology.source_locks.iter().cloned());
    let environment = PreparedEnvironment {
        samples_per_axis,
        geographic_millimeters_per_sample: estimate
            .effective_side_meters
            .checked_mul(1_000)
            .ok_or(GeodataError::Preparation("sample spacing overflows"))?
            .div_ceil(u64::from(samples_per_axis)),
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: fields.elevation,
        water: Some(fields.water),
        vegetation: Some(fields.vegetation),
        historical_land_use: Some(fields.historical_land_use),
        hydrology_evidence: Some(hydrology.evidence_index.clone()),
    };
    let package = MapPackage::with_prepared_environment(
        MAP_SCHEMA_VERSION,
        request,
        sources,
        ProjectionMetadata {
            horizontal_crs: crate::local_aeqd_definition(
                request.center_latitude_e7,
                request.center_longitude_e7,
            ),
            vertical_datum: VerticalDatum::Egm2008Orthometric,
            tool_version: tool_version(),
        },
        EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: overview.provenance.water,
            vegetation: overview.provenance.vegetation,
            historical_land_use: overview.provenance.historical_land_use,
        },
        environment,
    )?;
    package.validate()?;
    publish_staged_pages(&stage, &output_directory, &package, samples_per_axis)?;
    publish_streaming_manifest(&output_directory, &package)?;
    crate::GeneratedMap::verify_directory(&output_directory, &package.content_hash_hex())?;
    Ok(package)
}

#[derive(Clone, Copy)]
struct Bounds {
    min_latitude: i32,
    max_latitude: i32,
    min_longitude: i32,
    max_longitude: i32,
}

fn geographic_bounds(request: MapRequest, side: u64) -> Result<Bounds, GeodataError> {
    let center_latitude = f64::from(request.center_latitude_e7) / 10_000_000.0;
    let conservative_radius = side as f64 / 2.0 * 2.0_f64.sqrt();
    let pole_distance = (90.0 - center_latitude.abs()) * 110_000.0;
    if pole_distance <= conservative_radius {
        return Err(GeodataError::Preparation(
            "detailed footprint crosses an unsupported geographic boundary",
        ));
    }
    let points = crate::footprint::projected_footprint(
        request,
        crate::footprint::MAX_FOOTPRINT_SAMPLES_PER_EDGE,
    )?;
    let min_latitude_e7 = points
        .iter()
        .map(|point| point.latitude_e7)
        .min()
        .ok_or(GeodataError::Coordinate)?;
    let max_latitude_e7 = points
        .iter()
        .map(|point| point.latitude_e7)
        .max()
        .ok_or(GeodataError::Coordinate)?;
    let min_longitude_e7 = points
        .iter()
        .map(|point| point.longitude_e7)
        .min()
        .ok_or(GeodataError::Coordinate)?;
    let max_longitude_e7 = points
        .iter()
        .map(|point| point.longitude_e7)
        .max()
        .ok_or(GeodataError::Coordinate)?;
    if min_latitude_e7 <= -900_000_000
        || max_latitude_e7 >= 900_000_000
        || min_longitude_e7 <= -1_800_000_000
        || max_longitude_e7 >= 1_800_000_000
        || i64::from(max_longitude_e7) - i64::from(min_longitude_e7) > 1_800_000_000
    {
        return Err(GeodataError::Preparation(
            "detailed footprint crosses an unsupported geographic boundary",
        ));
    }
    let min_latitude = min_latitude_e7.div_euclid(10_000_000);
    let max_latitude = max_latitude_e7.div_euclid(10_000_000);
    let min_longitude = min_longitude_e7.div_euclid(10_000_000);
    let max_longitude = max_longitude_e7.div_euclid(10_000_000);
    Ok(Bounds {
        min_latitude: min_latitude.saturating_sub(1).max(-90),
        max_latitude: max_latitude.saturating_add(1).min(89),
        min_longitude: min_longitude.saturating_sub(1).max(-180),
        max_longitude: max_longitude.saturating_add(1).min(179),
    })
}

impl Bounds {
    fn contains(&self, latitude: i32, longitude: i32) -> bool {
        (self.min_latitude..=self.max_latitude).contains(&latitude)
            && (self.min_longitude..=self.max_longitude).contains(&longitude)
    }
}

fn source_backed_overview_ocean(
    overview: &crate::PreparedOverview,
) -> Result<Vec<u8>, GeodataError> {
    const AXIS: usize = 128;
    if overview.provenance.water != LayerProvenance::SourceDerived {
        return Err(GeodataError::Preparation(
            "overview ocean coverage is not source-backed",
        ));
    }
    let mut values = vec![0_u8; AXIS * AXIS];
    let mut seen = vec![false; AXIS * AXIS];
    for page in overview.water_pages.iter().filter(|page| page.level == 0) {
        let width = usize::from(page.width);
        let height = usize::from(page.height);
        let start_x = usize::from(page.x) * usize::from(PAGE);
        let start_y = usize::from(page.y) * usize::from(PAGE);
        if start_x + width > AXIS || start_y + height > AXIS {
            return Err(GeodataError::Preparation(
                "overview ocean page exceeds its 128-sample grid",
            ));
        }
        for row in 0..height {
            for column in 0..width {
                let index = (start_y + row) * AXIS + start_x + column;
                if seen[index] {
                    return Err(GeodataError::Preparation(
                        "overview ocean pages contain a duplicate coordinate",
                    ));
                }
                seen[index] = true;
                values[index] = page.ocean_coverage_percent[row * width + column];
            }
        }
    }
    if seen.iter().all(|covered| *covered) {
        Ok(values)
    } else {
        Err(GeodataError::Preparation(
            "source-backed overview ocean coverage is incomplete",
        ))
    }
}

fn acquire_tiles(
    cache: &SourceCache,
    bounds: Bounds,
    resolution: DemResolution,
    cancelled: &AtomicBool,
) -> Result<TileCoverage, GeodataError> {
    validate_tile_budget(bounds)?;
    let mut selected = Vec::new();
    let mut absent_tiles = BTreeSet::new();
    let mut input_bytes = 0_u64;
    for latitude in bounds.min_latitude..=bounds.max_latitude {
        for longitude in bounds.min_longitude..=bounds.max_longitude {
            if cancelled.load(Ordering::SeqCst) {
                return Err(GeodataError::Cache(crate::CacheError::Cancelled));
            }
            let mut source = public_tile_source(latitude, longitude, resolution)?;
            if source.is_none()
                && let Some(fallback) = resolution.fallback()
            {
                source = public_tile_source(latitude, longitude, fallback)?;
            }
            let Some(source) = source else {
                absent_tiles.insert((latitude, longitude));
                continue;
            };
            input_bytes = input_bytes.saturating_add(source.bytes);
            if input_bytes > MAX_DETAILED_INPUT_BYTES {
                return Err(GeodataError::Preparation(
                    "regional Copernicus DEM inputs exceed the 4 GiB limit",
                ));
            }
            selected.push((latitude, longitude, source));
        }
    }
    let mut tiles = Vec::with_capacity(selected.len());
    for (latitude, longitude, source) in selected {
        let lock = cache.acquire_known(&source, cancelled)?;
        let path = cache.object_path(&lock)?;
        tiles.push(Tile {
            latitude,
            longitude,
            path,
            lock,
        });
    }
    Ok(TileCoverage {
        tiles,
        absent_tiles,
    })
}

fn validate_tile_budget(bounds: Bounds) -> Result<(), GeodataError> {
    let lat_count = usize::try_from(bounds.max_latitude - bounds.min_latitude + 1)
        .map_err(|_| GeodataError::Preparation("tile latitude range overflows"))?;
    let lon_count = usize::try_from(bounds.max_longitude - bounds.min_longitude + 1)
        .map_err(|_| GeodataError::Preparation("tile longitude range overflows"))?;
    if lat_count.saturating_mul(lon_count) > MAX_DETAILED_TILES {
        return Err(GeodataError::Preparation(
            "detailed footprint exceeds the 64 tile limit",
        ));
    }
    Ok(())
}

fn public_tile_source(
    latitude: i32,
    longitude: i32,
    resolution: DemResolution,
) -> Result<Option<KnownSource>, GeodataError> {
    let prefix = tile_prefix(latitude, longitude, resolution.suffix());
    let host = if resolution == DemResolution::Glo90 {
        "copernicus-dem-90m.s3.amazonaws.com"
    } else {
        "copernicus-dem-30m.s3.amazonaws.com"
    };
    let url = format!("https://{host}/{prefix}/{prefix}.tif");
    let connector = ureq::native_tls::TlsConnector::new()
        .map_err(|error| GeodataError::Source(error.to_string()))?;
    let agent = ureq::AgentBuilder::new()
        .tls_connector(Arc::new(connector))
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(30))
        .timeout_write(Duration::from_secs(30))
        .build();
    let response = agent.head(&url).call();
    let response = match response {
        Ok(response) if response.status() == 200 => response,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(error) => return Err(GeodataError::Source(error.to_string())),
        Ok(response) => {
            return Err(GeodataError::Source(format!(
                "Copernicus HEAD returned HTTP {} for {url}",
                response.status()
            )));
        }
    };
    let bytes = response
        .header("Content-Length")
        .ok_or_else(|| GeodataError::Source("Copernicus tile has no Content-Length".to_owned()))?
        .parse::<u64>()
        .map_err(|_| GeodataError::Source("Copernicus tile length is invalid".to_owned()))?;
    if bytes == 0 || bytes > MAX_DETAILED_INPUT_BYTES {
        return Err(GeodataError::Source(
            "Copernicus tile exceeds input limits".to_owned(),
        ));
    }
    let etag = response
        .header("ETag")
        .ok_or_else(|| GeodataError::Source("Copernicus tile has no ETag".to_owned()))?
        .trim_matches('"');
    if etag.len() != 32 || !etag.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(GeodataError::Source(
            "Copernicus tile ETag is not a single-object MD5".to_owned(),
        ));
    }
    let mut digest = [0_u8; 16];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&etag[index * 2..index * 2 + 2], 16).map_err(|_| {
            GeodataError::Source("Copernicus tile ETag is not hexadecimal".to_owned())
        })?;
    }
    Ok(Some(KnownSource {
        id: format!("copernicus-dem-{}-{prefix}", resolution.native_resolution()),
        provider: Provider::Copernicus,
        release: "Copernicus DEM 2021 public COG".to_owned(),
        url,
        bytes,
        expected_checksum: ExpectedChecksum::Md5(digest),
        native_resolution: resolution.native_resolution().to_owned(),
        crs: "EPSG:4326".to_owned(),
        vertical_datum: "EGM2008 orthometric".to_owned(),
        license_reference: "Copernicus DEM public COG licence".to_owned(),
    }))
}

fn tile_prefix(latitude: i32, longitude: i32, suffix: &str) -> String {
    let lat = if latitude < 0 {
        format!("S{:02}_00", latitude.unsigned_abs())
    } else {
        format!("N{latitude:02}_00")
    };
    let lon = if longitude < 0 {
        format!("W{:03}_00", longitude.unsigned_abs())
    } else {
        format!("E{longitude:03}_00")
    };
    format!("Copernicus_DSM_COG_{suffix}_{lat}_{lon}_DEM")
}

mod entry;
pub use entry::{
    prepare_detailed_directory, prepare_detailed_directory_with_water_corrections,
};

mod sampler;
use sampler::Sampler;

mod pyramid;
use pyramid::build_pyramids;

mod stage;
#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "tests/stage.rs"]
mod tests_stage;
use stage::{Stage, publish_staged_pages};
