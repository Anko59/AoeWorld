//! Native-only GDAL and PROJ boundary for geographic source preparation.
//!
//! This crate intentionally does not enter the deterministic map core. It
//! validates local rasters and evaluates the documented azimuthal-equidistant
//! projection before a future preparation pipeline freezes map inputs.

use gdal::{
    Dataset,
    spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef},
};
use std::path::Path;

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
}
