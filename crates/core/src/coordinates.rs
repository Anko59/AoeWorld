use crate::Seed;
use serde::{Deserialize, Serialize};

pub const FIXED_SUBUNITS_PER_TILE: i32 = 1_024;
pub const TILE_GROUND_RADIUS_SUBUNITS: i32 = FIXED_SUBUNITS_PER_TILE / 4;
pub const DEFAULT_WORLD_WIDTH_TILES: i32 = 16_384;
pub const DEFAULT_WORLD_HEIGHT_TILES: i32 = 16_384;
pub const MAX_WORLD_DIMENSION_TILES: i32 = 1_048_576;
pub const SPATIAL_CHUNK_TILES: i32 = 32;
pub const DEFAULT_SIMULATION_HZ: u32 = 20;
pub const DEFAULT_MOVE_SPEED_SUBUNITS_PER_TICK: i32 = 128;

fn default_move_speed_subunits_per_tick_denominator() -> u64 {
    1
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn checked(self) -> Result<Self, CoordinateError> {
        (self.x >= 0 && self.y >= 0)
            .then_some(self)
            .ok_or(CoordinateError::NegativeTile)
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct WorldPosition {
    pub x: i32,
    pub y: i32,
}

impl WorldPosition {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub fn from_tile(tile: TileCoord) -> Result<Self, CoordinateError> {
        let tile = tile.checked()?;
        let x = i64::from(tile.x)
            .checked_mul(i64::from(FIXED_SUBUNITS_PER_TILE))
            .ok_or(CoordinateError::Overflow)?;
        let y = i64::from(tile.y)
            .checked_mul(i64::from(FIXED_SUBUNITS_PER_TILE))
            .ok_or(CoordinateError::Overflow)?;
        Self::from_i64(x, y)
    }

    pub fn from_tile_center(tile: TileCoord) -> Result<Self, CoordinateError> {
        let origin = Self::from_tile(tile)?;
        Self::from_i64(
            i64::from(origin.x) + i64::from(FIXED_SUBUNITS_PER_TILE / 2),
            i64::from(origin.y) + i64::from(FIXED_SUBUNITS_PER_TILE / 2),
        )
    }

    pub fn from_i64(x: i64, y: i64) -> Result<Self, CoordinateError> {
        Ok(Self {
            x: i32::try_from(x).map_err(|_| CoordinateError::Overflow)?,
            y: i32::try_from(y).map_err(|_| CoordinateError::Overflow)?,
        })
    }

    pub fn tile_floor(self) -> TileCoord {
        TileCoord::new(
            self.x.div_euclid(FIXED_SUBUNITS_PER_TILE),
            self.y.div_euclid(FIXED_SUBUNITS_PER_TILE),
        )
    }

    pub fn as_tiles(self) -> [f64; 2] {
        [
            f64::from(self.x) / f64::from(FIXED_SUBUNITS_PER_TILE),
            f64::from(self.y) / f64::from(FIXED_SUBUNITS_PER_TILE),
        ]
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorldRect {
    pub min: WorldPosition,
    pub max: WorldPosition,
}

impl WorldRect {
    pub const fn new(min: WorldPosition, max: WorldPosition) -> Self {
        Self { min, max }
    }

    pub fn contains(self, position: WorldPosition) -> bool {
        position.x >= self.min.x
            && position.y >= self.min.y
            && position.x < self.max.x
            && position.y < self.max.y
    }

    pub fn from_tiles(rect: TileRect) -> Result<Self, CoordinateError> {
        let min = WorldPosition::from_tile(rect.min)?;
        let max = WorldPosition::from_tile(rect.max)?;
        Ok(Self { min, max })
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct TileRect {
    pub min: TileCoord,
    pub max: TileCoord,
}

impl TileRect {
    pub const fn new(min: TileCoord, max: TileCoord) -> Self {
        Self { min, max }
    }

    pub const fn from_xywh(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            min: TileCoord::new(x, y),
            max: TileCoord::new(x.saturating_add(width), y.saturating_add(height)),
        }
    }

    pub fn width(self) -> i32 {
        self.max.x.saturating_sub(self.min.x)
    }

    pub fn height(self) -> i32 {
        self.max.y.saturating_sub(self.min.y)
    }

    pub fn valid(self, world_width: i32, world_height: i32) -> bool {
        self.min.x >= 0
            && self.min.y >= 0
            && self.max.x > self.min.x
            && self.max.y > self.min.y
            && self.max.x <= world_width
            && self.max.y <= world_height
    }

    pub fn contains(self, tile: TileCoord) -> bool {
        tile.x >= self.min.x && tile.y >= self.min.y && tile.x < self.max.x && tile.y < self.max.y
    }

    pub fn clamp(self, world_width: i32, world_height: i32) -> Self {
        let width = self.width().clamp(1, world_width.max(1));
        let height = self.height().clamp(1, world_height.max(1));
        let x = self.min.x.clamp(0, world_width.saturating_sub(width));
        let y = self.min.y.clamp(0, world_height.saturating_sub(height));
        Self::from_xywh(x, y, width, height)
    }
}

#[derive(
    Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize,
)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl ChunkCoord {
    pub fn from_position(position: WorldPosition) -> Self {
        let chunk_subunits = SPATIAL_CHUNK_TILES * FIXED_SUBUNITS_PER_TILE;
        Self {
            x: position.x.div_euclid(chunk_subunits),
            y: position.y.div_euclid(chunk_subunits),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorldConfig {
    pub width_tiles: i32,
    pub height_tiles: i32,
    pub seed: Seed,
    pub tick_hz: u32,
    pub move_speed_subunits_per_tick: i32,
    #[serde(default = "default_move_speed_subunits_per_tick_denominator")]
    pub move_speed_subunits_per_tick_denominator: u64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            width_tiles: DEFAULT_WORLD_WIDTH_TILES,
            height_tiles: DEFAULT_WORLD_HEIGHT_TILES,
            seed: Seed(1),
            tick_hz: DEFAULT_SIMULATION_HZ,
            move_speed_subunits_per_tick: DEFAULT_MOVE_SPEED_SUBUNITS_PER_TICK,
            move_speed_subunits_per_tick_denominator: 1,
        }
    }
}

impl WorldConfig {
    pub fn new(width_tiles: i32, height_tiles: i32, seed: Seed) -> Result<Self, CoordinateError> {
        let config = Self {
            width_tiles,
            height_tiles,
            seed,
            ..Self::default()
        };
        config.validate()?;
        Ok(config)
    }

    pub fn validate(self) -> Result<(), CoordinateError> {
        if !(1..=MAX_WORLD_DIMENSION_TILES).contains(&self.width_tiles)
            || !(1..=MAX_WORLD_DIMENSION_TILES).contains(&self.height_tiles)
        {
            return Err(CoordinateError::InvalidDimensions);
        }
        if self.tick_hz == 0
            || self.move_speed_subunits_per_tick <= 0
            || self.move_speed_subunits_per_tick_denominator == 0
        {
            return Err(CoordinateError::InvalidSimulationConfig);
        }
        self.world_width_subunits()?;
        self.world_height_subunits()?;
        Ok(())
    }

    pub fn world_width_subunits(self) -> Result<i32, CoordinateError> {
        self.width_tiles
            .checked_mul(FIXED_SUBUNITS_PER_TILE)
            .ok_or(CoordinateError::Overflow)
    }

    pub fn world_height_subunits(self) -> Result<i32, CoordinateError> {
        self.height_tiles
            .checked_mul(FIXED_SUBUNITS_PER_TILE)
            .ok_or(CoordinateError::Overflow)
    }

    pub fn valid_ground_position(self, position: WorldPosition) -> bool {
        let Ok((width, height)) = self
            .world_width_subunits()
            .and_then(|width| self.world_height_subunits().map(|height| (width, height)))
        else {
            return false;
        };
        position.x >= TILE_GROUND_RADIUS_SUBUNITS
            && position.y >= TILE_GROUND_RADIUS_SUBUNITS
            && position.x < width - TILE_GROUND_RADIUS_SUBUNITS
            && position.y < height - TILE_GROUND_RADIUS_SUBUNITS
    }

    pub fn valid_map_position(self, position: WorldPosition) -> bool {
        let Ok((width, height)) = self
            .world_width_subunits()
            .and_then(|width| self.world_height_subunits().map(|height| (width, height)))
        else {
            return false;
        };
        position.x >= 0 && position.y >= 0 && position.x < width && position.y < height
    }

    pub fn clamp_ground_position(self, position: WorldPosition) -> WorldPosition {
        let width = self.world_width_subunits().unwrap_or(i32::MAX);
        let height = self.world_height_subunits().unwrap_or(i32::MAX);
        WorldPosition::new(
            position.x.clamp(
                TILE_GROUND_RADIUS_SUBUNITS,
                width - TILE_GROUND_RADIUS_SUBUNITS,
            ),
            position.y.clamp(
                TILE_GROUND_RADIUS_SUBUNITS,
                height - TILE_GROUND_RADIUS_SUBUNITS,
            ),
        )
    }

    pub fn snap_ground_position(self, position: WorldPosition) -> WorldPosition {
        let position = self.clamp_ground_position(position);
        let tile = position.tile_floor();
        let tile = TileCoord::new(
            tile.x.clamp(0, self.width_tiles.saturating_sub(1)),
            tile.y.clamp(0, self.height_tiles.saturating_sub(1)),
        );
        WorldPosition::from_tile_center(tile).unwrap_or(position)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CoordinateError {
    #[error("tile coordinates must be non-negative")]
    NegativeTile,
    #[error("coordinate arithmetic overflowed")]
    Overflow,
    #[error("world dimensions must be between 1 and {MAX_WORLD_DIMENSION_TILES} tiles")]
    InvalidDimensions,
    #[error("simulation tick rate and movement speed must be positive")]
    InvalidSimulationConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_point_conversion_is_checked_and_flooring_handles_negative_intermediates() {
        assert_eq!(
            WorldPosition::from_tile(TileCoord::new(16_384, 16_384))
                .unwrap()
                .x,
            16_777_216
        );
        assert!(WorldPosition::from_tile(TileCoord::new(i32::MAX, 0)).is_err());
        assert_eq!(
            WorldPosition::new(-1, -1).tile_floor(),
            TileCoord::new(-1, -1)
        );
        assert_eq!(
            WorldPosition::from_tile_center(TileCoord::new(4, 9)).unwrap(),
            WorldPosition::new(4_608, 9_728)
        );
    }

    #[test]
    fn world_config_rejects_invalid_dimensions_and_reserves_ground_radius() {
        assert!(WorldConfig::new(0, 1, Seed(1)).is_err());
        assert!(WorldConfig::new(MAX_WORLD_DIMENSION_TILES + 1, 1, Seed(1)).is_err());
        let config = WorldConfig::default();
        assert!(config.valid_ground_position(WorldPosition::new(256, 256)));
        assert!(!config.valid_ground_position(WorldPosition::new(0, 0)));
        assert!(config.valid_ground_position(WorldPosition::new(16_776_959, 16_776_959)));
        assert_eq!(
            config.snap_ground_position(WorldPosition::new(8_388_608, 8_388_608)),
            WorldPosition::new(8_389_120, 8_389_120)
        );
        assert!(config.valid_map_position(WorldPosition::new(0, 0)));
    }

    #[test]
    fn world_config_reads_legacy_json_and_rejects_zero_speed_denominator() {
        let legacy = r#"{
            "width_tiles": 64,
            "height_tiles": 64,
            "seed": 7,
            "tick_hz": 20,
            "move_speed_subunits_per_tick": 128
        }"#;
        let config: WorldConfig = serde_json::from_str(legacy).expect("legacy config");
        assert_eq!(config.move_speed_subunits_per_tick_denominator, 1);

        let mut invalid = config;
        invalid.move_speed_subunits_per_tick_denominator = 0;
        assert_eq!(
            invalid.validate(),
            Err(CoordinateError::InvalidSimulationConfig)
        );
    }

    #[test]
    fn tile_rect_is_half_open_and_clamps_without_allocating_world_area() {
        let rect = TileRect::from_xywh(-4, 2, 12, 8).clamp(100, 100);
        assert_eq!(rect.min, TileCoord::new(0, 2));
        assert_eq!(rect.max, TileCoord::new(12, 10));
        assert!(rect.contains(TileCoord::new(11, 9)));
        assert!(!rect.contains(TileCoord::new(12, 9)));
    }

    #[test]
    fn world_positions_convert_to_tiles_and_rectangles_remain_half_open() {
        let position = WorldPosition::new(1_024, 2_048);
        assert_eq!(position.as_tiles(), [1.0, 2.0]);
        let rect = WorldRect::from_tiles(TileRect::from_xywh(1, 2, 3, 4)).unwrap();
        assert!(rect.contains(position));
        assert!(!rect.contains(WorldPosition::new(4_096, 2_048)));
    }
}
