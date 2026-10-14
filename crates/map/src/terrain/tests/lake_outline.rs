//! Fine lake evidence must own its outline even over coarse inland coverage.
use super::shoreline::{Pages, bind, environment, page_grid};
use super::*;
use crate::{
    EnvironmentPage, HydrologyEvidenceIndex, HydrologyEvidenceMethod, HydrologyEvidencePage,
    HydrologyKind, HydrologyWaterPolicy,
};

fn polygon(x: u16, y: u16) -> bool {
    // Stepped headland, a six-sample bay and a five-sample island.
    (8..=48).contains(&x)
        && (6..=54).contains(&y)
        && !(x >= 38 && (20..26).contains(&y))
        && !((22..27).contains(&x) && (32..37).contains(&y))
        && !(x < 16 && y < 15)
}

fn lake_with_coarse_coverage() -> MapChunkGenerator {
    let (mut environment, mut pages) = environment(2, 8, &|_, _| (0, 100));
    for (x, y, width, height) in page_grid(65) {
        let wet = (0..u16::from(height))
            .flat_map(|dy| (0..u16::from(width)).map(move |dx| polygon(x * 64 + dx, y * 64 + dy)))
            .collect::<Vec<_>>();
        let page = EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
            level: 0, x, y, width, height, river_segments: Vec::new(),
            kind: wet.iter().map(|wet| if *wet { HydrologyKind::Lake } else { HydrologyKind::NoEvidence } as u8).collect(),
            method: wet.iter().map(|wet| if *wet { HydrologyEvidenceMethod::HydroLakesExtent } else { HydrologyEvidenceMethod::None } as u8).collect(),
            water_model: None,
        });
        pages.0.insert(page.key(), Arc::new(page));
    }
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 65,
        page_samples: 64,
        world_cover_year: 2021,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [3; 32],
        modern_land_cover_page_root: [4; 32],
        water_model: None,
    });
    bind(17, environment, Pages(pages.0))
}

#[test]
fn fine_lake_mask_keeps_bay_headland_and_island_over_coarse_water() {
    let generator = lake_with_coarse_coverage();
    for sy in 0..65_u16 {
        for sx in 0..65_u16 {
            let tile = TileCoord::new(i32::from(sx) * 4, i32::from(sy) * 4);
            let water = provider::water::water_kind(&generator, tile, &|| false).unwrap();
            assert_eq!(
                water == WaterKind::Lake,
                polygon(sx, sy),
                "sample {sx},{sy}"
            );
        }
    }
    for (sx, sy) in [(42, 23), (24, 34), (11, 10)] {
        for dy in -1..=1 {
            for dx in -1..=1 {
                assert_eq!(
                    provider::water::water_kind(
                        &generator,
                        TileCoord::new(sx * 4 + dx, sy * 4 + dy),
                        &|| false
                    )
                    .unwrap(),
                    WaterKind::None
                );
            }
        }
    }
}

#[test]
fn coarse_inland_coverage_cannot_reach_into_a_dry_source_bay() {
    let generator = lake_with_coarse_coverage();
    let bay = TileCoord::new(42 * 4, 23 * 4);
    assert!(!provider::water::water_near(&generator, bay, 1, &|| false).unwrap());
    assert!(
        provider::water::water_near(&generator, TileCoord::new(30 * 4, 30 * 4), 1, &|| false)
            .unwrap()
    );
}

#[test]
fn overview_water_stays_available_when_no_mapped_hydrology_exists() {
    for (ocean, inland, expected) in [(100, 0, WaterKind::Ocean), (0, 100, WaterKind::Lake)] {
        let (environment, pages) = environment(2, 8, &|_, _| (ocean, inland));
        let generator = bind(17, environment, pages);
        assert_eq!(
            provider::water::water_kind(&generator, TileCoord::new(100, 100), &|| false).unwrap(),
            expected
        );
    }
}
