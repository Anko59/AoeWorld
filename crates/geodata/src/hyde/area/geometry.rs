use super::HydeGeographicPoint;
use crate::GeodataError;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use std::cmp::Ordering;

const MAX_POLYGON_VERTICES: usize = 16;
const AREA_EPSILON: f64 = 1.0e-9;
const DENSIFY_STEP_DEGREES: f64 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Point {
    pub(super) x: f64,
    pub(super) y: f64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    pub(super) min_x: f64,
    pub(super) min_y: f64,
    pub(super) max_x: f64,
    pub(super) max_y: f64,
}

#[derive(Clone, Debug)]
pub(super) struct Polygon {
    pub(super) points: Vec<Point>,
    pub(super) triangles: Vec<[Point; 3]>,
    pub(super) bounds: Bounds,
    pub(super) area: f64,
}

pub(super) fn equal_area_transform(
    latitude_e7: i32,
    longitude_e7: i32,
) -> Result<CoordTransform, GeodataError> {
    let latitude = f64::from(latitude_e7) / 10_000_000.0;
    let longitude = f64::from(longitude_e7) / 10_000_000.0;
    let definition = format!(
        "+proj=laea +lat_0={latitude:.7} +lon_0={longitude:.7} +datum=WGS84 +units=m +no_defs +type=crs"
    );
    let mut source = SpatialRef::from_epsg(4326).map_err(|_| GeodataError::Projection)?;
    let mut target =
        SpatialRef::from_definition(&definition).map_err(|_| GeodataError::Projection)?;
    source.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    target.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    CoordTransform::new(&source, &target).map_err(|_| GeodataError::Projection)
}

pub(super) fn project_polygon(
    geographic: &[HydeGeographicPoint],
    transform: &CoordTransform,
) -> Result<Polygon, GeodataError> {
    if !(3..=MAX_POLYGON_VERTICES).contains(&geographic.len())
        || geographic.iter().any(|point| {
            !point.longitude_degrees.is_finite()
                || !point.latitude_degrees.is_finite()
                || !(-180.0..=180.0).contains(&point.longitude_degrees)
                || !(-90.0..=90.0).contains(&point.latitude_degrees)
        })
    {
        return Err(GeodataError::Preparation(
            "HYDE allocation polygon coordinates are invalid",
        ));
    }
    validate_boundary_edges(geographic)?;
    let geographic = densify_geographic(geographic);
    let mut x = geographic
        .iter()
        .map(|point| point.longitude_degrees)
        .collect::<Vec<_>>();
    let mut y = geographic
        .iter()
        .map(|point| point.latitude_degrees)
        .collect::<Vec<_>>();
    transform
        .transform_coords(&mut x, &mut y, &mut [])
        .map_err(|_| GeodataError::Coordinate)?;
    let mut points = x
        .into_iter()
        .zip(y)
        .map(|(x, y)| Point { x, y })
        .collect::<Vec<_>>();
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(GeodataError::Coordinate);
    }
    normalize_polygon(&mut points)?;
    let area = signed_area(&points).abs();
    if !area.is_finite() || area <= 0.0 {
        return Err(GeodataError::Preparation(
            "HYDE allocation polygon has no area",
        ));
    }
    let bounds = bounds(&points);
    let triangles = triangulate(&points)?;
    Ok(Polygon {
        points,
        triangles,
        bounds,
        area,
    })
}

fn validate_boundary_edges(geographic: &[HydeGeographicPoint]) -> Result<(), GeodataError> {
    for index in 0..geographic.len() {
        let start = geographic[index];
        let end = geographic[(index + 1) % geographic.len()];
        if start.longitude_degrees.abs() >= 180.0
            || end.longitude_degrees.abs() >= 180.0
            || (start.longitude_degrees - end.longitude_degrees).abs() > 180.0
        {
            return Err(GeodataError::Preparation(
                "HYDE allocation polygon touches or crosses the antimeridian",
            ));
        }
        if start.latitude_degrees.abs() >= 90.0 || end.latitude_degrees.abs() >= 90.0 {
            return Err(GeodataError::Preparation(
                "HYDE allocation polygon touches or crosses a pole",
            ));
        }
    }
    Ok(())
}

fn normalize_polygon(points: &mut [Point]) -> Result<(), GeodataError> {
    if points.len() < 3 {
        return Err(GeodataError::Preparation(
            "HYDE allocation polygon has no area",
        ));
    }
    if points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .any(|(left, right)| left.x == right.x && left.y == right.y)
    {
        return Err(GeodataError::Preparation(
            "HYDE allocation polygon has a repeated vertex",
        ));
    }
    if signed_area(points) < 0.0 {
        points.reverse();
    }
    let start = points
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| point_order(a, b))
        .map(|(index, _)| index)
        .unwrap_or(0);
    points.rotate_left(start);
    Ok(())
}

fn densify_geographic(geographic: &[HydeGeographicPoint]) -> Vec<HydeGeographicPoint> {
    let mut output = Vec::with_capacity(geographic.len());
    for index in 0..geographic.len() {
        let start = geographic[index];
        let end = geographic[(index + 1) % geographic.len()];
        output.push(start);
        let mut steps = Vec::new();
        collect_axis_steps(start.longitude_degrees, end.longitude_degrees, &mut steps);
        collect_axis_steps(start.latitude_degrees, end.latitude_degrees, &mut steps);
        steps.sort_by(f64::total_cmp);
        steps.dedup_by(|left, right| (*left - *right).abs() <= f64::EPSILON);
        for fraction in steps {
            if fraction <= f64::EPSILON || fraction >= 1.0 - f64::EPSILON {
                continue;
            }
            let longitude_crossing =
                grid_crossing(start.longitude_degrees, end.longitude_degrees, fraction);
            let latitude_crossing =
                grid_crossing(start.latitude_degrees, end.latitude_degrees, fraction);
            output.push(HydeGeographicPoint {
                longitude_degrees: longitude_crossing.unwrap_or({
                    start.longitude_degrees
                        + fraction * (end.longitude_degrees - start.longitude_degrees)
                }),
                latitude_degrees: latitude_crossing.unwrap_or({
                    start.latitude_degrees
                        + fraction * (end.latitude_degrees - start.latitude_degrees)
                }),
            });
        }
    }
    output
}

fn collect_axis_steps(start: f64, end: f64, steps: &mut Vec<f64>) {
    if start == end {
        return;
    }
    let first = (start / DENSIFY_STEP_DEGREES).floor() as i64 + 1;
    let last = (end / DENSIFY_STEP_DEGREES).ceil() as i64 - 1;
    let (low, high, sign) = if start < end {
        (first, last, 1.0)
    } else {
        (last, first, -1.0)
    };
    if low > high {
        return;
    }
    for index in low..=high {
        let crossing = index as f64 * DENSIFY_STEP_DEGREES;
        if (crossing - start).signum() == (end - start).signum()
            && (crossing - end).signum() == -sign
        {
            steps.push((crossing - start) / (end - start));
        }
    }
}

fn grid_crossing(start: f64, end: f64, fraction: f64) -> Option<f64> {
    if start == end {
        return None;
    }
    let value = start + fraction * (end - start);
    let grid = (value / DENSIFY_STEP_DEGREES).round() * DENSIFY_STEP_DEGREES;
    ((value - grid).abs() <= 4.0 * f64::EPSILON * grid.abs().max(1.0)).then_some(grid)
}

fn triangulate(points: &[Point]) -> Result<Vec<[Point; 3]>, GeodataError> {
    let mut remaining = (0..points.len()).collect::<Vec<_>>();
    let mut triangles = Vec::with_capacity(points.len().saturating_sub(2));
    while remaining.len() > 3 {
        let mut clipped = false;
        for index in 0..remaining.len() {
            let previous = remaining[(index + remaining.len() - 1) % remaining.len()];
            let current = remaining[index];
            let next = remaining[(index + 1) % remaining.len()];
            let triangle = [points[previous], points[current], points[next]];
            if cross(triangle[0], triangle[1], triangle[2]) <= AREA_EPSILON {
                continue;
            }
            if remaining.iter().copied().any(|candidate| {
                candidate != previous
                    && candidate != current
                    && candidate != next
                    && point_in_triangle(points[candidate], triangle)
            }) {
                continue;
            }
            triangles.push(triangle);
            remaining.remove(index);
            clipped = true;
            break;
        }
        if !clipped {
            return Err(GeodataError::Preparation(
                "HYDE allocation polygon is not simple",
            ));
        }
    }
    let triangle = [
        points[remaining[0]],
        points[remaining[1]],
        points[remaining[2]],
    ];
    if cross(triangle[0], triangle[1], triangle[2]) <= AREA_EPSILON {
        return Err(GeodataError::Preparation(
            "HYDE allocation polygon has no area",
        ));
    }
    triangles.push(triangle);
    Ok(triangles)
}

fn point_in_triangle(point: Point, triangle: [Point; 3]) -> bool {
    cross(triangle[0], triangle[1], point) > 0.0
        && cross(triangle[1], triangle[2], point) > 0.0
        && cross(triangle[2], triangle[0], point) > 0.0
}

pub(super) fn point_order(left: &Point, right: &Point) -> Ordering {
    left.x
        .total_cmp(&right.x)
        .then_with(|| left.y.total_cmp(&right.y))
}

fn bounds(points: &[Point]) -> Bounds {
    let mut result = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for point in points {
        result.min_x = result.min_x.min(point.x);
        result.min_y = result.min_y.min(point.y);
        result.max_x = result.max_x.max(point.x);
        result.max_y = result.max_y.max(point.y);
    }
    result
}

fn signed_area(points: &[Point]) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let next = points[(index + 1) % points.len()];
            point.x * next.y - next.x * point.y
        })
        .sum::<f64>()
        / 2.0
}

pub(super) fn intersection_area(subject: &Polygon, clip: &Polygon) -> f64 {
    subject
        .triangles
        .iter()
        .flat_map(|subject_triangle| {
            clip.triangles.iter().map(move |clip_triangle| {
                triangle_intersection_area(*subject_triangle, *clip_triangle)
            })
        })
        .sum()
}

fn triangle_intersection_area(subject: [Point; 3], clip: [Point; 3]) -> f64 {
    let mut output = subject.to_vec();
    for index in 0..3 {
        let edge_start = clip[index];
        let edge_end = clip[(index + 1) % 3];
        let input = std::mem::take(&mut output);
        if input.is_empty() {
            return 0.0;
        }
        let mut previous = input[input.len() - 1];
        let mut previous_inside = cross(edge_start, edge_end, previous) >= -AREA_EPSILON;
        for current in input {
            let current_inside = cross(edge_start, edge_end, current) >= -AREA_EPSILON;
            if current_inside != previous_inside
                && let Some(intersection) =
                    line_intersection(previous, current, edge_start, edge_end)
            {
                output.push(intersection);
            }
            if current_inside {
                output.push(current);
            }
            previous = current;
            previous_inside = current_inside;
        }
    }
    if output.len() < 3 || signed_area(&output).abs() <= AREA_EPSILON {
        0.0
    } else {
        signed_area(&output).abs()
    }
}

fn line_intersection(
    start: Point,
    end: Point,
    edge_start: Point,
    edge_end: Point,
) -> Option<Point> {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let ex = edge_end.x - edge_start.x;
    let ey = edge_end.y - edge_start.y;
    let denominator = dx * ey - dy * ex;
    if denominator.abs() <= AREA_EPSILON {
        return None;
    }
    let t = ((edge_start.x - start.x) * ey - (edge_start.y - start.y) * ex) / denominator;
    Some(Point {
        x: start.x + t * dx,
        y: start.y + t * dy,
    })
}

fn cross(a: Point, b: Point, c: Point) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
