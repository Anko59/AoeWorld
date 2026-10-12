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
