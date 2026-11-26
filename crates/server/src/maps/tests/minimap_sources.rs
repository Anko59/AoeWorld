//! Synthetic raster sources: prepared pages of chosen layers behind one
//! provider, so that several layers and several pages per layer combine.
use super::rivers::{river_network_fixture, vertical};
use super::*;
use aoe_map::{
    HYDROLOGY_WATER_MODEL_VERSION, HistoricalLandUsePage, HydrologyEvidenceIndex,
    HydrologyEvidencePage, HydrologyKind, HydrologyWaterModelIndex, HydrologyWaterModelPage,
    HydrologyWaterPolicy, ModernLandCoverPage, WORLD_COVER_OBSERVATION_YEAR,
    WaterCorrectionDocument, WaterModelProvenance,
};

pub(super) const MILLIMETERS_PER_TILE: u64 = 60_000;

/// A complete pyramid index; only level zero has pages.
fn pyramid_of(axis: u16) -> FieldPyramid {
    let halves = std::iter::successors(Some(axis), |axis| (*axis > 1).then(|| axis.div_ceil(2)));
    FieldPyramid {
        levels: halves
            .map(|samples_per_axis| PyramidLevel {
                samples_per_axis,
                ordered_page_root: [7; 32],
            })
            .collect(),
    }
}

pub(super) fn relief_environment(axis: u16) -> PreparedEnvironment {
    PreparedEnvironment {
        samples_per_axis: axis,
        geographic_millimeters_per_sample: 1_000_000,
        page_samples: 64,
        elevation: pyramid_of(axis),
        water: Some(pyramid_of(axis)),
        vegetation: Some(pyramid_of(axis)),
        historical_land_use: None,
        hydrology_evidence: None,
    }
}

/// Elevation, water and vegetation pages of 64 samples per side. The closures
/// take source sample coordinates; `ocean` is a coverage percentage.
fn layered_pages(
    axis: u16,
    class: impl Fn(i32, i32) -> u8,
    height: impl Fn(i32, i32) -> i32,
    ocean: impl Fn(i32, i32) -> u8,
) -> Vec<EnvironmentPage> {
    let mut pages = Vec::new();
    for y in 0..axis.div_ceil(64) {
        for x in 0..axis.div_ceil(64) {
            let (columns, rows) = ((axis - x * 64).min(64), (axis - y * 64).min(64));
            let cells = || {
                let (left, top) = (i32::from(x) * 64, i32::from(y) * 64);
                let width = i32::from(columns);
                (0..width * i32::from(rows))
                    .map(move |index| (left + index % width, top + index / width))
            };
            let (width, page_height) = (columns as u8, rows as u8);
            pages.push(EnvironmentPage::Elevation(ElevationPage {
                level: 0,
                x,
                y,
                width,
                height: page_height,
                geographic_height_centimeters: cells().map(|(x, y)| height(x, y)).collect(),
            }));
            pages.push(EnvironmentPage::Water(WaterPage {
                level: 0,
                x,
                y,
                width,
                height: page_height,
                ocean_coverage_percent: cells().map(|(x, y)| ocean(x, y)).collect(),
                inland_coverage_percent: cells().map(|_| 0).collect(),
            }));
            pages.push(EnvironmentPage::Vegetation(PotentialBiomePage {
                level: 0,
                x,
                y,
                width,
                height: page_height,
                potential_biome_class: cells().map(|(x, y)| class(x, y)).collect(),
            }));
        }
    }
    pages
}

/// Dry land of one vegetation class; `height` is centimetres per sample.
pub(super) fn relief_pages(
    axis: u16,
    class: u8,
    height: impl Fn(i32, i32) -> i32,
) -> Vec<EnvironmentPage> {
    layered_pages(axis, |_, _| class, height, |_, _| 0)
}

fn source_over(
    tiles: i32,
    environment: PreparedEnvironment,
    pages: Vec<EnvironmentPage>,
) -> MapChunkGenerator {
    let mut provider = MemoryPageProvider::default();
    for page in pages {
        provider.pages.insert(page.key(), Arc::new(page));
    }
    MapChunkGenerator::new([41; 32], 1, tiles)
        .with_page_provider(
            Ratio::new(30, 1).expect("ratio"),
            environment,
            Arc::new(provider),
        )
        .expect("synthetic source")
}

pub(super) fn relief_source(
    tiles: i32,
    axis: u16,
    class: u8,
    height: impl Fn(i32, i32) -> i32,
) -> MapChunkGenerator {
    let pages = relief_pages(axis, class, height);
    source_over(tiles, relief_environment(axis), pages)
}

/// Ridges and valleys between 0 and 3,600 m: cliffs, scree, bare rock, snow.
pub(super) fn alpine(tiles: i32) -> MapChunkGenerator {
    const WAVE: [i32; 8] = [0, 1, 2, 1, 0, -1, -2, -1];
    relief_source(tiles, 16, 13, |x, y| {
        (1_800 + 450 * WAVE[(x % 8) as usize] + 450 * WAVE[((y + 2) % 8) as usize]) * 100
    })
    .with_latitude(459_000_000, MILLIMETERS_PER_TILE)
}

/// Flat dry grassland at 300 m, where loose rock piles grow.
pub(super) fn steppe(tiles: i32) -> MapChunkGenerator {
    relief_source(tiles, 16, 20, |_, _| 30_000)
}

/// The narrowest drawn river, a stream too narrow to draw and a main river;
/// the last two run through sample centres of a 128-sample raster.
pub(super) fn network() -> MapChunkGenerator {
    river_network_fixture(false, vec![vertical(312, 3), vertical(1_000, 8)], 2)
}

/// Elevation and vegetation of two by two pages (axis 128), each page with its
/// own biome.
pub(super) fn mosaic(tiles: i32) -> MapChunkGenerator {
    const CLASSES: [u8; 4] = [13, 20, 27, 14];
    let pages = layered_pages(
        128,
        |x, y| CLASSES[(y / 64 * 2 + x / 64) as usize],
        |x, y| 20_000 + 900 * ((x + 2 * y) % 40 - 20).abs(),
        |_, _| 0,
    );
    source_over(tiles, relief_environment(128), pages)
}

/// Boreal country. Farmed, half of its west is fields and half of its east is
/// pasture, which lowers every cell's wildness and so clears the forests.
pub(super) fn country(tiles: i32, farmed: bool) -> MapChunkGenerator {
    let mut environment = relief_environment(16);
    let mut pages = relief_pages(16, 14, |_, _| 20_000);
    if farmed {
        let share = |west: u8, east: u8| {
            (0..16 * 16)
                .map(|index| if index % 16 < 8 { west } else { east })
                .collect::<Vec<_>>()
        };
        environment.historical_land_use = Some(pyramid_of(16));
        pages.push(EnvironmentPage::HistoricalLandUse(HistoricalLandUsePage {
            level: 0,
            x: 0,
            y: 0,
            width: 16,
            height: 16,
            crop_percent: share(50, 0),
            grazing_percent: share(0, 50),
            population_pressure_per_square_kilometer: vec![0; 16 * 16],
            coverage: Vec::new(),
        }));
    }
    source_over(tiles, environment, pages)
}

/// A water model over four by four evidence cells: a modelled lake along the
/// north and one cited land correction, which dries mapped ocean.
pub(super) fn modelled(tiles: i32) -> MapChunkGenerator {
    const CELLS: usize = 16;
    const CORRECTED: usize = 5;
    let mut environment = relief_environment(16);
    environment.hydrology_evidence = Some(HydrologyEvidenceIndex {
        samples_per_axis: 4,
        page_samples: 64,
        world_cover_year: WORLD_COVER_OBSERVATION_YEAR,
        policy: HydrologyWaterPolicy::HistoricalOverviewWithMappedNaturalWaterV1,
        hydrology_page_root: [3; 32],
        modern_land_cover_page_root: [4; 32],
        water_model: Some(HydrologyWaterModelIndex {
            model_version: HYDROLOGY_WATER_MODEL_VERSION,
            samples_per_axis: 4,
            target_year_ce: 600,
            correction_document: WaterCorrectionDocument::empty(MapRequest::default(), 4)
                .expect("correction document"),
        }),
    });
    let mut model = HydrologyWaterModelPage {
        kind: vec![HydrologyKind::NoEvidence as u8; CELLS],
        surface_level_centimeters: vec![None; CELLS],
        flow_direction: vec![0; CELLS],
        provenance: vec![WaterModelProvenance::EvidenceOnly as u8; CELLS],
    };
    for cell in 0..4 {
        model.kind[cell] = HydrologyKind::Lake as u8;
        model.surface_level_centimeters[cell] = Some(30_000);
        model.provenance[cell] = WaterModelProvenance::ModelledLakeSurface as u8;
    }
    model.kind[CORRECTED] = HydrologyKind::Land as u8;
    model.provenance[CORRECTED] = WaterModelProvenance::GeographicCorrection as u8;
    let dried = |at: i32| (4..8).contains(&at);
    let mut pages = layered_pages(
        16,
        |_, _| 14,
        |_, _| 30_000,
        |x, y| if dried(x) && dried(y) { 100 } else { 0 },
    );
    pages.push(EnvironmentPage::HydrologyEvidence(HydrologyEvidencePage {
        level: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        kind: vec![HydrologyKind::NoEvidence as u8; CELLS],
        method: vec![0; CELLS],
        river_segments: Vec::new(),
        water_model: Some(model),
    }));
    pages.push(EnvironmentPage::ModernLandCover(ModernLandCoverPage {
        level: 0,
        x: 0,
        y: 0,
        width: 4,
        height: 4,
        worldcover_class: vec![0; CELLS],
    }));
    source_over(tiles, environment, pages)
}
