use super::{
    ElevationPage, EnvironmentError, HydrologyEvidencePage, ModernLandCoverPage,
    PotentialBiomePage, WaterPage,
};
use crate::{HistoricalLandUsePage, PageLayer};
use std::sync::Arc;

/// A coordinate in the immutable prepared-page index. Page coordinates are
/// always expressed in the source pyramid, never in virtual game tiles.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentPageKey {
    pub layer: PageLayer,
    pub level: u8,
    pub x: u16,
    pub y: u16,
}

/// One verified page returned by an environment adapter. Adapters return an
/// immutable handle so repeated tile samples do not clone page arrays.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EnvironmentPage {
    Elevation(ElevationPage),
    Water(WaterPage),
    Vegetation(PotentialBiomePage),
    HistoricalLandUse(HistoricalLandUsePage),
    HydrologyEvidence(HydrologyEvidencePage),
    ModernLandCover(ModernLandCoverPage),
}

impl EnvironmentPage {
    pub fn key(&self) -> EnvironmentPageKey {
        match self {
            Self::Elevation(page) => EnvironmentPageKey {
                layer: PageLayer::Elevation,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::Water(page) => EnvironmentPageKey {
                layer: PageLayer::Water,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::Vegetation(page) => EnvironmentPageKey {
                layer: PageLayer::Vegetation,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::HistoricalLandUse(page) => EnvironmentPageKey {
                layer: PageLayer::HistoricalLandUse,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::HydrologyEvidence(page) => EnvironmentPageKey {
                layer: PageLayer::HydrologyEvidence,
                level: page.level,
                x: page.x,
                y: page.y,
            },
            Self::ModernLandCover(page) => EnvironmentPageKey {
                layer: PageLayer::ModernLandCover,
                level: page.level,
                x: page.x,
                y: page.y,
            },
        }
    }

    pub fn validate(&self) -> Result<(), EnvironmentError> {
        match self {
            Self::Elevation(page) => page.validate(),
            Self::Water(page) => page.validate(),
            Self::Vegetation(page) => page.validate(),
            Self::HistoricalLandUse(page) => page.validate(),
            Self::HydrologyEvidence(page) => page.validate(),
            Self::ModernLandCover(page) => page.validate(),
        }
    }

    pub fn content_hash(&self) -> Result<[u8; 32], EnvironmentError> {
        match self {
            Self::Elevation(page) => page.content_hash(),
            Self::Water(page) => page.content_hash(),
            Self::Vegetation(page) => page.content_hash(),
            Self::HistoricalLandUse(page) => page.content_hash(),
            Self::HydrologyEvidence(page) => page.content_hash(),
            Self::ModernLandCover(page) => page.content_hash(),
        }
    }
}

/// Errors crossing the pure map/provider boundary. The server adapter maps
/// filesystem, hash, and request cancellation failures into these stable
/// categories; simulation never sees OS or transport errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum EnvironmentPageError {
    #[error("requested environmental page is missing")]
    Missing,
    #[error("requested environmental page is corrupt")]
    Corrupt,
    #[error("environmental page source is unavailable")]
    Unavailable,
    #[error("environmental page request was cancelled")]
    Cancelled,
    #[error("environmental page request is invalid")]
    Invalid,
}

/// Pure typed boundary for lazy environment residency. Implementations may
/// use filesystem or GDAL capabilities outside aoe-map, but only one verified
/// page handle may cross this boundary per request.
pub trait EnvironmentPageProvider: Send + Sync + std::fmt::Debug {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError>;
}
