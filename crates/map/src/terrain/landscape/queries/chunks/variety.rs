//! Purely visual ground variety inside one biome family, the way Age of
//! Empires II maps mix related terrains: irregular patches of worn grass, a
//! grass and dirt mix with small bare cores, darker grass, and gravel near rock.
//!
//! A pure integer function of the world tile: bounded value noise on rotated
//! lattices whose periods are no multiple of the chunk or of any source sample
//! spacing, so patches are seamless across chunks and never follow a grid. No
//! float, allocation, source query or neighbour lookup. Only the drawn ground
//! sheet changes: heights, water, passability, movement cost, resources and
//! vegetation are decided before this and never read its result.
use crate::historical_parcels::LandUse;
use crate::{GroundMaterial, NativeHeightBand};
use aoe_core::TileCoord;

#[path = "tests/variety.rs"]
#[cfg(test)]
mod tests;

const UNIT: i64 = 4096;
const HIGH_BAND_GRAVEL_WEAR: i64 = 770;

pub(super) fn seed(geography_key: [u8; 32], procedural_seed: u64) -> u64 {
    let mut key = [0; 8];
    key.copy_from_slice(&geography_key[..8]);
    mix(u64::from_le_bytes(key) ^ procedural_seed.rotate_left(23))
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn lattice(seed: u64, x: i64, y: i64) -> i64 {
    #[cfg(test)]
    tests::cost::record(Some(true));
    (mix(seed ^ mix((x as u64) ^ (y as u64).rotate_left(32))) % (UNIT as u64 + 1)) as i64
}

/// Value noise in `0..=UNIT`; `at` and `period` are in 1/256 tile.
fn value(seed: u64, at: [i64; 2], period: i64) -> i64 {
    #[cfg(test)]
    tests::cost::record(Some(false));
    let cell = at.map(|v| v.div_euclid(period));
    let [tx, ty] = at.map(|v| {
        let t = v.rem_euclid(period) * UNIT / period;
        t * t * (3 * UNIT - 2 * t) / (UNIT * UNIT)
    });
    let corner = |dx, dy| lattice(seed, cell[0] + dx, cell[1] + dy);
    let lerp = |a: i64, b: i64, t: i64| a + (b - a) * t / UNIT;
    lerp(
        lerp(corner(0, 0), corner(1, 0), tx),
        lerp(corner(0, 1), corner(1, 1), tx),
        ty,
    )
}

/// The tile centre in 1/256 tile on axes rotated by `(cos, sin) * 256`.
fn rotated(tile: TileCoord, [cos, sin]: [i64; 2]) -> [i64; 2] {
    let (x, y) = (2 * i64::from(tile.x) + 1, 2 * i64::from(tile.y) + 1);
    [(x * cos + y * sin) / 2, (y * cos - x * sin) / 2]
}

/// Bare, worn ground in `0..=UNIT`: patches of about eleven, five and two tiles,
/// with a regional drift so some stretches are plain and others broken up.
fn wear(seed: u64, tile: TileCoord) -> i64 {
    let at = rotated(tile, [241, 86]);
    let region = value(seed ^ 0x51, rotated(tile, [226, -120]), 17_920);
    (9 * value(seed ^ 0x11, at, 2_893)
        + 5 * value(seed ^ 0x22, at, 1_203)
        + 2 * value(seed ^ 0x33, at, 538))
        / 16
        + (region - UNIT / 2) / 5
}

/// Richer growth in `0..=UNIT`: broad soft patches of about twenty and nine tiles.
fn lush(seed: u64, tile: TileCoord) -> i64 {
    let at = rotated(tile, [226, -120]);
    (10 * value(seed ^ 0x44, at, 5_888) + 6 * value(seed ^ 0x55, at, 2_330)) / 16
}

/// The ground sheet drawn on a walkable land tile. Variety stays in the
/// family the tile's own material allows: temperate grass never turns to sand
/// and desert sand never to green grass. Ploughed and grazed parcels are a
/// little more worn; ground at and above the subalpine band breaks to gravel.
pub(super) fn vary(
    seed: u64,
    tile: TileCoord,
    material: GroundMaterial,
    band: NativeHeightBand,
    land_use: LandUse,
) -> GroundMaterial {
    use GroundMaterial::{
        Dirt, DryGrass, GrassDirt, Gravel, LushGrass, Rock, Sand, TemperateGrass, WornGrass,
    };
    if !matches!(
        material,
        TemperateGrass | DryGrass | LushGrass | Sand | Rock
    ) {
        return material;
    }
    let high = !matches!(band, NativeHeightBand::Lowland | NativeHeightBand::Montane);
    let bias = match land_use {
        LandUse::Crop => 400,
        LandUse::Grazing => 200,
        _ => 0,
    } + if high { 250 } else { 0 };
    let wear = (wear(seed, tile) + bias) * 1000 / UNIT;
    // One shared decision gives every grass family the identical gravel mask
    // on high ground; their distinct low-band thresholds remain below.
    if high
        && matches!(material, TemperateGrass | DryGrass | LushGrass)
        && wear >= HIGH_BAND_GRAVEL_WEAR
    {
        return Gravel;
    }
    // Only the grasses consult the lush field; sand and rock skip its work.
    let lush_field = || lush(seed, tile) * 1000 / UNIT;
    match material {
        TemperateGrass if wear >= 790 => Dirt,
        TemperateGrass if wear >= 690 => GrassDirt,
        TemperateGrass if wear >= 600 => WornGrass,
        TemperateGrass if lush_field() >= 660 => LushGrass,
        LushGrass if wear >= 770 => WornGrass,
        LushGrass if wear >= 650 => TemperateGrass,
        DryGrass if wear >= 770 => Dirt,
        DryGrass if wear >= 640 => GrassDirt,
        DryGrass if lush_field() >= 690 => WornGrass,
        Sand if wear >= 800 => Dirt,
        Sand if wear >= 680 => Gravel,
        Rock if wear >= 770 => Dirt,
        Rock if wear >= 630 => Gravel,
        _ => material,
    }
}
