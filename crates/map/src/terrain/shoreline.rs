//! Deterministic sub-cell shoreline shaping for categorical water grids.
//!
//! Source water arrives as cells (prepared coverage, modeled hydrology or the
//! seeded fallback blocks). Reading the nearest cell per tile draws straight
//! shorelines and right angles. A tile instead reads the grid at a mildly
//! warped position, blends the wet/dry state of the four surrounding cells
//! bilinearly into a wetness between zero and one and compares it with a
//! noisy threshold around one half. The blend rounds corners and turns
//! staircases into diagonals; the threshold noise adds bays, headlands and
//! tile-scale irregularity; the warp bends shores of cells only a few tiles
//! wide, where sub-cell noise alone moves a shoreline by about one tile.
//!
//! Every constant here is a modelling choice, not observed geography:
//! - threshold noise never exceeds `core - 0.5` (at most 0.42), so wetness at
//!   or above `core` is always water and at or below `1 - core` always land;
//! - the noise has three bilinear value-noise octaves: 2.5 cells (32% of the
//!   budget), 0.8 cells (30%) and five tiles (38%, at most two tiles);
//!   their sum passes a monotonic soft gain so the budget is actually used;
//! - the warp is one bilinear octave per axis on a 16-tile lattice, at most
//!   2.5 tiles and at most 0.65 cells;
//! - a shaped shoreline therefore stays within `0.42 * cell + 2.5` tiles of
//!   the half-wet contour of the source cells: under one cell for cells of
//!   five tiles or more;
//! - cells narrower than three tiles are already tile-sized and stay exact.
//!
//! Connectivity is structural, not statistical: the segment joining two
//! four-adjacent wet cell centres has wetness one, the warp gradient is at
//! most 0.32, and the protected core is wide enough that the tile row nearest
//! the warped segment always stays water (and symmetrically land between dry
//! cells). Diagonal-only wet pairs are joined by an explicit ridge so
//! rasterised rivers stay connected.
//! Pure integer arithmetic on world coordinates: chunk seams, halos and point
//! queries are identical by construction.
use super::landscape::mix64;
use aoe_core::TileCoord;

/// Sub-tile and sub-cell fixed-point scale.
const UNIT: i64 = 256;
const MIN_SHAPED_CELL: i64 = 3 * UNIT;
/// Wetness and noise scale: 1024 is fully wet.
const FULL: i64 = 1024;
/// Protected core: wetness that noise can never turn into land.
const CORE: i64 = 942;
/// The tile row nearest a warped cell-centre segment lies within this many
/// sub-tile units of it (half a tile stretched by the warp gradient, plus
/// margin) and must stay inside the core.
const CORE_ROW: i64 = 176;
const WARP_LATTICE: i64 = 16 * UNIT;
const WARP_TILES: i64 = UNIT * 5 / 2;
/// Shares of the noise budget, in percent.
const COAST_SHARE: i64 = 32;
const BAY_SHARE: i64 = 30;
const DETAIL_SHARE: i64 = 38;
const DETAIL_TILES: i64 = UNIT * 2;
const DETAIL_LATTICE: i64 = 5 * UNIT;

/// Affine tile-to-cell mapping: `cell * 256 = (tile * 256 * numerator +
/// offset) / denominator`, with sample centres on integer cells.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CellGrid {
    numerator: i64,
    denominator: i64,
    offset: i64,
    /// Last valid cell of a bounded source axis; seeded blocks are unbounded.
    last: Option<i64>,
}

impl CellGrid {
    /// `samples` source samples spread over `width_tiles` tiles, sample `i`
    /// centred on tile `i * (width - 1) / (samples - 1)`.
    pub(super) fn source(samples: u16, width_tiles: i32) -> Self {
        let last = i64::from(samples.max(1)) - 1;
        Self {
            numerator: last,
            denominator: i64::from(width_tiles.max(2)) - 1,
            offset: 0,
            last: Some(last),
        }
    }

    /// Seeded blocks of `size` tiles: block `i` covers `size * i ..`.
    pub(super) const fn blocks(size: i64) -> Self {
        Self {
            numerator: 1,
            denominator: size,
            offset: -(UNIT / 2) * (size - 1),
            last: None,
        }
    }

    /// Cell width in sub-tile units; zero for a single-sample axis.
    fn cell(self) -> i64 {
        if self.numerator == 0 {
            0
        } else {
            UNIT * self.denominator / self.numerator
        }
    }

    pub(super) fn shaped(self) -> bool {
        self.cell() >= MIN_SHAPED_CELL
    }

    /// Sub-cell coordinate of a sub-tile position, clamped to a bounded axis.
    fn coordinate(self, position: i64) -> i64 {
        let value = (position * self.numerator + self.offset).div_euclid(self.denominator);
        match self.last {
            Some(last) => value.clamp(0, last * UNIT),
            None => value,
        }
    }

    /// The nearest cell of one tile coordinate: the unshaped source lookup.
    fn nearest(self, tile: i32) -> i64 {
        let tile = match self.last {
            Some(_) => i64::from(tile).clamp(0, self.denominator),
            None => i64::from(tile),
        };
        (tile * UNIT * self.numerator + self.offset + (UNIT / 2) * self.denominator)
            .div_euclid(UNIT * self.denominator)
    }
}

/// Bilinear value noise in `-amplitude..=amplitude`. Node magnitudes are at
/// least one half so the blend uses most of its range.
fn octave(key: u64, domain: u64, position: (i64, i64), lattice: i64, amplitude: i64) -> i64 {
    let (cx, cy) = (
        position.0.div_euclid(lattice),
        position.1.div_euclid(lattice),
    );
    let sx = position.0.rem_euclid(lattice) * FULL / lattice;
    let sy = position.1.rem_euclid(lattice) * FULL / lattice;
    let node = |dx: i64, dy: i64| {
        let value = mix64(
            key ^ mix64(domain)
                ^ mix64((cx + dx) as u64)
                ^ mix64((cy + dy) as u64 ^ 0xd1b5_4a32_d192_ed03),
        );
        let magnitude = 512 + (value % 513) as i64;
        if value >> 63 == 0 {
            magnitude
        } else {
            -magnitude
        }
    };
    let north = node(0, 0) * (FULL - sx) + node(1, 0) * sx;
    let south = node(0, 1) * (FULL - sx) + node(1, 1) * sx;
    (north * (FULL - sy) + south * sy) / FULL * amplitude / (FULL * FULL)
}

/// Core wetness for one cell width: lowered for narrow cells so the tile row
/// nearest a wet segment is still protected.
fn core(cell: i64) -> i64 {
    CORE.min(FULL - FULL * CORE_ROW / cell)
}

/// Threshold offset of one tile, in wetness units; never beyond the core.
fn threshold_noise(key: u64, cell: i64, tile: TileCoord) -> i64 {
    let position = (i64::from(tile.x) * UNIT, i64::from(tile.y) * UNIT);
    let budget = core(cell) - FULL / 2;
    let detail = (budget * DETAIL_SHARE / 100).min(DETAIL_TILES * FULL / cell);
    let raw = octave(key, 1, position, cell * 5 / 2, budget * COAST_SHARE / 100)
        + octave(key, 2, position, cell * 4 / 5, budget * BAY_SHARE / 100)
        + octave(key, 3, position, DETAIL_LATTICE, detail);
    // Monotonic soft gain: summed value noise clusters near zero, which
    // would leave most of the budget unused; no plateau, same extremes.
    raw * 3 * budget / (budget + 2 * raw.abs())
}

/// Smooth world-anchored value in `-1024..=1024` on a lattice of
/// `lattice_tiles`; the shore band varies its widths with it.
pub(super) fn variation(key: u64, domain: u64, tile: TileCoord, lattice_tiles: i64) -> i64 {
    let position = (i64::from(tile.x) * UNIT, i64::from(tile.y) * UNIT);
    octave(key, domain, position, lattice_tiles * UNIT, FULL)
}

/// Largest warp displacement in sub-tile units for one cell width.
fn warp_reach(cell: i64) -> i64 {
    WARP_TILES.min(cell * 13 / 20)
}

/// Warped sub-tile position of a tile centre.
fn warped(key: u64, cell: i64, tile: TileCoord) -> (i64, i64) {
    let position = (i64::from(tile.x) * UNIT, i64::from(tile.y) * UNIT);
    let reach = warp_reach(cell);
    (
        position.0 + octave(key, 4, position, WARP_LATTICE, reach),
        position.1 + octave(key, 5, position, WARP_LATTICE, reach),
    )
}

/// The wet source cell that owns `tile`, or `None` for land. `cell` returns a
/// value for wet cells only; coordinates are always within a bounded axis.
pub(super) fn sample<T: Copy, E>(
    key: u64,
    grid: CellGrid,
    tile: TileCoord,
    cell: &mut impl FnMut(i64, i64) -> Result<Option<T>, E>,
) -> Result<Option<T>, E> {
    if !grid.shaped() {
        return cell(grid.nearest(tile.x), grid.nearest(tile.y));
    }
    let position = warped(key, grid.cell(), tile);
    let u = grid.coordinate(position.0);
    let v = grid.coordinate(position.1);
    let (x0, fu) = (u.div_euclid(UNIT), u.rem_euclid(UNIT));
    let (y0, fv) = (v.div_euclid(UNIT), v.rem_euclid(UNIT));
    let x1 = grid.last.map_or(x0 + 1, |last| (x0 + 1).min(last));
    let y1 = grid.last.map_or(y0 + 1, |last| (y0 + 1).min(last));
    let corners = [
        (x0, y0, (UNIT - fu) * (UNIT - fv)),
        (x1, y0, fu * (UNIT - fv)),
        (x0, y1, (UNIT - fu) * fv),
        (x1, y1, fu * fv),
    ];
    let mut wet = [false; 4];
    let mut wet_weight = 0;
    let mut best: Option<(i64, T)> = None;
    for (index, (x, y, weight)) in corners.into_iter().enumerate() {
        if let Some(value) = cell(x, y)? {
            wet[index] = true;
            wet_weight += weight;
            if best.is_none_or(|(heaviest, _)| weight > heaviest) {
                best = Some((weight, value));
            }
        }
    }
    let Some((_, value)) = best else {
        return Ok(None);
    };
    // Diagonal-only pairs touch at one point; a ridge along the diagonal
    // keeps them joined: full on the diagonal, gone 0.87 cells across.
    let across = match wet {
        [true, false, false, true] => Some((fu - fv).abs()),
        [false, true, true, false] => Some((fu + fv - UNIT).abs()),
        _ => None,
    };
    let ridge = across.map_or(0, |across| {
        (FULL * 13 / 10 - across * FULL * 3 / (2 * UNIT)).clamp(0, FULL)
    });
    let wetness = (wet_weight * FULL / (UNIT * UNIT)).max(ridge);
    let core = core(grid.cell());
    let water = wetness >= core
        || (wetness > FULL - core
            && wetness * 2 >= FULL + 2 * threshold_noise(key, grid.cell(), tile));
    Ok(water.then_some(value))
}

/// Conservative reach test: `false` proves that no tile within `radius`
/// tiles of `tile` samples a wet cell.
pub(super) fn any_wet_near<E>(
    grid: CellGrid,
    tile: TileCoord,
    radius: i32,
    wet: &mut impl FnMut(i64, i64) -> Result<bool, E>,
) -> Result<bool, E> {
    if !grid.shaped() {
        // Unshaped tiles read exactly their nearest cell.
        let mut previous = None;
        for dy in -radius..=radius {
            let y = grid.nearest(tile.y.saturating_add(dy));
            for dx in -radius..=radius {
                let x = grid.nearest(tile.x.saturating_add(dx));
                if previous != Some((x, y)) && wet(x, y)? {
                    return Ok(true);
                }
                previous = Some((x, y));
            }
        }
        return Ok(false);
    }
    // A shaped tile is water only if one of the four cells around its warped
    // position is.
    let span = |tile: i32| {
        let centre = i64::from(tile) * UNIT;
        let reach = i64::from(radius) * UNIT + warp_reach(grid.cell());
        let low = grid.coordinate(centre - reach).div_euclid(UNIT);
        let high = grid.coordinate(centre + reach).div_euclid(UNIT) + 1;
        (low, grid.last.map_or(high, |last| high.min(last)))
    };
    let (x0, x1) = span(tile.x);
    let (y0, y1) = span(tile.y);
    for y in y0..=y1 {
        for x in x0..=x1 {
            if wet(x, y)? {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unshaped_grids_reproduce_the_rounded_source_lookup() {
        let grid = CellGrid::source(128, 200);
        assert!(!grid.shaped());
        for tile in 0..200_i64 {
            assert_eq!(grid.nearest(tile as i32), (tile * 127 + 99) / 199);
        }
        let blocks = CellGrid::blocks(16);
        for tile in -40..40_i32 {
            assert_eq!(blocks.nearest(tile), i64::from(tile.div_euclid(16)));
        }
        assert!(blocks.shaped() && CellGrid::blocks(4).shaped());
        assert!(!CellGrid::source(1, 512).shaped());
    }

    #[test]
    fn threshold_noise_is_bounded_and_the_core_protects_the_nearest_tile_row() {
        for cell in [3 * UNIT, 4 * UNIT, 8 * UNIT, 40 * UNIT, 315 * UNIT] {
            let limit = core(cell) - FULL / 2;
            let mut extremes = (0, 0);
            for y in -80..80 {
                for x in -80..80 {
                    let step = (cell / (8 * UNIT)).max(1) as i32;
                    let tile = TileCoord::new(x * 3 * step, y * 7 * step);
                    let noise = threshold_noise(0x51ed_270b, cell, tile);
                    assert!(noise.abs() <= limit);
                    extremes = (extremes.0.min(noise), extremes.1.max(noise));
                }
            }
            assert!(
                extremes.0 < -limit / 3 && extremes.1 > limit / 3,
                "{cell}: {extremes:?}"
            );
            // Half a tile beside a fully wet segment, stretched by the largest
            // warp gradient (two reaches per lattice), stays in the core.
            let gradient = 2 * warp_reach(cell) * FULL / WARP_LATTICE;
            let row = (UNIT / 2) * (FULL + gradient) / FULL;
            assert!(gradient <= FULL * 32 / 100);
            assert!(FULL - FULL * row / cell >= core(cell), "{cell}");
            assert!(core(cell) > FULL * 3 / 4);
            for y in -40..40 {
                for x in -40..40 {
                    let tile = TileCoord::new(x * 5, y * 3);
                    let (wx, wy) = warped(0x51ed_270b, cell, tile);
                    assert!((wx - i64::from(tile.x) * UNIT).abs() <= warp_reach(cell));
                    assert!((wy - i64::from(tile.y) * UNIT).abs() <= warp_reach(cell));
                }
            }
        }
    }
}
