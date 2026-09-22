use super::{DirectoryLayer, GeodataError, MAX_DIRECTORY_PAGE_BYTES, PageKey, invalid, json_error};
use aoe_map::{
    ElevationPage, HistoricalLandUsePage, HydrologyEvidencePage, ModernLandCoverPage,
    PotentialBiomePage, WaterPage,
};

pub(super) enum PageRef<'a> {
    Elevation(&'a ElevationPage),
    Water(&'a WaterPage),
    Vegetation(&'a PotentialBiomePage),
    HistoricalLandUse(&'a HistoricalLandUsePage),
    HydrologyEvidence(&'a HydrologyEvidencePage),
    ModernLandCover(&'a ModernLandCoverPage),
}

impl PageRef<'_> {
    pub(super) fn layer(&self) -> DirectoryLayer {
        match self {
            Self::Elevation(_) => DirectoryLayer::Elevation,
            Self::Water(_) => DirectoryLayer::Water,
            Self::Vegetation(_) => DirectoryLayer::Vegetation,
            Self::HistoricalLandUse(_) => DirectoryLayer::HistoricalLandUse,
            Self::HydrologyEvidence(_) => DirectoryLayer::HydrologyEvidence,
            Self::ModernLandCover(_) => DirectoryLayer::ModernLandCover,
        }
    }

    pub(super) fn key(&self) -> PageKey {
        let (level, x, y) = match self {
            Self::Elevation(page) => (page.level, page.x, page.y),
            Self::Water(page) => (page.level, page.x, page.y),
            Self::Vegetation(page) => (page.level, page.x, page.y),
            Self::HistoricalLandUse(page) => (page.level, page.x, page.y),
            Self::HydrologyEvidence(page) => (page.level, page.x, page.y),
            Self::ModernLandCover(page) => (page.level, page.x, page.y),
        };
        PageKey {
            layer: self.layer(),
            level,
            x,
            y,
        }
    }

    pub(super) fn serialized(&self) -> Result<(PageKey, Vec<u8>), GeodataError> {
        let bytes = match self {
            Self::Elevation(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::Water(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::Vegetation(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::HistoricalLandUse(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::HydrologyEvidence(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
            Self::ModernLandCover(page) => {
                page.content_hash()?;
                serde_json::to_vec(page).map_err(json_error)?
            }
        };
        if bytes.is_empty() || bytes.len() as u64 > MAX_DIRECTORY_PAGE_BYTES {
            return Err(invalid("serialized page exceeds the directory page limit"));
        }
        Ok((self.key(), bytes))
    }
}

pub(super) enum PageValue {
    Elevation(ElevationPage),
    Water(WaterPage),
    Vegetation(PotentialBiomePage),
    HistoricalLandUse(HistoricalLandUsePage),
    HydrologyEvidence(HydrologyEvidencePage),
    ModernLandCover(ModernLandCoverPage),
}

impl PageValue {
    pub(super) fn key(&self) -> PageKey {
        let (layer, level, x, y) = match self {
            Self::Elevation(page) => (DirectoryLayer::Elevation, page.level, page.x, page.y),
            Self::Water(page) => (DirectoryLayer::Water, page.level, page.x, page.y),
            Self::Vegetation(page) => (DirectoryLayer::Vegetation, page.level, page.x, page.y),
            Self::HistoricalLandUse(page) => (
                DirectoryLayer::HistoricalLandUse,
                page.level,
                page.x,
                page.y,
            ),
            Self::HydrologyEvidence(page) => (
                DirectoryLayer::HydrologyEvidence,
                page.level,
                page.x,
                page.y,
            ),
            Self::ModernLandCover(page) => {
                (DirectoryLayer::ModernLandCover, page.level, page.x, page.y)
            }
        };
        PageKey { layer, level, x, y }
    }

    pub(super) fn content_hash(&self) -> Result<[u8; 32], GeodataError> {
        Ok(match self {
            Self::Elevation(page) => page.content_hash()?,
            Self::Water(page) => page.content_hash()?,
            Self::Vegetation(page) => page.content_hash()?,
            Self::HistoricalLandUse(page) => page.content_hash()?,
            Self::HydrologyEvidence(page) => page.content_hash()?,
            Self::ModernLandCover(page) => page.content_hash()?,
        })
    }

    pub(super) fn dimensions(&self) -> (u8, u8) {
        match self {
            Self::Elevation(page) => (page.width, page.height),
            Self::Water(page) => (page.width, page.height),
            Self::Vegetation(page) => (page.width, page.height),
            Self::HistoricalLandUse(page) => (page.width, page.height),
            Self::HydrologyEvidence(page) => (page.width, page.height),
            Self::ModernLandCover(page) => (page.width, page.height),
        }
    }
}
