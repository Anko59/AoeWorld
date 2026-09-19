use crate::{
    CAVALRY_METERS_PER_SECOND, GAME_TILE_METERS, REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR,
    REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR,
};
use serde::{Deserialize, Serialize};

pub const MIN_SIDE_METERS: u64 = 250;
pub const MAX_SIDE_METERS: u64 = 10_000_000_000;
pub const MIN_TILES_PER_SIDE: u64 = 64;
pub const MAX_TILES_PER_SIDE: u64 = 262_144;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconstructionProfile {
    #[default]
    Circa600V1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailProfile {
    #[default]
    StandardV1,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Ratio {
    pub numerator: u32,
    pub denominator: u32,
}

impl Ratio {
    pub fn new(numerator: u32, denominator: u32) -> Result<Self, MapRequestError> {
        if numerator == 0 || denominator == 0 {
            return Err(MapRequestError::InvalidCompression);
        }
        let divisor = gcd(numerator, denominator);
        let value = Self {
            numerator: numerator / divisor,
            denominator: denominator / divisor,
        };
        if !(u64::from(value.denominator)..=10_000 * u64::from(value.denominator))
            .contains(&u64::from(value.numerator))
        {
            return Err(MapRequestError::InvalidCompression);
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapRequest {
    pub schema_version: u16,
    pub center_latitude_e7: i32,
    pub center_longitude_e7: i32,
    pub requested_side_meters: u64,
    pub compression: Ratio,
    pub year_ce: u16,
    pub seed: u64,
    #[serde(default)]
    pub reconstruction_profile: ReconstructionProfile,
    #[serde(default)]
    pub detail_profile: DetailProfile,
}

impl Default for MapRequest {
    fn default() -> Self {
        Self {
            schema_version: 1,
            center_latitude_e7: 488_500_000,
            center_longitude_e7: 23_500_000,
            requested_side_meters: 30_000,
            compression: Ratio::new(30, 1).unwrap_or(Ratio {
                numerator: 30,
                denominator: 1,
            }),
            year_ce: 600,
            seed: 1,
            reconstruction_profile: ReconstructionProfile::Circa600V1,
            detail_profile: DetailProfile::StandardV1,
        }
    }
}

impl MapRequest {
    pub fn normalized(mut self) -> Result<Self, MapRequestError> {
        if self.schema_version != 1 {
            return Err(MapRequestError::UnsupportedSchema);
        }
        if !(-900_000_000..=900_000_000).contains(&self.center_latitude_e7) {
            return Err(MapRequestError::InvalidLatitude);
        }
        self.center_longitude_e7 = normalize_longitude(self.center_longitude_e7);
        if !(MIN_SIDE_METERS..=MAX_SIDE_METERS).contains(&self.requested_side_meters) {
            return Err(MapRequestError::InvalidSide);
        }
        self.compression = Ratio::new(self.compression.numerator, self.compression.denominator)?;
        if self.year_ce != 600 {
            return Err(MapRequestError::UnsupportedYear);
        }
        self.estimate().map(|_| self)
    }

    pub fn estimate(self) -> Result<MapEstimate, MapRequestError> {
        let request = self.normalized_without_estimate()?;
        let numerator = u128::from(request.requested_side_meters)
            .checked_mul(u128::from(request.compression.denominator))
            .ok_or(MapRequestError::Overflow)?;
        let denominator = u128::from(GAME_TILE_METERS)
            .checked_mul(u128::from(request.compression.numerator))
            .ok_or(MapRequestError::Overflow)?;
        let tiles = numerator
            .checked_add(denominator - 1)
            .ok_or(MapRequestError::Overflow)?
            / denominator;
        if !(MIN_TILES_PER_SIDE..=MAX_TILES_PER_SIDE).contains(&(tiles as u64)) {
            let tiles = tiles.min(u128::from(u64::MAX)) as u64;
            let Some(suggestion) = request.compatible_compression() else {
                return Err(MapRequestError::NoCompatibleCompression { tiles });
            };
            return Err(MapRequestError::TileEnvelope {
                tiles,
                suggested_numerator: suggestion.numerator,
                suggested_denominator: suggestion.denominator,
            });
        }
        let tiles = u64::try_from(tiles).map_err(|_| MapRequestError::Overflow)?;
        let effective_side_meters = u128::from(tiles)
            .checked_mul(denominator)
            .ok_or(MapRequestError::Overflow)?
            / u128::from(request.compression.denominator);
        let game_side_meters = tiles
            .checked_mul(u64::from(GAME_TILE_METERS))
            .ok_or(MapRequestError::Overflow)?;
        let walking_seconds = u128::from(game_side_meters)
            .checked_mul(u128::from(REFERENCE_WALK_METERS_PER_SECOND_DENOMINATOR))
            .ok_or(MapRequestError::Overflow)?
            / u128::from(REFERENCE_WALK_METERS_PER_SECOND_NUMERATOR);
        Ok(MapEstimate {
            compression_numerator: request.compression.numerator,
            compression_denominator: request.compression.denominator,
            effective_side_meters: u64::try_from(effective_side_meters)
                .map_err(|_| MapRequestError::Overflow)?,
            tiles_per_side: tiles,
            game_side_meters,
            geographic_millimeters_per_tile: u64::from(GAME_TILE_METERS)
                .checked_mul(u64::from(request.compression.numerator))
                .ok_or(MapRequestError::Overflow)?
                .checked_mul(1_000)
                .ok_or(MapRequestError::Overflow)?
                / u64::from(request.compression.denominator),
            walking_crossing_seconds: u64::try_from(walking_seconds)
                .map_err(|_| MapRequestError::Overflow)?,
            cavalry_crossing_seconds: game_side_meters / u64::from(CAVALRY_METERS_PER_SECOND),
        })
    }

    pub fn compatible_compression(self) -> Option<Ratio> {
        let request = self.normalized_without_estimate().ok()?;
        let numerator = u128::from(request.requested_side_meters)
            .checked_mul(u128::from(request.compression.denominator))?;
        let denominator =
            u128::from(GAME_TILE_METERS).checked_mul(u128::from(request.compression.numerator))?;
        let tiles = numerator.checked_add(denominator - 1)? / denominator;
        if (MIN_TILES_PER_SIDE..=MAX_TILES_PER_SIDE).contains(&(tiles as u64)) {
            return Some(request.compression);
        }
        let target = if tiles > u128::from(MAX_TILES_PER_SIDE) {
            MAX_TILES_PER_SIDE
        } else {
            MIN_TILES_PER_SIDE
        };
        let denominator = u64::from(GAME_TILE_METERS).checked_mul(target)?;
        let raw = if target == MAX_TILES_PER_SIDE {
            request.requested_side_meters.div_ceil(denominator)
        } else {
            request.requested_side_meters / denominator
        };
        let raw = raw.clamp(1, 10_000);
        let ratio = Ratio::new(u32::try_from(raw).ok()?, 1).ok()?;
        request
            .with_compression(ratio)
            .estimate()
            .ok()
            .map(|_| ratio)
    }

    fn with_compression(mut self, compression: Ratio) -> Self {
        self.compression = compression;
        self
    }

    fn normalized_without_estimate(mut self) -> Result<Self, MapRequestError> {
        if self.schema_version != 1 {
            return Err(MapRequestError::UnsupportedSchema);
        }
        if !(-900_000_000..=900_000_000).contains(&self.center_latitude_e7) {
            return Err(MapRequestError::InvalidLatitude);
        }
        self.center_longitude_e7 = normalize_longitude(self.center_longitude_e7);
        if !(MIN_SIDE_METERS..=MAX_SIDE_METERS).contains(&self.requested_side_meters) {
            return Err(MapRequestError::InvalidSide);
        }
        self.compression = Ratio::new(self.compression.numerator, self.compression.denominator)?;
        if self.year_ce != 600 {
            return Err(MapRequestError::UnsupportedYear);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MapEstimate {
    pub compression_numerator: u32,
    pub compression_denominator: u32,
    pub effective_side_meters: u64,
    pub tiles_per_side: u64,
    pub game_side_meters: u64,
    pub geographic_millimeters_per_tile: u64,
    pub walking_crossing_seconds: u64,
    pub cavalry_crossing_seconds: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MapRequestError {
    #[error("map request schema version is unsupported")]
    UnsupportedSchema,
    #[error("latitude must be within WGS84 bounds")]
    InvalidLatitude,
    #[error("side length must be 250 m through 10,000 km")]
    InvalidSide,
    #[error("compression must be within 1:1 through 10,000:1")]
    InvalidCompression,
    #[error("only 600 CE is supported")]
    UnsupportedYear,
    #[error(
        "request produces {tiles} tiles per side; try compression {suggested_numerator}:{suggested_denominator} to stay within 64 through 262144"
    )]
    TileEnvelope {
        tiles: u64,
        suggested_numerator: u32,
        suggested_denominator: u32,
    },
    #[error(
        "request produces {tiles} tiles per side and no compression within 1:1 through 10000:1 can fit the supported envelope"
    )]
    NoCompatibleCompression { tiles: u64 },
    #[error("map-size arithmetic overflowed")]
    Overflow,
}

fn normalize_longitude(value: i32) -> i32 {
    let full = 3_600_000_000_i64;
    let wrapped = (i64::from(value) + 1_800_000_000).rem_euclid(full) - 1_800_000_000;
    i32::try_from(wrapped).unwrap_or(-1_800_000_000)
}

fn gcd(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_ratios_and_longitudes_have_the_same_normalized_form() {
        let first = MapRequest {
            center_longitude_e7: 1_795_000_000,
            ..MapRequest::default()
        }
        .normalized()
        .expect("valid request");
        let second = MapRequest {
            center_longitude_e7: -1_805_000_000,
            compression: Ratio::new(300, 10).expect("ratio"),
            ..MapRequest::default()
        }
        .normalized()
        .expect("valid request");
        assert_eq!(first.compression, second.compression);
        assert_eq!(first.center_longitude_e7, second.center_longitude_e7);
    }

    #[test]
    fn estimate_rounds_the_geographic_extent_outward_to_tiles() {
        let estimate = MapRequest::default().estimate().expect("estimate");
        assert_eq!(estimate.tiles_per_side, 500);
        assert_eq!(estimate.compression_numerator, 30);
        assert_eq!(estimate.compression_denominator, 1);
        assert_eq!(estimate.effective_side_meters, 30_000);
        assert_eq!(estimate.walking_crossing_seconds, 857);
        assert_eq!(estimate.cavalry_crossing_seconds, 333);
    }

    #[test]
    fn rejects_outside_the_tile_envelope_instead_of_changing_ratio() {
        let request = MapRequest {
            requested_side_meters: 250,
            compression: Ratio::new(10_000, 1).expect("ratio"),
            ..MapRequest::default()
        };
        assert!(matches!(
            request.estimate(),
            Err(MapRequestError::TileEnvelope {
                suggested_numerator: 1,
                suggested_denominator: 1,
                ..
            })
        ));
        assert_eq!(
            request.compatible_compression(),
            Some(Ratio::new(1, 1).expect("ratio"))
        );
    }
}
