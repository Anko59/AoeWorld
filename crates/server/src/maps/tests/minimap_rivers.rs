use super::*;
use aoe_map::{Provenance, WaterKind};

fn river_fixture(corrected: bool) -> MapChunkGenerator {
    river_network_fixture(corrected, Vec::new())
}

fn river_network_fixture(corrected: bool, extra: Vec<aoe_map::RiverSegment>) -> MapChunkGenerator {
    use aoe_map::{
        HydrologyEvidenceIndex, HydrologyEvidencePage, HydrologyWaterPolicy, ModernLandCoverPage,
        RiverSegment,
    };
    let (package, _) = coast_forest_mountain();
    let mut environment = package.environment.clone();
    let correction_document = aoe_map::WaterCorrectionDocument::new(
        MapRequest {
            requested_side_meters: 122_880,
            ..package.request
        },
        2,
        if corrected {
            vec![aoe_map::GeographicWaterPatch {
                id: "dry-lower-reach".to_owned(),
                precedence: 1,
                applies_from_year_ce: 500,
                applies_through_year_ce: 700,
                source_citation: "curated synthetic historical source for this regression"
                    .to_owned(),
                operation: aoe_map::WaterCorrectionOperation::SetLand,
                polygon: [
                    (240_000_000, 609_000_000),
                    (255_000_000, 609_000_000),
                    (255_000_000, 615_000_000),
                    (240_000_000, 615_000_000),
                ]
                .into_iter()
                .map(
                    |(longitude_e7, latitude_e7)| aoe_map::WaterCorrectionVertex {
                        longitude_e7,
                        latitude_e7,
                    },
                )
                .collect(),
            }]
        } else {
            vec![]
        },
    )
    .expect("supported cited correction");
    let model = corrected.then(|| aoe_map::HydrologyWaterModelPage {
        kind: vec![
            aoe_map::HydrologyKind::NoEvidence as u8,
            aoe_map::HydrologyKind::NoEvidence as u8,
            aoe_map::HydrologyKind::Land as u8,
            aoe_map::HydrologyKind::NoEvidence as u8,
        ],
        surface_level_centimeters: vec![None; 4],
        flow_direction: vec![0; 4],
        provenance: vec![
            0,
            0,
            aoe_map::WaterModelProvenance::GeographicCorrection as u8,
            0,
        ],
    });
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 2,
        page_samples: 64,
        world_cover_year: 2021,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [3; 32],
        modern_land_cover_page_root: [4; 32],
        water_model: corrected.then_some(aoe_map::HydrologyWaterModelIndex {
            model_version: 1,
            samples_per_axis: 2,
            target_year_ce: 600,
            correction_document,
        }),
    });
    let pages = [
        EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            geographic_height_centimeters: vec![0; 16],
        }),
        EnvironmentPage::Water(WaterPage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            ocean_coverage_percent: vec![0; 16],
            inland_coverage_percent: vec![0; 16],
        }),
        EnvironmentPage::Vegetation(PotentialBiomePage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            potential_biome_class: vec![2; 16],
        }),
        EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            kind: vec![aoe_map::HydrologyKind::NoEvidence as u8; 4],
            method: vec![0; 4],
            water_model: model,
            river_segments: std::iter::once(RiverSegment {
                start: [64 * 256, 0],
                end: [64 * 256, 2048 * 256],
                width_units: 3 * 256,
                station_start: 0,
                station_end: 2048 * 256,
                source_delta: [0, 524288],
                source_station_delta: 524288,
            })
            .chain(extra)
            .collect(),
        }),
        EnvironmentPage::ModernLandCover(ModernLandCoverPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            worldcover_class: vec![0; 4],
        }),
    ];
    let mut provider = MemoryPageProvider::default();
    for page in pages {
        provider.pages.insert(page.key(), Arc::new(page));
    }
    MapChunkGenerator::new([7; 32], 1, 2048)
        .with_page_provider(
            Ratio::new(30, 1).expect("ratio"),
            environment,
            Arc::new(provider),
        )
        .expect("river provider")
}

#[test]
fn subpixel_source_river_remains_continuous_in_the_minimap() {
    let generator = river_fixture(false);
    let center = TileCoord::new(66, 1026);
    assert_eq!(
        generator
            .tile_at_with_cancel(center, &|| false)
            .expect("point")
            .expect("tile")
            .water,
        WaterKind::None
    );
    let raster = minimap_raster(&generator, 2048, 60000, 4, &|| false).expect("strategic river");
    for y in 0..512 {
        assert_eq!(
            raster[MINIMAP_HEADER_BYTES + y * 512 + 16] & 15,
            MinimapClass::River as u8,
            "row {y}"
        );
    }
}

#[test]
fn cited_set_land_reach_is_not_redrawn_by_the_minimap_footprint() {
    let generator = river_fixture(true);
    let raster = minimap_raster(&generator, 2048, 60000, 4, &|| false).expect("corrected raster");
    for row in [256, 300, 400, 511] {
        let position = TileCoord::new(66, row * 4 + 2);
        assert!(
            generator
                .river_within_with_cancel(position, 3, &|| false)
                .unwrap()
        );
        let point = generator
            .landscape_point_with_cancel(position, &|| false)
            .unwrap()
            .unwrap();
        assert_eq!(point.tile.terrain.water, WaterKind::None);
        assert_eq!(
            point.tile.terrain.water_provenance,
            Provenance::HistoricallyCorrected
        );
        assert_ne!(classify(&point), MinimapClass::River);
        assert_eq!(
            raster[MINIMAP_HEADER_BYTES + row as usize * 512 + 16] & 15,
            classify(&point) as u8,
            "corrected reach row {row}"
        );
    }
    assert_eq!(
        raster[MINIMAP_HEADER_BYTES + 100 * 512 + 16] & 15,
        MinimapClass::River as u8,
        "the uncorrected subpixel reach must remain visible"
    );
}

fn vertical(tile_x: i64, width_tiles: i64) -> aoe_map::RiverSegment {
    aoe_map::RiverSegment {
        start: [tile_x * 256, 0],
        end: [tile_x * 256, 2048 * 256],
        width_units: (width_tiles * 256) as u16,
        station_start: 0,
        station_end: 2048 * 256,
        source_delta: [0, 524288],
        source_station_delta: 524288,
    }
}

#[test]
fn minimap_draws_rivers_by_importance_not_every_stream() {
    // The fixture's own three-tile stream at x = 64, sixty more three-tile
    // streams, and one main river eight tiles wide.
    let mut network = (0..60)
        .map(|i| vertical(140 + 30 * i, 3))
        .collect::<Vec<_>>();
    network.push(vertical(1000, 8));
    let generator = river_network_fixture(false, network);
    let raster = minimap_raster(&generator, 2048, 60000, 4, &|| false).expect("network raster");
    let pixels = &raster[MINIMAP_HEADER_BYTES..];
    let water = pixels
        .iter()
        .filter(|p| **p & 15 == MinimapClass::River as u8)
        .count();
    assert!(
        water * 100 < pixels.len() * 5,
        "{water} of {} pixels are water: small streams must not flood the map",
        pixels.len()
    );
    for row in 0..512 {
        assert_eq!(
            pixels[row * 512 + 250] & 15,
            MinimapClass::River as u8,
            "main river, row {row}"
        );
    }
}
