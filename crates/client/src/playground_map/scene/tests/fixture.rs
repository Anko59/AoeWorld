//! Public-API equivalent of map's mountains::plane fixture; no recorded output.
use aoe_core::TileCoord;
use aoe_map::*;

pub(super) fn plane(x: i32, y: i32) -> MapChunkGenerator {
    let pages: Vec<_> = (0..5)
        .map(|level| {
            let side = 16 >> level;
            ElevationPage {
                level,
                x: 0,
                y: 0,
                width: side,
                height: side,
                geographic_height_centimeters: (0..i32::from(side).pow(2))
                    .map(|i| {
                        let scale = 1 << level;
                        (3000
                            + (i % i32::from(side) * scale - 8) * x
                            + (i / i32::from(side) * scale - 8) * y)
                            * 100
                    })
                    .collect(),
            }
        })
        .collect();
    let vegetation = PotentialBiomePage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        potential_biome_class: vec![13],
    };
    let water = WaterPage {
        level: 0,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        ocean_coverage_percent: vec![0],
        inland_coverage_percent: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 16,
        geographic_millimeters_per_sample: 2_880_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: pages
                .iter()
                .map(|p| PyramidLevel {
                    samples_per_axis: u16::from(p.width),
                    ordered_page_root: ordered_page_root(std::slice::from_ref(p)).unwrap(),
                })
                .collect(),
        },
        vegetation: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&vegetation))
                    .unwrap(),
            }],
        }),
        water: Some(FieldPyramid {
            levels: vec![PyramidLevel {
                samples_per_axis: 1,
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(&water)).unwrap(),
            }],
        }),
        ..Default::default()
    };
    MapChunkGenerator::new([41; 32], 1, 768)
        .with_prepared_elevation(Ratio::new(30, 1).unwrap(), &environment, pages)
        .unwrap()
        .with_prepared_biomes(&environment, vec![vegetation])
        .unwrap()
        .with_prepared_water(&environment, vec![water])
        .unwrap()
}

pub(super) fn region(
    generator: &MapChunkGenerator,
    xs: std::ops::Range<i32>,
    ys: std::ops::Range<i32>,
) -> Vec<super::super::SceneTerrain> {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .map(|(x, y)| {
            let at = TileCoord::new(x, y);
            let p = generator
                .landscape_point_with_cancel(at, &|| false)
                .unwrap()
                .unwrap();
            super::super::terrain_scene_sample(at, &p.tile.terrain, p.tile.appearance)
        })
        .collect()
}
