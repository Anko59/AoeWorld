//! Deterministic, environment-independent geographic map contracts.
mod biome;
mod biome_rules;
mod environment;
#[path = "environment/root.rs"]
mod page_root;
pub use page_root::{PageLayer, PageRootBuilder};
mod generator;
mod land_use;
mod navigation;
mod overlay;
mod package;
mod request;
mod route_hierarchy;
mod terrain;
mod water;
mod wire;

pub use biome_rules::Biome;
pub use environment::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, EnvironmentPage,
    EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider, FieldPyramid,
    HydrologyEvidenceIndex, HydrologyEvidenceMethod, HydrologyEvidencePage, HydrologyKind,
    HydrologyObservation, HydrologyWaterPolicy, MAX_ENVIRONMENT_SAMPLES_PER_AXIS,
    MAX_HYDROLOGY_EVIDENCE_SAMPLES_PER_AXIS, ModernLandCoverPage, PotentialBiomePage,
    PreparedEnvironment, PyramidLevel, WORLD_COVER_OBSERVATION_YEAR, WaterPage,
    ordered_biome_page_root, ordered_hydrology_page_root, ordered_modern_land_cover_page_root,
    ordered_page_root, ordered_water_page_root,
};
pub use land_use::{HistoricalLandUsePage, ordered_land_use_page_root};
pub use navigation::{
    MAX_ROUTE_PLANNER_NODES, MAX_ROUTE_SEGMENT_TILES, MovementOutcome, Path, RoutePlanner,
    RoutePlannerPoll, find_path, find_path_segment_with_overlay, find_path_with_overlay,
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
    Chunk, EdgePassability, GroundMaterial, MapChunkGenerator, ObjectKind, Provenance,
    ResourceKind, ResourceNode, SurfaceDiagonal, SurfaceKind, Tile, TileSurface, WaterKind,
};
pub use wire::{CompactChunk, CompactChunkError, MAX_DECODED_CHUNK_BYTES};

pub const CHUNK_TILES: i32 = 32;
/// Version 9 adds independently rooted modern hydrology and land-cover pages.
pub const MAP_SCHEMA_VERSION: u16 = 9;
/// Version 8 packages remain readable when they contain no typed evidence.
pub const LEGACY_MAP_SCHEMA_VERSION: u16 = 8;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
/// Increment when deterministic generation behavior changes. This version is
/// part of the canonical package identity but does not reseed geography.
pub const GENERATION_RECIPE_VERSION: u16 = 3;
/// Resource placement keeps its recipe identity separate so terrain recipe
/// changes do not reseed already published resource detail.
pub const RESOURCE_PLACEMENT_RECIPE_VERSION: u16 = 2;
