//! Deterministic, environment-independent geographic map contracts.
mod biome;
mod biome_rules;
mod environment;
mod generator;
mod land_use;
mod navigation;
mod overlay;
mod package;
mod request;
mod terrain;
mod water;
mod wire;

pub use biome_rules::Biome;
pub use environment::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, FieldPyramid,
    MAX_ENVIRONMENT_SAMPLES_PER_AXIS, PotentialBiomePage, PreparedEnvironment, PyramidLevel,
    WaterPage, ordered_biome_page_root, ordered_page_root, ordered_water_page_root,
};
pub use land_use::{HistoricalLandUsePage, ordered_land_use_page_root};
pub use navigation::{
    MAX_ROUTE_SEGMENT_TILES, MovementOutcome, Path, find_path, find_path_segment_with_overlay,
    find_path_with_overlay,
};
pub use overlay::{Depletion, ResourceOverlay, ResourceOverlayError};
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
/// Version 8 adds separately persisted inland-lake coverage.
pub const MAP_SCHEMA_VERSION: u16 = 8;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
