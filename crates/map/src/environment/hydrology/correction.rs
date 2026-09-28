use crate::{EnvironmentError, MapRequest};
use serde::{Deserialize, Serialize};

pub const WATER_CORRECTION_SCHEMA_VERSION: u16 = 1;
pub const WATER_CORRECTION_TARGET_YEAR_CE: u16 = 600;
pub const MAX_WATER_CORRECTIONS: usize = 64;
pub const MAX_WATER_CORRECTION_BYTES: usize = 24 * 1024;
const MAX_PATCH_ID_BYTES: usize = 64;
const MAX_SOURCE_CITATION_BYTES: usize = 256;
const MAX_PATCH_VERTICES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterCorrectionProjection {
    LocalAeqdWgs84V1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterCorrectionOperation {
    SetNaturalLake,
    SetLand,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterCorrectionVertex {
    pub longitude_e7: i32,
    pub latitude_e7: i32,
}

/// A bounded geographic correction to the historical water model. Corrections
/// are curation decisions, never observations; the citation and applicability
/// interval are retained in package identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeographicWaterPatch {
    pub id: String,
    pub precedence: i16,
    pub applies_from_year_ce: u16,
    pub applies_through_year_ce: u16,
    pub source_citation: String,
    pub operation: WaterCorrectionOperation,
    pub polygon: Vec<WaterCorrectionVertex>,
}

/// Versioned, request-bound correction input. A missing or empty document is a
/// valid default. Patches use center inclusion on the explicitly bound grid.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterCorrectionDocument {
    pub schema_version: u16,
    pub target_year_ce: u16,
    pub samples_per_axis: u16,
    pub request: MapRequest,
    pub projection: WaterCorrectionProjection,
    pub patches: Vec<GeographicWaterPatch>,
}

impl WaterCorrectionDocument {
    pub fn empty(request: MapRequest, samples_per_axis: u16) -> Result<Self, EnvironmentError> {
        Self::new(request, samples_per_axis, Vec::new())
    }

    pub fn new(
        request: MapRequest,
        samples_per_axis: u16,
        mut patches: Vec<GeographicWaterPatch>,
    ) -> Result<Self, EnvironmentError> {
        patches.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.id.cmp(&right.id))
        });
        let document = Self {
            schema_version: WATER_CORRECTION_SCHEMA_VERSION,
            target_year_ce: WATER_CORRECTION_TARGET_YEAR_CE,
            samples_per_axis,
            request: request
                .normalized()
                .map_err(|_| EnvironmentError::InvalidIndex)?,
            projection: WaterCorrectionProjection::LocalAeqdWgs84V1,
            patches,
        };
        document.validate()?;
        Ok(document)
    }

    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.schema_version != WATER_CORRECTION_SCHEMA_VERSION
            || self.target_year_ce != WATER_CORRECTION_TARGET_YEAR_CE
            || !(2..=1_024).contains(&self.samples_per_axis)
            || self.request.normalized().ok() != Some(self.request)
            || self.patches.len() > MAX_WATER_CORRECTIONS
        {
            return Err(EnvironmentError::InvalidIndex);
        }
        let encoded_size = serde_json::to_vec(self)
            .map_err(|_| EnvironmentError::InvalidIndex)?
            .len();
        if encoded_size > MAX_WATER_CORRECTION_BYTES {
            return Err(EnvironmentError::InvalidIndex);
        }
        let mut ids = self
            .patches
            .iter()
            .map(|patch| patch.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        if ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(EnvironmentError::InvalidIndex);
        }
        for patch in &self.patches {
            if patch.id.is_empty()
                || patch.id.len() > MAX_PATCH_ID_BYTES
                || !patch
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
                || patch.applies_from_year_ce > WATER_CORRECTION_TARGET_YEAR_CE
                || patch.applies_through_year_ce < WATER_CORRECTION_TARGET_YEAR_CE
                || patch.applies_from_year_ce > patch.applies_through_year_ce
                || patch.source_citation.trim().is_empty()
                || patch.source_citation.len() > MAX_SOURCE_CITATION_BYTES
                || patch.polygon.len() < 3
                || patch.polygon.len() > MAX_PATCH_VERTICES
                || !simple_geographic_polygon(&patch.polygon)
            {
                return Err(EnvironmentError::InvalidIndex);
            }
        }
        if self.patches.windows(2).any(|pair| {
            (pair[0].precedence, pair[0].id.as_str()) >= (pair[1].precedence, pair[1].id.as_str())
        }) {
            return Err(EnvironmentError::InvalidIndex);
        }
        Ok(())
    }

    pub fn validate_for(
        &self,
        request: MapRequest,
        samples_per_axis: u16,
    ) -> Result<(), EnvironmentError> {
        self.validate()?;
        let normalized = request
            .normalized()
            .map_err(|_| EnvironmentError::InvalidIndex)?;
        (self.request == normalized && self.samples_per_axis == samples_per_axis)
            .then_some(())
            .ok_or(EnvironmentError::InvalidIndex)
    }

    pub fn digest(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| EnvironmentError::InvalidIndex)?;
        if bytes.len() > MAX_WATER_CORRECTION_BYTES {
            return Err(EnvironmentError::InvalidIndex);
        }
        Ok(*blake3::hash(&bytes).as_bytes())
    }

    pub fn serialize(&self) -> Result<Vec<u8>, EnvironmentError> {
        let bytes = serde_json::to_vec(self).map_err(|_| EnvironmentError::InvalidIndex)?;
        if bytes.len() > MAX_WATER_CORRECTION_BYTES {
            return Err(EnvironmentError::InvalidIndex);
        }
        self.validate()?;
        Ok(bytes)
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, EnvironmentError> {
        if bytes.len() > MAX_WATER_CORRECTION_BYTES {
            return Err(EnvironmentError::InvalidIndex);
        }
        let document: Self =
            serde_json::from_slice(bytes).map_err(|_| EnvironmentError::InvalidIndex)?;
        document.validate()?;
        Ok(document)
    }
}

fn simple_geographic_polygon(points: &[WaterCorrectionVertex]) -> bool {
    if points.iter().any(|point| {
        !(-1_800_000_000..=1_800_000_000).contains(&point.longitude_e7)
            || !(-899_999_999..=899_999_999).contains(&point.latitude_e7)
    }) {
        return false;
    }
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        if point.longitude_e7.abs_diff(next.longitude_e7) >= 1_800_000_000 || *point == next {
            return false;
        }
    }
    let area = points
        .iter()
        .enumerate()
        .fold(0_i128, |total, (index, point)| {
            let next = points[(index + 1) % points.len()];
            total + i128::from(point.longitude_e7) * i128::from(next.latitude_e7)
                - i128::from(next.longitude_e7) * i128::from(point.latitude_e7)
        });
    if area == 0 {
        return false;
    }
    for first in 0..points.len() {
        let first_next = (first + 1) % points.len();
        for second in first + 1..points.len() {
            let second_next = (second + 1) % points.len();
            if first_next == second || second_next == first {
                continue;
            }
            if segments_intersect(
                points[first],
                points[first_next],
                points[second],
                points[second_next],
            ) {
                return false;
            }
        }
    }
    true
}

fn segments_intersect(
    a: WaterCorrectionVertex,
    b: WaterCorrectionVertex,
    c: WaterCorrectionVertex,
    d: WaterCorrectionVertex,
) -> bool {
    let ab_c = orientation(a, b, c);
    let ab_d = orientation(a, b, d);
    let cd_a = orientation(c, d, a);
    let cd_b = orientation(c, d, b);
    if (ab_c == 0 && on_segment(a, b, c))
        || (ab_d == 0 && on_segment(a, b, d))
        || (cd_a == 0 && on_segment(c, d, a))
        || (cd_b == 0 && on_segment(c, d, b))
    {
        return true;
    }
    (ab_c < 0) != (ab_d < 0) && (cd_a < 0) != (cd_b < 0)
}

fn orientation(
    a: WaterCorrectionVertex,
    b: WaterCorrectionVertex,
    c: WaterCorrectionVertex,
) -> i128 {
    let ab_x = i128::from(b.longitude_e7) - i128::from(a.longitude_e7);
    let ab_y = i128::from(b.latitude_e7) - i128::from(a.latitude_e7);
    let ac_x = i128::from(c.longitude_e7) - i128::from(a.longitude_e7);
    let ac_y = i128::from(c.latitude_e7) - i128::from(a.latitude_e7);
    ab_x * ac_y - ab_y * ac_x
}

fn on_segment(
    a: WaterCorrectionVertex,
    b: WaterCorrectionVertex,
    point: WaterCorrectionVertex,
) -> bool {
    point.longitude_e7 >= a.longitude_e7.min(b.longitude_e7)
        && point.longitude_e7 <= a.longitude_e7.max(b.longitude_e7)
        && point.latitude_e7 >= a.latitude_e7.min(b.latitude_e7)
        && point.latitude_e7 <= a.latitude_e7.max(b.latitude_e7)
}
