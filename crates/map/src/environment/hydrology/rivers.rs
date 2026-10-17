//! Source centreline geometry at map scale. Integer coordinates and stations
//! use 1/256 tile units; widths and crossings are explicit gameplay modelling.
use crate::EnvironmentError;
use serde::{Deserialize, Serialize};

pub const FORD_SPACING_UNITS: i64 = 40 * 256;
pub const FORD_HALF_LENGTH_UNITS: i64 = 5 * 256 / 2;
pub const MAX_RIVER_SEGMENTS_PER_PAGE: usize = 1024;
/// Narrowest drawn channel: a one-tile stream, the smallest kept discharge.
pub const MIN_RIVER_WIDTH_UNITS: u16 = 256;
/// Widest drawn channel, kept for playability.
pub const MAX_RIVER_WIDTH_UNITS: u16 = 10 * 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "[i64; 10]", into = "[i64; 10]")]
pub struct RiverSegment {
    pub start: [i32; 2],
    pub end: [i32; 2],
    pub width_units: u16,
    pub station_start: i64,
    pub station_end: i64,
    /// Original, unclipped direction and station span for page-invariant bends.
    pub source_delta: [i32; 2],
    pub source_station_delta: i64,
}

impl From<RiverSegment> for [i64; 10] {
    fn from(s: RiverSegment) -> Self {
        [
            i64::from(s.start[0]),
            i64::from(s.start[1]),
            i64::from(s.end[0]),
            i64::from(s.end[1]),
            i64::from(s.width_units),
            s.station_start,
            s.station_end,
            i64::from(s.source_delta[0]),
            i64::from(s.source_delta[1]),
            s.source_station_delta,
        ]
    }
}

impl TryFrom<[i64; 10]> for RiverSegment {
    type Error = EnvironmentError;
    fn try_from(v: [i64; 10]) -> Result<Self, Self::Error> {
        let coordinate = |n| i32::try_from(n).map_err(|_| EnvironmentError::InvalidPage);
        let segment = Self {
            start: [coordinate(v[0])?, coordinate(v[1])?],
            end: [coordinate(v[2])?, coordinate(v[3])?],
            width_units: u16::try_from(v[4]).map_err(|_| EnvironmentError::InvalidPage)?,
            station_start: v[5],
            station_end: v[6],
            source_delta: [coordinate(v[7])?, coordinate(v[8])?],
            source_station_delta: v[9],
        };
        segment.validate()?;
        Ok(segment)
    }
}

impl RiverSegment {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        let delta = [0, 1].map(|i| i64::from(self.end[i]) - i64::from(self.start[i]));
        let source = self.source_delta.map(i64::from);
        let length = source[0].abs().max(source[1].abs());
        let cross = i128::from(delta[0]) * i128::from(source[1])
            - i128::from(delta[1]) * i128::from(source[0]);
        let dot = i128::from(delta[0]) * i128::from(source[0])
            + i128::from(delta[1]) * i128::from(source[1]);
        if length == 0
            || length < delta[0].abs().max(delta[1].abs())
            || dot <= 0
            // Both original and clipped endpoints round on each axis.
            || cross.abs() > i128::from(length) * 4 + 2
            || self.source_station_delta.unsigned_abs() > (1 << 49)
            || self.start == self.end
            || !(MIN_RIVER_WIDTH_UNITS..=MAX_RIVER_WIDTH_UNITS).contains(&self.width_units)
            || self
                .start
                .iter()
                .chain(&self.end)
                .any(|v| v.unsigned_abs() > 68_000_000)
            || self.station_start.unsigned_abs() > (1 << 48)
            || self.station_end.unsigned_abs() > (1 << 48)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    /// Distance and ford eligibility of a point within the expanded corridor.
    /// Expanded queries are for conservative shore/minimap discovery only.
    /// Negative expansion is zero; expansion is capped at twice the maximum
    /// map side in units, enough to cover any supported map's diagonal.
    pub fn sample(&self, point: [i64; 2], extra_units: i64) -> Option<(i128, bool)> {
        let max_expansion = (crate::request::MAX_TILES_PER_SIDE * 2 * 256) as i64;
        let radius_units =
            (i64::from(self.width_units) / 2).saturating_add(extra_units.clamp(0, max_expansion));
        for (axis, value) in point.iter().enumerate() {
            if *value < i64::from(self.start[axis].min(self.end[axis])) - radius_units
                || *value > i64::from(self.start[axis].max(self.end[axis])) + radius_units
            {
                return None;
            }
        }
        let dx = i128::from(self.end[0]) - i128::from(self.start[0]);
        let dy = i128::from(self.end[1]) - i128::from(self.start[1]);
        let px = i128::from(point[0]) - i128::from(self.start[0]);
        let py = i128::from(point[1]) - i128::from(self.start[1]);
        let length = dx * dx + dy * dy;
        if length == 0 {
            return None;
        }
        let fraction = (px * dx + py * dy).clamp(0, length);
        let ax = px - dx * fraction / length;
        let ay = py - dy * fraction / length;
        let distance = ax * ax + ay * ay;
        let radius = i128::from(radius_units);
        if distance > radius * radius {
            return None;
        }
        let station = i128::from(self.station_start)
            + (i128::from(self.station_end) - i128::from(self.station_start)) * fraction / length;
        let phase = (station as i64).rem_euclid(FORD_SPACING_UNITS);
        let ford = phase.min(FORD_SPACING_UNITS - phase) <= FORD_HALF_LENGTH_UNITS;
        Some((distance, ford))
    }
}

impl super::HydrologyEvidencePage {
    /// Package-context validation bounds index work to this page and a five-tile halo.
    pub fn validate_river_bounds(&self, samples: u16, tiles: u64) -> Result<(), EnvironmentError> {
        self.validate()?;
        if !(2..=super::MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS).contains(&samples)
            || tiles == 0
            || tiles > crate::request::MAX_TILES_PER_SIDE
        {
            return Err(EnvironmentError::InvalidPage);
        }
        let units = i128::from(tiles) * 256;
        let axis = i128::from(samples);
        for (dimension, (page, size)) in [(self.x, self.width), (self.y, self.height)]
            .into_iter()
            .enumerate()
        {
            let start = i128::from(page) * i128::from(super::ENVIRONMENT_PAGE_SAMPLES);
            let end = start + i128::from(size);
            if end > axis {
                return Err(EnvironmentError::InvalidPage);
            }
            // Compare rational bounds without rounding the fractional page edges.
            let low = start * units - 5 * 256 * axis;
            let high = end * units + 5 * 256 * axis;
            for (segment, geometry) in self.river_segments.iter().enumerate() {
                for (endpoint, point) in [geometry.start, geometry.end].into_iter().enumerate() {
                    let coordinate = i128::from(point[dimension]) * axis;
                    // Persisted coordinates round to the nearest integer unit.
                    if coordinate * 2 < low * 2 - axis || coordinate * 2 > high * 2 + axis {
                        return Err(EnvironmentError::RiverOutsidePage {
                            segment,
                            endpoint,
                            axis: dimension,
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

impl super::HydrologyEvidenceIndex {
    /// Shared acceptance check for a page in a frozen package. Structural
    /// roots alone cannot bind geometry or optional models to their index.
    pub fn validate_page(
        &self,
        page: &super::HydrologyEvidencePage,
        tiles: u64,
    ) -> Result<(), EnvironmentError> {
        self.validate()?;
        page.validate_river_bounds(self.samples_per_axis, tiles)?;
        let side = u16::from(super::ENVIRONMENT_PAGE_SAMPLES);
        let shape = |coordinate: u16| {
            self.samples_per_axis
                .checked_sub(coordinate.checked_mul(side)?)
                .map(|remaining| remaining.min(side) as u8)
        };
        if shape(page.x) != Some(page.width) || shape(page.y) != Some(page.height) {
            return Err(EnvironmentError::InvalidPage);
        }
        if page.water_model.is_some() != self.water_model.is_some() {
            return Err(EnvironmentError::WaterModelPresence);
        }
        Ok(())
    }
}
