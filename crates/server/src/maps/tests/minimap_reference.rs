//! The raster against a slow reference kept in this file: one thread, row
//! order, the complete public surface query for every sample and relief
//! filtered with plain loops. It stays true when the landscape itself changes
//! on purpose; the recorded pins below do not and are then filled again.
use super::super::super::minimap::{MINIMAP_MIN_RIVER_WIDTH_UNITS, minimap_raster_at_samples};
use super::sources::{MILLIMETERS_PER_TILE, alpine, country, modelled, mosaic, network, steppe};
use super::*;
use aoe_map::{LandscapePoint, Provenance, WaterKind};

/// The raster's private constants, restated so a changed one is noticed.
const WARP_MARGIN_TILES: i32 = 3;
const SHADE_STEP_PER_MILLE: i64 = 25;

pub(super) fn raster_at(
    generator: &MapChunkGenerator,
    tiles: u64,
    axis: u16,
    samples: u16,
) -> Vec<u8> {
    let millimeters = MILLIMETERS_PER_TILE;
    minimap_raster_at_samples(generator, tiles, millimeters, axis, samples, &|| false)
        .expect("raster")
}

fn surface(generator: &MapChunkGenerator, tile: TileCoord) -> Option<LandscapePoint> {
    generator
        .landscape_surface_with_cancel(tile, &|| false)
        .expect("surface query")
}

fn sample_tile(tiles: u64, samples: u16, x: u16, y: u16) -> TileCoord {
    TileCoord::new(
        minimap_tile(tiles, samples, x).expect("x"),
        minimap_tile(tiles, samples, y).expect("y"),
    )
}

/// The first dry land on two rings around an omitted stream.
fn surrounding_class(generator: &MapChunkGenerator, tile: TileCoord, reach: i32) -> MinimapClass {
    for distance in [reach, reach * 2] {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let probe = TileCoord::new(tile.x + dx * distance, tile.y + dy * distance);
            if let Some(point) = surface(generator, probe)
                && point.tile.terrain.water == WaterKind::None
            {
                return classify(&point);
            }
        }
    }
    MinimapClass::Grass
}

pub(super) struct Reference {
    pub(super) raster: Vec<u8>,
    /// Samples on river water that the raster shows as land.
    omitted_streams: usize,
    /// Samples a cited correction made land.
    corrected: usize,
    cliffs: usize,
}

pub(super) fn reference(
    generator: &MapChunkGenerator,
    tiles: u64,
    axis: u16,
    samples: u16,
) -> Reference {
    let side = usize::from(samples);
    let geometry = generator.has_river_geometry();
    let footprint = (tiles.div_ceil(u64::from(samples)) * 181).div_ceil(256) as i32;
    let drawn_river = |tile, radius| {
        generator
            .river_within_with_cancel(tile, radius, MINIMAP_MIN_RIVER_WIDTH_UNITS, &|| false)
            .expect("river query")
    };
    let (mut classes, mut heights) = (Vec::new(), Vec::new());
    let (mut omitted_streams, mut corrected, mut cliffs) = (0, 0, 0);
    for y in 0..samples {
        for x in 0..samples {
            let tile = sample_tile(tiles, samples, x, y);
            let point = surface(generator, tile).expect("inside the map");
            let terrain = point.tile.terrain;
            let mut class = classify(&point);
            let corrected_land = terrain.water == WaterKind::None
                && terrain.water_provenance == Provenance::HistoricallyCorrected;
            let open_water = matches!(
                class,
                MinimapClass::Ocean | MinimapClass::Lake | MinimapClass::River
            );
            if geometry && class == MinimapClass::River {
                let reach = footprint + WARP_MARGIN_TILES;
                if !drawn_river(tile, reach) {
                    class = surrounding_class(generator, tile, reach + 2);
                    omitted_streams += 1;
                }
            } else if geometry && !corrected_land && !open_water && drawn_river(tile, footprint) {
                class = MinimapClass::River;
            }
            corrected += usize::from(corrected_land);
            cliffs += usize::from(terrain.surface.is_cliff());
            classes.push(class);
            heights.push(i64::from(terrain.geographic_height_centimeters));
        }
    }
    let shades = reference_shades(&heights, tiles, axis, side);
    let mut raster = MINIMAP_MAGIC.to_vec();
    raster.extend_from_slice(&samples.to_le_bytes());
    raster.extend_from_slice(&[0, 0]);
    raster.extend(classes.iter().zip(shades).map(|(class, shade)| {
        let flat = matches!(
            class,
            MinimapClass::Shore | MinimapClass::River | MinimapClass::Lake | MinimapClass::Ocean
        );
        let shade = if flat { FLAT_SHADE } else { shade };
        *class as u8 | shade << 4
    }));
    Reference {
        raster,
        omitted_streams,
        corrected,
        cliffs,
    }
}

fn reference_radius(tiles: u64, axis: u16, side: usize) -> usize {
    if side <= 1 {
        return 0;
    }
    let facets = if axis == 0 {
        (32 * side as u64).div_ceil(tiles)
    } else {
        (4 * side as u64).div_ceil(u64::from(axis))
    };
    (facets as usize).clamp(4, (side / 8).max(4)).min(side - 1)
}

/// North-west light on the mean rise of a mirrored square window.
fn reference_shades(heights: &[i64], tiles: u64, axis: u16, side: usize) -> Vec<u8> {
    let spacing = (tiles * MILLIMETERS_PER_TILE / side as u64).max(1) as i64;
    let reach = reference_radius(tiles, axis, side) as isize;
    let mirror = |position: isize| {
        let period = 2 * (side as isize - 1);
        if period == 0 {
            return 0;
        }
        let position = position.rem_euclid(period);
        position.min(period - position) as usize
    };
    let rise = |before: usize, after: usize, span: usize| {
        if span == 0 {
            0
        } else {
            (heights[after] - heights[before]) / span as i64
        }
    };
    let cells = move || (0..side).flat_map(move |y| (0..side).map(move |x| (x, y)));
    let rises = cells()
        .map(|(x, y)| {
            let (left, right) = (x.saturating_sub(1), (x + 1).min(side - 1));
            let (up, down) = (y.saturating_sub(1), (y + 1).min(side - 1));
            rise(y * side + left, y * side + right, right - left)
                + rise(up * side + x, down * side + x, down - up)
        })
        .collect::<Vec<i64>>();
    let along_rows = cells()
        .map(|(x, y)| {
            (-reach..=reach)
                .map(|step| rises[y * side + mirror(x as isize + step)])
                .sum::<i64>()
        })
        .collect::<Vec<i64>>();
    let area = (2 * reach as i64 + 1).pow(2);
    cells()
        .map(|(x, y)| {
            let sum = (-reach..=reach)
                .map(|step| along_rows[mirror(y as isize + step) * side + x])
                .sum::<i64>();
            let per_mille = (sum / area).saturating_mul(10_000) / spacing;
            (i64::from(FLAT_SHADE) + per_mille / SHADE_STEP_PER_MILLE).clamp(0, 15) as u8
        })
        .collect()
}

fn compare(
    name: &str,
    generator: &MapChunkGenerator,
    tiles: u64,
    axis: u16,
    samples: u16,
) -> Reference {
    let expected = reference(generator, tiles, axis, samples);
    let actual = raster_at(generator, tiles, axis, samples);
    assert_eq!(actual.len(), expected.raster.len(), "{name} at {samples}");
    let different = (0..actual.len()).find(|at| actual[*at] != expected.raster[*at]);
    if let Some(at) = different {
        let cell = at.saturating_sub(MINIMAP_HEADER_BYTES);
        panic!(
            "{name} at {samples}: sample ({}, {}) is {:#04x}, not {:#04x}",
            cell % usize::from(samples),
            cell / usize::from(samples),
            actual[at],
            expected.raster[at]
        );
    }
    expected
}

#[test]
fn raster_bytes_equal_the_row_order_reference_on_every_kind_of_terrain() {
    let (package, coast) = coast_forest_mountain();
    let coast = compare("coast", &coast, package.estimate.tiles_per_side, 4, 64);
    for class in [
        MinimapClass::Ocean,
        MinimapClass::Lake,
        MinimapClass::Forest,
        MinimapClass::Grass,
        MinimapClass::Rock,
    ] {
        assert!(count(&coast.raster, class) > 0, "{class:?} is missing");
    }
    let alpine = compare("alpine", &alpine(768), 768, 16, 96);
    assert!(alpine.cliffs > 0, "no sample lies on a cliff");
    assert!(count(&alpine.raster, MinimapClass::Snow) > 0);
    assert!(count(&alpine.raster, MinimapClass::Rock) > 0);
    let network = compare("network", &network(), 2_048, 4, 128);
    assert!(network.omitted_streams > 0, "no omitted stream is sampled");
    assert!(count(&network.raster, MinimapClass::River) > 0);
    compare("steppe", &steppe(512), 512, 16, 64);
}

#[test]
fn raster_bytes_equal_the_reference_on_modelled_water_land_use_and_several_pages() {
    let modelled = compare("modelled", &modelled(512), 512, 16, 64);
    assert!(count(&modelled.raster, MinimapClass::Lake) > 0);
    assert!(modelled.corrected > 0, "no sample lies on corrected land");
    let farmed = compare("farmed", &country(256, true), 256, 16, 64);
    assert!(count(&farmed.raster, MinimapClass::Cropland) > 0);
    assert!(count(&farmed.raster, MinimapClass::Pasture) > 0);
    compare("mosaic", &mosaic(512), 512, 128, 128);
}

/// The reference runs on one thread and production on four, so this is also
/// the worker-count check. Sizes with one sample per tile include every edge.
#[test]
fn raster_bytes_equal_the_reference_at_odd_sizes_and_map_edges() {
    let (package, coast) = coast_forest_mountain();
    for samples in [1, 2, 63, 65] {
        let tiles = package.estimate.tiles_per_side;
        compare("coast", &coast, tiles, 4, samples);
    }
    compare("alpine edges", &alpine(95), 95, 16, 95);
    for tiles in [63_u16, 65] {
        let fallback = MapChunkGenerator::new([3; 32], 5, i32::from(tiles));
        compare("fallback edges", &fallback, u64::from(tiles), 0, tiles);
    }
    let fallback = MapChunkGenerator::new([3; 32], 5, 512);
    compare("fallback", &fallback, 512, 0, 511);
}

#[test]
fn a_rock_pile_under_a_sample_changes_its_passability_but_not_its_byte() {
    let rocky = steppe(96);
    let bare = rocky.clone().without_loose_rocks();
    assert_eq!(
        raster_at(&rocky, 96, 16, 96),
        raster_at(&bare, 96, 16, 96),
        "class and relief never depend on loose rocks"
    );
    let passable = |generator, x, y| {
        let point = surface(generator, TileCoord::new(x, y)).expect("inside the map");
        point.tile.terrain.passable
    };
    let blocked = (0..96 * 96)
        .filter(|index| {
            let (x, y) = (index % 96, index / 96);
            passable(&bare, x, y) && !passable(&rocky, x, y)
        })
        .count();
    // The public surface query still reports the rock's footprint.
    assert!(blocked >= 9, "only {blocked} samples lie under a rock pile");
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// FNV-1a 64 of complete rasters (this crate has no BLAKE3 dependency).
/// UNFILLED: run this test on the revision before the raster changed (6f73ef9)
/// and paste the lines its failure prints.
const PINS: [(&str, u64); 5] = [
    ("coast coarse", 0x3eb3_49d6_c520_c38c),
    ("coast fine", 0xdb05_6133_2230_c47c),
    ("alpine", 0xf827_dc71_6a36_b931),
    ("network coarse", 0x1d1e_86fe_565f_91c3),
    ("network", 0x9c96_e5e9_d5db_c6ff),
];

#[test]
fn raster_bytes_match_the_recorded_pins() {
    let (package, coast) = coast_forest_mountain();
    let tiles = package.estimate.tiles_per_side;
    let (alpine, network) = (alpine(768), network());
    let actual = [
        ("coast coarse", raster_at(&coast, tiles, 4, 64)),
        ("coast fine", raster_at(&coast, tiles, 4, 512)),
        ("alpine", raster_at(&alpine, 768, 16, 96)),
        ("network coarse", raster_at(&network, 2_048, 4, 64)),
        ("network", raster_at(&network, 2_048, 4, 128)),
    ]
    .map(|(name, raster)| (name, fnv1a(&raster)));
    let lines = actual.map(|(name, hash)| format!("    ({name:?}, {hash:#018x}),\n"));
    assert!(
        actual == PINS,
        "replace the lines of PINS with:\n{}",
        lines.concat()
    );
}
