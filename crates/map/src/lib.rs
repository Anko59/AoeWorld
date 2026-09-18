//! Deterministic, environment-independent geographic map contracts.
mod biome;
mod biome_rules;
mod environment;
mod generator;
mod navigation;
mod overlay;
mod package;
mod request;
mod terrain;
mod water;

pub use biome_rules::Biome;
pub use environment::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentError, FieldPyramid,
    MAX_ENVIRONMENT_SAMPLES_PER_AXIS, PotentialBiomePage, PreparedEnvironment, PyramidLevel,
    WaterPage, ordered_biome_page_root, ordered_page_root, ordered_water_page_root,
};
pub use navigation::{MovementOutcome, Path, find_path, find_path_with_overlay};
pub use overlay::{Depletion, ResourceOverlay, ResourceOverlayError};
pub use package::{
    EnvironmentalProvenance, LayerProvenance, MapPackage, MapPackageError, ProjectionMetadata,
    SourceLock, VerticalDatum,
};
pub use request::{
    DetailProfile, MapEstimate, MapRequest, MapRequestError, Ratio, ReconstructionProfile,
};
pub use terrain::{
    Chunk, GroundMaterial, MapChunkGenerator, ObjectKind, Provenance, ResourceKind, ResourceNode,
    Tile, WaterKind,
};

pub const CHUNK_TILES: i32 = 32;
/// Version 4 adds immutable potential-biome page roots, per-tile vegetation
/// provenance, and biome-specific correlated tree-generation rules.
pub const MAP_SCHEMA_VERSION: u16 = 4;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
