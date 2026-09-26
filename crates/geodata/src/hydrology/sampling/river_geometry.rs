use super::super::MAX_PAGE_GEOMETRY_BYTES;
use gdal::vector::{Feature, FieldValue, Geometry};

pub(in crate::hydrology) struct RiverReachGeometry {
    pub(in crate::hydrology) id: u32,
    pub(in crate::hydrology) next_down_id: u32,
    pub(in crate::hydrology) distance_to_sink_centimeters: u32,
    pub(in crate::hydrology) line_points: Vec<(f64, f64)>,
}

/// Returns the closest point's distance from and along a projected line string.
pub(in crate::hydrology) fn line_position(
    points: &[(f64, f64)],
    x: f64,
    y: f64,
) -> Option<(f64, f64)> {
    if points.len() < 2 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    let mut best_distance_squared = f64::INFINITY;
    let mut best_station = 0.0;
    let mut station = 0.0;
    for pair in points.windows(2) {
        let (start_x, start_y) = pair[0];
        let (end_x, end_y) = pair[1];
        let segment_x = end_x - start_x;
        let segment_y = end_y - start_y;
        let segment_length_squared = segment_x * segment_x + segment_y * segment_y;
        let segment_length = segment_length_squared.sqrt();
        if !segment_length.is_finite() {
            return None;
        }
        if segment_length_squared > 0.0 {
            let fraction = (((x - start_x) * segment_x + (y - start_y) * segment_y)
                / segment_length_squared)
                .clamp(0.0, 1.0);
            let nearest_x = start_x + fraction * segment_x;
            let nearest_y = start_y + fraction * segment_y;
            let dx = x - nearest_x;
            let dy = y - nearest_y;
            let distance_squared = dx * dx + dy * dy;
            if distance_squared < best_distance_squared {
                best_distance_squared = distance_squared;
                best_station = station + fraction * segment_length;
            }
        }
        station += segment_length;
    }
    best_distance_squared
        .is_finite()
        .then_some((best_distance_squared.sqrt(), best_station))
}

pub(in crate::hydrology) fn river_reach_geometry(
    feature: &Feature<'_>,
    geometry: &Geometry,
) -> Option<RiverReachGeometry> {
    if geometry.geometry_name() != "LINESTRING"
        || geometry.point_count() > MAX_PAGE_GEOMETRY_BYTES / std::mem::size_of::<(f64, f64)>()
    {
        return None;
    }
    let integer_field = |name: &str| {
        let index = feature.field_index(name).ok()?;
        feature.field(index).ok()??.into_int64()
    };
    let real_field = |name: &str| {
        let index = feature.field_index(name).ok()?;
        let value = feature.field(index).ok()??;
        match value {
            FieldValue::RealValue(value) => Some(value),
            FieldValue::IntegerValue(value) => Some(f64::from(value)),
            FieldValue::Integer64Value(value) => Some(value as f64),
            _ => None,
        }
    };
    let id = u32::try_from(integer_field("HYRIV_ID")?).ok()?;
    let next_down_id = u32::try_from(integer_field("NEXT_DOWN")?).ok()?;
    let distance_to_sink_km = real_field("DIST_DN_KM")?;
    if id == 0 || !distance_to_sink_km.is_finite() || distance_to_sink_km < 0.0 {
        return None;
    }
    let distance_to_sink_centimeters = distance_to_sink_km * 100_000.0;
    if distance_to_sink_centimeters > f64::from(u32::MAX) {
        return None;
    }
    let mut points_3d = Vec::with_capacity(geometry.point_count());
    geometry.get_points(&mut points_3d);
    let line_points = points_3d
        .into_iter()
        .map(|(x, y, _)| (x, y))
        .collect::<Vec<_>>();
    if line_points.len() < 2
        || line_points
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return None;
    }
    Some(RiverReachGeometry {
        id,
        next_down_id,
        distance_to_sink_centimeters: distance_to_sink_centimeters.round() as u32,
        line_points,
    })
}
