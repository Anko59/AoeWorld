//! Native-only GDAL and PROJ boundary for geographic source preparation.
//!
//! This crate intentionally does not enter the deterministic map core. It
//! validates local rasters and evaluates the documented azimuthal-equidistant
//! projection before a future preparation pipeline freezes map inputs.

use aoe_map::{
    ElevationPage, EnvironmentalProvenance, HistoricalLandUsePage, LayerProvenance,
    MAP_SCHEMA_VERSION, MapPackage, MapRequest, PotentialBiomePage, PreparedEnvironment,
    ProjectionMetadata, VerticalDatum, WaterPage,
};
use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod elevation;
pub use elevation::{MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, PreparedElevation, prepare_elevation};

mod hyde;
pub use hyde::{PreparedHistoricalLandUse, prepare_hyde_600, prepare_hyde_lake_coverage};

mod source_cache;
pub use source_cache::{
    CacheError, DEFAULT_CACHE_QUOTA_BYTES, DEFAULT_JOB_ACQUISITION_BUDGET_BYTES, DownloadPolicy,
    Provider, SourceCache, SourceLock,
};

mod source_manifest;

mod water;
pub use water::{PreparedWater, prepare_ocean_coverage};

mod vegetation;
pub use vegetation::{PreparedVegetation, prepare_potential_biomes, verify_potential_biome_legend};

mod source_catalog;
pub use source_catalog::{
    ExpectedChecksum, KnownSource, SourceCatalogError, etopo_2022_60s_surface, hyde_sources,
    natural_earth_10m_land, potential_biome_sources,
};

/// Catalog identifiers required by the first source-backed overview recipe.
/// They let offline verification report exactly which verified cache objects
/// are absent without querying a provider.
pub const REQUIRED_OVERVIEW_SOURCE_IDS: &[&str] = &[
    "etopo-2022-v1-60s-surface",
    "natural-earth-10m-land-v5.1.1",
    "potential-biome-v0.2:pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif",
    "potential-biome-v0.2:pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv",
    "hyde-3.2.1:HYDE3_2_1-baseline.zip",
    "hyde-3.2.1:HYDE3_2_1-general_supplementary.zip",
    "hyde-3.2.1:readme_release_HYDE3.2.1.txt",
];

/// Resolves the allowlisted sources needed by the overview recipe. Calling
/// this may contact the metadata endpoints for the mutable provider catalogs;
/// callers that need an offline check should use `REQUIRED_OVERVIEW_SOURCE_IDS`.
pub fn overview_sources() -> Result<Vec<KnownSource>, GeodataError> {
    let mut sources = vec![etopo_2022_60s_surface(), natural_earth_10m_land()];
    sources.extend(potential_biome_sources()?);
    sources.extend(hyde_sources()?);
    Ok(sources)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterDimensions {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum GeodataError {
    #[error("geographic source I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("GDAL could not open the source raster: {0}")]
    Gdal(#[from] gdal::errors::GdalError),
    #[error("PROJ could not create the requested local projection")]
    Projection,
    #[error("PROJ could not project the requested coordinate")]
    Coordinate,
    #[error("geographic preparation failed: {0}")]
    Preparation(&'static str),
    #[error(transparent)]
    Environment(#[from] aoe_map::EnvironmentError),
    #[error(transparent)]
    Package(#[from] aoe_map::MapPackageError),
    #[error(transparent)]
    SourceCatalog(#[from] SourceCatalogError),
    #[error(transparent)]
    Cache(#[from] CacheError),
}

/// A bounded native-worker operation passed on stdin by a direct process spawn.
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WorkerRequest {
    PrepareOverviewElevation {
        cache_root: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
    },
    ListOverviewSources,
    ListPotentialBiomeSources,
    ListHydeSources,
    InspectRaster {
        path: PathBuf,
    },
    ProjectPoint {
        center_latitude_e7: i32,
        center_longitude_e7: i32,
        longitude: f64,
        latitude: f64,
    },
    PrepareElevation {
        path: PathBuf,
        request: MapRequest,
        samples_per_axis: u16,
    },
}

#[derive(Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WorkerResponse {
    KnownSources {
        sources: Vec<KnownSource>,
    },
    RasterDimensions {
        width: usize,
        height: usize,
    },
    ProjectedPoint {
        east_meters: i64,
        north_meters: i64,
    },
    PreparedElevation {
        environment: PreparedEnvironment,
        pages: Vec<ElevationPage>,
    },
    PreparedOverview(Box<PreparedOverview>),
}

/// Source-backed overview data returned by the worker in a bounded response.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct PreparedOverview {
    pub source_lock: aoe_map::SourceLock,
    pub water_source_lock: aoe_map::SourceLock,
    pub vegetation_source_lock: aoe_map::SourceLock,
    pub vegetation_classes_source_lock: aoe_map::SourceLock,
    pub hyde_baseline_source_lock: aoe_map::SourceLock,
    pub hyde_supplementary_source_lock: aoe_map::SourceLock,
    pub hyde_readme_source_lock: aoe_map::SourceLock,
    pub projection: ProjectionMetadata,
    pub provenance: EnvironmentalProvenance,
    pub environment: PreparedEnvironment,
    pub pages: Vec<ElevationPage>,
    pub water_pages: Vec<WaterPage>,
    pub vegetation_pages: Vec<PotentialBiomePage>,
    pub historical_land_use_pages: Vec<HistoricalLandUsePage>,
}

/// A self-contained, source-backed map result suitable for offline package
/// verification or later persistence by a server adapter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeneratedMap {
    pub package: MapPackage,
    pub elevation_pages: Vec<ElevationPage>,
    pub water_pages: Vec<WaterPage>,
    pub vegetation_pages: Vec<PotentialBiomePage>,
    pub historical_land_use_pages: Vec<HistoricalLandUsePage>,
}

impl GeneratedMap {
    pub fn from_prepared(
        request: MapRequest,
        prepared: PreparedOverview,
    ) -> Result<Self, GeodataError> {
        let package = MapPackage::with_prepared_environment(
            MAP_SCHEMA_VERSION,
            request,
            vec![
                prepared.source_lock,
                prepared.water_source_lock,
                prepared.vegetation_source_lock,
                prepared.vegetation_classes_source_lock,
                prepared.hyde_baseline_source_lock,
                prepared.hyde_supplementary_source_lock,
                prepared.hyde_readme_source_lock,
            ],
            prepared.projection,
            prepared.provenance,
            prepared.environment,
        )?;
        Ok(Self {
            package,
            elevation_pages: prepared.pages,
            water_pages: prepared.water_pages,
            vegetation_pages: prepared.vegetation_pages,
            historical_land_use_pages: prepared.historical_land_use_pages,
        })
    }

    /// Confirms both the canonical manifest and every frozen page needed for
    /// terrain generation without asking a provider for additional data.
    pub fn validate(&self) -> Result<(), GeodataError> {
        self.package.validate()?;
        self.package.generator_with_environment(
            self.elevation_pages.clone(),
            self.water_pages.clone(),
            self.vegetation_pages.clone(),
            self.historical_land_use_pages.clone(),
        )?;
        Ok(())
    }
}

pub fn execute(request: WorkerRequest) -> Result<WorkerResponse, GeodataError> {
    match request {
        WorkerRequest::PrepareOverviewElevation {
            cache_root,
            request,
            samples_per_axis,
        } => Ok(WorkerResponse::PreparedOverview(Box::new(
            prepare_overview(cache_root, request, samples_per_axis)?,
        ))),
        WorkerRequest::ListOverviewSources => Ok(WorkerResponse::KnownSources {
            sources: vec![etopo_2022_60s_surface()],
        }),
        WorkerRequest::ListPotentialBiomeSources => Ok(WorkerResponse::KnownSources {
            sources: potential_biome_sources()?,
        }),
        WorkerRequest::ListHydeSources => Ok(WorkerResponse::KnownSources {
            sources: hyde_sources()?,
        }),
        WorkerRequest::InspectRaster { path } => {
            let dimensions = raster_dimensions(&path)?;
            Ok(WorkerResponse::RasterDimensions {
                width: dimensions.width,
                height: dimensions.height,
            })
        }
        WorkerRequest::ProjectPoint {
            center_latitude_e7,
            center_longitude_e7,
            longitude,
            latitude,
        } => {
            let definition = local_aeqd_definition(center_latitude_e7, center_longitude_e7);
            let (east_meters, north_meters) = project_wgs84(&definition, longitude, latitude)?;
            Ok(WorkerResponse::ProjectedPoint {
                east_meters: round_meters(east_meters)?,
                north_meters: round_meters(north_meters)?,
            })
        }
        WorkerRequest::PrepareElevation {
            path,
            request,
            samples_per_axis,
        } => {
            let prepared = prepare_elevation(&path, request, samples_per_axis)?;
            Ok(WorkerResponse::PreparedElevation {
                environment: prepared.environment,
                pages: prepared.pages,
            })
        }
    }
}

pub fn prepare_overview(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedOverview, GeodataError> {
    let source = etopo_2022_60s_surface();
    let lock = source
        .cache_lock()
        .ok_or(GeodataError::Preparation("overview source lacks SHA-256"))?;
    let water_source = natural_earth_10m_land();
    let water_lock = water_source
        .cache_lock()
        .ok_or(GeodataError::Preparation("coastline source lacks SHA-256"))?;
    let cache = SourceCache::new(cache_root, DownloadPolicy::default())?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let path = cache.acquire(&lock, &cancelled)?;
    let water_path = cache.acquire(&water_lock, &cancelled)?;
    let potential_sources = potential_biome_sources().ok();
    let vegetation_lock = acquire_or_cached(
        &cache,
        potential_sources.as_deref(),
        POTENTIAL_BIOME_RASTER_ID,
        &cancelled,
    )?;
    let vegetation_classes_lock = acquire_or_cached(
        &cache,
        potential_sources.as_deref(),
        POTENTIAL_BIOME_CLASSES_ID,
        &cancelled,
    )?;
    let vegetation_path = cache.object_path(&vegetation_lock)?;
    let vegetation_classes_path = cache.object_path(&vegetation_classes_lock)?;
    let hyde_sources = hyde_sources().ok();
    let hyde_baseline_lock = acquire_or_cached(
        &cache,
        hyde_sources.as_deref(),
        HYDE_BASELINE_ID,
        &cancelled,
    )?;
    let hyde_supplementary_lock = acquire_or_cached(
        &cache,
        hyde_sources.as_deref(),
        HYDE_SUPPLEMENTARY_ID,
        &cancelled,
    )?;
    let hyde_readme_lock =
        acquire_or_cached(&cache, hyde_sources.as_deref(), HYDE_README_ID, &cancelled)?;
    let hyde_baseline_path = cache.object_path(&hyde_baseline_lock)?;
    let hyde_supplementary_path = cache.object_path(&hyde_supplementary_lock)?;
    let mut prepared = prepare_elevation(&path, request, samples_per_axis)?;
    let lake_coverage =
        prepare_hyde_lake_coverage(&hyde_supplementary_path, request, samples_per_axis)?;
    let water = prepare_ocean_coverage(&water_path, request, samples_per_axis, lake_coverage)?;
    let vegetation = prepare_potential_biomes(&vegetation_path, request, samples_per_axis)?;
    let historical_land_use = prepare_hyde_600(
        &hyde_baseline_path,
        &hyde_supplementary_path,
        request,
        samples_per_axis,
    )?;
    verify_potential_biome_legend(&vegetation_classes_path)?;
    prepared.environment.water = Some(water.field);
    prepared.environment.vegetation = Some(vegetation.field);
    prepared.environment.historical_land_use = Some(historical_land_use.field);
    prepared.environment.validate()?;
    Ok(PreparedOverview {
        source_lock: lock
            .to_map_source_lock(acquisition_marker(), "etopo-overview-gdal-0.19".to_owned())?,
        water_source_lock: water_lock.to_map_source_lock(
            acquisition_marker(),
            "natural-earth-coastline-gdal-0.19".to_owned(),
        )?,
        vegetation_source_lock: vegetation_lock.to_map_source_lock(
            acquisition_marker(),
            "potential-biome-nearest-gdal-0.19".to_owned(),
        )?,
        vegetation_classes_source_lock: vegetation_classes_lock.to_map_source_lock(
            acquisition_marker(),
            "potential-biome-class-legend-v0.2".to_owned(),
        )?,
        hyde_baseline_source_lock: hyde_baseline_lock.to_map_source_lock(
            acquisition_marker(),
            "hyde-600ad-readonly-zip-v1".to_owned(),
        )?,
        hyde_supplementary_source_lock: hyde_supplementary_lock.to_map_source_lock(
            acquisition_marker(),
            "hyde-600ad-readonly-zip-v1".to_owned(),
        )?,
        hyde_readme_source_lock: hyde_readme_lock.to_map_source_lock(
            acquisition_marker(),
            "hyde-3.2.1-release-notes-v1".to_owned(),
        )?,
        projection: ProjectionMetadata {
            horizontal_crs: local_aeqd_definition(
                request.center_latitude_e7,
                request.center_longitude_e7,
            ),
            vertical_datum: VerticalDatum::Egm2008Orthometric,
            tool_version: "GDAL Rust bindings 0.19 / PROJ native".to_owned(),
        },
        provenance: EnvironmentalProvenance {
            elevation: LayerProvenance::SourceDerived,
            water: LayerProvenance::SourceDerived,
            vegetation: LayerProvenance::SourceDerived,
            historical_land_use: LayerProvenance::SourceDerived,
        },
        environment: prepared.environment,
        pages: prepared.pages,
        water_pages: water.pages,
        vegetation_pages: vegetation.pages,
        historical_land_use_pages: historical_land_use.pages,
    })
}

const POTENTIAL_BIOME_RASTER_ID: &str =
    "potential-biome-v0.2:pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif";
const POTENTIAL_BIOME_CLASSES_ID: &str =
    "potential-biome-v0.2:pnv_biome.type_biome00k_c_250m_s0..0cm_2000..2017_v0.2.tif.csv";
const HYDE_BASELINE_ID: &str = "hyde-3.2.1:HYDE3_2_1-baseline.zip";
const HYDE_SUPPLEMENTARY_ID: &str = "hyde-3.2.1:HYDE3_2_1-general_supplementary.zip";
const HYDE_README_ID: &str = "hyde-3.2.1:readme_release_HYDE3.2.1.txt";

fn acquire_or_cached(
    cache: &SourceCache,
    sources: Option<&[KnownSource]>,
    id: &str,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<SourceLock, GeodataError> {
    if let Some(source) = sources.and_then(|sources| sources.iter().find(|source| source.id == id))
    {
        return Ok(cache.acquire_known(source, cancelled)?);
    }
    cache.known_lock(id)?.ok_or(GeodataError::Preparation(
        "source catalog is unavailable and no verified cached source lock exists",
    ))
}

fn acquisition_marker() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix-seconds-{seconds}")
}

fn round_meters(value: f64) -> Result<i64, GeodataError> {
    if !value.is_finite() || value.abs() > i64::MAX as f64 {
        return Err(GeodataError::Coordinate);
    }
    Ok(value.round() as i64)
}

pub fn raster_dimensions(path: &Path) -> Result<RasterDimensions, GeodataError> {
    let dataset = Dataset::open(path)?;
    let (width, height) = dataset.raster_size();
    Ok(RasterDimensions { width, height })
}

pub fn local_aeqd_definition(latitude_e7: i32, longitude_e7: i32) -> String {
    format!(
        "+proj=aeqd +lat_0={:.7} +lon_0={:.7} +datum=WGS84 +units=m +no_defs",
        f64::from(latitude_e7) / 10_000_000.0,
        f64::from(longitude_e7) / 10_000_000.0
    )
}

pub fn project_wgs84(
    definition: &str,
    longitude: f64,
    latitude: f64,
) -> Result<(f64, f64), GeodataError> {
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    let mut target =
        SpatialRef::from_definition(definition).map_err(|_| GeodataError::Projection)?;
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)?;
    let mut east = [longitude];
    let mut north = [latitude];
    transform
        .transform_coords(&mut east, &mut north, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    Ok((east[0], north[0]))
}

#[cfg(test)]
#[path = "tests/geodata.rs"]
mod geodata_tests;
