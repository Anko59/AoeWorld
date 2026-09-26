use super::super::{HydeAreaState, HydeTargetAreaCell};
use crate::GeodataError;
use aoe_map::MapRequest;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum RasterValue {
    Data(f64),
    NoData,
    Invalid,
}

const EARTH_RADIUS_METERS: f64 = 6_371_008.8;

pub(super) fn classify_raster_value(value: f64, nodata: Option<f64>) -> RasterValue {
    if nodata.is_some_and(|missing| {
        if missing.is_nan() {
            value.is_nan()
        } else {
            value == missing
        }
    }) {
        RasterValue::NoData
    } else if value.is_finite() {
        RasterValue::Data(value)
    } else {
        RasterValue::Invalid
    }
}

pub(super) fn classify_land_lake_value(value: RasterValue) -> Result<HydeAreaState, GeodataError> {
    match value {
        // The HYDE 600 land/lake mask documents its declared nodata sentinel
        // as ocean. Outside-raster target area is tracked separately.
        RasterValue::NoData => Ok(HydeAreaState::Ocean),
        RasterValue::Data(1.0) => Ok(HydeAreaState::Land),
        RasterValue::Data(0.0) => Ok(HydeAreaState::Lake),
        RasterValue::Data(_) => Err(GeodataError::Preparation(
            "HYDE land-lake mask has an unsupported value",
        )),
        RasterValue::Invalid => Err(GeodataError::Preparation(
            "HYDE land-lake mask contains a non-finite value",
        )),
    }
}

pub(super) fn quantity_value(value: RasterValue) -> Result<Option<f64>, GeodataError> {
    match value {
        RasterValue::Data(value) => Ok(Some(value)),
        RasterValue::NoData => Ok(None),
        RasterValue::Invalid => Err(GeodataError::Preparation(
            "HYDE land quantity contains a non-finite value",
        )),
    }
}

pub(crate) fn validate_target_geography(
    targets: &[HydeTargetAreaCell],
) -> Result<(), GeodataError> {
    for target in targets {
        if target.polygon.len() < 3 {
            return Err(GeodataError::Preparation("HYDE target geometry is empty"));
        }
        for (index, point) in target.polygon.iter().enumerate() {
            if !point.longitude_degrees.is_finite()
                || !point.latitude_degrees.is_finite()
                || point.longitude_degrees.abs() > 540.0
            {
                return Err(GeodataError::Preparation(
                    "HYDE target geometry has invalid coordinates",
                ));
            }
            if point.latitude_degrees.abs() >= 90.0 - 1.0e-8 {
                return Err(GeodataError::Preparation(
                    "HYDE target geometry touches or crosses a pole",
                ));
            }
            let next = target.polygon[(index + 1) % target.polygon.len()];
            if (point.longitude_degrees - next.longitude_degrees).abs() >= 180.0 {
                return Err(GeodataError::Preparation(
                    "HYDE target polygon is not unwrapped around its projection center",
                ));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_polar_footprint(
    request: MapRequest,
    side_meters: u64,
) -> Result<(), GeodataError> {
    let latitude = (f64::from(request.center_latitude_e7) / 10_000_000.0).to_radians();
    let distance_to_nearest_pole =
        EARTH_RADIUS_METERS * (std::f64::consts::FRAC_PI_2 - latitude.abs());
    if side_meters as f64 / 2.0 >= distance_to_nearest_pole - 1.0e-6 {
        return Err(GeodataError::Preparation(
            "HYDE target footprint touches or crosses a pole",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_nodata_is_ocean_but_unexpected_nan_fails_closed() {
        let nodata = classify_raster_value(-9_999.0, Some(-9_999.0));
        let nan = classify_raster_value(f64::NAN, Some(-9_999.0));

        assert!(matches!(
            classify_land_lake_value(nodata),
            Ok(HydeAreaState::Ocean)
        ));
        assert!(matches!(
            classify_land_lake_value(nan),
            Err(GeodataError::Preparation(
                "HYDE land-lake mask contains a non-finite value"
            ))
        ));
        assert_eq!(
            classify_raster_value(f64::NAN, Some(f64::NAN)),
            RasterValue::NoData
        );
    }

    #[test]
    fn polar_footprints_fail_before_raster_window_selection() {
        let request = MapRequest {
            center_latitude_e7: 899_000_000,
            requested_side_meters: 80_000,
            ..MapRequest::default()
        };

        assert!(matches!(
            validate_polar_footprint(request, request.requested_side_meters),
            Err(GeodataError::Preparation(
                "HYDE target footprint touches or crosses a pole"
            ))
        ));
    }
}
