use crate::{GeodataError, local_aeqd_definition};
use aoe_map::MapRequest;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use serde::{Deserialize, Serialize};

pub const MAX_FOOTPRINT_SAMPLES_PER_EDGE: u8 = 64;
const DISTORTION_GRID_CELLS_PER_SIDE: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GeographicPoint {
    pub latitude_e7: i32,
    pub longitude_e7: i32,
}

/// The range of local-projection scale error across a bounded square,
/// expressed in parts per million. Positive values mean a projected meter is
/// longer than its WGS84 ellipsoidal ground-distance counterpart.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectionDistortion {
    pub min_scale_error_ppm: i64,
    pub max_scale_error_ppm: i64,
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

/// Samples east/west and north/south local-grid segments, comparing their
/// projected length against a WGS84 ellipsoidal inverse distance. The sample
/// count is fixed, so this diagnostic remains bounded even for continental
/// selections.
pub fn projection_distortion(request: MapRequest) -> Result<ProjectionDistortion, GeodataError> {
    let request = request
        .normalized()
        .map_err(aoe_map::MapPackageError::from)?;
    let estimate = request.estimate().map_err(aoe_map::MapPackageError::from)?;
    let side = estimate.effective_side_meters as f64;
    let half = side / 2.0;
    let cells = f64::from(DISTORTION_GRID_CELLS_PER_SIDE);
    let segment = side / cells;
    let definition = local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7);
    let mut min_error = i64::MAX;
    let mut max_error = i64::MIN;
    let mut previous_row = Vec::with_capacity(usize::from(DISTORTION_GRID_CELLS_PER_SIDE) + 1);
    for y in 0..=DISTORTION_GRID_CELLS_PER_SIDE {
        let north = half - segment * f64::from(y);
        let mut row = Vec::with_capacity(usize::from(DISTORTION_GRID_CELLS_PER_SIDE) + 1);
        for x in 0..=DISTORTION_GRID_CELLS_PER_SIDE {
            let east = -half + segment * f64::from(x);
            let coordinate = inverse_project(&definition, east, north)?;
            if let Some(west) = row.last().copied() {
                accumulate_scale_error(&mut min_error, &mut max_error, segment, west, coordinate)?;
            }
            if let Some(previous) = previous_row.get(usize::from(x)).copied() {
                accumulate_scale_error(
                    &mut min_error,
                    &mut max_error,
                    segment,
                    previous,
                    coordinate,
                )?;
            }
            row.push(coordinate);
        }
        previous_row = row;
    }
    if min_error == i64::MAX || max_error == i64::MIN {
        return Err(GeodataError::Coordinate);
    }
    Ok(ProjectionDistortion {
        min_scale_error_ppm: min_error,
        max_scale_error_ppm: max_error,
    })
}

fn inverse_project_e7(
    definition: &str,
    east: f64,
    north: f64,
) -> Result<GeographicPoint, GeodataError> {
    let (longitude, latitude) = inverse_project(definition, east, north)?;
    Ok(GeographicPoint {
        latitude_e7: rounded_e7(latitude)?,
        longitude_e7: rounded_e7(longitude)?,
    })
}

fn inverse_project(definition: &str, east: f64, north: f64) -> Result<(f64, f64), GeodataError> {
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
    if !longitude[0].is_finite() || !latitude[0].is_finite() {
        return Err(GeodataError::Coordinate);
    }
    Ok((longitude[0], latitude[0]))
}

fn accumulate_scale_error(
    min_error: &mut i64,
    max_error: &mut i64,
    projected_meters: f64,
    first: (f64, f64),
    second: (f64, f64),
) -> Result<(), GeodataError> {
    let ground_meters = wgs84_distance_meters(first, second)?;
    let error = ((projected_meters / ground_meters) - 1.0) * 1_000_000.0;
    if !error.is_finite() || error < i64::MIN as f64 || error > i64::MAX as f64 {
        return Err(GeodataError::Coordinate);
    }
    let error = error.round() as i64;
    *min_error = (*min_error).min(error);
    *max_error = (*max_error).max(error);
    Ok(())
}

/// Vincenty's inverse solution on the WGS84 ellipsoid. The selected map
/// envelope keeps each short grid segment far from the antipodal singularity.
fn wgs84_distance_meters(first: (f64, f64), second: (f64, f64)) -> Result<f64, GeodataError> {
    const SEMI_MAJOR_AXIS: f64 = 6_378_137.0;
    const FLATTENING: f64 = 1.0 / 298.257_223_563;
    const SEMI_MINOR_AXIS: f64 = SEMI_MAJOR_AXIS * (1.0 - FLATTENING);
    let longitude1 = first.0.to_radians();
    let latitude1 = first.1.to_radians();
    let longitude2 = second.0.to_radians();
    let latitude2 = second.1.to_radians();
    let reduced1 = ((1.0 - FLATTENING) * latitude1.tan()).atan();
    let reduced2 = ((1.0 - FLATTENING) * latitude2.tan()).atan();
    let (sin1, cos1) = reduced1.sin_cos();
    let (sin2, cos2) = reduced2.sin_cos();
    let difference = longitude2 - longitude1;
    let mut lambda = difference;
    let mut cos_squared_alpha = 0.0;
    let mut sine_sigma = 0.0;
    let mut cosine_sigma = 1.0;
    let mut sigma = 0.0;
    let mut cosine_two_sigma_midpoint = 0.0;
    let mut converged = false;
    for _ in 0..100 {
        let (sine_lambda, cosine_lambda) = lambda.sin_cos();
        let first_term = cos2 * sine_lambda;
        let second_term = cos1 * sin2 - sin1 * cos2 * cosine_lambda;
        sine_sigma = (first_term * first_term + second_term * second_term).sqrt();
        if sine_sigma == 0.0 {
            return Ok(0.0);
        }
        cosine_sigma = sin1 * sin2 + cos1 * cos2 * cosine_lambda;
        sigma = sine_sigma.atan2(cosine_sigma);
        let sine_alpha = cos1 * cos2 * sine_lambda / sine_sigma;
        cos_squared_alpha = 1.0 - sine_alpha * sine_alpha;
        cosine_two_sigma_midpoint = if cos_squared_alpha <= f64::EPSILON {
            0.0
        } else {
            cosine_sigma - 2.0 * sin1 * sin2 / cos_squared_alpha
        };
        let correction = FLATTENING / 16.0
            * cos_squared_alpha
            * (4.0 + FLATTENING * (4.0 - 3.0 * cos_squared_alpha));
        let next = difference
            + (1.0 - correction)
                * FLATTENING
                * sine_alpha
                * (sigma
                    + correction
                        * sine_sigma
                        * (cosine_two_sigma_midpoint
                            + correction
                                * cosine_sigma
                                * (-1.0 + 2.0 * cosine_two_sigma_midpoint.powi(2))));
        if (next - lambda).abs() <= 1e-12 {
            converged = true;
            break;
        }
        lambda = next;
    }
    if !converged {
        return Err(GeodataError::Coordinate);
    }
    let squared_u = cos_squared_alpha * (SEMI_MAJOR_AXIS.powi(2) - SEMI_MINOR_AXIS.powi(2))
        / SEMI_MINOR_AXIS.powi(2);
    let coefficient_a = 1.0
        + squared_u / 16_384.0
            * (4_096.0 + squared_u * (-768.0 + squared_u * (320.0 - 175.0 * squared_u)));
    let coefficient_b = squared_u / 1_024.0
        * (256.0 + squared_u * (-128.0 + squared_u * (74.0 - 47.0 * squared_u)));
    let delta_sigma = coefficient_b
        * sine_sigma
        * (cosine_two_sigma_midpoint
            + coefficient_b / 4.0
                * (cosine_sigma * (-1.0 + 2.0 * cosine_two_sigma_midpoint.powi(2))
                    - coefficient_b / 6.0
                        * cosine_two_sigma_midpoint
                        * (-3.0 + 4.0 * sine_sigma.powi(2))
                        * (-3.0 + 4.0 * cosine_two_sigma_midpoint.powi(2))));
    let distance = SEMI_MINOR_AXIS * coefficient_a * (sigma - delta_sigma);
    if distance.is_finite() && distance > 0.0 {
        Ok(distance)
    } else {
        Err(GeodataError::Coordinate)
    }
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

    #[test]
    fn distortion_is_bounded_and_grows_with_a_continental_footprint() {
        let local = projection_distortion(MapRequest::default()).expect("local distortion");
        let continental = projection_distortion(MapRequest {
            requested_side_meters: 3_000_000,
            ..MapRequest::default()
        })
        .expect("continental distortion");
        assert!(local.min_scale_error_ppm <= local.max_scale_error_ppm);
        assert!(continental.min_scale_error_ppm <= continental.max_scale_error_ppm);
        assert!(continental.max_scale_error_ppm > local.max_scale_error_ppm);
    }
}
