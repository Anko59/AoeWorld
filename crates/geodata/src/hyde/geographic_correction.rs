//! Geographic binding and application for sparse circa-600 history patches.
use super::HydeAreaAllocation;
use crate::{GeodataError, local_aeqd_definition};
use aoe_map::MapRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const GEOGRAPHIC_HISTORICAL_CORRECTION_SCHEMA_VERSION: u16 = 2;
// Detailed worker requests may carry this with water and vegetation patches
// inside the 64 KiB stdin envelope; schema 1 stays an intermediate format.
const MAX_DOCUMENT_BYTES: usize = 24 * 1024;
const MAX_TEXT_BYTES: usize = 512;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalGridBinding {
    pub center_latitude_e7: i32,
    pub center_longitude_e7: i32,
    pub effective_side_meters: u64,
    pub projection: String,
    pub samples_per_axis: u16,
    pub preprocessing_identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalSourceCitation {
    pub id: String,
    pub citation: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoricalQuantityPatch {
    pub valid_land_area_square_meters: f64,
    pub crop_area_square_kilometers: f64,
    pub grazing_area_square_kilometers: f64,
    pub population: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GeographicHistoricalEvidence {
    HistoricalModel {
        quantities: HistoricalQuantityPatch,
    },
    ModernObservation {
        observation_year_ce: u16,
        quantities: HistoricalQuantityPatch,
    },
    FallbackEvidence {
        quantities: HistoricalQuantityPatch,
    },
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographicHistoricalCorrection {
    pub cell_x: u16,
    pub cell_y: u16,
    pub source_id: String,
    pub evidence: GeographicHistoricalEvidence,
}

/// Schema-2 records bind whole-cell historical changes to one normalized
/// footprint and grid. Schema-1 quantity documents remain intermediates and
/// cannot be passed to this production consumer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographicHistoricalCorrectionDocument {
    pub schema_version: u16,
    pub target_year_ce: u16,
    pub binding: HistoricalGridBinding,
    pub sources: Vec<HistoricalSourceCitation>,
    pub corrections: Vec<GeographicHistoricalCorrection>,
}

impl GeographicHistoricalCorrectionDocument {
    pub fn empty(
        request: MapRequest,
        samples_per_axis: u16,
        preprocessing_identity: &str,
    ) -> Result<Self, GeodataError> {
        let request = request
            .normalized()
            .map_err(|_| invalid("historical correction request is invalid"))?;
        let document = Self {
            schema_version: GEOGRAPHIC_HISTORICAL_CORRECTION_SCHEMA_VERSION,
            target_year_ce: 600,
            binding: HistoricalGridBinding {
                center_latitude_e7: request.center_latitude_e7,
                center_longitude_e7: request.center_longitude_e7,
                effective_side_meters: request
                    .estimate()
                    .map_err(|_| invalid("historical correction footprint is invalid"))?
                    .effective_side_meters,
                projection: local_aeqd_definition(
                    request.center_latitude_e7,
                    request.center_longitude_e7,
                ),
                samples_per_axis,
                preprocessing_identity: preprocessing_identity.to_owned(),
            },
            sources: Vec::new(),
            corrections: Vec::new(),
        };
        document.validate_for(request, samples_per_axis, preprocessing_identity)?;
        Ok(document)
    }

    pub fn validate_for(
        &self,
        request: MapRequest,
        samples_per_axis: u16,
        preprocessing_identity: &str,
    ) -> Result<(), GeodataError> {
        let expected = Self::empty_binding(request, samples_per_axis, preprocessing_identity)?;
        if self.schema_version != GEOGRAPHIC_HISTORICAL_CORRECTION_SCHEMA_VERSION
            || self.target_year_ce != 600
            || self.binding != expected
        {
            return Err(invalid(
                "historical correction binding does not match request",
            ));
        }
        if self.corrections.len() > usize::from(samples_per_axis).pow(2) {
            return Err(invalid("historical correction exceeds its grid cell limit"));
        }
        let mut sources = BTreeMap::new();
        let mut previous_source = None;
        for source in &self.sources {
            if !valid_text(&source.id) || !valid_text(&source.citation) {
                return Err(invalid("historical correction source citation is invalid"));
            }
            if previous_source.is_some_and(|previous: &str| previous >= source.id.as_str()) {
                return Err(invalid("historical correction sources are not canonical"));
            }
            previous_source = Some(&source.id);
            sources.insert(source.id.as_str(), source);
        }
        let mut previous_cell = None;
        for record in &self.corrections {
            if record.cell_x >= samples_per_axis || record.cell_y >= samples_per_axis {
                return Err(invalid("historical correction cell is outside its grid"));
            }
            let cell = (record.cell_y, record.cell_x);
            if previous_cell.is_some_and(|previous| previous >= cell) {
                return Err(invalid("historical correction cells are not canonical"));
            }
            previous_cell = Some(cell);
            if !sources.contains_key(record.source_id.as_str()) {
                return Err(invalid("historical correction lacks a cited source"));
            }
            match &record.evidence {
                GeographicHistoricalEvidence::HistoricalModel { quantities }
                | GeographicHistoricalEvidence::FallbackEvidence { quantities } => {
                    validate_quantities(*quantities)?;
                }
                GeographicHistoricalEvidence::ModernObservation {
                    observation_year_ce,
                    quantities,
                } => {
                    if !(1900..=9999).contains(observation_year_ce) {
                        return Err(invalid("historical correction modern date is invalid"));
                    }
                    validate_quantities(*quantities)?;
                }
                GeographicHistoricalEvidence::Unknown => {}
            }
        }
        if serde_json::to_vec(self)
            .map_err(|_| invalid("historical correction JSON is invalid"))?
            .len()
            > MAX_DOCUMENT_BYTES
        {
            return Err(invalid("historical correction exceeds its byte limit"));
        }
        Ok(())
    }

    fn empty_binding(
        request: MapRequest,
        samples_per_axis: u16,
        preprocessing_identity: &str,
    ) -> Result<HistoricalGridBinding, GeodataError> {
        if !(2..=super::MAX_HISTORICAL_GRID_SAMPLES_PER_AXIS).contains(&samples_per_axis)
            || !valid_text(preprocessing_identity)
        {
            return Err(invalid(
                "historical correction grid or preprocessing is invalid",
            ));
        }
        let request = request
            .normalized()
            .map_err(|_| invalid("historical correction request is invalid"))?;
        Ok(HistoricalGridBinding {
            center_latitude_e7: request.center_latitude_e7,
            center_longitude_e7: request.center_longitude_e7,
            effective_side_meters: request
                .estimate()
                .map_err(|_| invalid("historical correction footprint is invalid"))?
                .effective_side_meters,
            projection: local_aeqd_definition(
                request.center_latitude_e7,
                request.center_longitude_e7,
            ),
            samples_per_axis,
            preprocessing_identity: preprocessing_identity.to_owned(),
        })
    }

    pub fn serialize(&self, request: MapRequest) -> Result<Vec<u8>, GeodataError> {
        self.validate_for(
            request,
            self.binding.samples_per_axis,
            &self.binding.preprocessing_identity,
        )?;
        let bytes = serde_json::to_vec(self)
            .map_err(|_| invalid("historical correction JSON is invalid"))?;
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("historical correction exceeds its byte limit"));
        }
        Ok(bytes)
    }

    pub fn deserialize(
        bytes: &[u8],
        request: MapRequest,
        samples_per_axis: u16,
        preprocessing_identity: &str,
    ) -> Result<Self, GeodataError> {
        if bytes.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("historical correction exceeds its byte limit"));
        }
        let document: Self = serde_json::from_slice(bytes)
            .map_err(|_| invalid("historical correction JSON is invalid"))?;
        document.validate_for(request, samples_per_axis, preprocessing_identity)?;
        Ok(document)
    }

    pub fn canonical_digest(&self, request: MapRequest) -> Result<[u8; 32], GeodataError> {
        let mut hash = Sha256::new();
        hash.update(b"aoe-geographic-historical-correction-v2\0");
        hash.update(&self.serialize(request)?);
        Ok(hash.finalize().into())
    }

    pub fn canonical_digest_hex(&self, request: MapRequest) -> Result<String, GeodataError> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in self.canonical_digest(request)? {
            encoded.push(HEX[usize::from(byte >> 4)] as char);
            encoded.push(HEX[usize::from(byte & 0x0f)] as char);
        }
        Ok(encoded)
    }

    pub fn changes_historical_land_use(&self) -> bool {
        self.corrections.iter().any(|record| {
            matches!(
                &record.evidence,
                GeographicHistoricalEvidence::HistoricalModel { .. }
                    | GeographicHistoricalEvidence::Unknown
            )
        })
    }

    /// Apply only circa-600 evidence to the matching page. Modern and fallback
    /// observations remain cited context; explicit unknown clears valid history.
    pub fn apply_page(
        &self,
        request: MapRequest,
        page_x: u16,
        page_y: u16,
        width: u16,
        height: u16,
        allocations: &mut [HydeAreaAllocation],
    ) -> Result<(), GeodataError> {
        self.validate_for(
            request,
            self.binding.samples_per_axis,
            &self.binding.preprocessing_identity,
        )?;
        self.apply_page_validated(page_x, page_y, width, height, allocations)
    }

    pub(crate) fn apply_page_validated(
        &self,
        page_x: u16,
        page_y: u16,
        width: u16,
        height: u16,
        allocations: &mut [HydeAreaAllocation],
    ) -> Result<(), GeodataError> {
        if allocations.len() != usize::from(width) * usize::from(height) {
            return Err(invalid("historical correction page shape is invalid"));
        }
        let axis = self.binding.samples_per_axis;
        if page_x >= axis
            || page_y >= axis
            || page_x.saturating_add(width) > axis
            || page_y.saturating_add(height) > axis
        {
            return Err(invalid("historical correction page is outside its grid"));
        }
        for row in page_y..page_y + height {
            let start = self
                .corrections
                .partition_point(|record| (record.cell_y, record.cell_x) < (row, page_x));
            for record in self.corrections[start..]
                .iter()
                .take_while(|record| record.cell_y == row && record.cell_x < page_x + width)
            {
                let index = usize::from(row - page_y) * usize::from(width)
                    + usize::from(record.cell_x - page_x);
                let allocation = &mut allocations[index];
                match &record.evidence {
                    GeographicHistoricalEvidence::HistoricalModel { quantities } => {
                        if quantities.valid_land_area_square_meters
                            > allocation.land_area_square_meters * (1.0 + 1.0e-9)
                        {
                            return Err(invalid(
                                "historical correction exceeds observed land area",
                            ));
                        }
                        allocation.valid_land_area_square_meters =
                            quantities.valid_land_area_square_meters;
                        allocation.crop_area_square_kilometers =
                            quantities.crop_area_square_kilometers;
                        allocation.grazing_area_square_kilometers =
                            quantities.grazing_area_square_kilometers;
                        allocation.population = quantities.population;
                    }
                    GeographicHistoricalEvidence::Unknown => {
                        allocation.valid_land_area_square_meters = 0.0;
                        allocation.crop_area_square_kilometers = 0.0;
                        allocation.grazing_area_square_kilometers = 0.0;
                        allocation.population = 0.0;
                    }
                    GeographicHistoricalEvidence::ModernObservation { .. }
                    | GeographicHistoricalEvidence::FallbackEvidence { .. } => {}
                }
            }
        }
        Ok(())
    }
}

fn validate_quantities(quantities: HistoricalQuantityPatch) -> Result<(), GeodataError> {
    let values = [
        quantities.valid_land_area_square_meters,
        quantities.crop_area_square_kilometers,
        quantities.grazing_area_square_kilometers,
        quantities.population,
    ];
    if values
        .into_iter()
        .any(|value| !value.is_finite() || value < 0.0)
    {
        return Err(invalid("historical correction quantities are invalid"));
    }
    let valid_km2 = quantities.valid_land_area_square_meters / 1_000_000.0;
    if quantities.crop_area_square_kilometers + quantities.grazing_area_square_kilometers
        > valid_km2 * (1.0 + 1.0e-9)
    {
        return Err(invalid("historical correction exceeds valid land area"));
    }
    if valid_km2 == 0.0 && quantities.population > 0.0 {
        return Err(invalid(
            "historical correction population requires valid land",
        ));
    }
    Ok(())
}

fn valid_text(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= MAX_TEXT_BYTES && !text.chars().any(char::is_control)
}

fn invalid(reason: &str) -> GeodataError {
    GeodataError::HistoricalCorrection(reason.to_owned())
}

#[cfg(test)]
#[path = "../tests/hyde_geographic_correction.rs"]
mod tests;
