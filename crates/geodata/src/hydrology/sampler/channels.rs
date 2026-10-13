//! Preserve projected source lines, simplifying by at most a quarter tile,
//! clipped to each evidence page with a maximum river half-width discovery halo.
use super::super::GeodataError;
use super::super::hydrology_sampling::{VectorFeature, line_position};
use aoe_map::RiverSegment;

pub(super) fn minimum_discharge(cell_meters: f64) -> f64 {
    if cell_meters <= 60.0 {
        0.1
    } else {
        (cell_meters / 100.0).clamp(0.1, 50.0)
    }
}

pub(super) fn segments(
    features: &[VectorFeature],
    side: f64,
    tiles: u64,
    bounds: [f64; 4],
    spacing: f64,
) -> Result<Vec<RiverSegment>, GeodataError> {
    let mut output = Vec::new();
    let meters_per_tile = side / tiles as f64;
    let units = 256.0 / meters_per_tile;
    let halo = 5.0 * meters_per_tile;
    let bounds = [
        bounds[0] - halo,
        bounds[1] - halo,
        bounds[2] + halo,
        bounds[3] + halo,
    ];
    let coordinate = |point: (f64, f64)| {
        [
            ((point.0 + side / 2.0) * units).round() as i32,
            ((side / 2.0 - point.1) * units).round() as i32,
        ]
    };
    for feature in features {
        let Some(reach) = &feature.river_reach else {
            continue;
        };
        if reach.discharge < minimum_discharge(spacing) {
            continue;
        }
        let points = simplify(&reach.line_points, meters_per_tile / 4.0);
        let length = points.windows(2).map(|p| distance(p[0], p[1])).sum::<f64>();
        let length_units = length * units;
        let sink = f64::from(reach.distance_to_sink_centimeters) / 100.0 * units;
        // Short reaches (including inter-lake streams) always contain a ford.
        let end_station = if length_units < 10240.0 {
            (sink / 10240.0).round() * 10240.0 - length_units / 2.0
        } else {
            sink
        };
        let width_units = ((10.0 * reach.discharge.sqrt()).clamp(5.0, 600.0) * units)
            .round()
            .clamp(3.0 * 256.0, 10.0 * 256.0) as u16;
        let mut station = end_station + length_units;
        for pair in points.windows(2) {
            let segment_length = distance(pair[0], pair[1]);
            if let Some((a, b)) = clip(pair[0], pair[1], bounds) {
                let interpolate = |t: f64| {
                    (
                        pair[0].0 + t * (pair[1].0 - pair[0].0),
                        pair[0].1 + t * (pair[1].1 - pair[0].1),
                    )
                };
                let start = coordinate(interpolate(a));
                let end = coordinate(interpolate(b));
                if start != end {
                    let source_start = coordinate(pair[0]);
                    let source_end = coordinate(pair[1]);
                    let mut source_delta = [0; 2];
                    for axis in 0..2 {
                        source_delta[axis] = source_end[axis]
                            .checked_sub(source_start[axis])
                            .ok_or(GeodataError::Preparation(
                                "river source segment exceeds coordinate range",
                            ))?;
                    }
                    let segment = RiverSegment {
                        start,
                        end,
                        width_units,
                        source_delta,
                        source_station_delta: (station - segment_length * units).round() as i64
                            - station.round() as i64,
                        station_start: (station - a * segment_length * units).round() as i64,
                        station_end: (station - b * segment_length * units).round() as i64,
                    };
                    segment.validate()?;
                    output.push(segment);
                    if output.len() > aoe_map::MAX_RIVER_SEGMENTS_PER_PAGE {
                        return Err(GeodataError::Preparation(
                            "river page exceeds its segment budget",
                        ));
                    }
                }
            }
            station -= segment_length * units;
        }
    }
    Ok(output)
}

fn distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn simplify(points: &[(f64, f64)], tolerance: f64) -> Vec<(f64, f64)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut pending = vec![(0, points.len() - 1)];
    while let Some((a, b)) = pending.pop() {
        let line = [points[a], points[b]];
        let mut farthest = None;
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let deviation = line_position(&line, p.0, p.1).map_or(f64::INFINITY, |v| v.0);
            if deviation > tolerance && farthest.is_none_or(|(_, d)| deviation > d) {
                farthest = Some((i, deviation));
            }
        }
        if let Some((i, _)) = farthest {
            keep[i] = true;
            pending.extend([(a, i), (i, b)]);
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, keep)| keep.then_some(*p))
        .collect()
}

fn clip(a: (f64, f64), b: (f64, f64), bounds: [f64; 4]) -> Option<(f64, f64)> {
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for (start, delta, min, max) in [
        (a.0, b.0 - a.0, bounds[0], bounds[2]),
        (a.1, b.1 - a.1, bounds[1], bounds[3]),
    ] {
        if delta == 0.0 {
            if start < min || start > max {
                return None;
            }
        } else {
            let t0 = (min - start) / delta;
            let t1 = (max - start) / delta;
            low = low.max(t0.min(t1));
            high = high.min(t0.max(t1));
        }
    }
    (low <= high).then_some((low, high))
}

/// Exact distance from each source segment to the closed evidence-cell rectangle.
pub(super) fn corridor_intersects_cell(
    points: &[(f64, f64)],
    x: f64,
    y: f64,
    spacing: f64,
    half_width: f64,
) -> bool {
    let half = spacing / 2.0;
    let bounds = [x - half, y - half, x + half, y + half];
    let corners = [
        (bounds[0], bounds[1]),
        (bounds[0], bounds[3]),
        (bounds[2], bounds[1]),
        (bounds[2], bounds[3]),
    ];
    points.windows(2).any(|pair| {
        if clip(pair[0], pair[1], bounds).is_some() {
            return true;
        }
        // Disjoint segments/rectangles attain their minimum at an endpoint
        // or a rectangle corner projected onto the segment.
        let endpoint_hit = pair.iter().any(|p| {
            let dx = p.0 - p.0.clamp(bounds[0], bounds[2]);
            let dy = p.1 - p.1.clamp(bounds[1], bounds[3]);
            dx.hypot(dy) <= half_width
        });
        endpoint_hit
            || corners.iter().any(|p| {
                line_position(pair, p.0, p.1).is_some_and(|(distance, _)| distance <= half_width)
            })
    })
}
