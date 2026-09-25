use super::area::HydeAreaAllocation;
use crate::GeodataError;
use serde::{Deserialize, Serialize};

pub const HISTORICAL_CORRECTION_SCHEMA_VERSION: u16 = 1;
pub const HISTORICAL_CORRECTION_TARGET_YEAR_CE: u16 = 600;
pub const MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS: u16 = 1_024;
pub const MAX_HISTORICAL_CORRECTION_JSON_BYTES: usize = 64 * 1024 * 1024;
const MIN_MODERN_OBSERVATION_YEAR_CE: u16 = 1_900;

/// Unrounded extensive totals for one whole HYDE target cell. These are the
/// same quantities retained by area allocation before page rounding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HydeWholeCellQuantities {
    pub land_area_square_meters: f64,
    pub crop_area_square_kilometers: f64,
    pub grazing_area_square_kilometers: f64,
    pub population: f64,
}

/// Evidence attached to one proposed historical correction. The variants are
/// deliberately distinct on the wire: modern observations, historical models,
/// procedural additions, and fallback values cannot substitute for one
/// another. Unknown evidence never defaults to a quantity or zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoricalCorrectionEvidence {
    Circa600Hyde {
        quantities: HydeWholeCellQuantities,
    },
    ModernObservation {
        observation_year_ce: u16,
        quantities: HydeWholeCellQuantities,
    },
    ProceduralAddition {
        quantities: HydeWholeCellQuantities,
    },
    FallbackEvidence {
        quantities: HydeWholeCellQuantities,
    },
    Unknown,
}

/// One sparse, whole-cell correction record in row-major target-grid order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCorrection {
    pub cell_x: u16,
    pub cell_y: u16,
    pub evidence: HistoricalCorrectionEvidence,
}

/// Versioned JSON serialization contract for sparse historical corrections.
/// Records must be strictly row-major and unique so equivalent documents have
/// one byte representation. A missing record means "no proposed correction";
/// absent evidence must instead be written as an explicit `Unknown` record.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalCorrectionDocument {
    pub schema_version: u16,
    pub target_year_ce: u16,
    pub samples_per_axis: u16,
    pub corrections: Vec<HistoricalCorrection>,
}

impl HistoricalCorrectionDocument {
    /// Sorts records into their canonical row-major order and validates the
    /// complete document before it can be serialized.
    pub fn new(
        samples_per_axis: u16,
        mut corrections: Vec<HistoricalCorrection>,
    ) -> Result<Self, GeodataError> {
        corrections.sort_by_key(|record| (record.cell_y, record.cell_x));
        let document = Self {
            schema_version: HISTORICAL_CORRECTION_SCHEMA_VERSION,
            target_year_ce: HISTORICAL_CORRECTION_TARGET_YEAR_CE,
            samples_per_axis,
            corrections,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), GeodataError> {
        if self.schema_version != HISTORICAL_CORRECTION_SCHEMA_VERSION {
            return Err(invalid(
                "historical correction schema version is unsupported",
            ));
        }
        if self.target_year_ce != HISTORICAL_CORRECTION_TARGET_YEAR_CE {
            return Err(invalid(
                "historical correction target year is not circa 600 CE",
            ));
        }
        if !(2..=MAX_HISTORICAL_CORRECTION_SAMPLES_PER_AXIS).contains(&self.samples_per_axis) {
            return Err(invalid(
                "historical correction grid is outside direct bounds",
            ));
        }
        let cell_count = usize::from(self.samples_per_axis).pow(2);
        if self.corrections.len() > cell_count {
            return Err(invalid(
                "historical correction document exceeds its cell limit",
            ));
        }

        for record in &self.corrections {
            if record.cell_x >= self.samples_per_axis || record.cell_y >= self.samples_per_axis {
                return Err(invalid("historical correction cell is outside its grid"));
            }
            match &record.evidence {
                HistoricalCorrectionEvidence::Circa600Hyde { quantities } => {
                    validate_quantities(quantities)?;
                }
                HistoricalCorrectionEvidence::ModernObservation {
                    observation_year_ce,
                    quantities,
                } => {
                    if !(MIN_MODERN_OBSERVATION_YEAR_CE..=9_999).contains(observation_year_ce) {
                        return Err(invalid(
                            "modern correction observation year is outside modern bounds",
                        ));
                    }
                    validate_quantities(quantities)?;
                }
                HistoricalCorrectionEvidence::ProceduralAddition { quantities }
                | HistoricalCorrectionEvidence::FallbackEvidence { quantities } => {
                    validate_quantities(quantities)?;
                }
                HistoricalCorrectionEvidence::Unknown => {}
            }
        }

        for pair in self.corrections.windows(2) {
            let left = (pair[0].cell_y, pair[0].cell_x);
            let right = (pair[1].cell_y, pair[1].cell_x);
            if left == right {
                return Err(invalid("historical correction cells are duplicated"));
            }
            if left > right {
                return Err(invalid(
                    "historical correction cells are not in canonical row-major order",
                ));
            }
        }
        Ok(())
    }

    pub fn serialize(&self) -> Result<Vec<u8>, GeodataError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|error| {
            invalid(format!(
                "historical correction serialization failed: {error}"
            ))
        })?;
        if bytes.len() > MAX_HISTORICAL_CORRECTION_JSON_BYTES {
            return Err(invalid(
                "historical correction document exceeds its byte limit",
            ));
        }
        Ok(bytes)
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, GeodataError> {
        if bytes.len() > MAX_HISTORICAL_CORRECTION_JSON_BYTES {
            return Err(invalid(
                "historical correction document exceeds its byte limit",
            ));
        }
        let document = serde_json::from_slice(bytes).map_err(|error| {
            invalid(format!(
                "historical correction deserialization failed: {error}"
            ))
        })?;
        Self::validate(&document)?;
        Ok(document)
    }

    pub fn historical_corrections(&self) -> impl Iterator<Item = &HistoricalCorrection> {
        self.corrections.iter().filter(|record| {
            matches!(
                record.evidence,
                HistoricalCorrectionEvidence::Circa600Hyde { .. }
            )
        })
    }
}

impl HistoricalCorrection {
    /// Converts one allocation produced by `allocate_hyde_area_window`.
    /// Complete land/lake/ocean coverage becomes explicit HYDE circa-600
    /// evidence. Any nodata or caller-uncovered target area becomes explicit
    /// unknown evidence instead of a zero-valued historical correction.
    pub fn from_whole_cell_allocation(
        cell_x: u16,
        cell_y: u16,
        allocation: &HydeAreaAllocation,
    ) -> Result<Self, GeodataError> {
        if !allocation.is_valid() {
            return Err(invalid(
                "whole-cell HYDE allocation contains an invalid quantity",
            ));
        }
        let quantities = HydeWholeCellQuantities {
            land_area_square_meters: allocation.land_area_square_meters,
            crop_area_square_kilometers: allocation.crop_area_square_kilometers,
            grazing_area_square_kilometers: allocation.grazing_area_square_kilometers,
            population: allocation.population,
        };
        validate_quantities(&quantities)?;
        let evidence = if allocation.nodata_area_square_meters > 0.0
            || allocation.outside_area_square_meters > 0.0
        {
            HistoricalCorrectionEvidence::Unknown
        } else {
            HistoricalCorrectionEvidence::Circa600Hyde { quantities }
        };
        Ok(Self {
            cell_x,
            cell_y,
            evidence,
        })
    }

    /// Returns quantities only for evidence explicitly identified as the HYDE
    /// circa-600 historical model. Modern, procedural, fallback, and unknown
    /// evidence never become historical quantities through this accessor.
    pub fn historical_quantities(&self) -> Option<&HydeWholeCellQuantities> {
        match &self.evidence {
            HistoricalCorrectionEvidence::Circa600Hyde { quantities } => Some(quantities),
            HistoricalCorrectionEvidence::ModernObservation { .. }
            | HistoricalCorrectionEvidence::ProceduralAddition { .. }
            | HistoricalCorrectionEvidence::FallbackEvidence { .. }
            | HistoricalCorrectionEvidence::Unknown => None,
        }
    }

    pub fn evidence_year_ce(&self) -> Option<u16> {
        match &self.evidence {
            HistoricalCorrectionEvidence::Circa600Hyde { .. } => {
                Some(HISTORICAL_CORRECTION_TARGET_YEAR_CE)
            }
            HistoricalCorrectionEvidence::ModernObservation {
                observation_year_ce,
                ..
            } => Some(*observation_year_ce),
            HistoricalCorrectionEvidence::ProceduralAddition { .. }
            | HistoricalCorrectionEvidence::FallbackEvidence { .. }
            | HistoricalCorrectionEvidence::Unknown => None,
        }
    }
}

fn validate_quantities(quantities: &HydeWholeCellQuantities) -> Result<(), GeodataError> {
    let values = [
        quantities.land_area_square_meters,
        quantities.crop_area_square_kilometers,
        quantities.grazing_area_square_kilometers,
        quantities.population,
    ];
    if values
        .into_iter()
        .any(|value| !value.is_finite() || value.is_sign_negative())
    {
        return Err(invalid(
            "historical correction quantities must be finite and nonnegative",
        ));
    }
    let land = quantities.land_area_square_meters / 1_000_000.0;
    let tolerance = land.max(1.0) * 1.0e-9;
    if quantities.crop_area_square_kilometers > land + tolerance
        || quantities.grazing_area_square_kilometers > land + tolerance
        || quantities.crop_area_square_kilometers + quantities.grazing_area_square_kilometers
            > land + tolerance
    {
        return Err(invalid(
            "historical correction crop and grazing exceed whole-cell land capacity",
        ));
    }
    if land <= 0.0 && quantities.population != 0.0 {
        return Err(invalid(
            "historical correction population requires positive land area",
        ));
    }
    Ok(())
}

fn invalid(reason: impl Into<String>) -> GeodataError {
    GeodataError::HistoricalCorrection(reason.into())
}

#[cfg(test)]
#[path = "../tests/hyde_correction.rs"]
mod tests;
