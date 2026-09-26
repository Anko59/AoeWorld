use super::{EnvironmentError, HydrologyKind};
use serde::{Deserialize, Serialize};

pub const HYDROLOGY_WATER_MODEL_VERSION: u16 = 2;
pub const MODELLING_GRID_LIMIT: u16 = 1_024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum WaterModelProvenance {
    EvidenceOnly = 0,
    ModelledLakeSurface = 1,
    GeographicCorrection = 2,
    ModelledOceanSurface = 3,
    ModelledJunctionSurface = 4,
    ModelledRiverSurface = 5,
}

impl TryFrom<u8> for WaterModelProvenance {
    type Error = EnvironmentError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::EvidenceOnly),
            1 => Ok(Self::ModelledLakeSurface),
            2 => Ok(Self::GeographicCorrection),
            3 => Ok(Self::ModelledOceanSurface),
            4 => Ok(Self::ModelledJunctionSurface),
            5 => Ok(Self::ModelledRiverSurface),
            _ => Err(EnvironmentError::InvalidPage),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum WaterFlowDirection {
    Unknown = 0,
    North = 1,
    NorthEast = 2,
    East = 3,
    SouthEast = 4,
    South = 5,
    SouthWest = 6,
    West = 7,
    NorthWest = 8,
}

impl TryFrom<u8> for WaterFlowDirection {
    type Error = EnvironmentError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::North),
            2 => Ok(Self::NorthEast),
            3 => Ok(Self::East),
            4 => Ok(Self::SouthEast),
            5 => Ok(Self::South),
            6 => Ok(Self::SouthWest),
            7 => Ok(Self::West),
            8 => Ok(Self::NorthWest),
            _ => Err(EnvironmentError::InvalidPage),
        }
    }
}

/// Model output stored separately from `HydrologyObservation`. River direction
/// is emitted only where HydroRIVERS downstream topology supports the edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HydrologyWaterModelPage {
    pub kind: Vec<u8>,
    pub surface_level_centimeters: Vec<Option<i32>>,
    pub flow_direction: Vec<u8>,
    pub provenance: Vec<u8>,
}

impl HydrologyWaterModelPage {
    pub fn validate(&self, evidence: &[u8]) -> Result<(), EnvironmentError> {
        let len = evidence.len();
        if len == 0
            || self.kind.len() != len
            || self.surface_level_centimeters.len() != len
            || self.flow_direction.len() != len
            || self.provenance.len() != len
        {
            return Err(EnvironmentError::InvalidPage);
        }
        for (index, &original) in evidence.iter().enumerate() {
            let kind = HydrologyKind::try_from(self.kind[index])?;
            let original = HydrologyKind::try_from(original)?;
            let provenance = WaterModelProvenance::try_from(self.provenance[index])?;
            let flow_direction = WaterFlowDirection::try_from(self.flow_direction[index])?;
            if provenance == WaterModelProvenance::ModelledLakeSurface
                && (kind != HydrologyKind::Lake || self.surface_level_centimeters[index].is_none())
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::ModelledJunctionSurface
                && (kind != HydrologyKind::River || self.surface_level_centimeters[index].is_none())
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::ModelledRiverSurface
                && (kind != HydrologyKind::River || self.surface_level_centimeters[index].is_none())
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::ModelledOceanSurface
                && (kind != HydrologyKind::Ocean
                    || self.surface_level_centimeters[index] != Some(0))
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if self.surface_level_centimeters[index].is_some()
                && !matches!(
                    kind,
                    HydrologyKind::Lake | HydrologyKind::River | HydrologyKind::Ocean
                )
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::EvidenceOnly
                && self.surface_level_centimeters[index].is_some()
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::EvidenceOnly && kind != original {
                return Err(EnvironmentError::InvalidPage);
            }
            if provenance == WaterModelProvenance::GeographicCorrection
                && kind == original
                && kind != HydrologyKind::Land
                && self.surface_level_centimeters[index].is_none()
            {
                return Err(EnvironmentError::InvalidPage);
            }
            if kind != HydrologyKind::River && flow_direction != WaterFlowDirection::Unknown {
                return Err(EnvironmentError::InvalidPage);
            }
        }
        Ok(())
    }

    pub fn kind_at(&self, index: usize) -> Result<HydrologyKind, EnvironmentError> {
        self.kind
            .get(index)
            .copied()
            .ok_or(EnvironmentError::InvalidPage)?
            .try_into()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HydrologyWaterModelIndex {
    pub model_version: u16,
    pub samples_per_axis: u16,
    pub target_year_ce: u16,
    pub correction_document: super::WaterCorrectionDocument,
}

impl HydrologyWaterModelIndex {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if ![1, HYDROLOGY_WATER_MODEL_VERSION].contains(&self.model_version)
            || !(2..=MODELLING_GRID_LIMIT).contains(&self.samples_per_axis)
            || self.target_year_ce != super::WATER_CORRECTION_TARGET_YEAR_CE
        {
            return Err(EnvironmentError::InvalidIndex);
        }
        self.correction_document
            .validate_for(self.correction_document.request, self.samples_per_axis)?;
        Ok(())
    }

    pub fn digest(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|_| EnvironmentError::InvalidIndex)?;
        Ok(*blake3::hash(&bytes).as_bytes())
    }

    pub(crate) fn hash_into(&self, hash: &mut blake3::Hasher) {
        hash.update(b"aoe-hydrology-water-model-index-v1\0");
        hash.update(&self.model_version.to_le_bytes());
        hash.update(&self.samples_per_axis.to_le_bytes());
        hash.update(&self.target_year_ce.to_le_bytes());
        if let Ok(digest) = self.correction_document.digest() {
            hash.update(&digest);
        }
    }
}
