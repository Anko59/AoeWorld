use crate::{GeodataError, local_aeqd_definition};
use aoe_map::MapRequest;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use serde::{Deserialize, Serialize};

pub const MAX_FOOTPRINT_SAMPLES_PER_EDGE: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeographicPoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

/// Returns a closed, densified geographic boundary for the actual square in
/// the local WGS84 azimuthal-equidistant projection.
pub fn projected_footprint(
    request: MapRequest,
    samples_per_edge: u8,
) -> Result<Vec<GeographicPoint>, GeodataError> {
    if !(1..=MAX_FOOTPRINT_SAMPLES_PER_EDGE).contains(&samples_per_edge) {
        return Err(GeodataError::Preparation(
            "footprint samples per edge are outside the supported bound",
        ));
    }
    let request = request
        .normalized()
        .map_err(aoe_map::MapPackageError::from)?;
    let estimate = request.estimate().map_err(aoe_map::MapPackageError::from)?;
    let side = estimate.effective_side_meters as f64;
    let half = side / 2.0;
    let mut coordinates = Vec::with_capacity(usize::from(samples_per_edge) * 4 + 1);
    for edge in 0..4 {
        for sample in 0..samples_per_edge {
            let fraction = f64::from(sample) / f64::from(samples_per_edge);
            let coordinate = -half + side * fraction;
            coordinates.push(match edge {
                0 => (coordinate, half),
                1 => (half, -coordinate),
                2 => (-coordinate, -half),
                _ => (-half, coordinate),
            });
        }
    }
    coordinates.push((-half, half));
    let definition = local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    coordinates
        .into_iter()
        .map(|(east, north)| inverse_project_e7(&definition, east, north))
        .collect()
}

fn inverse_project_e7(
    definition: &str,
    east: f64,
    north: f64,
) -> Result<GeographicPoint, GeodataError> {
    let mut source =
        SpatialRef::from_definition(definition).map_err(|_| GeodataError::Projection)?;
    let mut target = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let transform = CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)?;
    let mut longitude = [east];
    let mut latitude = [north];
    transform
        .transform_coords(&mut longitude, &mut latitude, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    Ok(GeographicPoint {
        latitude_e7: rounded_e7(latitude[0])?,
        longitude_e7: rounded_e7(longitude[0])?,
    })
}

fn rounded_e7(value: f64) -> Result<i32, GeodataError> {
    let value = value * 10_000_000.0;
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(GeodataError::Coordinate);
    }
    Ok(value.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footprint_is_closed_and_centered_on_the_requested_projection() {
        let points = projected_footprint(MapRequest::default(), 8).expect("footprint");
        assert_eq!(points.len(), 33);
        assert_eq!(points.first(), points.last());
        let north = points
            .iter()
            .map(|point| point.latitude_e7)
            .max()
            .expect("north");
        let south = points
            .iter()
            .map(|point| point.latitude_e7)
            .min()
            .expect("south");
        assert!(north > 488_500_000);
        assert!(south < 488_500_000);
    }
}
