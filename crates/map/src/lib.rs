//! Deterministic, environment-independent geographic map contracts.
mod biome;
mod biome_rules;
mod environment;
#[path = "environment/root.rs"]
mod page_root;
pub use page_root::{PageLayer, PageRootBuilder};
mod generator;
#[path = "terrain/landscape/parcels.rs"]
pub mod historical_parcels;
mod land_use;
#[path = "terrain/landscape/ecology.rs"]
pub mod landscape_ecology;
#[path = "terrain/landscape/patches.rs"]
pub mod landscape_patches;
mod navigation;
mod overlay;
mod package;
mod request;
mod terrain;
mod water;
mod wire;

pub use biome_rules::Biome;
pub use environment::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, EnvironmentPage,
    EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid,
    GeographicWaterPatch, HYDROLOGY_WATER_MODEL_VERSION, HydrologyEvidenceIndex,
    HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind, HydrologyObservation,
    HydrologyWaterModelIndex, HydrologyWaterModelPage, HydrologyWaterPolicy,
    MAX_ENVIRONMENT_SAMPLES_PER_AXIS, MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS,
    MAX_WATER_CORRECTION_BYTES, MAX_WATER_CORRECTIONS, MODELLING_GRID_LIMIT, ModernLandCoverPage,
    PotentialBiomePage, PreparedEnvironment, PyramidLevel, WATER_CORRECTION_SCHEMA_VERSION,
    WATER_CORRECTION_TARGET_YEAR_CE, WORLD_COVER_OBSERVATION_YEAR, WaterCorrectionDocument,
    WaterCorrectionOperation, WaterCorrectionProjection, WaterCorrectionVertex, WaterFlowDirection,
    WaterModelProvenance, WaterPage, ordered_biome_page_root, ordered_hydrology_page_root,
    ordered_modern_land_cover_page_root, ordered_page_root, ordered_water_page_root,
};
pub use land_use::{
    HistoricalCoverage, HistoricalLandUseObservation, HistoricalLandUsePage,
    ordered_land_use_page_root,
};
pub use navigation::{
    MAX_ROUTE_PLANNER_NODES, MAX_ROUTE_PLANNER_WORK, MAX_ROUTE_SEGMENT_TILES, MAX_ROUTE_TILES,
    MovementOutcome, Path, RoutePlanner, RoutePlannerPoll, find_path,
    find_path_segment_with_overlay, find_path_with_overlay,
};
pub use overlay::{
    Depletion, MAX_RESOURCE_OVERLAY_CHANGES, RESOURCE_OVERLAY_SCHEMA_VERSION, ResourceChange,
    ResourceOverlay, ResourceOverlayError, ResourceOverlaySnapshot,
};
pub use package::{
    EnvironmentalProvenance, LayerProvenance, MapPackage, MapPackageError, ProjectionMetadata,
    SourceLock, VerticalDatum,
};
pub use request::{MapEstimate, MapRequest, MapRequestError, Ratio, ReconstructionProfile};
pub use terrain::{
    Chunk, DecorationFamily, EcologicalPalette, EdgePassability, GroundMaterial,
    LandscapeAppearance, LandscapeChunk, LandscapeDecoration, LandscapePoint, LandscapePolicy,
    LandscapeResource, LandscapeSample, LandscapeTile, MapChunkGenerator, NativeExposure,
    NativeHeightBand, ObjectKind, Provenance, ResourceKind, ResourceNode, ResourceVisualFamily,
    SurfaceDiagonal, SurfaceKind, Tile, TileSurface, WaterKind,
};
pub use wire::{CHUNK_FORMAT_VERSION, CompactChunk, CompactChunkError, MAX_DECODED_CHUNK_BYTES};

pub const CHUNK_TILES: i32 = 32;
/// The only package schema. Before v1.0 formats change in place: there is no
/// reader for earlier schemas and local packages are regenerated instead.
pub const MAP_SCHEMA_VERSION: u16 = 1;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
/// The only deterministic generation recipe: the composed landscape. It is part
/// of canonical package identity; packages naming any other value are rejected.
pub const GENERATION_RECIPE_VERSION: u16 = 1;
