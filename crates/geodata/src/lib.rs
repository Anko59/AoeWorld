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
    prepare_detailed_directory_with_water_corrections,
};
mod footprint;
pub use footprint::{
    GeographicPoint, MAX_FOOTPRINT_SAMPLES_PER_EDGE, ProjectionDistortion, projected_footprint,
    projection_distortion,
};
mod hyde;
pub use hyde::{
    GEOGRAPHIC_HISTORICAL_CORRECTION_SCHEMA_VERSION, GeographicHistoricalCorrection,
    GeographicHistoricalCorrectionDocument, GeographicHistoricalEvidence,
    HISTORICAL_CORRECTION_SCHEMA_VERSION, HISTORICAL_CORRECTION_TARGET_YEAR_CE,
    HistoricalCorrection, HistoricalCorrectionDocument, HistoricalCorrectionEvidence,
    HistoricalGridBinding, HistoricalQuantityPatch, HistoricalSourceCitation, HydeAreaAllocation,
    HydeAreaState, HydeGeographicPoint, HydeSourceAreaCell, HydeTargetAreaCell,
    HydeWholeCellQuantities, MAX_HISTORICAL_CORRECTION_JSON_BYTES,
    MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS, MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS,
    PreparedHistoricalLandUse, allocate_hyde_area_window, prepare_hyde_600, prepare_hyde_area_600,
    prepare_hyde_area_600_with_corrections, prepare_hyde_area_pyramid, prepare_hyde_lake_coverage,
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

#[path = "lib/overview.rs"]
mod overview;
pub use overview::{
    prepare_overview, prepare_overview_with_all_corrections, prepare_overview_with_corrections,
    prepare_overview_with_historical_axis, prepare_overview_with_vegetation_corrections,
};

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
