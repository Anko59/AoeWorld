use serde::{Deserialize, Serialize};

mod axes;
mod hydrology;
mod pages;
mod preparation;
pub use preparation::{
    LANDSCAPE_OVERVIEW_SAMPLES_PER_AXIS, LANDSCAPE_OVERVIEW_SOURCE_LOCKS, OverviewFieldAxes,
    OverviewHydrologyMode, VECTOR_HYDROLOGY_SOURCE_LOCKS,
};
mod provider;
pub use hydrology::{
    GeographicWaterPatch, HYDROLOGY_WATER_MODEL_VERSION, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyObservation,
    HydrologyWaterModelIndex, HydrologyWaterModelPage, HydrologyWaterPolicy,
    MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS, MAX_RIVER_SEGMENTS_PER_PAGE, MAX_RIVER_WIDTH_UNITS,
    MAX_WADEABLE_WIDTH_UNITS, MAX_WATER_CORRECTION_BYTES, MAX_WATER_CORRECTIONS,
    MIN_RIVER_WIDTH_UNITS, MODELLING_GRID_LIMIT, ModernLandCoverPage, RiverSegment,
    WATER_CORRECTION_SCHEMA_VERSION, WATER_CORRECTION_TARGET_YEAR_CE, WORLD_COVER_OBSERVATION_YEAR,
    WaterCorrectionDocument, WaterCorrectionOperation, WaterCorrectionProjection,
    WaterCorrectionVertex, WaterFlowDirection, WaterModelProvenance, ordered_hydrology_page_root,
    ordered_modern_land_cover_page_root,
};
pub(crate) use pages::level_zero_pages;
pub use pages::{ordered_biome_page_root, ordered_page_root, ordered_water_page_root};
pub use provider::{
    EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
};

pub const MAX_ENVIRONMENT_SAMPLES_PER_AXIS: u16 = 16_384;
pub const ENVIRONMENT_PAGE_SAMPLES: u8 = 64;
const MAX_PYRAMID_LEVELS: usize = 15;

/// Immutable, externalized environmental field index. Page bytes live in the
/// server adapter; their ordered BLAKE3 roots are part of map identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct PreparedEnvironment {
    pub samples_per_axis: u16,
    pub geographic_millimeters_per_sample: u64,
    pub page_samples: u8,
    pub elevation: FieldPyramid,
    /// Optional prepared water coverage on its own axis, declared by its first
    /// pyramid level.
    pub water: Option<FieldPyramid>,
    /// Optional potential-natural-vegetation classification. Values retain the
    /// source's published class identifiers and are mapped to game biomes only
    /// when terrain is queried.
    pub vegetation: Option<FieldPyramid>,
    /// Optional HYDE 600 AD land-use fractions and population pressure.
    pub historical_land_use: Option<FieldPyramid>,
    /// Optional, independently sampled modern water and land-cover evidence.
    /// It is a one-level source grid, not a pyramid over the elevation axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hydrology_evidence: Option<HydrologyEvidenceIndex>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct FieldPyramid {
    pub levels: Vec<PyramidLevel>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PyramidLevel {
    pub samples_per_axis: u16,
    pub ordered_page_root: [u8; 32],
}

/// One bounded environmental page, stored and verified independently from the
/// package index. Heights use the selected source vertical datum in centimeters.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ElevationPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub geographic_height_centimeters: Vec<i32>,
}

/// One bounded page with percent ocean and inland-lake coverage.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WaterPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub ocean_coverage_percent: Vec<u8>,
    pub inland_coverage_percent: Vec<u8>,
}
/// One bounded potential-biome page. `potential_biome_class` stores the
/// source's integer class, with zero reserved for nodata/fallback.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PotentialBiomePage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub potential_biome_class: Vec<u8>,
}

impl PreparedEnvironment {
    /// Each field declares its own canonical axis in its first level and every
    /// pyramid ends at one sample.
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.samples_per_axis == 0 {
            return (self.elevation.levels.is_empty()
                && self.water.is_none()
                && self.vegetation.is_none()
                && self.historical_land_use.is_none()
                && self.hydrology_evidence.is_none())
            .then_some(())
            .ok_or(EnvironmentError::InvalidPyramid);
        }
        if self.samples_per_axis > MAX_ENVIRONMENT_SAMPLES_PER_AXIS
            || self.geographic_millimeters_per_sample == 0
            || self.page_samples != ENVIRONMENT_PAGE_SAMPLES
        {
            return Err(EnvironmentError::InvalidIndex);
        }
        self.elevation.validate_axis(self.samples_per_axis)?;
        for field in [&self.water, &self.vegetation].into_iter().flatten() {
            field.validate_axis(field.axis().ok_or(EnvironmentError::InvalidPyramid)?)?;
        }
        if let Some(land_use) = &self.historical_land_use {
            let axis = land_use
                .levels
                .first()
                .ok_or(EnvironmentError::InvalidPyramid)?
                .samples_per_axis;
            if axis == 0 || axis > MAX_ENVIRONMENT_SAMPLES_PER_AXIS {
                return Err(EnvironmentError::InvalidPyramid);
            }
            land_use.validate_axis(axis)?;
        }
        self.hydrology_evidence
            .as_ref()
            .map_or(Ok(()), HydrologyEvidenceIndex::validate)
    }

    pub(crate) fn hash_into(&self, hash: &mut blake3::Hasher) {
        hash.update(&self.samples_per_axis.to_le_bytes());
        hash.update(&self.geographic_millimeters_per_sample.to_le_bytes());
        hash.update(&[self.page_samples]);
        hash.update(&(self.elevation.levels.len() as u64).to_le_bytes());
        for level in &self.elevation.levels {
            hash.update(&level.samples_per_axis.to_le_bytes());
            hash.update(&level.ordered_page_root);
        }
        hash_optional_field(hash, &self.water);
        hash_optional_field(hash, &self.vegetation);
        hash_optional_field(hash, &self.historical_land_use);
        if let Some(evidence) = &self.hydrology_evidence {
            evidence.hash_into(hash);
        }
    }
}

fn hash_optional_field(hash: &mut blake3::Hasher, field: &Option<FieldPyramid>) {
    hash.update(&[u8::from(field.is_some())]);
    if let Some(field) = field {
        hash.update(&(field.levels.len() as u64).to_le_bytes());
        for level in &field.levels {
            hash.update(&level.samples_per_axis.to_le_bytes());
            hash.update(&level.ordered_page_root);
        }
    }
}

impl FieldPyramid {
    fn validate(&self, samples_per_axis: u16) -> Result<(), EnvironmentError> {
        if self.levels.is_empty() || self.levels.len() > MAX_PYRAMID_LEVELS {
            return Err(EnvironmentError::InvalidPyramid);
        }
        let mut expected = samples_per_axis;
        for level in &self.levels {
            if level.samples_per_axis != expected || level.ordered_page_root == [0; 32] {
                return Err(EnvironmentError::InvalidPyramid);
            }
            expected = expected.div_ceil(2);
        }
        (expected == 1)
            .then_some(())
            .ok_or(EnvironmentError::InvalidPyramid)
    }
}

impl ElevationPage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.geographic_height_centimeters.len()
                != usize::from(self.width) * usize::from(self.height)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-elevation-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        for height in &self.geographic_height_centimeters {
            hash.update(&height.to_le_bytes());
        }
        Ok(*hash.finalize().as_bytes())
    }
}

impl WaterPage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.ocean_coverage_percent.len()
                != usize::from(self.width) * usize::from(self.height)
            || self.inland_coverage_percent.len()
                != usize::from(self.width) * usize::from(self.height)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-water-page-v2\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        hash.update(&self.ocean_coverage_percent);
        hash.update(&self.inland_coverage_percent);
        Ok(*hash.finalize().as_bytes())
    }
}

impl PotentialBiomePage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.potential_biome_class.len()
                != usize::from(self.width) * usize::from(self.height)
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-potential-biome-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        hash.update(&self.potential_biome_class);
        Ok(*hash.finalize().as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EnvironmentError {
    #[error("environmental field index is invalid")]
    InvalidIndex,
    #[error("environmental field pyramid is invalid")]
    InvalidPyramid,
    #[error("environmental page is invalid")]
    InvalidPage,
    #[error(
        "river segment {segment} endpoint {endpoint} axis {axis} lies outside its page plus five-tile margin"
    )]
    RiverOutsidePage {
        segment: usize,
        endpoint: usize,
        axis: usize,
    },
    #[error("hydrology page water-model presence does not match its index")]
    WaterModelPresence,
}

#[cfg(test)]
#[path = "environment/tests/environment.rs"]
mod tests;
