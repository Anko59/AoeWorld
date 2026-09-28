use super::super::hydrology_sampling::{RiverReachGeometry, VectorFeature, line_position};
use super::super::{GeodataError, RiverTopologyCell, RiverTopologyGrid};
use super::Sampler;
use gdal::vector::Geometry;
use std::collections::BTreeMap;

const MAX_REACH_METADATA: usize = 100_000;
const MAX_REACH_CONNECTION_CENTIMETERS: i128 = 100_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RiverReachMetadata {
    next_down_id: u32,
    distance_to_sink_centimeters: u32,
    start_x_centimeters: i32,
    start_y_centimeters: i32,
    end_x_centimeters: i32,
    end_y_centimeters: i32,
    length_centimeters: u32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct RiverCellProjection {
    pub(super) reach_id: u32,
    pub(super) distance_from_start_centimeters: u32,
}

impl Sampler {
    pub(super) fn record_reach(&mut self, reach: &RiverReachGeometry) -> Result<(), GeodataError> {
        let (Some(first), Some(last), Some(length)) = (
            reach.line_points.first().copied(),
            reach.line_points.last().copied(),
            polyline_length(&reach.line_points),
        ) else {
            return Ok(());
        };
        let (Some(start_x), Some(start_y), Some(end_x), Some(end_y), Some(length_centimeters)) = (
            coordinate_to_centimeters(first.0),
            coordinate_to_centimeters(first.1),
            coordinate_to_centimeters(last.0),
            coordinate_to_centimeters(last.1),
            meters_to_centimeters(length),
        ) else {
            // The corridor is still useful, but this reach cannot safely take
            // part in the bounded topology model.
            return Ok(());
        };
        let metadata = RiverReachMetadata {
            next_down_id: reach.next_down_id,
            distance_to_sink_centimeters: reach.distance_to_sink_centimeters,
            start_x_centimeters: start_x,
            start_y_centimeters: start_y,
            end_x_centimeters: end_x,
            end_y_centimeters: end_y,
            length_centimeters,
        };
        if let Some(existing) = self.river_reaches.get(&reach.id) {
            if *existing != metadata {
                return Err(GeodataError::Preparation(
                    "HydroRIVERS topology disagrees for a repeated reach",
                ));
            }
            return Ok(());
        }
        if self.river_reaches.len() >= MAX_REACH_METADATA {
            // Preserve the evidence corridor but leave topology unsupported for
            // reaches beyond the bounded transient index.
            return Ok(());
        }
        self.river_reaches.insert(reach.id, metadata);
        Ok(())
    }

    pub(super) fn resolve_river_topology(&self) -> Result<Option<RiverTopologyGrid>, GeodataError> {
        if self.river_reaches.is_empty() {
            return Ok(None);
        }
        let mut downstream_at_start = BTreeMap::new();
        for (&reach_id, reach) in &self.river_reaches {
            if reach.next_down_id == 0 {
                continue;
            }
            let Some(downstream) = self.river_reaches.get(&reach.next_down_id) else {
                continue;
            };
            let start_distance = endpoint_distance_squared(
                reach.start_x_centimeters,
                reach.start_y_centimeters,
                downstream,
            );
            let end_distance = endpoint_distance_squared(
                reach.end_x_centimeters,
                reach.end_y_centimeters,
                downstream,
            );
            let max_gap_squared =
                MAX_REACH_CONNECTION_CENTIMETERS * MAX_REACH_CONNECTION_CENTIMETERS;
            let at_start = if start_distance < end_distance && start_distance <= max_gap_squared {
                Some(true)
            } else if end_distance < start_distance && end_distance <= max_gap_squared {
                Some(false)
            } else {
                None
            };
            if let Some(at_start) = at_start {
                downstream_at_start.insert(reach_id, at_start);
            }
        }
        let mut cells = vec![None; self.river_cells.len()];
        for (index, projection) in self.river_cells.iter().enumerate() {
            let Some(projection) = projection else {
                continue;
            };
            let Some(reach) = self.river_reaches.get(&projection.reach_id) else {
                continue;
            };
            let Some(downstream_at_start) = downstream_at_start.get(&projection.reach_id) else {
                continue;
            };
            let station = projection
                .distance_from_start_centimeters
                .min(reach.length_centimeters);
            let distance_to_outlet = if *downstream_at_start {
                station
            } else {
                reach.length_centimeters - station
            };
            let distance_to_sink_centimeters =
                u64::from(reach.distance_to_sink_centimeters) + u64::from(distance_to_outlet);
            cells[index] = Some(RiverTopologyCell {
                reach_id: projection.reach_id,
                next_down_id: reach.next_down_id,
                distance_to_sink_centimeters,
            });
        }
        Ok(Some(RiverTopologyGrid { cells }))
    }
}

pub(super) fn nearest_river_reach<'a>(
    features: &'a [VectorFeature],
    point: &Geometry,
    x: f64,
    y: f64,
) -> Option<(&'a VectorFeature, f64)> {
    let mut best: Option<(&VectorFeature, f64, f64, u32)> = None;
    for feature in features {
        if !feature.geometry.contains(point) {
            continue;
        }
        let Some(reach) = feature.river_reach.as_ref() else {
            continue;
        };
        let Some((distance, station)) = line_position(&reach.line_points, x, y) else {
            continue;
        };
        let replace = best.is_none_or(|(_, best_distance, _, best_id)| {
            distance.total_cmp(&best_distance).is_lt()
                || (distance.total_cmp(&best_distance).is_eq() && reach.id < best_id)
        });
        if replace {
            best = Some((feature, distance, station, reach.id));
        }
    }
    best.map(|(feature, _, station, _)| (feature, station))
}

pub(super) fn meters_to_centimeters(meters: f64) -> Option<u32> {
    let centimeters = meters * 100.0;
    (centimeters.is_finite() && (0.0..=f64::from(u32::MAX)).contains(&centimeters))
        .then(|| centimeters.round() as u32)
}

fn endpoint_distance_squared(x: i32, y: i32, reach: &RiverReachMetadata) -> i128 {
    let start_dx = i128::from(x) - i128::from(reach.start_x_centimeters);
    let start_dy = i128::from(y) - i128::from(reach.start_y_centimeters);
    let end_dx = i128::from(x) - i128::from(reach.end_x_centimeters);
    let end_dy = i128::from(y) - i128::from(reach.end_y_centimeters);
    (start_dx * start_dx + start_dy * start_dy).min(end_dx * end_dx + end_dy * end_dy)
}

fn polyline_length(points: &[(f64, f64)]) -> Option<f64> {
    (points.len() >= 2)
        .then(|| {
            points
                .windows(2)
                .map(|pair| {
                    let dx = pair[1].0 - pair[0].0;
                    let dy = pair[1].1 - pair[0].1;
                    (dx * dx + dy * dy).sqrt()
                })
                .sum::<f64>()
        })
        .filter(|length| length.is_finite())
}

fn coordinate_to_centimeters(meters: f64) -> Option<i32> {
    let centimeters = meters * 100.0;
    (centimeters.is_finite() && (i32::MIN as f64..=i32::MAX as f64).contains(&centimeters))
        .then(|| centimeters.round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn endpoint_distance_handles_extreme_projected_coordinates_without_overflow() {
        let reach = RiverReachMetadata {
            next_down_id: 0,
            distance_to_sink_centimeters: 0,
            start_x_centimeters: i32::MAX,
            start_y_centimeters: i32::MAX,
            end_x_centimeters: i32::MAX,
            end_y_centimeters: i32::MIN,
            length_centimeters: 0,
        };
        let delta = i128::from(i32::MAX) - i128::from(i32::MIN);
        assert_eq!(
            endpoint_distance_squared(i32::MIN, i32::MIN, &reach),
            delta * delta
        );
    }
}
