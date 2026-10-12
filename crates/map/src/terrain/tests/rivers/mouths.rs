use super::*;
use crate::{
    HydrologyKind, HydrologyWaterModelIndex, HydrologyWaterModelPage, WaterModelProvenance,
};

#[test]
fn river_mouths_keep_lake_and_sea_priority_with_and_without_water_models() {
    for (kind, expected) in [
        (HydrologyKind::Lake, WaterKind::Lake),
        (HydrologyKind::Ocean, WaterKind::Ocean),
    ] {
        for (modeled, categorical) in [(false, false), (false, true), (true, false), (true, true)] {
            let coverage = |_: u16, y: u16| {
                if y < 20 || !categorical {
                    (0, 0)
                } else if kind == HydrologyKind::Ocean {
                    (100, 0)
                } else {
                    (0, 100)
                }
            };
            let (mut environment, mut pages) = environment(33, 8, &coverage);
            let kinds = (0..33)
                .flat_map(|y| {
                    std::iter::repeat_n(
                        if y < 20 {
                            HydrologyKind::NoEvidence as u8
                        } else {
                            kind as u8
                        },
                        33,
                    )
                })
                .collect::<Vec<_>>();
            let methods = kinds
                .iter()
                .map(|&k| match HydrologyKind::try_from(k).unwrap() {
                    HydrologyKind::Ocean => crate::HydrologyEvidenceMethod::OverviewOcean as u8,
                    HydrologyKind::Lake => crate::HydrologyEvidenceMethod::HydroLakesExtent as u8,
                    _ => crate::HydrologyEvidenceMethod::None as u8,
                })
                .collect();
            let water_model = modeled.then(|| HydrologyWaterModelPage {
                kind: kinds.clone(),
                surface_level_centimeters: kinds
                    .iter()
                    .map(|&k| (k == kind as u8).then_some(0))
                    .collect(),
                flow_direction: vec![0; 1089],
                provenance: kinds
                    .iter()
                    .map(|&k| {
                        if k != kind as u8 {
                            WaterModelProvenance::EvidenceOnly as u8
                        } else if kind == HydrologyKind::Ocean {
                            WaterModelProvenance::ModelledOceanSurface as u8
                        } else {
                            WaterModelProvenance::ModelledLakeSurface as u8
                        }
                    })
                    .collect(),
            });
            let page = HydrologyEvidencePage {
                level: 0,
                x: 0,
                y: 0,
                width: 33,
                height: 33,
                kind: kinds,
                method: methods,
                water_model,
                river_segments: vec![crate::RiverSegment {
                    start: [129 * 256, 0],
                    end: [129 * 256, 257 * 256],
                    width_units: 6 * 256,
                    station_start: 0,
                    station_end: 257 * 256,
                }],
            };
            let root = crate::ordered_hydrology_page_root(std::slice::from_ref(&page)).unwrap();
            let page = EnvironmentPage::HydrologyEvidence(page);
            pages.0.insert(page.key(), Arc::new(page));
            let cover = crate::ModernLandCoverPage {
                level: 0,
                x: 0,
                y: 0,
                width: 33,
                height: 33,
                worldcover_class: vec![0; 1089],
            };
            let cover_root =
                crate::ordered_modern_land_cover_page_root(std::slice::from_ref(&cover)).unwrap();
            let cover = EnvironmentPage::ModernLandCover(cover);
            pages.0.insert(cover.key(), Arc::new(cover));
            environment.hydrology_evidence = Some(crate::HydrologyEvidenceIndex {
                samples_per_axis: 33,
                page_samples: 64,
                world_cover_year: 2021,
                policy: crate::HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
                hydrology_page_root: root,
                modern_land_cover_page_root: cover_root,
                water_model: modeled.then(|| HydrologyWaterModelIndex {
                    model_version: 1,
                    samples_per_axis: 33,
                    target_year_ce: 600,
                    correction_document: crate::WaterCorrectionDocument::empty(
                        crate::MapRequest::default(),
                        33,
                    )
                    .unwrap(),
                }),
            });
            let generator = bind(91, environment, pages);
            assert_eq!(
                generator.tile_at(TileCoord::new(129, 80)).unwrap().water,
                WaterKind::River
            );
            let composed_tiles = composed(&generator);
            let component = super::super::shoreline::main_water(&composed_tiles);
            assert!(
                component[(80 * WIDTH + 129) as usize],
                "upstream channel must join {kind:?}, model={modeled}, categorical={categorical}"
            );
            for y in [190, 220, 250] {
                let tile = TileCoord::new(129, y);
                let sample = generator.tile_at(tile).unwrap();
                assert_eq!(
                    sample.water, expected,
                    "{kind:?}, model={modeled}, {tile:?}"
                );
                assert_ne!(sample.material, GroundMaterial::Ford);
                let chunk = generator
                    .landscape_chunk_with_cancel(tile.x / 32, tile.y / 32, &|| false)
                    .unwrap();
                assert_eq!(
                    chunk
                        .tiles
                        .iter()
                        .find(|t| t.tile == tile)
                        .unwrap()
                        .terrain
                        .water,
                    expected
                );
            }
        }
    }
}
