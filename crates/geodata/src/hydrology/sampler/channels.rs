//! Preserve projected source lines, simplifying by at most a quarter tile,
//! clipped to each evidence page with a maximum river half-width discovery halo.
use super::super::GeodataError;
use super::super::hydrology_sampling::{RiverReachGeometry, VectorFeature, line_position};
use aoe_map::RiverSegment;

/// Smallest discharge HydroRIVERS lists, in m³/s: no reach below it exists.
const BASE_DISCHARGE_CMS: f64 = 0.1;

/// Tile size in metres at which the base floor applies unchanged (the default
/// 30x compression of 2 m game tiles). Finer maps keep every listed reach.
const REFERENCE_METERS_PER_TILE: f64 = 60.0;

pub(super) fn base_discharge() -> f64 {
    BASE_DISCHARGE_CMS
}

/// Discharge floor in m³/s for a map whose tile spans `meters_per_tile` metres.
///
/// A river is kept only if its real channel width (`10 * sqrt(discharge)` m)
/// is at least the same fraction of a tile as the smallest listed river was on
/// the reference 60 m tile (5%). Equivalently the floor grows with the square
/// of the tile size beyond the reference: 0.1 m³/s up to compression 30, about
/// 0.4 at 60, 1.6 at 120 and 6.4 at 240, where only streams that would be
/// about 4 tiles wide survive. A pure function of the compression, so the
/// output is deterministic and independent of page, order or file.
pub(super) fn minimum_discharge(meters_per_tile: f64) -> f64 {
    let scale = (meters_per_tile / REFERENCE_METERS_PER_TILE).max(1.0);
    BASE_DISCHARGE_CMS * scale * scale
}

/// Tiles of channel width gained per decade of discharge above the floor.
/// Rivers are drawn by importance: 0.1 m³/s is a one-tile stream, 1 m³/s 2.5
/// tiles, 10 m³/s 4, 100 m³/s 5.5, 1,000 m³/s 7 and the ten-tile ceiling
/// is reached at 100,000 m³/s. The mapping depends on discharge alone, never on
/// the metres per tile, so a river keeps its rank at every map size.
const TILES_PER_DISCHARGE_DECADE: f64 = 1.5;

/// Gameplay channel width in 1/256 tile units for a mean discharge in m³/s.
pub(super) fn width_units(discharge: f64) -> u16 {
    let decades = (discharge / BASE_DISCHARGE_CMS).max(1.0).log10();
    let tiles = 1.0 + TILES_PER_DISCHARGE_DECADE * decades;
    (tiles * 256.0).round().clamp(
        f64::from(aoe_map::MIN_RIVER_WIDTH_UNITS),
        f64::from(aoe_map::MAX_RIVER_WIDTH_UNITS),
    ) as u16
}

/// Serialized size reserved for segments in one page. A page file is at most
/// 128 KiB and its evidence and water-model arrays take up to about 64 KiB, so
/// this keeps a dense page inside the directory page limit.
pub(super) const MAX_PAGE_SEGMENT_BYTES: usize = 48 * 1024;

/// Segments of every kept reach on one page. A page holds at most
/// `MAX_RIVER_SEGMENTS_PER_PAGE` segments and `MAX_PAGE_SEGMENT_BYTES` of
/// serialized segments; when the reaches would exceed either, whole reaches are
/// kept in order of falling discharge (lower reach id first on a tie) and the
/// remainder is dropped, so the cap never depends on file order.
pub(super) fn segments(
    features: &[VectorFeature],
    side: f64,
    tiles: u64,
    bounds: [f64; 4],
) -> Result<Vec<RiverSegment>, GeodataError> {
    let meters_per_tile = side / tiles as f64;
    let halo = 5.0 * meters_per_tile;
    let bounds = [
        bounds[0] - halo,
        bounds[1] - halo,
        bounds[2] + halo,
        bounds[3] + halo,
    ];
    let mut reaches = Vec::new();
    for feature in features {
        let Some(reach) = &feature.river_reach else {
            continue;
        };
        if reach.discharge < minimum_discharge(meters_per_tile) {
            continue;
        }
        let lines = reach_segments(reach, side, meters_per_tile, bounds)?;
        if !lines.is_empty() {
            reaches.push((reach.discharge, reach.id, lines));
        }
    }
    reaches.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut output = Vec::new();
    let mut bytes = 0_usize;
    for (_, _, lines) in reaches {
        let size = lines
            .iter()
            .map(|line| serde_json::to_vec(line).map_or(usize::MAX, |json| json.len() + 1))
            .fold(0_usize, usize::saturating_add);
        if output.len() + lines.len() > aoe_map::MAX_RIVER_SEGMENTS_PER_PAGE
            || bytes.saturating_add(size) > MAX_PAGE_SEGMENT_BYTES
        {
            if output.is_empty() {
                return Err(GeodataError::Preparation(
                    "one river reach exceeds the page segment budget",
                ));
            }
            break;
        }
        bytes += size;
        output.extend(lines);
    }
    Ok(output)
}

fn reach_segments(
    reach: &RiverReachGeometry,
    side: f64,
    meters_per_tile: f64,
    bounds: [f64; 4],
) -> Result<Vec<RiverSegment>, GeodataError> {
    let mut output = Vec::new();
    let units = 256.0 / meters_per_tile;
    let coordinate = |point: (f64, f64)| {
        [
            ((point.0 + side / 2.0) * units).round() as i32,
            ((side / 2.0 - point.1) * units).round() as i32,
        ]
    };
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
    let width_units = width_units(reach.discharge);
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
                    source_delta[axis] = source_end[axis].checked_sub(source_start[axis]).ok_or(
                        GeodataError::Preparation("river source segment exceeds coordinate range"),
                    )?;
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
                        "one river reach exceeds the page segment budget",
                    ));
                }
            }
        }
        station -= segment_length * units;
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
