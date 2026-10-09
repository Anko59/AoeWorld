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
pub use request::{
    DetailProfile, MapEstimate, MapRequest, MapRequestError, Ratio, ReconstructionProfile,
};
pub use terrain::{
    Chunk, DecorationFamily, EcologicalPalette, EdgePassability, GroundMaterial,
    LandscapeAppearance, LandscapeChunk, LandscapeDecoration, LandscapePoint, LandscapePolicy,
    LandscapeResource, LandscapeSample, LandscapeTile, MapChunkGenerator, NativeExposure,
    NativeHeightBand, ObjectKind, Provenance, ResourceKind, ResourceNode, ResourceVisualFamily,
    SurfaceDiagonal, SurfaceKind, Tile, TileSurface, WaterKind,
};
pub use wire::{CompactChunk, CompactChunkError, LandscapeChunkError, MAX_DECODED_CHUNK_BYTES};

pub const CHUNK_TILES: i32 = 32;
/// StandardV1 default schema: independently rooted water/land-cover evidence.
/// Keep this creator default pinned; opt-in landscape uses its explicit version.
pub const MAP_SCHEMA_VERSION: u16 = 9;
pub const LANDSCAPE_MAP_SCHEMA_VERSION: u16 = 10;
pub const LANDSCAPE_GENERATION_RECIPE_VERSION: u16 = 9;
/// Version 8 packages remain readable when they contain no typed evidence.
pub const LEGACY_MAP_SCHEMA_VERSION: u16 = 8;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
/// Increment when deterministic generation behavior changes. This version is
/// part of the canonical package identity but does not reseed geography.
pub const GENERATION_RECIPE_VERSION: u16 = 8;
/// Published recipe 8 behavior is pinned independently of the latest recipe.
/// Use this constant, not the latest-version alias, for legacy forest rules.
pub const CONNECTED_FOREST_GENERATION_RECIPE_VERSION: u16 = 8;
/// Recipe 8 connects the central glade and the local opening graph; natural
/// water and cliff constraints still determine actual traversal.
pub const WATER_MODEL_GENERATION_RECIPE_VERSION: u16 = 8;
/// Published recipe 7 retains its original canopy, openings and optional trails.
pub const PRIOR_FOREST_GENERATION_RECIPE_VERSION: u16 = 7;
/// Recipe 6 remains readable for packages generated before recipe 7.
pub const PRIOR_WATER_MODEL_GENERATION_RECIPE_VERSION: u16 = 6;
/// Recipe 5 remains readable for model-free overview packages.
pub const PRIOR_OVERVIEW_GENERATION_RECIPE_VERSION: u16 = 5;
/// Recipe 4 retains bilinear elevation and the published recipe-2 resources.
pub const PRIOR_GENERATION_RECIPE_VERSION: u16 = 4;
pub const LEGACY_GENERATION_RECIPE_VERSION: u16 = 3;
/// Resource placement keeps its recipe identity separate so terrain recipe
/// changes do not reseed already published resource detail.
pub const RESOURCE_PLACEMENT_RECIPE_VERSION: u16 = 2;
