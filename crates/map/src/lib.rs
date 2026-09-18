//! Deterministic, environment-independent geographic map contracts.
mod navigation;
mod overlay;
mod package;
mod request;
mod terrain;

pub use navigation::{MovementOutcome, Path, find_path, find_path_with_overlay};
pub use overlay::{Depletion, ResourceOverlay, ResourceOverlayError};
pub use package::{MapPackage, SourceLock};
pub use request::{MapEstimate, MapRequest, MapRequestError, Ratio};
pub use terrain::{
    Biome, Chunk, GroundMaterial, MapChunkGenerator, ObjectKind, Provenance, ResourceKind,
    ResourceNode, Tile, WaterKind,
};

pub const CHUNK_TILES: i32 = 32;
pub const MAP_SCHEMA_VERSION: u16 = 1;
pub const GAME_TILE_METERS: u32 = 2;
pub const ELEVATION_LEVEL_CENTIMETERS: i32 = 100;
pub const REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR: u32 = 7;
pub const REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR: u32 = 6;
pub const CAVALRY_METERS_PER_SECOND: u32 = 3;
