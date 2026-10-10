//! Native source-worker preparation options shared by the server and the
//! geographic worker. They are not part of `MapRequest` or the game wire.

use serde::{Deserialize, Serialize};

/// Canonical overview axis for elevation, history and vector hydrology.
pub const LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS: u16 = 1024;
/// Categorical overview axis for potential vegetation and water.
const LANDSCAPE_CATEGORICAL_SAMPLES_PER_AXIS: u16 = 128;

/// Independent per-field overview axes requested from the source worker.
/// Overview packages always use [`OverviewFieldAxes::LANDSCAPE`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverviewFieldAxes {
    pub elevation: u16,
    pub vegetation: u16,
    pub water: u16,
    pub historical: u16,
}

impl OverviewFieldAxes {
    pub const LANDSCAPE: Self = Self {
        elevation: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
        vegetation: LANDSCAPE_CATEGORICAL_SAMPLES_PER_AXIS,
        water: LANDSCAPE_CATEGORICAL_SAMPLES_PER_AXIS,
        historical: LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS,
    };

    /// One elevation/vegetation/water axis, as sampled by the overview
    /// context of detailed preparation.
    pub const fn coupled(axis: u16, historical: u16) -> Self {
        Self {
            elevation: axis,
            vegetation: axis,
            water: axis,
            historical,
        }
    }
}

/// Native overview-only hydrology selection; never part of `MapRequest`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverviewHydrologyMode {
    #[default]
    None,
    Vectors,
}

impl OverviewHydrologyMode {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// Inclusive geographic window in degrees times 10^7.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeographicWindowE7 {
    pub min_longitude_e7: i32,
    pub max_longitude_e7: i32,
    pub min_latitude_e7: i32,
    pub max_latitude_e7: i32,
}

impl GeographicWindowE7 {
    pub const fn inset(self, margin_e7: i32) -> Self {
        Self {
            min_longitude_e7: self.min_longitude_e7 + margin_e7,
            max_longitude_e7: self.max_longitude_e7 - margin_e7,
            min_latitude_e7: self.min_latitude_e7 + margin_e7,
            max_latitude_e7: self.max_latitude_e7 - margin_e7,
        }
    }

    pub const fn contains_e7(self, longitude_e7: i32, latitude_e7: i32) -> bool {
        self.min_longitude_e7 <= longitude_e7
            && longitude_e7 <= self.max_longitude_e7
            && self.min_latitude_e7 <= latitude_e7
            && latitude_e7 <= self.max_latitude_e7
    }

    /// Degrees are the correctly rounded quotient of each exact e7 bound.
    pub fn longitude_degrees(self) -> std::ops::RangeInclusive<f64> {
        degrees(self.min_longitude_e7)..=degrees(self.max_longitude_e7)
    }

    pub fn latitude_degrees(self) -> std::ops::RangeInclusive<f64> {
        degrees(self.min_latitude_e7)..=degrees(self.max_latitude_e7)
    }
}

fn degrees(value_e7: i32) -> f64 {
    f64::from(value_e7) / 10_000_000.0
}

/// Coverage window of the worker's pinned regional river-vector source
/// (HydroRIVERS v1.0 Europe and Middle East). This is source-coverage data,
/// not a map rule: requests outside it have no river vectors.
pub const VECTOR_HYDROLOGY_SOURCE_COVERAGE: GeographicWindowE7 = GeographicWindowE7 {
    min_longitude_e7: -120_000_000,
    max_longitude_e7: 250_000_000,
    min_latitude_e7: 360_000_000,
    max_latitude_e7: 600_000_000,
};
/// Geographic padding the worker adds to each vector query (0.01 degrees).
pub const VECTOR_HYDROLOGY_QUERY_PADDING_E7: i32 = 100_000;
/// Vector hydrology requires the whole padded footprint inside coverage.
pub const VECTOR_HYDROLOGY_FOOTPRINT_WINDOW: GeographicWindowE7 =
    VECTOR_HYDROLOGY_SOURCE_COVERAGE.inset(VECTOR_HYDROLOGY_QUERY_PADDING_E7);
/// Locks in an overview package: ETOPO, Natural Earth, potential
/// biome and HYDE sources, as listed by the worker's overview source catalog.
pub const LANDSCAPE_OVERVIEW_SOURCE_LOCKS: usize = 7;
/// Locks added by vector hydrology: HydroLAKES and HydroRIVERS.
pub const VECTOR_HYDROLOGY_SOURCE_LOCKS: usize = 2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_footprint_window_is_source_coverage_minus_query_padding() {
        let window = VECTOR_HYDROLOGY_FOOTPRINT_WINDOW;
        assert_eq!(window.longitude_degrees(), -11.99..=24.99);
        assert_eq!(window.latitude_degrees(), 36.01..=59.99);
        assert_eq!(
            VECTOR_HYDROLOGY_SOURCE_COVERAGE.longitude_degrees(),
            -12.0..=25.0
        );
        assert!(window.contains_e7(-119_900_000, 599_900_000));
        assert!(!window.contains_e7(-120_000_000, 400_000_000));
        assert!(!window.contains_e7(0, 600_000_000));
    }

    #[test]
    fn shared_worker_options_keep_their_wire_shape() {
        assert_eq!(
            serde_json::to_value(OverviewFieldAxes::LANDSCAPE).unwrap(),
            serde_json::json!({"elevation": 1024, "vegetation": 128, "water": 128, "historical": 1024})
        );
        assert_eq!(
            serde_json::to_value(OverviewHydrologyMode::Vectors).unwrap(),
            "vectors"
        );
        assert!(OverviewHydrologyMode::default().is_none());
    }
}
