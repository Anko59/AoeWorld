//! Bounded, cited geographic patches for potential natural vegetation.
use crate::{GeodataError, MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS, local_aeqd_definition};
use aoe_map::MapRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

// History and water patches can share a single 64 KiB worker request.
const MAX_DOCUMENT_BYTES: usize = 8 * 1024;
pub const VEGETATION_PATCH_PREPROCESSING_IDENTITY: &str =
    "potential-biome-nearest-gdal-0.19-patches-v1";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VegetationPatchBinding {
    pub center_latitude_e7: i32,
    pub center_longitude_e7: i32,
    pub effective_side_meters: u64,
    pub projection: String,
    pub samples_per_axis: u16,
    pub preprocessing_identity: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VegetationPatchSource {
    pub id: String,
    pub citation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VegetationPatchOperation {
    HistoricalBiome { class: u8 },
    ModernObservation { observation_year_ce: u16, class: u8 },
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VegetationPatch {
    pub id: String,
    pub source_id: String,
    pub priority: u16,
    /// Local projected metres: west, south, east, north.
    pub rectangle_east_north_meters: [i32; 4],
    pub applicable_year_start_ce: u16,
    pub applicable_year_end_ce: u16,
    pub operation: VegetationPatchOperation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VegetationPatchDocument {
    pub schema_version: u16,
    pub target_year_ce: u16,
    pub binding: VegetationPatchBinding,
    pub sources: Vec<VegetationPatchSource>,
    /// Canonical precedence order: ascending priority, then ascending ID;
    /// the last applicable patch covering a cell wins.
    pub patches: Vec<VegetationPatch>,
}

impl VegetationPatchDocument {
    pub fn empty(request: MapRequest, axis: u16) -> Result<Self, GeodataError> {
        Ok(Self {
            schema_version: 1,
            target_year_ce: 600,
            binding: binding(request, axis)?,
            sources: Vec::new(),
            patches: Vec::new(),
        })
    }

    pub fn validate_for(&self, request: MapRequest, axis: u16) -> Result<(), GeodataError> {
        if self.schema_version != 1
            || self.target_year_ce != 600
            || self.binding != binding(request, axis)?
        {
            return Err(invalid("vegetation patch binding does not match request"));
        }
        if self.sources.len() > 128 || self.patches.len() > 128 {
            return Err(invalid("vegetation patch count exceeds limit"));
        }
        let mut sources = BTreeSet::new();
        let mut previous_source = None;
        for source in &self.sources {
            if !valid_id(&source.id)
                || source.citation.trim().is_empty()
                || source.citation.len() > 512
                || source.citation.chars().any(char::is_control)
            {
                return Err(invalid("vegetation patch source is invalid"));
            }
            if previous_source.is_some_and(|previous: &str| previous >= source.id.as_str()) {
                return Err(invalid("vegetation patch sources are not canonical"));
            }
            previous_source = Some(&source.id);
            sources.insert(source.id.as_str());
        }
        let half = i64::try_from(self.binding.effective_side_meters / 2)
            .map_err(|_| invalid("vegetation footprint is invalid"))?;
        let mut previous_patch = None;
        let mut patch_ids = BTreeSet::new();
        for patch in &self.patches {
            if !valid_id(&patch.id)
                || !patch_ids.insert(patch.id.as_str())
                || !sources.contains(patch.source_id.as_str())
            {
                return Err(invalid("vegetation patch ID or citation is invalid"));
            }
            let key = (patch.priority, patch.id.as_str());
            if previous_patch.is_some_and(|previous| previous >= key) {
                return Err(invalid("vegetation patch precedence is not canonical"));
            }
            previous_patch = Some(key);
            let [west, south, east, north] = patch.rectangle_east_north_meters;
            if west >= east
                || south >= north
                || i64::from(west) < -half
                || i64::from(east) > half
                || i64::from(south) < -half
                || i64::from(north) > half
            {
                return Err(invalid("vegetation patch rectangle is outside footprint"));
            }
            if patch.applicable_year_start_ce > 600
                || patch.applicable_year_end_ce < 600
                || patch.applicable_year_start_ce > patch.applicable_year_end_ce
            {
                return Err(invalid("vegetation patch year does not include 600 CE"));
            }
            match patch.operation {
                VegetationPatchOperation::HistoricalBiome { class } => valid_class(class)?,
                VegetationPatchOperation::ModernObservation {
                    observation_year_ce,
                    class,
                } => {
                    if observation_year_ce < 1900 {
                        return Err(invalid("modern vegetation observation year is invalid"));
                    }
                    valid_class(class)?;
                }
                VegetationPatchOperation::Unknown => {}
            }
        }
        if self.bytes()?.len() > MAX_DOCUMENT_BYTES {
            return Err(invalid("vegetation patch document exceeds byte limit"));
        }
        Ok(())
    }

    pub fn digest_hex(&self, request: MapRequest) -> Result<String, GeodataError> {
        self.validate_for(request, self.binding.samples_per_axis)?;
        let mut hash = Sha256::new();
        hash.update(b"aoe-vegetation-geographic-patch-v1\0");
        hash.update(self.bytes()?);
        Ok(format!("{:x}", hash.finalize()))
    }

    pub fn changes_historical_vegetation(&self) -> bool {
        self.patches.iter().any(|patch| {
            matches!(
                patch.operation,
                VegetationPatchOperation::HistoricalBiome { .. }
                    | VegetationPatchOperation::Unknown
            )
        })
    }

    pub(crate) fn apply(&self, axis: u16, classes: &mut [u8]) -> Result<(), GeodataError> {
        if classes.len() != usize::from(axis).pow(2) || axis != self.binding.samples_per_axis {
            return Err(invalid("vegetation patch grid shape is invalid"));
        }
        let side = self.binding.effective_side_meters as f64;
        for patch in &self.patches {
            let class = match patch.operation {
                VegetationPatchOperation::HistoricalBiome { class } => class,
                VegetationPatchOperation::Unknown => 0,
                VegetationPatchOperation::ModernObservation { .. } => continue,
            };
            let [west, south, east, north] = patch.rectangle_east_north_meters;
            for y in 0..axis {
                let projected_north = side / 2.0 - (f64::from(y) + 0.5) * side / f64::from(axis);
                if projected_north < f64::from(south) || projected_north >= f64::from(north) {
                    continue;
                }
                for x in 0..axis {
                    let projected_east =
                        -side / 2.0 + (f64::from(x) + 0.5) * side / f64::from(axis);
                    if projected_east >= f64::from(west) && projected_east < f64::from(east) {
                        classes[usize::from(y) * usize::from(axis) + usize::from(x)] = class;
                    }
                }
            }
        }
        Ok(())
    }

    fn bytes(&self) -> Result<Vec<u8>, GeodataError> {
        serde_json::to_vec(self).map_err(|_| invalid("vegetation patch JSON is invalid"))
    }
}

fn binding(request: MapRequest, axis: u16) -> Result<VegetationPatchBinding, GeodataError> {
    if !(2..=MAX_DIRECT_ELEVATION_SAMPLES_PER_AXIS).contains(&axis) {
        return Err(invalid("vegetation patch axis is outside direct bounds"));
    }
    let request = request
        .normalized()
        .map_err(|_| invalid("vegetation patch request is invalid"))?;
    Ok(VegetationPatchBinding {
        center_latitude_e7: request.center_latitude_e7,
        center_longitude_e7: request.center_longitude_e7,
        effective_side_meters: request
            .estimate()
            .map_err(|_| invalid("vegetation patch footprint is invalid"))?
            .effective_side_meters,
        projection: local_aeqd_definition(request.center_latitude_e7, request.center_longitude_e7),
        samples_per_axis: axis,
        preprocessing_identity: VEGETATION_PATCH_PREPROCESSING_IDENTITY.to_owned(),
    })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
        })
}

fn valid_class(class: u8) -> Result<(), GeodataError> {
    if matches!(class, 1..=4 | 7..=9 | 13..=20 | 22 | 27..=28 | 30..=32) {
        Ok(())
    } else {
        Err(invalid("vegetation patch class is unsupported"))
    }
}

fn invalid(message: &'static str) -> GeodataError {
    GeodataError::Preparation(message)
}

#[cfg(test)]
#[path = "../tests/vegetation_correction.rs"]
mod tests;
