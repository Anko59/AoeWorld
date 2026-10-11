//! Shoreline shaping on synthetic source water. The fixture maps 33 water
//! samples onto 257 tiles, so one source cell is exactly eight tiles and the
//! unshaped (nearest-sample) boundary of cell `i` is the tile range
//! `8i - 4 ..= 8i + 3`.
use super::*;
use crate::{EnvironmentPage, EnvironmentPageKey, FieldPyramid, LandscapeTile, PyramidLevel};
use std::collections::BTreeMap;

pub(super) const WIDTH: i32 = 257;
pub(super) const CELL: i32 = 8;
/// Modelling bound, not geography: threshold noise keeps a shoreline within
/// 0.42 cells (3.4 tiles for the eight-tile fixture cell) of the source one.
pub(super) const SHAPE_TILES: i32 = 4;
/// Bilinear thresholding additionally rounds corners by at most half a cell.
pub(super) const CORNER_TILES: i32 = CELL / 2;

#[derive(Debug, Default)]
pub(super) struct Pages(pub(super) BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);

impl EnvironmentPageProvider for Pages {
    fn page(
        &self,
        key: EnvironmentPageKey,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Arc<EnvironmentPage>, EnvironmentPageError> {
        if cancelled() {
            return Err(EnvironmentPageError::Cancelled);
        }
        self.0
            .get(&key)
            .cloned()
            .ok_or(EnvironmentPageError::Missing)
    }
}

pub(super) fn pyramid(axis: u16) -> FieldPyramid {
    let mut levels = Vec::new();
    let mut samples = axis;
    loop {
        levels.push(PyramidLevel {
            samples_per_axis: samples,
            ordered_page_root: [7; 32],
        });
        if samples == 1 {
            break;
        }
        samples = samples.div_ceil(2);
    }
    FieldPyramid { levels }
}

/// Level-zero page rectangles of one field axis.
pub(super) fn page_grid(axis: u16) -> Vec<(u16, u16, u8, u8)> {
    let mut pages = Vec::new();
    for y in 0..axis.div_ceil(64) {
        for x in 0..axis.div_ceil(64) {
            pages.push((
                x,
                y,
                (axis - x * 64).min(64) as u8,
                (axis - y * 64).min(64) as u8,
            ));
        }
    }
    pages
}

/// Flat, dry-relief provider environment with one potential-vegetation class
/// and the given `(ocean, inland)` coverage per water sample.
pub(super) fn environment(
    water_axis: u16,
    biome_class: u8,
    coverage: &dyn Fn(u16, u16) -> (u8, u8),
) -> (PreparedEnvironment, Pages) {
    let mut pages = Pages::default();
    let mut insert = |page: EnvironmentPage| {
        pages.0.insert(page.key(), Arc::new(page));
    };
    insert(EnvironmentPage::Elevation(ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    }));
    insert(EnvironmentPage::Vegetation(PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![biome_class],
    }));
    for (x, y, width, height) in page_grid(water_axis) {
        let cells = (0..u16::from(height))
            .flat_map(|ly| (0..u16::from(width)).map(move |lx| (x * 64 + lx, y * 64 + ly)))
            .map(|(sx, sy)| coverage(sx, sy))
            .collect::<Vec<_>>();
        insert(EnvironmentPage::Water(WaterPage {
            level: 0,
            x,
            y,
            width,
            height,
            ocean_coverage_percent: cells.iter().map(|cell| cell.0).collect(),
            inland_coverage_percent: cells.iter().map(|cell| cell.1).collect(),
        }));
    }
    let environment = PreparedEnvironment {
        samples_per_axis: 1,
        geographic_millimeters_per_sample: 1_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: pyramid(1),
        water: Some(pyramid(water_axis)),
        vegetation: Some(pyramid(1)),
        historical_land_use: None,
        hydrology_evidence: None,
    };
    (environment, pages)
}

pub(super) fn bind(key: u8, environment: PreparedEnvironment, pages: Pages) -> MapChunkGenerator {
    MapChunkGenerator::new([key; 32], 5, WIDTH)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(pages),
        )
        .expect("synthetic provider")
}

/// Temperate (PNV 8) land over 33 water samples.
pub(super) fn coast(key: u8, coverage: &dyn Fn(u16, u16) -> (u8, u8)) -> MapChunkGenerator {
    let (environment, pages) = environment(33, 8, coverage);
    bind(key, environment, pages)
}

/// Every composed tile, row-major over the whole fixture map.
pub(super) fn composed(generator: &MapChunkGenerator) -> Vec<LandscapeTile> {
    let chunks = (WIDTH + crate::CHUNK_TILES - 1) / crate::CHUNK_TILES;
    let mut tiles = BTreeMap::new();
    for y in 0..chunks {
        for x in 0..chunks {
            let chunk = generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .expect("fixture chunk");
            for tile in chunk.tiles {
                tiles.insert((tile.tile.y, tile.tile.x), tile);
            }
        }
    }
    assert_eq!(tiles.len(), (WIDTH * WIDTH) as usize);
    tiles.into_values().collect()
}

pub(super) fn at(tiles: &[LandscapeTile], x: i32, y: i32) -> Option<&LandscapeTile> {
    ((0..WIDTH).contains(&x) && (0..WIDTH).contains(&y)).then(|| &tiles[(y * WIDTH + x) as usize])
}

pub(super) fn wet(tiles: &[LandscapeTile], x: i32, y: i32) -> bool {
    at(tiles, x, y).is_some_and(|tile| tile.terrain.water != WaterKind::None)
}

/// Tiles reachable from `start` through eight-connected water.
pub(super) fn water_component(tiles: &[LandscapeTile], start: (i32, i32)) -> Vec<bool> {
    let mut seen = vec![false; (WIDTH * WIDTH) as usize];
    if !wet(tiles, start.0, start.1) {
        return seen;
    }
    let mut pending = vec![start];
    seen[(start.1 * WIDTH + start.0) as usize] = true;
    while let Some((x, y)) = pending.pop() {
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (nx, ny) = (x + dx, y + dy);
                if wet(tiles, nx, ny) && !seen[(ny * WIDTH + nx) as usize] {
                    seen[(ny * WIDTH + nx) as usize] = true;
                    pending.push((nx, ny));
                }
            }
        }
    }
    seen
}

fn straight_coast(key: u8) -> MapChunkGenerator {
    // Ocean for source columns 16.., i.e. unshaped water for tile x >= 124.
    coast(key, &|sx, _| (if sx >= 16 { 100 } else { 0 }, 0))
}

#[test]
fn straight_source_coast_becomes_a_bounded_irregular_shoreline() {
    for key in [41, 42, 43] {
        let tiles = composed(&straight_coast(key));
        let mut first_water = Vec::new();
        for y in 0..WIDTH {
            for x in 0..WIDTH {
                if x < 124 - SHAPE_TILES {
                    assert!(!wet(&tiles, x, y), "key {key}: water far inland at {x},{y}");
                }
                if x >= 124 + SHAPE_TILES {
                    assert!(wet(&tiles, x, y), "key {key}: land far offshore at {x},{y}");
                }
            }
            first_water.push((0..WIDTH).find(|x| wet(&tiles, *x, y)).expect("coast"));
        }
        let mut distinct = first_water.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert!(
            distinct.len() >= 4,
            "key {key}: straight coast {distinct:?}"
        );
        let longest_straight_run = first_water
            .chunk_by(|left, right| left == right)
            .map(<[i32]>::len)
            .max()
            .unwrap_or(0);
        assert!(
            longest_straight_run <= 24,
            "key {key}: {longest_straight_run} rows share one shoreline column"
        );
    }
}

#[test]
fn shoreline_is_deterministic_and_keyed_by_geography() {
    let water = |key| {
        composed(&straight_coast(key))
            .iter()
            .map(|tile| tile.terrain.water)
            .collect::<Vec<_>>()
    };
    assert_eq!(water(41), water(41));
    assert_ne!(water(41), water(42));
}

/// Source staircase: ocean south-east of the anti-diagonal through the map
/// centre, one eight-tile step per source cell.
fn staircase_coast(key: u8) -> MapChunkGenerator {
    coast(key, &|sx, sy| (if sx + sy >= 32 { 100 } else { 0 }, 0))
}

#[test]
fn shoreline_is_seam_free_between_chunks_and_point_queries() {
    let generator = staircase_coast(41);
    // The coast (x + y = 252) crosses these four chunks.
    for (cx, cy) in [(3, 4), (4, 3), (2, 5), (5, 2)] {
        let chunk = generator
            .landscape_chunk_with_cancel(cx, cy, &|| false)
            .expect("chunk");
        let mut kinds = [false; 2];
        for tile in &chunk.tiles {
            let point = generator
                .landscape_point_with_cancel(tile.tile, &|| false)
                .expect("point query")
                .expect("point in bounds");
            assert_eq!(
                point.tile, *tile,
                "chunk {cx},{cy} disagrees with its point"
            );
            assert_eq!(generator.tile_at(tile.tile), Some(tile.terrain));
            kinds[usize::from(tile.terrain.water != WaterKind::None)] = true;
        }
        assert_eq!(kinds, [true; 2], "chunk {cx},{cy} holds no shoreline");
    }
}

#[test]
fn source_staircase_coast_becomes_a_diagonal_shoreline_without_long_steps() {
    for key in [41, 42, 43] {
        let tiles = composed(&staircase_coast(key));
        let first_water = (40..216)
            .map(|y| (0..WIDTH).find(|x| wet(&tiles, *x, y)).expect("coast"))
            .collect::<Vec<_>>();
        // Unshaped, seven of eight rows repeat the column above and the
        // eighth jumps by eight tiles.
        let repeated = first_water
            .windows(2)
            .filter(|pair| pair[0] == pair[1])
            .count();
        assert!(
            repeated * 100 < first_water.len() * 55,
            "key {key}: {repeated} of {} rows repeat a column",
            first_water.len()
        );
        for (index, x) in first_water.iter().enumerate() {
            let y = index as i32 + 40;
            assert!(
                (x + y - 252).abs() <= CELL + SHAPE_TILES,
                "key {key}: coast at {x},{y} left the source diagonal"
            );
        }
    }
}

#[test]
fn rectangular_source_lake_is_rounded_but_stays_where_the_source_puts_it() {
    // Source cells 12..=17 x 12..=16: unshaped tiles 92..=139 x 92..=131.
    let (left, right, top, bottom) = (92, 139, 92, 131);
    for key in [51, 52, 53] {
        let generator = coast(key, &|sx, sy| {
            (
                0,
                if (12..=17).contains(&sx) && (12..=16).contains(&sy) {
                    100
                } else {
                    0
                },
            )
        });
        let tiles = composed(&generator);
        let reach = SHAPE_TILES + CORNER_TILES;
        let mut area = 0;
        for y in 0..WIDTH {
            for x in 0..WIDTH {
                let water = wet(&tiles, x, y);
                area += i32::from(water);
                let outside = (left - x).max(x - right).max(top - y).max(y - bottom);
                if outside > SHAPE_TILES {
                    assert!(
                        !water,
                        "key {key}: water {outside} tiles outside at {x},{y}"
                    );
                }
                if -outside > reach {
                    assert!(water, "key {key}: dry lake interior at {x},{y}");
                }
            }
        }
        // Right angles are gone: the four unshaped 4x4 corner blocks (64
        // water tiles) are partly land, and neither the west shore nor the
        // north shore is one straight line.
        let corner_water = [
            (left, top),
            (right - 3, top),
            (left, bottom - 3),
            (right - 3, bottom - 3),
        ]
        .into_iter()
        .flat_map(|(x, y)| (0..4).flat_map(move |dy| (0..4).map(move |dx| (x + dx, y + dy))))
        .filter(|(x, y)| wet(&tiles, *x, *y))
        .count();
        assert!(
            corner_water < 56,
            "key {key}: square corners keep {corner_water}/64"
        );
        let mut west = (top + 8..bottom - 8)
            .map(|y| (0..WIDTH).find(|x| wet(&tiles, *x, y)).expect("lake row"))
            .collect::<Vec<_>>();
        let mut north = (left + 8..right - 8)
            .map(|x| {
                (0..WIDTH)
                    .find(|y| wet(&tiles, x, *y))
                    .expect("lake column")
            })
            .collect::<Vec<_>>();
        for shore in [&mut west, &mut north] {
            shore.sort_unstable();
            shore.dedup();
            assert!(shore.len() >= 3, "key {key}: straight lake shore {shore:?}");
        }
        let source_area = (right - left + 1) * (bottom - top + 1);
        assert!(
            (source_area * 80 / 100..=source_area * 115 / 100).contains(&area),
            "key {key}: lake area {area} drifted from {source_area}"
        );
        // One lake, not fragments.
        let component = water_component(&tiles, (116, 112));
        assert_eq!(
            component.iter().filter(|member| **member).count() as i32,
            area
        );
    }
}

#[test]
fn one_cell_wide_source_rivers_stay_connected_through_bends_and_diagonal_steps() {
    // West-east channel with two four-connected bends and one diagonal-only
    // step. Partial coverage (shallow cells) must not break the channel.
    let channel = |sx: u16, sy: u16| match sx {
        4..=10 => sy == 16,
        11..=15 => sy == 15 || (sx == 11 && sy == 16),
        16..=20 => sy == 14,
        21..=28 => sy == 13 || (sx == 21 && sy == 14) || (sx == 28 && (9..=13).contains(&sy)),
        _ => false,
    };
    assert!(channel(15, 15) && channel(16, 14) && !channel(15, 14) && !channel(16, 15));
    for key in [61, 62, 63, 64, 65, 66] {
        let generator = coast(key, &|sx, sy| {
            let coverage = if (sx + sy) % 2 == 0 { 100 } else { 5 };
            (0, if channel(sx, sy) { coverage } else { 0 })
        });
        let tiles = composed(&generator);
        // Every source cell centre is protected core water, and all of them
        // share one eight-connected body. Detached ponds beside the channel
        // are allowed; cuts are not.
        let component = water_component(&tiles, (4 * CELL, 16 * CELL));
        for (sx, sy) in (0..33).flat_map(|sy| (0..33).map(move |sx| (sx, sy))) {
            if channel(sx, sy) {
                let (x, y) = (i32::from(sx) * CELL, i32::from(sy) * CELL);
                assert!(
                    component[(y * WIDTH + x) as usize],
                    "key {key}: river cell {sx},{sy} is cut off"
                );
            }
        }
        // The channel stays a channel: no water far from its source cells.
        for y in 0..WIDTH {
            for x in 0..WIDTH {
                if wet(&tiles, x, y) {
                    let near =
                        (0..33)
                            .flat_map(|sy| (0..33).map(move |sx| (sx, sy)))
                            .any(|(sx, sy)| {
                                channel(sx, sy)
                                    && (x - i32::from(sx) * CELL).abs() <= CELL / 2 + SHAPE_TILES
                                    && (y - i32::from(sy) * CELL).abs() <= CELL / 2 + SHAPE_TILES
                            });
                    assert!(near, "key {key}: stray water at {x},{y}");
                }
            }
        }
    }
}

#[test]
fn sources_finer_than_the_shaping_cell_keep_their_exact_water() {
    // 257 samples over 257 tiles: every tile has its own source sample.
    let pond = |sx: u16, sy: u16| (100..=140).contains(&sx) && (90..=120).contains(&sy);
    let (environment, pages) =
        environment(257, 8, &|sx, sy| (0, if pond(sx, sy) { 100 } else { 0 }));
    let tiles = composed(&bind(71, environment, pages));
    for y in 0..WIDTH {
        for x in 0..WIDTH {
            assert_eq!(wet(&tiles, x, y), pond(x as u16, y as u16), "tile {x},{y}");
        }
    }
}

#[test]
fn seeded_fallback_water_is_not_made_of_square_blocks() {
    let generator = MapChunkGenerator::new([23; 32], 1, 512);
    let mut water = 0;
    let mut partial_lake_blocks = 0;
    for by in 0..32 {
        for bx in 0..32 {
            let mut wet_tiles = 0;
            for y in 0..16 {
                for x in 0..16 {
                    let tile = generator
                        .tile_at(TileCoord::new(bx * 16 + x, by * 16 + y))
                        .expect("fallback tile");
                    wet_tiles += i32::from(tile.water != WaterKind::None);
                }
            }
            water += wet_tiles;
            partial_lake_blocks += i32::from((64..256).contains(&wet_tiles));
        }
    }
    assert!(water > 0);
    assert!(
        partial_lake_blocks > 0,
        "every seeded lake still fills its 16-tile block exactly"
    );
}
