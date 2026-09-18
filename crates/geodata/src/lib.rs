//! Native-only GDAL and PROJ boundary for geographic source preparation.
//!
//! This crate intentionally does not enter the deterministic map core. It
//! validates local rasters and evaluates the documented azimuthal-equidistant
//! projection before a future preparation pipeline freezes map inputs.

use aoe_map::{
    ElevationPage, EnvironmentalProvenance, LayerProvenance, MapRequest, PotentialBiomePage,
    PreparedEnvironment, ProjectionMetadata, VerticalDatum, WaterPage,
};
use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

mod elevation;
pub use elevation::{MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, PreparedElevation, prepare_elevation};

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
    source_lock: aoe_map::SourceLock,
    water_source_lock: aoe_map::SourceLock,
    vegetation_source_lock: aoe_map::SourceLock,
    vegetation_classes_source_lock: aoe_map::SourceLock,
    projection: ProjectionMetadata,
    provenance: EnvironmentalProvenance,
    environment: PreparedEnvironment,
    pages: Vec<ElevationPage>,
    water_pages: Vec<WaterPage>,
    vegetation_pages: Vec<PotentialBiomePage>,
}

pub fn execute(request: WorkerRequest) -> Result<WorkerResponse, GeodataError> {
    match request {
        WorkerRequest::PrepareOverviewElevation {
            cache_root,
            request,
            samples_per_axis,
        } => prepare_overview_elevation(cache_root, request, samples_per_axis),
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

fn prepare_overview_elevation(
    cache_root: PathBuf,
    request: MapRequest,
    samples_per_axis: u16,
) -> Result<WorkerResponse, GeodataError> {
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
    let potential_sources = potential_biome_sources()?;
    let vegetation_source = potential_sources
        .iter()
        .find(|source| source.id.ends_with(".tif"))
        .ok_or(GeodataError::Preparation(
            "potential biome raster is missing",
        ))?;
    let vegetation_classes_source = potential_sources
        .iter()
        .find(|source| source.id.ends_with(".tif.csv"))
        .ok_or(GeodataError::Preparation(
            "potential biome class legend is missing",
        ))?;
    let vegetation_lock = cache.acquire_known(vegetation_source, &cancelled)?;
    let vegetation_classes_lock = cache.acquire_known(vegetation_classes_source, &cancelled)?;
    let vegetation_path = cache.object_path(&vegetation_lock)?;
    let vegetation_classes_path = cache.object_path(&vegetation_classes_lock)?;
    let mut prepared = prepare_elevation(&path, request, samples_per_axis)?;
    let water = prepare_ocean_coverage(&water_path, request, samples_per_axis)?;
    let vegetation = prepare_potential_biomes(&vegetation_path, request, samples_per_axis)?;
    verify_potential_biome_legend(&vegetation_classes_path)?;
    prepared.environment.water = Some(water.field);
    prepared.environment.vegetation = Some(vegetation.field);
    prepared.environment.validate()?;
    Ok(WorkerResponse::PreparedOverview(Box::new(
        PreparedOverview {
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
                historical_land_use: LayerProvenance::Fallback,
            },
            environment: prepared.environment,
            pages: prepared.pages,
            water_pages: water.pages,
            vegetation_pages: vegetation.pages,
        },
    )))
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
mod tests {
    use super::*;

    #[test]
    fn local_projection_places_its_center_at_the_origin() {
        let definition = local_aeqd_definition(488_500_000, 23_500_000);
        let (east, north) = project_wgs84(&definition, 2.35, 48.85).expect("projection");
        assert!(east.abs() < 0.01);
        assert!(north.abs() < 0.01);
    }

    #[test]
    fn worker_projects_the_requested_center_to_zero_meters() {
        let response = execute(WorkerRequest::ProjectPoint {
            center_latitude_e7: 488_500_000,
            center_longitude_e7: 23_500_000,
            longitude: 2.35,
            latitude: 48.85,
        })
        .expect("projected point");
        assert_eq!(
            response,
            WorkerResponse::ProjectedPoint {
                east_meters: 0,
                north_meters: 0,
            }
        );
    }

    #[test]
    fn worker_lists_only_the_allowlisted_global_overview() {
        let response = execute(WorkerRequest::ListOverviewSources).expect("sources");
        let WorkerResponse::KnownSources { sources } = response else {
            panic!("source response");
        };
        assert_eq!(sources, vec![etopo_2022_60s_surface()]);
    }

    #[test]
    fn overview_response_keeps_its_bounded_protocol_shape_when_boxed() {
        let response = WorkerResponse::PreparedOverview(Box::new(PreparedOverview {
            source_lock: aoe_map::SourceLock {
                id: "overview".to_owned(),
                provider: "provider".to_owned(),
                release: "release".to_owned(),
                url: "https://example.invalid/overview.tif".to_owned(),
                sha256: [7; 32],
                acquired_at: "2026-09-18".to_owned(),
                native_resolution: "1 arc-minute".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "EGM2008".to_owned(),
                license: "test".to_owned(),
                preprocessing_version: "test".to_owned(),
            },
            water_source_lock: aoe_map::SourceLock {
                id: "coastline".to_owned(),
                provider: "provider".to_owned(),
                release: "release".to_owned(),
                url: "https://example.invalid/land.zip".to_owned(),
                sha256: [8; 32],
                acquired_at: "2026-09-18".to_owned(),
                native_resolution: "1:10m".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license: "test".to_owned(),
                preprocessing_version: "test".to_owned(),
            },
            vegetation_source_lock: aoe_map::SourceLock {
                id: "vegetation".to_owned(),
                provider: "provider".to_owned(),
                release: "release".to_owned(),
                url: "https://example.invalid/vegetation.tif".to_owned(),
                sha256: [9; 32],
                acquired_at: "2026-09-18".to_owned(),
                native_resolution: "250m".to_owned(),
                crs: "EPSG:4326".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license: "test".to_owned(),
                preprocessing_version: "test".to_owned(),
            },
            vegetation_classes_source_lock: aoe_map::SourceLock {
                id: "vegetation-classes".to_owned(),
                provider: "provider".to_owned(),
                release: "release".to_owned(),
                url: "https://example.invalid/vegetation.csv".to_owned(),
                sha256: [10; 32],
                acquired_at: "2026-09-18".to_owned(),
                native_resolution: "table".to_owned(),
                crs: "not applicable".to_owned(),
                vertical_datum: "not applicable".to_owned(),
                license: "test".to_owned(),
                preprocessing_version: "test".to_owned(),
            },
            projection: ProjectionMetadata::default(),
            provenance: EnvironmentalProvenance::default(),
            environment: PreparedEnvironment::default(),
            pages: Vec::new(),
            water_pages: Vec::new(),
            vegetation_pages: Vec::new(),
        }));
        let encoded = serde_json::to_value(response).expect("serializes");
        assert_eq!(encoded["operation"], "prepared_overview");
        assert_eq!(encoded["source_lock"]["id"], "overview");
        assert_eq!(encoded["water_source_lock"]["id"], "coastline");
        assert_eq!(encoded["vegetation_source_lock"]["id"], "vegetation");
        assert!(encoded.get("value").is_none());
    }
}
