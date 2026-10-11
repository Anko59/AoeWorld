//! The real-world forest signal and the vegetation nodata fill behind it.
use super::super::wildness::probe_wildness;
use super::*;
use crate::biome::PreparedBiome;
use crate::{Biome, PotentialBiomePage, Provenance};

fn observed(crop: u8, grazing: u8, people: u16) -> Option<HistoricalLandUseObservation> {
    Some(HistoricalLandUseObservation {
        crop_percent: crop,
        grazing_percent: grazing,
        population_pressure: people,
        coverage: None,
    })
}

#[test]
fn wildness_falls_with_cleared_land_and_population() {
    assert_eq!(probe_wildness(Biome::Temperate, observed(0, 0, 0), 0), 1000);
    assert_eq!(probe_wildness(Biome::Boreal, observed(100, 0, 0), 0), 0);
    assert_eq!(probe_wildness(Biome::Boreal, observed(60, 60, 0), 0), 0);
    let mut previous = 1001;
    for cleared in 0..=100 {
        let wildness = probe_wildness(Biome::Temperate, observed(cleared, 0, 0), 300);
        assert!(wildness < previous || wildness == 0, "cleared={cleared}");
        assert_eq!(
            wildness,
            probe_wildness(Biome::Temperate, observed(0, cleared, 0), -300),
            "grazing clears like crops and observed history ignores variation"
        );
        previous = wildness;
    }
    // The year-600 French range: the Paris basin against the Vosges.
    assert!(probe_wildness(Biome::Temperate, observed(30, 5, 20), 0) < 150);
    assert!(probe_wildness(Biome::Temperate, observed(2, 1, 4), 0) > 700);
    assert!(
        probe_wildness(Biome::Temperate, observed(5, 0, 40), 0)
            < probe_wildness(Biome::Temperate, observed(5, 0, 0), 0)
    );
}

#[test]
fn potential_vegetation_bounds_wildness_and_missing_history_keeps_biome_defaults() {
    let wild = observed(0, 0, 0);
    assert_eq!(probe_wildness(Biome::Woodland, wild, 0), 550);
    for biome in [
        Biome::Steppe,
        Biome::Desert,
        Biome::Tundra,
        Biome::Alpine,
        Biome::Polar,
    ] {
        assert_eq!(probe_wildness(biome, wild, 300), 0);
        assert_eq!(probe_wildness(biome, None, 300), 0);
    }
    assert_eq!(probe_wildness(Biome::Temperate, None, 0), 500);
    assert_eq!(probe_wildness(Biome::Temperate, None, -300), 200);
    assert_eq!(probe_wildness(Biome::Boreal, None, 300), 1000);
    assert_eq!(probe_wildness(Biome::Tropical, None, 0), 850);
    assert_eq!(probe_wildness(Biome::Savanna, None, -300), 1000);
    // Source nodata is not a measured zero: it falls back like no history.
    let nodata = Some(HistoricalLandUseObservation {
        coverage: Some(HistoricalCoverage {
            nodata_percent: 100,
            ..HistoricalCoverage::default()
        }),
        ..observed(90, 0, 0).unwrap()
    });
    assert_eq!(probe_wildness(Biome::Temperate, nodata, 0), 500);
}

/// Temperate potential forest everywhere; the west half of the map is
/// year-600 farmland (45% crop, 10% grazing), the east half untouched.
fn split_country(width: i32) -> MapChunkGenerator {
    let mut generator = MapChunkGenerator::new([41; 32], 9, width);
    generator.biome = Some(Arc::new(PreparedBiome::new(
        1,
        [(
            (0, 0),
            PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![13],
            },
        )]
        .into(),
    )));
    let page = HistoricalLandUsePage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        crop_percent: vec![45, 0, 45, 0],
        grazing_percent: vec![10, 0, 10, 0],
        population_pressure_per_square_kilometer: vec![20, 0, 20, 0],
        coverage: Vec::new(),
    };
    generator.historical_land_use =
        Some(Arc::new(HistoricalLandUse::new(2, [((0, 0), page)].into())));
    generator
}

#[test]
fn farmed_country_is_open_and_untouched_country_holds_large_forests() {
    let generator = split_country(1_536);
    assert!(generator.landscape_cell_wildness(1, 3, &|| false).unwrap() < 60);
    assert_eq!(
        generator.landscape_cell_wildness(6, 3, &|| false).unwrap(),
        1000
    );
    assert_eq!(
        generator.landscape_cell_wildness(1, 3, &|| true),
        Err(EnvironmentPageError::Cancelled)
    );
    // Cells outside the map have no probe and therefore no forest signal.
    assert_eq!(
        generator.landscape_cell_wildness(-2, 3, &|| false).unwrap(),
        0
    );
    let (mut canopy, mut openings) = ([0usize; 2], [0usize; 2]);
    for y in (384..1_152).step_by(3) {
        for x in (0..1_536).step_by(3) {
            if (576..960).contains(&x) {
                continue;
            }
            let side = usize::from(x >= 960);
            let point = generator
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .unwrap()
                .unwrap();
            canopy[side] += usize::from(point.tile.appearance.canopy_strength > 0);
            let sample = generator
                .evaluate_landscape_with_cancel(
                    TileCoord::new(x, y),
                    crate::LandscapePolicy {
                        wildness_per_thousand: None,
                        support_per_thousand: 1000,
                    },
                    &|_| crate::landscape_ecology::Reservations::default(),
                    &|| false,
                )
                .unwrap()
                .unwrap();
            openings[side] +=
                usize::from(sample.density.opening != crate::landscape_patches::Opening::None);
        }
    }
    let samples = 256 * 192;
    eprintln!("canopy={canopy:?} openings={openings:?} of {samples}");
    assert!(canopy[0] * 100 <= 3 * samples, "farmland stays open");
    assert!(
        canopy[1] * 100 >= 35 * samples,
        "untouched country is forested"
    );
    assert_eq!(openings[0], 0, "no glade or path outside large forests");
    assert!(openings[1] > 500, "large forests carry glades and paths");
}

#[test]
fn vegetation_nodata_takes_the_nearest_class_instead_of_biome_noise_squares() {
    let field = |classes: Vec<u8>| {
        let mut generator = MapChunkGenerator::new([41; 32], 9, 512);
        generator.biome = Some(Arc::new(PreparedBiome::new(
            4,
            [(
                (0, 0),
                PotentialBiomePage {
                    level: 0,
                    x: 0,
                    y: 0,
                    width: 4,
                    height: 4,
                    potential_biome_class: classes,
                },
            )]
            .into(),
        )));
        generator
    };
    // Boreal forest (class 15) in the west column, nodata everywhere else:
    // the situation beside Finnish lakes.
    let mut classes = vec![0; 16];
    for row in 0..4 {
        classes[row * 4] = 15;
    }
    let lakeside = field(classes);
    let mut filled = 0;
    for y in (0..512).step_by(7) {
        for x in (0..512).step_by(7) {
            let tile = lakeside.sample_base_tile(TileCoord::new(x, y));
            assert_eq!(tile.biome, Biome::Boreal, "({x},{y})");
            filled += usize::from(tile.vegetation_provenance == Provenance::Fallback);
        }
    }
    assert!(filled > 3_000, "filled samples keep fallback provenance");
    // With no usable class anywhere the seeded no-source biomes remain.
    let empty = field(vec![0; 16]);
    let biomes: std::collections::BTreeSet<u8> = (0..512)
        .step_by(32)
        .map(|x| empty.sample_base_tile(TileCoord::new(x, 100)).biome as u8)
        .collect();
    assert!(biomes.len() > 3);
}
