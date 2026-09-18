//! Native-only GDAL and PROJ boundary for geographic source preparation.
//!
//! This crate intentionally does not enter the deterministic map core. It
//! validates local rasters and evaluates the documented azimuthal-equidistant
//! projection before a future preparation pipeline freezes map inputs.

use aoe_map::{MapRequest, PreparedEnvironment};
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
    DEFAULT_CACHE_QUOTA_BYTES, DEFAULT_JOB_ACQUISITION_BUDGET_BYTES, DownloadPolicy, Provider,
    SourceCache, SourceLock,
};

mod source_catalog;
pub use source_catalog::{KnownSource, SourceCatalogError, potential_biome_sources};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterDimensions {
    pub width: usize,
    pub height: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum GeodataError {
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
}

/// A bounded native-worker operation passed on stdin by a direct process spawn.
#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum WorkerRequest {
    ListPotentialBiomeSources,
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
        page_count: usize,
    },
}

pub fn execute(request: WorkerRequest) -> Result<WorkerResponse, GeodataError> {
    match request {
        WorkerRequest::ListPotentialBiomeSources => Ok(WorkerResponse::KnownSources {
            sources: potential_biome_sources()?,
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
                page_count: prepared.pages.len(),
            })
        }
    }
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
}
