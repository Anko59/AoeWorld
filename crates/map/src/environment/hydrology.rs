use super::{ENVIRONMENT_PAGE_SAMPLES, EnvironmentError};
#[path = "hydrology/correction.rs"]
mod correction;
#[path = "hydrology/model.rs"]
mod model;
pub use correction::{
    GeographicWaterPatch, MAX_WATER_CORRECTION_BYTES, MAX_WATER_CORRECTIONS,
    WATER_CORRECTION_SCHEMA_VERSION, WATER_CORRECTION_TARGET_YEAR_CE, WaterCorrectionDocument,
    WaterCorrectionOperation, WaterCorrectionProjection, WaterCorrectionVertex,
};
pub use model::{
    HYDROLOGY_WATER_MODEL_VERSION, HydrologyWaterModelIndex, HydrologyWaterModelPage,
    MODELLING_GRID_LIMIT, WaterFlowDirection, WaterModelProvenance,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS: u16 = 1_024;
pub const WORLD_COVER_OBSERVATION_YEAR: u16 = 2_021;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum HydrologyKind {
    Land = 0,
    Ocean = 1,
    Lake = 2,
    River = 3,
    Shallow = 4,
    Reservoir = 5,
    UnknownWater = 6,
    RegulatedLake = 7,
    NoEvidence = 8,
}

impl TryFrom<u8> for HydrologyKind {
    type Error = EnvironmentError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Land),
            1 => Ok(Self::Ocean),
            2 => Ok(Self::Lake),
            3 => Ok(Self::River),
            4 => Ok(Self::Shallow),
            5 => Ok(Self::Reservoir),
            6 => Ok(Self::UnknownWater),
            7 => Ok(Self::RegulatedLake),
            8 => Ok(Self::NoEvidence),
            _ => Err(EnvironmentError::InvalidPage),
        }
    }
}

/// Identifies how a category was obtained, without assigning an unsupported
/// probability that it describes the historical landscape.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum HydrologyEvidenceMethod {
    None = 0,
    OverviewOcean = 1,
    HydroLakesExtent = 2,
    HydroRiversBufferedCorridor = 3,
    WorldCoverClass = 4,
}

impl TryFrom<u8> for HydrologyEvidenceMethod {
    type Error = EnvironmentError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::OverviewOcean),
            2 => Ok(Self::HydroLakesExtent),
            3 => Ok(Self::HydroRiversBufferedCorridor),
            4 => Ok(Self::WorldCoverClass),
            _ => Err(EnvironmentError::InvalidPage),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HydrologyObservation {
    pub kind: HydrologyKind,
    pub method: HydrologyEvidenceMethod,
}

impl HydrologyObservation {
    pub fn validate(self) -> Result<(), EnvironmentError> {
        let valid = match self.method {
            HydrologyEvidenceMethod::None => self.kind == HydrologyKind::NoEvidence,
            HydrologyEvidenceMethod::OverviewOcean => self.kind == HydrologyKind::Ocean,
            HydrologyEvidenceMethod::HydroLakesExtent => matches!(
                self.kind,
                HydrologyKind::Lake
                    | HydrologyKind::Reservoir
                    | HydrologyKind::RegulatedLake
                    | HydrologyKind::UnknownWater
            ),
            HydrologyEvidenceMethod::HydroRiversBufferedCorridor => {
                self.kind == HydrologyKind::River
            }
            HydrologyEvidenceMethod::WorldCoverClass => matches!(
                self.kind,
                HydrologyKind::Land | HydrologyKind::Shallow | HydrologyKind::UnknownWater
            ),
        };
        valid.then_some(()).ok_or(EnvironmentError::InvalidPage)
    }
}

/// Identity for bounded, level-zero observation grids. The WorldCover year is
/// separate from the HydroLAKES and HydroRIVERS release identities in source
/// locks; those sources do not share a claimed observation year.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HydrologyEvidenceIndex {
    pub samples_per_axis: u16,
    pub page_samples: u8,
    pub world_cover_year: u16,
    pub policy: HydrologyWaterPolicy,
    pub hydrology_page_root: [u8; 32],
    pub modern_land_cover_page_root: [u8; 32],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_model: Option<HydrologyWaterModelIndex>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum HydrologyWaterPolicy {
    /// Keep overview/HYDE coverage authoritative except for mapped natural
    /// lakes and the modeled HydroRIVERS corridor already used in the package.
    HistoricalOverviewWithMappedNaturalWaterV1 = 1,
}

impl HydrologyEvidenceIndex {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        if !(2..=MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS).contains(&self.samples_per_axis)
            || self.page_samples != ENVIRONMENT_PAGE_SAMPLES
            || self.world_cover_year != WORLD_COVER_OBSERVATION_YEAR
            || self.hydrology_page_root == [0; 32]
            || self.modern_land_cover_page_root == [0; 32]
        {
            return Err(EnvironmentError::InvalidIndex);
        }
        if let Some(model) = &self.water_model {
            model.validate()?;
            if model.samples_per_axis != self.samples_per_axis {
                return Err(EnvironmentError::InvalidIndex);
            }
        }
        Ok(())
    }

    pub(crate) fn hash_into(&self, hash: &mut blake3::Hasher) {
        hash.update(b"aoe-hydrology-evidence-index-v1\0");
        hash.update(&self.samples_per_axis.to_le_bytes());
        hash.update(&[self.page_samples]);
        hash.update(&self.world_cover_year.to_le_bytes());
        hash.update(&[self.policy as u8]);
        hash.update(&self.hydrology_page_root);
        hash.update(&self.modern_land_cover_page_root);
        if let Some(model) = &self.water_model {
            hash.update(&[1]);
            model.hash_into(hash);
        } else {
            hash.update(&[0]);
        }
    }

    pub fn validate_pages(
        &self,
        hydrology: &[HydrologyEvidencePage],
        land_cover: &[ModernLandCoverPage],
    ) -> Result<(), EnvironmentError> {
        self.validate()?;
        if let Some(model) = &self.water_model {
            model.validate()?;
            if model.samples_per_axis != self.samples_per_axis
                || model.correction_document.samples_per_axis != self.samples_per_axis
            {
                return Err(EnvironmentError::InvalidIndex);
            }
        }
        validate_page_set(
            self.samples_per_axis,
            hydrology,
            HydrologyEvidencePage::validate,
            |page| (page.x, page.y, page.width, page.height),
        )?;
        validate_page_set(
            self.samples_per_axis,
            land_cover,
            ModernLandCoverPage::validate,
            |page| (page.x, page.y, page.width, page.height),
        )?;
        if ordered_hydrology_page_root(hydrology)? != self.hydrology_page_root
            || ordered_modern_land_cover_page_root(land_cover)? != self.modern_land_cover_page_root
        {
            return Err(EnvironmentError::InvalidPyramid);
        }
        let expects_model = self.water_model.is_some();
        if hydrology
            .iter()
            .any(|page| page.water_model.is_some() != expects_model)
        {
            return Err(EnvironmentError::InvalidPyramid);
        }
        Ok(())
    }
}

fn validate_page_set<T>(
    axis: u16,
    pages: &[T],
    validate: impl Fn(&T) -> Result<(), EnvironmentError>,
    coordinates: impl Fn(&T) -> (u16, u16, u8, u8),
) -> Result<(), EnvironmentError> {
    let side = u16::from(ENVIRONMENT_PAGE_SAMPLES);
    let count = axis.div_ceil(side);
    if pages.len() != usize::from(count).pow(2) {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut seen = BTreeSet::new();
    for page in pages {
        validate(page)?;
        let (x, y, width, height) = coordinates(page);
        if x >= count || y >= count || !seen.insert((x, y)) {
            return Err(EnvironmentError::InvalidPyramid);
        }
        let expected_width = (axis - x * side).min(side) as u8;
        let expected_height = (axis - y * side).min(side) as u8;
        if (width, height) != (expected_width, expected_height) {
            return Err(EnvironmentError::InvalidPage);
        }
    }
    Ok(())
}

/// Bounded typed observations aligned to their own source grid (at most 1024²).
/// River corridors are buffered models; no water levels, flow directions or
/// barrier locations are implied by this page.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HydrologyEvidencePage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub kind: Vec<u8>,
    pub method: Vec<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub water_model: Option<HydrologyWaterModelPage>,
}

impl HydrologyEvidencePage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        let count = usize::from(self.width) * usize::from(self.height);
        if self.level != 0
            || self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.kind.len() != count
            || self.method.len() != count
        {
            return Err(EnvironmentError::InvalidPage);
        }
        for (&kind, &method) in self.kind.iter().zip(&self.method) {
            HydrologyObservation {
                kind: kind.try_into()?,
                method: method.try_into()?,
            }
            .validate()?;
        }
        if let Some(model) = &self.water_model {
            model.validate(&self.kind)?;
        }
        Ok(())
    }

    pub fn observation(&self, index: usize) -> Result<HydrologyObservation, EnvironmentError> {
        let count = usize::from(self.width) * usize::from(self.height);
        if self.level != 0
            || self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.kind.len() != count
            || self.method.len() != count
        {
            return Err(EnvironmentError::InvalidPage);
        }
        let observation = HydrologyObservation {
            kind: self
                .kind
                .get(index)
                .copied()
                .ok_or(EnvironmentError::InvalidPage)?
                .try_into()?,
            method: self
                .method
                .get(index)
                .copied()
                .ok_or(EnvironmentError::InvalidPage)?
                .try_into()?,
        };
        observation.validate()?;
        Ok(observation)
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-hydrology-evidence-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        hash.update(&self.kind);
        hash.update(&self.method);
        if let Some(model) = &self.water_model {
            hash.update(b"modeled-water-v1\0");
            for index in 0..model.kind.len() {
                hash.update(&[model.kind[index]]);
                if let Some(level) = model.surface_level_centimeters[index] {
                    hash.update(&[1]);
                    hash.update(&level.to_le_bytes());
                } else {
                    hash.update(&[0]);
                }
                hash.update(&[model.flow_direction[index], model.provenance[index]]);
            }
        }
        Ok(*hash.finalize().as_bytes())
    }
}

/// Raw ESA WorldCover class identifiers; zero represents no raster evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModernLandCoverPage {
    pub level: u8,
    pub x: u16,
    pub y: u16,
    pub width: u8,
    pub height: u8,
    pub worldcover_class: Vec<u8>,
}

impl ModernLandCoverPage {
    pub fn validate(&self) -> Result<(), EnvironmentError> {
        let count = usize::from(self.width) * usize::from(self.height);
        if self.level != 0
            || self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.worldcover_class.len() != count
            || self.worldcover_class.iter().any(|class| {
                !matches!(
                    *class,
                    0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 95 | 100
                )
            })
        {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(())
    }

    /// Reads one selected class without rescanning the full page on each tile query.
    /// Full validation and page hashing happen when the page enters residency.
    pub fn class_at(&self, index: usize) -> Result<u8, EnvironmentError> {
        let count = usize::from(self.width) * usize::from(self.height);
        if self.level != 0
            || self.width == 0
            || self.height == 0
            || self.width > ENVIRONMENT_PAGE_SAMPLES
            || self.height > ENVIRONMENT_PAGE_SAMPLES
            || self.worldcover_class.len() != count
        {
            return Err(EnvironmentError::InvalidPage);
        }
        let class = *self
            .worldcover_class
            .get(index)
            .ok_or(EnvironmentError::InvalidPage)?;
        if !matches!(
            class,
            0 | 10 | 20 | 30 | 40 | 50 | 60 | 70 | 80 | 90 | 95 | 100
        ) {
            return Err(EnvironmentError::InvalidPage);
        }
        Ok(class)
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        self.validate()?;
        let mut hash = blake3::Hasher::new();
        hash.update(b"aoe-modern-land-cover-page-v1\0");
        hash.update(&[self.level]);
        hash.update(&self.x.to_le_bytes());
        hash.update(&self.y.to_le_bytes());
        hash.update(&[self.width, self.height]);
        hash.update(&self.worldcover_class);
        Ok(*hash.finalize().as_bytes())
    }
}

pub fn ordered_hydrology_page_root(
    pages: &[HydrologyEvidencePage],
) -> Result<[u8; 32], EnvironmentError> {
    ordered_root(
        pages,
        crate::PageLayer::HydrologyEvidence,
        HydrologyEvidencePage::content_hash,
        |page| (page.x, page.y),
    )
}

pub fn ordered_modern_land_cover_page_root(
    pages: &[ModernLandCoverPage],
) -> Result<[u8; 32], EnvironmentError> {
    ordered_root(
        pages,
        crate::PageLayer::ModernLandCover,
        ModernLandCoverPage::content_hash,
        |page| (page.x, page.y),
    )
}

fn ordered_root<T>(
    pages: &[T],
    layer: crate::PageLayer,
    hash_page: impl Fn(&T) -> Result<[u8; 32], EnvironmentError>,
    coordinates: impl Fn(&T) -> (u16, u16),
) -> Result<[u8; 32], EnvironmentError> {
    if pages.is_empty() {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut ordered = pages.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|page| {
        let (x, y) = coordinates(page);
        (y, x)
    });
    if ordered
        .windows(2)
        .any(|pair| coordinates(pair[0]) == coordinates(pair[1]))
    {
        return Err(EnvironmentError::InvalidPyramid);
    }
    let mut root = crate::PageRootBuilder::new(layer, ordered.len())?;
    for page in ordered {
        root.push(hash_page(page)?)?;
    }
    root.finish()
}

#[cfg(test)]
mod tests;
