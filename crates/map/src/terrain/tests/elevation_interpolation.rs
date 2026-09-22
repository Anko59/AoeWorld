use super::elevation::{AxisPosition, bilinear_height, source_axis_position};
use super::*;
use crate::{
    EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
    FieldPyramid, PyramidLevel, ordered_page_root,
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Debug)]
struct Pages(BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>);

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

fn small_plane() -> (PreparedEnvironment, Vec<ElevationPage>, Pages) {
    let level_zero = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0, 100, 100, 200],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![100],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 8_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&level_zero))
                        .expect("fine page root"),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview))
                        .expect("overview page root"),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
    };
    let pages = Pages(BTreeMap::from([
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::Elevation,
                level: 0,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::Elevation(level_zero.clone())),
        ),
        (
            EnvironmentPageKey {
                layer: crate::PageLayer::Elevation,
                level: 1,
                x: 0,
                y: 0,
            },
            Arc::new(EnvironmentPage::Elevation(overview.clone())),
        ),
    ]));
    (environment, vec![level_zero, overview], pages)
}

#[test]
fn recipe_four_bilinear_dense_and_provider_samples_match_across_shared_corners() {
    let (environment, elevation, pages) = small_plane();
    let dense = MapChunkGenerator::new([4; 32], 3, 5)
        .with_elevation_sampling_recipe(crate::GENERATION_RECIPE_VERSION)
        .with_prepared_elevation(Ratio::new(1, 1).expect("ratio"), &environment, elevation)
        .expect("dense elevation");
    let lazy = MapChunkGenerator::new([4; 32], 3, 5)
        .with_elevation_sampling_recipe(crate::GENERATION_RECIPE_VERSION)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(pages),
        )
        .expect("provider elevation");
    for y in 0..5 {
        for x in 0..5 {
            let tile = TileCoord::new(x, y);
            let dense_tile = dense.tile_at(tile).expect("dense tile");
            let lazy_tile = lazy
                .tile_at_with_cancel(tile, &|| false)
                .expect("lazy query")
                .expect("lazy tile");
            assert_eq!(lazy_tile, dense_tile);
            assert!(dense_tile.passable);
        }
    }
    assert_eq!(
        dense
            .tile_at(TileCoord::new(2, 2))
            .expect("interpolated tile")
            .geographic_height_centimeters,
        100
    );
    let prepared = dense.elevation.as_deref().expect("dense elevation");
    assert_eq!(prepared.height_at(TileCoord::new(2, 2), 5), Some(100));
    assert_eq!(
        prepared.corner_heights(TileCoord::new(2, 2), 5),
        Some([60, 100, 140, 100])
    );
    assert_eq!(prepared.height_at(TileCoord::new(0, 0), 5), Some(0));
    assert_eq!(
        prepared.corner_heights(TileCoord::new(4, 4), 5),
        Some([200; 4])
    );
    for y in 0..4 {
        for x in 0..4 {
            let tile = dense.tile_at(TileCoord::new(x, y)).expect("west tile");
            let east = dense.tile_at(TileCoord::new(x + 1, y)).expect("east tile");
            let south = dense.tile_at(TileCoord::new(x, y + 1)).expect("south tile");
            assert_eq!(
                tile.surface.corner_game_height_levels[1],
                east.surface.corner_game_height_levels[0]
            );
            assert_eq!(
                tile.surface.corner_game_height_levels[2],
                south.surface.corner_game_height_levels[1]
            );
        }
    }
}

#[test]
fn cell_center_and_corner_positions_use_cell_centered_samples() {
    assert_eq!(
        source_axis_position(0, 3, 4, AxisPosition::Corner),
        Some((0, 0, 0, 8))
    );
    assert_eq!(
        source_axis_position(1, 3, 4, AxisPosition::Corner),
        Some((0, 1, 2, 8))
    );
    assert_eq!(
        source_axis_position(1, 3, 4, AxisPosition::TileCenter),
        Some((0, 1, 5, 8))
    );
    assert_eq!(
        source_axis_position(4, 3, 4, AxisPosition::Corner),
        Some((2, 2, 0, 8))
    );
    assert_eq!(
        source_axis_position(3, 3, 4, AxisPosition::TileCenter),
        Some((2, 2, 0, 8))
    );
}

#[test]
fn constant_fields_and_negative_half_rounding_are_deterministic() {
    let values = [-1; 4];
    assert_eq!(bilinear_height(values, 1, 3, 4), -1);
    assert_eq!(bilinear_height([0, -1, 0, -1], 1, 0, 2), -1);
    assert_eq!(bilinear_height([0, 1, 0, 1], 1, 0, 2), 1);
    assert_eq!(bilinear_height([42; 4], 5, 7, 8), 42);
}

#[test]
fn recipe_three_keeps_the_historical_nearest_cell_mapping() {
    let (environment, elevation, _) = small_plane();
    let legacy = MapChunkGenerator::new([4; 32], 3, 5)
        .with_elevation_sampling_recipe(crate::LEGACY_GENERATION_RECIPE_VERSION)
        .with_prepared_elevation(Ratio::new(1, 1).expect("ratio"), &environment, elevation)
        .expect("legacy elevation");
    assert_eq!(
        legacy
            .tile_at(TileCoord::new(3, 3))
            .expect("legacy tile")
            .geographic_height_centimeters,
        200
    );
}

fn pyramid_levels(mut axis: u16) -> Vec<PyramidLevel> {
    let mut levels = Vec::new();
    loop {
        levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: [u8::try_from(levels.len() + 1).expect("small test pyramid"); 32],
        });
        if axis == 1 {
            break;
        }
        axis = axis.div_ceil(2);
    }
    levels
}

fn odd_axis_data() -> (
    PreparedEnvironment,
    BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
) {
    let mut page_index = BTreeMap::new();
    for page_y in 0..2_u16 {
        for page_x in 0..2_u16 {
            let width = if page_x == 0 { 64 } else { 1 };
            let height = if page_y == 0 { 64 } else { 1 };
            let page = ElevationPage {
                level: 0,
                x: page_x,
                y: page_y,
                width,
                height,
                geographic_height_centimeters: (0..u16::from(height))
                    .flat_map(|y| {
                        (0..u16::from(width))
                            .map(move |x| i32::from(page_x * 64 + x + page_y * 64 + y) * 20)
                    })
                    .collect(),
            };
            page_index.insert(
                EnvironmentPageKey {
                    layer: crate::PageLayer::Elevation,
                    level: 0,
                    x: page_x,
                    y: page_y,
                },
                Arc::new(EnvironmentPage::Elevation(page)),
            );
        }
    }
    let environment = PreparedEnvironment {
        samples_per_axis: 65,
        geographic_millimeters_per_sample: 2_000,
        page_samples: crate::ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: pyramid_levels(65),
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
    };
    (environment, page_index)
}

fn odd_axis_generator(
    pages: BTreeMap<EnvironmentPageKey, Arc<EnvironmentPage>>,
) -> MapChunkGenerator {
    let (environment, _) = odd_axis_data();
    MapChunkGenerator::new([5; 32], 1, 65)
        .with_elevation_sampling_recipe(crate::GENERATION_RECIPE_VERSION)
        .with_page_provider(
            Ratio::new(1, 1).expect("ratio"),
            environment,
            Arc::new(Pages(pages)),
        )
        .expect("provider")
}

#[test]
fn provider_interpolates_across_page_seams_on_odd_source_axes() {
    let (_, page_index) = odd_axis_data();
    let generator = odd_axis_generator(page_index);
    let before_seam = generator
        .tile_at_with_cancel(TileCoord::new(63, 0), &|| false)
        .expect("page zero tile")
        .expect("tile");
    let after_seam = generator
        .tile_at_with_cancel(TileCoord::new(64, 0), &|| false)
        .expect("page one tile")
        .expect("tile");
    assert_eq!(
        before_seam.surface.corner_game_height_levels[1],
        after_seam.surface.corner_game_height_levels[0]
    );
    assert_eq!(before_seam.geographic_height_centimeters, 1_260);
    assert_eq!(after_seam.geographic_height_centimeters, 1_280);
}

#[test]
fn provider_reports_missing_corrupt_and_cancelled_neighbor_pages() {
    let (_, mut pages) = odd_axis_data();
    let neighbor = EnvironmentPageKey {
        layer: crate::PageLayer::Elevation,
        level: 0,
        x: 1,
        y: 0,
    };
    pages.remove(&neighbor);
    let missing = odd_axis_generator(pages);
    assert_eq!(
        missing.tile_at_with_cancel(TileCoord::new(63, 0), &|| false),
        Err(EnvironmentPageError::Missing)
    );

    let (_, mut pages) = odd_axis_data();
    pages.insert(
        neighbor,
        Arc::new(EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        })),
    );
    let corrupt = odd_axis_generator(pages);
    assert_eq!(
        corrupt.tile_at_with_cancel(TileCoord::new(63, 0), &|| false),
        Err(EnvironmentPageError::Corrupt)
    );

    let (_, pages) = odd_axis_data();
    let cancelled = odd_axis_generator(pages);
    assert_eq!(
        cancelled.tile_at_with_cancel(TileCoord::new(63, 0), &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
}
