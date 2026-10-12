use super::*;
use crate::terrain::{landscape, relief as cliff_relief};

fn reserved(generator: &MapChunkGenerator, tile: TileCoord) -> bool {
    landscape::starting_glade(generator, tile)
        || landscape::starting_connector_contains(generator, tile)
}

fn cell(anchor: TileCoord) -> impl Iterator<Item = TileCoord> {
    (-1..=1)
        .flat_map(move |dy| (-1..=1).map(move |dx| TileCoord::new(anchor.x + dx, anchor.y + dy)))
}

#[test]
fn reserved_cells_are_wholly_open_and_seed_changes_only_reserved_cells() {
    let first = plane(-432, 0);
    let mut second = first.clone();
    second.procedural_seed = 977;
    let mut boundary = 0;
    let mut pieces = 0;
    for y in (175..592).step_by(3) {
        for x in (175..592).step_by(3) {
            let anchor = TileCoord::new(x, y);
            let reservation = cell(anchor).any(|tile| reserved(&first, tile));
            let other_reservation = cell(anchor).any(|tile| reserved(&second, tile));
            boundary +=
                usize::from(reservation && !cell(anchor).all(|tile| reserved(&first, tile)));
            let piece = first.cliff_piece_with_cancel(anchor, &|| false).unwrap();
            pieces += usize::from(piece.is_some());
            if reservation {
                for tile in cell(anchor) {
                    assert!(
                        first
                            .cliff_piece_with_cancel(tile, &|| false)
                            .unwrap()
                            .is_none(),
                        "reserved cell {anchor:?}, tile {tile:?}"
                    );
                    assert!(
                        first.sample_base_tile(tile).passable,
                        "reserved cell stays open"
                    );
                }
            }
            if !reservation && !other_reservation {
                assert_eq!(
                    piece,
                    second.cliff_piece_with_cancel(anchor, &|| false).unwrap()
                );
            }
        }
    }
    assert!(boundary > 20 && pieces > 100);
}

#[test]
fn extreme_height_ramps_do_not_overflow_cliff_detection() {
    let generator = plane(-432, 0);
    let ratio = Ratio::new(1, 1).unwrap();
    let mut pieces = 0;
    for y in (601..631).step_by(3) {
        for x in (100..130).step_by(3) {
            let tile = TileCoord::new(x, y);
            let west = x - 1;
            let height =
                |cx: i32, _: i32| Ok::<_, ()>(if cx <= west { i32::MAX } else { i32::MIN });
            pieces += usize::from(
                cliff_relief::cliff_at(
                    &generator,
                    tile,
                    [i32::MAX, i32::MIN, i32::MIN, i32::MAX],
                    ratio,
                    &height,
                )
                .unwrap()
                .is_some(),
            );
        }
    }
    assert!(pieces > 0, "extreme camera-facing contours still classify");
}

#[test]
fn a_flat_tile_inside_a_steep_contour_cell_has_the_same_piece() {
    let generator = plane(-432, 0);
    let ratio = Ratio::new(1, 1).unwrap();
    let mut pieces = 0;
    for y in (601..631).step_by(3) {
        for x in (100..130).step_by(3) {
            let anchor = TileCoord::new(x, y);
            let height = |cx: i32, _: i32| Ok::<_, ()>(if cx <= x - 1 { 900 } else { 0 });
            let steep =
                cliff_relief::cliff_at(&generator, anchor, [900, 0, 0, 900], ratio, &height)
                    .unwrap();
            let flat = cliff_relief::cliff_at(&generator, anchor, [0; 4], ratio, &height).unwrap();
            assert_eq!(steep, flat, "eligibility belongs to the cell");
            pieces += usize::from(steep.is_some());
        }
    }
    assert!(pieces > 0);
}

#[test]
fn slopes_above_35_percent_remain_impassable_and_can_carry_faces() {
    let generator = plane(-1440, 0); // 50% source grade.
    let mut pieces = 0;
    for y in (601..631).step_by(3) {
        for x in (100..400).step_by(3) {
            let tile = TileCoord::new(x, y);
            assert!(is_cliff(generator.sample_base_tile(tile)));
            pieces += usize::from(
                generator
                    .cliff_piece_with_cancel(tile, &|| false)
                    .unwrap()
                    .is_some(),
            );
        }
    }
    assert!(pieces > 100, "steep terrain retains visible contour faces");
}

#[test]
fn one_wet_tile_rejects_the_whole_cliff_cell_in_dense_and_provider_queries() {
    use crate::{
        EnvironmentPage, EnvironmentPageError, EnvironmentPageKey, EnvironmentPageProvider,
        FieldPyramid, PyramidLevel,
    };
    use std::collections::BTreeMap;
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
    for (ocean, inland, expected) in [
        (100, 0, WaterKind::Ocean),
        (0, 100, WaterKind::Lake),
        (0, 30, WaterKind::Shallow),
    ] {
        let mut dense = plane(-432, 0);
        let wet = WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 16,
            height: 16,
            ocean_coverage_percent: (0..256)
                .map(|i| if i % 16 < 4 { ocean } else { 0 })
                .collect(),
            inland_coverage_percent: (0..256)
                .map(|i| if i % 16 < 4 { inland } else { 0 })
                .collect(),
        };
        dense.water = Some(Arc::new(PreparedWater::new(
            16,
            [((0, 0), wet.clone())].into(),
        )));
        let elevation = dense
            .elevation
            .as_ref()
            .unwrap()
            .pages
            .values()
            .next()
            .unwrap()
            .clone();
        let pyramid = |root| FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 16,
                ordered_page_root: root,
            }],
        };
        let environment = PreparedEnvironment {
            samples_per_axis: 16,
            geographic_millimeters_per_sample: 2_880_000,
            elevation: pyramid(crate::ordered_page_root(std::slice::from_ref(&elevation)).unwrap()),
            water: Some(pyramid(
                crate::ordered_water_page_root(std::slice::from_ref(&wet)).unwrap(),
            )),
            ..PreparedEnvironment::default()
        };
        let pages = [
            EnvironmentPage::Elevation(elevation),
            EnvironmentPage::Water(wet),
        ]
        .into_iter()
        .map(|page| (page.key(), Arc::new(page)))
        .collect();
        let mut provider = dense.clone();
        provider.provider = Some(Arc::new(Pages(pages)));
        provider.provider_environment = Some(Arc::new(environment));
        provider.provider_compression = Some(Ratio::new(30, 1).unwrap());
        for generator in [&dense, &provider] {
            let mut mixed = 0;
            for y in (601..661).step_by(3) {
                for x in (160..220).step_by(3) {
                    let anchor = TileCoord::new(x, y);
                    let waters = cell(anchor)
                        .map(|tile| {
                            generator
                                .base_water_with_cancel(tile, &|| false)
                                .unwrap()
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    if waters.contains(&expected) {
                        mixed += usize::from(waters.contains(&WaterKind::None));
                        for tile in cell(anchor) {
                            assert!(
                                generator
                                    .cliff_piece_with_cancel(tile, &|| false)
                                    .unwrap()
                                    .is_none(),
                                "wet cell {anchor:?}, {expected:?}"
                            );
                            let point = generator
                                .landscape_point_with_cancel(tile, &|| false)
                                .unwrap()
                                .unwrap();
                            assert!(
                                !point
                                    .decoration
                                    .is_some_and(|d| d.family == DecorationFamily::Cliff)
                            );
                        }
                    }
                }
            }
            assert!(mixed > 0, "shore fixture crosses cell boundaries");
        }
    }
}
