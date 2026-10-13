use super::*;

#[test]
fn straight_source_channels_meander_in_world_space_without_grid_alignment() {
    for width in [3, 6, 10] {
        let generator = crossing_with_geometry(
            false,
            false,
            vec![[
                128 * 256,
                -256,
                128 * 256,
                258 * 256,
                width * 256,
                0,
                259 * 256,
            ]],
        );
        let mut centres = std::collections::BTreeSet::new();
        for y in 0..WIDTH {
            let wet = (119..138)
                .filter(|x| {
                    generator.tile_at(TileCoord::new(*x, y)).unwrap().water == WaterKind::River
                })
                .collect::<Vec<_>>();
            assert!(!wet.is_empty(), "cut at row {y}");
            assert!(wet.windows(2).all(|p| p[1] == p[0] + 1), "split channel");
            assert!(
                (width as usize - 1..=width as usize + 1).contains(&wet.len()),
                "width {width}: {wet:?}"
            );
            centres.insert(wet[0] + wet[wet.len() - 1]);
            for x in wet {
                assert!((x - 128).abs() <= width as i32 / 2 + 3, "unbounded warp");
            }
        }
        assert!(
            centres.len() >= 4,
            "straight source stayed ruler-straight: {centres:?}"
        );
        for y in [31, 32, 63, 64, 127, 128] {
            let chunk = generator.chunk(4, y / 32).unwrap();
            for x in 128..137 {
                assert_eq!(
                    generator.tile_at(TileCoord::new(x, y)).unwrap(),
                    chunk.tiles[((y % 32) * 32 + x % 32) as usize]
                );
            }
        }
    }
}

#[test]
fn source_corner_rounding_moves_the_channel_toward_the_inside_of_a_bend() {
    // Aggregate several geography keys so symmetric warp cannot masquerade as
    // deliberate centreline rounding. A right angle should cut its inside.
    let mut inside = 0;
    for key in 1..=24 {
        let mut generator = crossing_with_geometry(
            false,
            false,
            vec![
                [
                    128 * 256,
                    64 * 256,
                    128 * 256,
                    128 * 256,
                    3 * 256,
                    0,
                    64 * 256,
                ],
                [
                    128 * 256,
                    128 * 256,
                    192 * 256,
                    128 * 256,
                    3 * 256,
                    64 * 256,
                    128 * 256,
                ],
            ],
        );
        generator.geography_key = [key; 32];
        inside += i32::from(
            generator.tile_at(TileCoord::new(130, 125)).unwrap().water == WaterKind::River,
        );
    }
    assert!(
        inside >= 8,
        "sharp source corner was not rounded: {inside}/24"
    );
}

// The review point is the centre of tile (511, 255). The unwarped complete
// curve has minimum squared distance 315274, outside half-width squared 147456.
// Test page discovery and actual composed water as well as bend shaping.
#[test]
fn page_clipped_bend_has_the_same_physical_curve_as_the_complete_source() {
    let incoming = crate::RiverSegment {
        start: [448 * 256, 256 * 256],
        end: [512 * 256, 256 * 256],
        width_units: 3 * 256,
        station_start: 128 * 256,
        station_end: 64 * 256,
        source_delta: [16384, 0],
        source_station_delta: -16384,
    };
    let outgoing = crate::RiverSegment {
        start: incoming.end,
        end: [512 * 256, 320 * 256],
        source_delta: [0, 16384],
        station_start: 64 * 256,
        station_end: 0,
        ..incoming
    };
    let clipped = crate::RiverSegment {
        start: [507 * 256, 256 * 256],
        station_start: 69 * 256,
        ..incoming
    };
    let generator = |key, duplicate| {
        let (mut environment, mut pages) = environment(128, 8, &|_, _| (0, 0));
        for (x, y, width, height) in super::super::shoreline::page_grid(128) {
            let len = usize::from(width) * usize::from(height);
            let page = EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
                level: 0,
                x,
                y,
                width,
                height,
                kind: vec![crate::HydrologyKind::NoEvidence as u8; len],
                method: vec![0; len],
                water_model: None,
                river_segments: if y == 0 && x == 0 {
                    vec![incoming, outgoing]
                } else if y == 0 && x == 1 && duplicate {
                    vec![clipped, outgoing]
                } else {
                    vec![]
                },
            });
            pages.0.insert(page.key(), Arc::new(page));
            let page = EnvironmentPage::ModernLandCover(crate::ModernLandCoverPage {
                level: 0,
                x,
                y,
                width,
                height,
                worldcover_class: vec![0; len],
            });
            pages.0.insert(page.key(), Arc::new(page));
        }
        environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
            samples_per_axis: 128,
            page_samples: 64,
            world_cover_year: 2021,
            policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
            hydrology_page_root: [3; 32],
            modern_land_cover_page_root: [4; 32],
            water_model: None,
        });
        MapChunkGenerator::new([key; 32], 1, 1024)
            .with_page_provider(
                crate::Ratio::new(30, 1).unwrap(),
                environment,
                Arc::new(pages),
            )
            .unwrap()
    };
    let review_tile = TileCoord::new(511, 255);
    assert_eq!(
        [
            i64::from(review_tile.x) * 256 + 128,
            i64::from(review_tile.y) * 256 + 128
        ],
        [130944, 65408]
    );
    for key in 1..=24 {
        let full = generator(key, false);
        let split = generator(key, true);
        for position in std::iter::once(review_tile)
            .chain((252..=260).flat_map(|y| (508..=516).map(move |x| TileCoord::new(x, y))))
        {
            assert_eq!(
                split.tile_at(position).unwrap().water,
                full.tile_at(position).unwrap().water,
                "page clipping changed physical water: key {key}, {position:?}"
            );
        }
    }
}
