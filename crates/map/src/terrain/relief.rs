//! Mountain relief derived from the prepared elevation field: cliff lines on
//! steep slopes. A cliff is an Age of Empires II style piece: a three-by-three
//! tile cell of impassable ground with a level ledge. Pieces sit
//! where a contour level crosses a steep cell, so a line of pieces follows the
//! real contour. Lines are broken by passes and never close around a summit.
//! Relief and passes use geography only. The reserved start cells are the
//! sole exception: the forest connector follows the procedural forest seed.
use super::{MapChunkGenerator, landscape};
use crate::{EnvironmentPageError, Ratio};
use aoe_core::TileCoord;

mod cover;
pub(crate) use cover::Cover;
mod rocks;
pub(crate) use rocks::{MAX_ROCK_PER_THOUSAND, SCREE_REACH};

/// Side of one connected cliff ledge in tiles.
pub(crate) const CLIFF_CELL_TILES: i32 = 3;
/// Contour interval in game centimetres: eight game metres, four tile widths.
const CONTOUR_GAME_CENTIMETERS: i64 = 800;
/// Rise across one cell (600 game cm) from which a slope is steep: 8%.
const STEEP_CELL_RISE: i64 = 48;
/// Share of a contour, in 256ths, that stays open as passes.
const PASS_SHARE: i64 = 116;
/// Wavelength of the pass pattern in cells (12 tiles).
const PASS_PERIOD_CELLS: i64 = 4;

/// Downhill direction of a cliff as seen on screen. Only slopes that face the
/// camera carry cliffs; slopes falling away to the northwest, north or
/// northeast, whose face would be hidden, stay open. Because of that open
/// sector a contour that closes around a summit or a hollow is never walled
/// all the way round.
pub(crate) const FACING_SOUTH: u8 = 0;
pub(crate) const FACING_NORTH: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CliffPiece {
    /// Octant of the downhill direction: 0 south (toward the camera), then
    /// clockwise on screen: southwest, west, northwest, north, northeast,
    /// east, southeast.
    pub facing: u8,
    pub variant: u8,
    /// Uppermost crossed contour. All nine tiles support this ledge, and
    /// cells along the same contour cannot introduce internal height steps.
    pub top_level: i16,
    /// The centre tile of the cell, which carries the anchor record.
    pub anchor: bool,
}

pub(super) fn game_centimeters(height: i32, compression: Ratio) -> i64 {
    i64::from(height) * i64::from(compression.denominator) / i64::from(compression.numerator.max(1))
}

/// Whole-cell cliff detection from the bound corner-height field. Water is
/// checked by both source adapters before promoting terrain or emitting art.
pub(super) fn cliff_at<E>(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    compression: Ratio,
    corner_height: &dyn Fn(i32, i32) -> Result<i32, E>,
) -> Result<Option<CliffPiece>, E> {
    let Some(piece) = candidate_at(generator, tile, compression, corner_height)? else {
        return Ok(None);
    };
    if cell_tiles(tile).any(|position| {
        landscape::starting_glade(generator, position)
            || landscape::starting_connector_contains(generator, position)
    }) {
        return Ok(None);
    }
    // A contour fragment without another piece is a square mesa, not a ridge.
    // Inspect the unpruned candidates so joined pairs and ends stay symmetric;
    // this does not recurse or depend on chunk generation order.
    for dy in -1..=1 {
        for dx in -1..=1 {
            if (dx != 0 || dy != 0)
                && candidate_at(
                    generator,
                    TileCoord::new(
                        tile.x + dx * CLIFF_CELL_TILES,
                        tile.y + dy * CLIFF_CELL_TILES,
                    ),
                    compression,
                    corner_height,
                )?
                .is_some()
            {
                return Ok(Some(piece));
            }
        }
    }
    Ok(None)
}

fn candidate_at<E>(
    generator: &MapChunkGenerator,
    tile: TileCoord,
    compression: Ratio,
    corner_height: &dyn Fn(i32, i32) -> Result<i32, E>,
) -> Result<Option<CliffPiece>, E> {
    if tile.x < 0
        || tile.y < 0
        || tile.x >= generator.width_tiles
        || tile.y >= generator.width_tiles
    {
        return Ok(None);
    }
    let cell = (
        tile.x.div_euclid(CLIFF_CELL_TILES),
        tile.y.div_euclid(CLIFF_CELL_TILES),
    );
    let (west, north) = (cell.0 * CLIFF_CELL_TILES, cell.1 * CLIFF_CELL_TILES);
    let (east, south) = (west + CLIFF_CELL_TILES, north + CLIFF_CELL_TILES);
    if east > generator.width_tiles || south > generator.width_tiles {
        return Ok(None);
    }
    let mut heights = [0_i64; 4];
    for (slot, (x, y)) in [(west, north), (east, north), (east, south), (west, south)]
        .into_iter()
        .enumerate()
    {
        heights[slot] = game_centimeters(corner_height(x, y)?, compression);
    }
    let rise_x = (heights[1] + heights[2] - heights[0] - heights[3]) / 2;
    let rise_y = (heights[3] + heights[2] - heights[0] - heights[1]) / 2;
    let (gradient_x, gradient_y) = (i128::from(rise_x), i128::from(rise_y));
    if gradient_x * gradient_x + gradient_y * gradient_y < i128::from(STEEP_CELL_RISE).pow(2) {
        return Ok(None);
    }
    let lowest = heights.into_iter().min().unwrap_or(0);
    let highest = heights.into_iter().max().unwrap_or(0);
    if lowest.div_euclid(CONTOUR_GAME_CENTIMETERS) == highest.div_euclid(CONTOUR_GAME_CENTIMETERS) {
        return Ok(None);
    }
    let facing = facing(-rise_x, -rise_y);
    if (3..=5).contains(&facing) {
        return Ok(None);
    }
    let key = relief_key(generator, 0x636c_6966_6673);
    if smooth(key, i64::from(cell.0), i64::from(cell.1), PASS_PERIOD_CELLS) < PASS_SHARE {
        return Ok(None);
    }
    let variant = (landscape::mix64(key ^ pair(i64::from(cell.0), i64::from(cell.1))) >> 24) as u8;
    Ok(Some(CliffPiece {
        facing,
        variant,
        top_level: (highest.div_euclid(CONTOUR_GAME_CENTIMETERS) * (CONTOUR_GAME_CENTIMETERS / 100))
            .clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16,
        anchor: tile.x.rem_euclid(CLIFF_CELL_TILES) == 1
            && tile.y.rem_euclid(CLIFF_CELL_TILES) == 1,
    }))
}

/// Screen octant of a downhill vector in tile space. On screen +x runs down
/// and to the right, +y down and to the left.
fn facing(x: i64, y: i64) -> u8 {
    let (ax, ay) = (x.abs(), y.abs());
    let diagonal = 5 * ax.min(ay) > 2 * ax.max(ay);
    match (diagonal, x > 0, y > 0, ax >= ay) {
        (true, true, true, _) => FACING_SOUTH,
        (true, false, true, _) => 2,
        (true, false, false, _) => FACING_NORTH,
        (true, true, false, _) => 6,
        (false, _, true, false) => 1,
        (false, false, _, true) => 3,
        (false, _, false, false) => 5,
        (false, true, _, true) => 7,
    }
}

pub(super) fn relief_key(generator: &MapChunkGenerator, domain: u64) -> u64 {
    let key = &generator.geography_key;
    u64::from_le_bytes([
        key[0], key[1], key[2], key[3], key[4], key[5], key[6], key[7],
    ]) ^ domain
}

fn pair(x: i64, y: i64) -> u64 {
    (x as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (y as u64).wrapping_mul(0xc2b2_ae3d_27d4_eb4f)
}

/// Continuous value noise in 0..=255: seeded lattice values every `period`
/// units, joined bilinearly. World-anchored, so it has no chunk or sample edge.
pub(super) fn smooth(key: u64, x: i64, y: i64, period: i64) -> i64 {
    let period = period.max(1);
    let (cell_x, cell_y) = (x.div_euclid(period), y.div_euclid(period));
    let (fx, fy) = (x.rem_euclid(period), y.rem_euclid(period));
    let value = |dx: i64, dy: i64| {
        (landscape::mix64(key ^ pair(cell_x + dx, cell_y + dy)) >> 32 & 0xff) as i64
    };
    let top = value(0, 0) * (period - fx) + value(1, 0) * fx;
    let bottom = value(0, 1) * (period - fx) + value(1, 1) * fx;
    (top * (period - fy) + bottom * fy) / (period * period)
}

impl MapChunkGenerator {
    /// Cliff piece of a tile from the bound elevation field, for the composer:
    /// it records the piece on the anchor tile.
    pub(crate) fn cliff_piece_with_cancel(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Option<CliffPiece>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        if self.provider.is_some() {
            return super::provider::cliff_piece(self, tile, cancelled);
        }
        let Some(elevation) = self.elevation.as_ref() else {
            return Ok(None);
        };
        let piece = cliff_at(self, tile, elevation.compression, &|x, y| {
            elevation
                .corner(x, y, self.width_tiles)
                .ok_or(EnvironmentPageError::Invalid)
        })?;
        if piece.is_some() && !self.cliff_cell_is_available(tile, cancelled)? {
            return Ok(None);
        }
        Ok(piece)
    }
}

/// Shared footprint for reservations and physical water, including cell edges.
fn cell_tiles(tile: TileCoord) -> impl Iterator<Item = TileCoord> {
    let west = tile.x.div_euclid(CLIFF_CELL_TILES) * CLIFF_CELL_TILES;
    let north = tile.y.div_euclid(CLIFF_CELL_TILES) * CLIFF_CELL_TILES;
    (0..CLIFF_CELL_TILES).flat_map(move |dy| {
        (0..CLIFF_CELL_TILES).map(move |dx| TileCoord::new(west + dx, north + dy))
    })
}

impl MapChunkGenerator {
    /// Shared dense/provider eligibility: read physical water without recursing
    /// through base terrain. Exclude partial cells, water and the widest river
    /// bank halo, so neither promotion nor art can replace a bank.
    pub(super) fn cliff_cell_is_available(
        &self,
        tile: TileCoord,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<bool, EnvironmentPageError> {
        for position in cell_tiles(tile) {
            if self.base_water_with_cancel(position, cancelled)? != Some(super::WaterKind::None) {
                return Ok(false);
            }
        }
        let west = tile.x.div_euclid(CLIFF_CELL_TILES) * CLIFF_CELL_TILES;
        let north = tile.y.div_euclid(CLIFF_CELL_TILES) * CLIFF_CELL_TILES;
        let center = TileCoord::new(west + 1, north + 1);
        if !self.water_possible_near(center, 4, cancelled)? {
            return Ok(true);
        }
        // At most forty halo positions; the core was already checked above.
        for y in north - 2..=north + 4 {
            for x in west - 2..=west + 4 {
                let dx = (west - x).max(0).max(x - west - 2);
                let dy = (north - y).max(0).max(y - north - 2);
                let distance = dx * dx + dy * dy;
                if distance > 0
                    && distance <= super::shoreline::RIVER_BANK_MAX_DISTANCE_SQUARED
                    && self.base_water_with_cancel(TileCoord::new(x, y), cancelled)?
                        == Some(super::WaterKind::River)
                {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
