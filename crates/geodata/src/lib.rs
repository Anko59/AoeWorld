//! Native-only GDAL and PROJ boundary for geographic source preparation.
//!
//! This crate intentionally does not enter the deterministic map core. It
//! validates local rasters and evaluates the documented azimuthal-equidistant
//! projection before a future preparation pipeline freezes map inputs.

use aoe_map::{
    EnvironmentalProvenance, LayerProvenance, MapRequest, ProjectionMetadata, VerticalDatum,
};
use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::path::{Path, PathBuf};

mod elevation;
#[path = "lib/progress.rs"]
pub mod preparation_progress;
pub use elevation::{MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, PreparedElevation, prepare_elevation};
mod directory;
pub use directory::{
    DIRECTORY_SCHEMA_VERSION, MAX_DIRECTORY_MANIFEST_BYTES, MAX_DIRECTORY_PAGE_BYTES,
};
mod copernicus;
pub use copernicus::{
    DemResolution, MAX_DETAILED_INPUT_BYTES, MAX_DETAILED_SAMPLES_PER_AXIS,
    MAX_DETAILED_STAGING_BYTES, MAX_DETAILED_TILES, prepare_detailed_directory,
};
mod footprint;
pub use footprint::{
    GeographicPoint, MAX_FOOTPRINT_SAMPLES_PER_EDGE, ProjectionDistortion, projected_footprint,
    projection_distortion,
};
mod hyde;
pub use hyde::{
    HISTORICAL_CORRECTION_SCHEMA_VERSION, HISTORICAL_CORRECTION_TARGET_YEAR_CE,
    HistoricalCorrection, HistoricalCorrectionDocument, HistoricalCorrectionEvidence,
    HydeAreaAllocation, HydeAreaState, HydeGeographicPoint, HydeSourceAreaCell, HydeTargetAreaCell,
    HydeWholeCellQuantities, MAX_HISTORICAL_CORRECTION_JSON_BYTES,
    MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS, MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS,
    PreparedHistoricalLandUse, allocate_hyde_area_window, prepare_hyde_600, prepare_hyde_area_600,
    prepare_hyde_area_pyramid, prepare_hyde_lake_coverage,
};
mod hydrology;
pub use hydrology::{
    HydrologyKind, HydrologyPage, MAX_HYDROLOGY_SAMPLES_PER_AXIS, PreparedHydrology,
    prepare_hydrology,
};
mod source_cache;
pub use source_cache::{
    AcquisitionEstimate, CacheError, DEFAULT_CACHE_QUOTA_BYTES,
    DEFAULT_JOB_ACQUISITION_BUDGET_BYTES, DownloadPolicy, Provider, SourceCache, SourceLock,
};
pub const MAX_OVERVIEW_INPUT_BYTES: u64 = 6 * 1024 * 1024 * 1024;
mod source_manifest;
mod water;
pub use water::{PreparedWater, prepare_ocean_coverage};
mod vegetation;
pub use vegetation::{
    PreparedVegetation, VEGETATION_PATCH_PREPROCESSING_IDENTITY, VegetationPatch,
    VegetationPatchBinding, VegetationPatchDocument, VegetationPatchOperation,
    VegetationPatchSource, prepare_potential_biomes, prepare_potential_biomes_with_corrections,
    verify_potential_biome_legend,
};
mod source_catalog;
pub(crate) use source_catalog::worldcover_sources_for_bounds_cached;
pub use source_catalog::{
    ExpectedChecksum, KnownSource, MAX_HYDROLOGY_DOWNLOAD_BYTES, MAX_WORLDCOVER_TILES,
    SourceCatalogError, etopo_2022_60s_surface, hyde_sources, hydrology_vector_sources,
    natural_earth_10m_land, potential_biome_sources, worldcover_sources_for_bounds,
    worldcover_tile_ids,
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
    #[error("historical correction format is invalid: {0}")]
    HistoricalCorrection(String),
    #[error("public Copernicus source failed: {0}")]
    Source(String),
    #[error("directory map package is invalid: {0}")]
    Directory(String),
    #[error(transparent)]
    Environment(#[from] aoe_map::EnvironmentError),
    #[error(transparent)]
    Package(#[from] aoe_map::MapPackageError),
    #[error(transparent)]
    SourceCatalog(#[from] SourceCatalogError),
    #[error(transparent)]
    Cache(#[from] CacheError),
}

mod map_result;
pub use map_result::{GeneratedMap, PreparedOverview, WorkerRequest, WorkerResponse};

mod worker;
pub use worker::execute;

pub fn prepare_overview(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<PreparedOverview, GeodataError> {
    prepare_overview_with_vegetation_corrections(cache_root, request, samples_per_axis, None)
}

pub fn prepare_overview_with_vegetation_corrections(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
    vegetation_corrections: Option<&VegetationPatchDocument>,
) -> Result<PreparedOverview, GeodataError> {
    let vegetation_corrections = match vegetation_corrections {
        Some(document) => std::borrow::Cow::Borrowed(document),
        None => std::borrow::Cow::Owned(VegetationPatchDocument::empty(request, samples_per_axis)?),
    };
    vegetation_corrections.validate_for(request, samples_per_axis)?;
    let source = etopo_2022_60s_surface();
    let lock = source
        .cache_lock()
        .ok_or(GeodataError::Preparation("overview source lacks SHA-256"))?;
    let water_source = natural_earth_10m_land();
    let water_lock = water_source
        .cache_lock()
        .ok_or(GeodataError::Preparation("coastline source lacks SHA-256"))?;
    let cache = SourceCache::new(
        cache_root,
        DownloadPolicy {
            cache_quota_bytes: DEFAULT_CACHE_QUOTA_BYTES,
            job_acquisition_budget_bytes: MAX_OVERVIEW_INPUT_BYTES,
        },
    )?;
    let potential_sources = potential_biome_sources().ok();
    let hyde_sources = hyde_sources().ok();
    preflight_overview_acquisition(
        &cache,
        potential_sources.as_deref(),
        hyde_sources.as_deref(),
    )?;
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let path = cache.acquire(&lock, &cancelled)?;
    let water_path = cache.acquire(&water_lock, &cancelled)?;
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
    let total_pages = preparation_progress::pyramid_page_count(samples_per_axis) * 4;
    let mut completed_pages = 0;
    preparation_progress::stage(preparation_progress::Phase::SamplingOverview);
    let mut prepared = prepare_elevation(&path, request, samples_per_axis)?;
    completed_pages += prepared.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let lake_coverage =
        prepare_hyde_lake_coverage(&hyde_supplementary_path, request, samples_per_axis)?;
    let water = prepare_ocean_coverage(&water_path, request, samples_per_axis, lake_coverage)?;
    completed_pages += water.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let vegetation = prepare_potential_biomes_with_corrections(
        &vegetation_path,
        request,
        samples_per_axis,
        &vegetation_corrections,
    )?;
    completed_pages += vegetation.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
    let historical_land_use = prepare_hyde_area_600(
        &hyde_baseline_path,
        &hyde_supplementary_path,
        request,
        samples_per_axis,
    )?;
    completed_pages += historical_land_use.pages.len() as u64;
    preparation_progress::count(
        preparation_progress::Phase::SamplingOverview,
        None,
        completed_pages,
        total_pages,
        preparation_progress::Unit::Pages,
    );
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
            format!(
                "{};corrections-sha256={}",
                VEGETATION_PATCH_PREPROCESSING_IDENTITY,
                vegetation_corrections.digest_hex(request)?
            ),
        )?,
        vegetation_classes_source_lock: vegetation_classes_lock.to_map_source_lock(
            acquisition_marker(),
            "potential-biome-class-legend-v0.2".to_owned(),
        )?,
        hyde_baseline_source_lock: hyde_baseline_lock
            .to_map_source_lock(acquisition_marker(), "hyde-600ad-area-pages-v2".to_owned())?,
        hyde_supplementary_source_lock: hyde_supplementary_lock
            .to_map_source_lock(acquisition_marker(), "hyde-600ad-area-pages-v2".to_owned())?,
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
            vegetation: if vegetation_corrections.changes_historical_vegetation() {
                LayerProvenance::HistoricallyCorrected
            } else {
                LayerProvenance::SourceDerived
            },
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

fn preflight_overview_acquisition(
    cache: &SourceCache,
    potential_sources: Option<&[KnownSource]>,
    hyde_sources: Option<&[KnownSource]>,
) -> Result<(), GeodataError> {
    let mut batch = vec![etopo_2022_60s_surface(), natural_earth_10m_land()];
    for (sources, id) in [
        (potential_sources, POTENTIAL_BIOME_RASTER_ID),
        (potential_sources, POTENTIAL_BIOME_CLASSES_ID),
        (hyde_sources, HYDE_BASELINE_ID),
        (hyde_sources, HYDE_SUPPLEMENTARY_ID),
        (hyde_sources, HYDE_README_ID),
    ] {
        if let Some(source) =
            sources.and_then(|sources| sources.iter().find(|source| source.id == id))
        {
            batch.push(source.clone());
        } else if cache.known_lock(id)?.is_none() {
            return Err(GeodataError::Preparation(
                "overview source catalog is unavailable and no verified cached lock exists",
            ));
        }
    }
    cache.estimate_known_acquisition(&batch)?;
    Ok(())
}

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
