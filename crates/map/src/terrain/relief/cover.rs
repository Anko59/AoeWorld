//! Snow and bare rock from an estimated warmest-month temperature. Packages
//! carry no temperature or precipitation field, so the estimate is a lapse
//! rate proxy from latitude and elevation, kept in one function that a real
//! climate field can replace. Both lines are perturbed by continuous
//! world-anchored noise, so their edges are irregular and never follow a
//! chunk, a source sample or a vegetation cell.
use super::{relief_key, smooth};
use crate::{Biome, MapChunkGenerator};
use aoe_core::TileCoord;

/// Latitude assumed for a map without a geographic request.
const DEFAULT_LATITUDE_E7: i32 = 460_000_000;
/// Summer snow and ice persist where the warmest month is at most 4 °C.
const SNOW_MILLICELSIUS: i64 = 4_000;
/// Bare rock replaces turf where the warmest month is at most 7.5 °C.
const ROCK_MILLICELSIUS: i64 = 7_500;
/// Lapse rate: 6.5 °C per kilometre, in millidegrees per metre.
const LAPSE_PER_METER_TIMES_TEN: i64 = 65;
/// Tundra vegetation is a second cold signal: its snow line is 150 m lower.
const TUNDRA_LOWERING_CENTIMETERS: i64 = 15_000;
const HIGHEST_SNOW_LINE: i64 = 520_000;
const MILLIMETERS_PER_DEGREE: i64 = 111_195_000;

/// Estimated mean temperature of the warmest month in millidegrees Celsius:
/// 22 °C at sea level at 46°, 0.65 °C colder per degree toward the pole and
/// 6.5 °C colder per kilometre of height. The only climate input of the map.
pub(crate) fn warmest_month_millicelsius(latitude_e7: i32, height_centimeters: i32) -> i64 {
    let poleward_e7 = i64::from(latitude_e7).abs() - i64::from(DEFAULT_LATITUDE_E7);
    22_000
        - poleward_e7 * 65 / 1_000_000
        - i64::from(height_centimeters) * LAPSE_PER_METER_TIMES_TEN / 1_000
}

/// Height in centimetres at which the estimate falls to `millicelsius`.
fn isotherm_centimeters(latitude_e7: i32, millicelsius: i64) -> i64 {
    (warmest_month_millicelsius(latitude_e7, 0) - millicelsius) * 1_000 / LAPSE_PER_METER_TIMES_TEN
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cover {
    Snow,
    Rock,
}

impl MapChunkGenerator {
    /// Binds the geographic latitude of the map centre and the ground size of
    /// one tile; north is toward smaller `y`.
    pub fn with_latitude(mut self, center_e7: i32, millimeters_per_tile: u64) -> Self {
        self.latitude = Some((center_e7, millimeters_per_tile));
        self.point_memo = Default::default();
        self
    }

    pub fn latitude_e7(&self, tile: TileCoord) -> i32 {
        let Some((center, millimeters_per_tile)) = self.latitude else {
            return DEFAULT_LATITUDE_E7;
        };
        let south = i64::from(tile.y) - i64::from(self.width_tiles.max(1) - 1) / 2;
        let millimeters = south.saturating_mul(i64::try_from(millimeters_per_tile).unwrap_or(0));
        let offset = (i128::from(millimeters) * 10_000_000 / i128::from(MILLIMETERS_PER_DEGREE))
            .clamp(-1_800_000_000, 1_800_000_000) as i64;
        (i64::from(center) - offset).clamp(-900_000_000, 900_000_000) as i32
    }

    /// Height of the snow line in centimetres at a tile's latitude: about
    /// 2,770 m at 46°, 100 m lower per degree toward the pole, never above
    /// 5,200 m, and below sea level from about 74°.
    pub fn snow_line_centimeters(&self, tile: TileCoord) -> i32 {
        isotherm_centimeters(self.latitude_e7(tile), SNOW_MILLICELSIUS)
            .clamp(-HIGHEST_SNOW_LINE, HIGHEST_SNOW_LINE) as i32
    }

    /// Snow above the noisy snow line, bare rock in the band below it.
    pub(crate) fn ground_cover(
        &self,
        tile: TileCoord,
        height_centimeters: i32,
        biome: Biome,
    ) -> Option<Cover> {
        let lowering = if biome == Biome::Tundra {
            TUNDRA_LOWERING_CENTIMETERS
        } else {
            0
        };
        let snow = i64::from(self.snow_line_centimeters(tile)) - lowering;
        let rock =
            snow - (ROCK_MILLICELSIUS - SNOW_MILLICELSIUS) * 1_000 / LAPSE_PER_METER_TIMES_TEN;
        let height = i64::from(height_centimeters);
        // The noise moves a line by at most 384 m; skip it far below both.
        if height < rock - 40_000 {
            return None;
        }
        if height + wander(self, tile, 0x736e_6f77) > snow {
            return Some(Cover::Snow);
        }
        (height + wander(self, tile, 0x726f_636b) > rock).then_some(Cover::Rock)
    }
}

/// Three octaves (40, 13 and 5 tiles), centred on zero, in centimetres.
fn wander(generator: &MapChunkGenerator, tile: TileCoord, domain: u64) -> i64 {
    let (x, y) = (i64::from(tile.x), i64::from(tile.y));
    let octave = |index: u64, period: i64| {
        smooth(relief_key(generator, domain ^ (index << 40)), x, y, period) - 128
    };
    (octave(1, 40) * 3 + octave(2, 13) * 2 + octave(3, 5)) * 50
}
