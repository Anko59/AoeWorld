use super::{Stage, nearest_page, reduce_elevation_page};
use aoe_map::{
    ElevationPage, HistoricalLandUsePage, PageLayer, PageRootBuilder, PotentialBiomePage, WaterPage,
};

const SOURCE_AXIS: u16 = 129;
const NEXT_AXIS: u16 = 65;
const PAGE: u16 = 64;

fn dimensions(x: u16, y: u16) -> (u8, u8) {
    (
        (SOURCE_AXIS - x * PAGE).min(PAGE) as u8,
        (SOURCE_AXIS - y * PAGE).min(PAGE) as u8,
    )
}

fn elevation_page(x: u16, y: u16) -> ElevationPage {
    let (width, height) = dimensions(x, y);
    let mut values = Vec::with_capacity(usize::from(width) * usize::from(height));
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            let global_x = x * PAGE + column;
            let global_y = y * PAGE + row;
            values.push(i32::from(global_y) * 1_000 + i32::from(global_x));
        }
    }
    ElevationPage {
        level: 0,
        x,
        y,
        width,
        height,
        geographic_height_centimeters: values,
    }
}

fn water_page(x: u16, y: u16) -> WaterPage {
    let (width, height) = dimensions(x, y);
    let mut ocean = Vec::new();
    let mut inland = Vec::new();
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            ocean.push(((y * PAGE + row) % 101) as u8);
            inland.push(((x * PAGE + column) % 101) as u8);
        }
    }
    WaterPage {
        level: 0,
        x,
        y,
        width,
        height,
        ocean_coverage_percent: ocean,
        inland_coverage_percent: inland,
    }
}

fn vegetation_page(x: u16, y: u16) -> PotentialBiomePage {
    let (width, height) = dimensions(x, y);
    let mut classes = Vec::new();
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            classes.push(((x * PAGE + column + y * PAGE + row) % 255) as u8);
        }
    }
    PotentialBiomePage {
        level: 0,
        x,
        y,
        width,
        height,
        potential_biome_class: classes,
    }
}

fn historical_page(x: u16, y: u16) -> HistoricalLandUsePage {
    let (width, height) = dimensions(x, y);
    let mut crop = Vec::new();
    let mut grazing = Vec::new();
    let mut population = Vec::new();
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            crop.push(((x * PAGE + column) % 50) as u8);
            grazing.push(((y * PAGE + row) % 50) as u8);
            population.push(x * PAGE + column + y * PAGE + row);
        }
    }
    HistoricalLandUsePage {
        level: 0,
        x,
        y,
        width,
        height,
        crop_percent: crop,
        grazing_percent: grazing,
        population_pressure_per_square_kilometer: population,
        coverage: Vec::new(),
    }
}

fn write_page<T: serde::Serialize>(stage: &Stage, layer: PageLayer, x: u16, y: u16, page: &T) {
    let bytes = serde_json::to_vec(page).expect("fixture serialization");
    stage
        .write(layer, 0, x, y, &bytes)
        .expect("fixture staging");
}

fn expected_elevation_page(x: u16, y: u16) -> ElevationPage {
    let width = (NEXT_AXIS - x * PAGE).min(PAGE) as u8;
    let height = (NEXT_AXIS - y * PAGE).min(PAGE) as u8;
    let mut values = Vec::new();
    for row in 0..u16::from(height) {
        for column in 0..u16::from(width) {
            let source_x = (x * PAGE + column) * 2;
            let source_y = (y * PAGE + row) * 2;
            let mut sum = 0_i64;
            let mut count = 0_i64;
            for dy in 0..2 {
                for dx in 0..2 {
                    if source_x + dx < SOURCE_AXIS && source_y + dy < SOURCE_AXIS {
                        sum += i64::from(source_y + dy) * 1_000 + i64::from(source_x + dx);
                        count += 1;
                    }
                }
            }
            values.push(((sum + count / 2) / count) as i32);
        }
    }
    ElevationPage {
        level: 1,
        x,
        y,
        width,
        height,
        geographic_height_centimeters: values,
    }
}

fn expected_nearest_page<T>(x: u16, y: u16, value: impl Fn(u16, u16) -> T) -> Vec<T> {
    let width = (NEXT_AXIS - x * PAGE).min(PAGE);
    let height = (NEXT_AXIS - y * PAGE).min(PAGE);
    let value = &value;
    (0..height)
        .flat_map(move |row| {
            (0..width).map(move |column| value((x * PAGE + column) * 2, (y * PAGE + row) * 2))
        })
        .collect()
}

#[test]
fn odd_axis_reductions_match_reference_values_and_roots() {
    let stage = Stage::new(&std::env::temp_dir().join("aoe-pyramid-reduction-test"))
        .expect("fixture stage");
    for y in 0..3 {
        for x in 0..3 {
            write_page(&stage, PageLayer::Elevation, x, y, &elevation_page(x, y));
            write_page(&stage, PageLayer::Water, x, y, &water_page(x, y));
            write_page(&stage, PageLayer::Vegetation, x, y, &vegetation_page(x, y));
            write_page(
                &stage,
                PageLayer::HistoricalLandUse,
                x,
                y,
                &historical_page(x, y),
            );
        }
    }

    let mut elevation_root = PageRootBuilder::new(PageLayer::Elevation, 4).expect("root");
    let mut water_root = PageRootBuilder::new(PageLayer::Water, 4).expect("root");
    let mut vegetation_root = PageRootBuilder::new(PageLayer::Vegetation, 4).expect("root");
    let mut historical_root = PageRootBuilder::new(PageLayer::HistoricalLandUse, 4).expect("root");
    for y in 0..2 {
        for x in 0..2 {
            let elevation = reduce_elevation_page(&stage, SOURCE_AXIS, 1, x, y).expect("elevation");
            let expected_elevation = expected_elevation_page(x, y);
            assert_eq!(elevation, expected_elevation);
            elevation_root
                .push(elevation.content_hash().expect("elevation hash"))
                .expect("root");

            let water = nearest_page::<WaterPage>(&stage, SOURCE_AXIS, 1, x, y).expect("water");
            let expected_water = expected_nearest_page(x, y, |source_x, source_y| {
                let page = water_page(source_x / PAGE, source_y / PAGE);
                page.ocean_coverage_percent[usize::from(source_y % PAGE) * usize::from(page.width)
                    + usize::from(source_x % PAGE)]
            });
            assert_eq!(water.ocean_coverage_percent, expected_water);
            water_root
                .push(water.content_hash().expect("water hash"))
                .expect("root");

            let vegetation = nearest_page::<PotentialBiomePage>(&stage, SOURCE_AXIS, 1, x, y)
                .expect("vegetation");
            let expected_vegetation = expected_nearest_page(x, y, |source_x, source_y| {
                let page = vegetation_page(source_x / PAGE, source_y / PAGE);
                page.potential_biome_class[usize::from(source_y % PAGE) * usize::from(page.width)
                    + usize::from(source_x % PAGE)]
            });
            assert_eq!(vegetation.potential_biome_class, expected_vegetation);
            vegetation_root
                .push(vegetation.content_hash().expect("vegetation hash"))
                .expect("root");

            let historical = nearest_page::<HistoricalLandUsePage>(&stage, SOURCE_AXIS, 1, x, y)
                .expect("historical");
            let expected_historical = expected_nearest_page(x, y, |source_x, source_y| {
                let page = historical_page(source_x / PAGE, source_y / PAGE);
                let index = usize::from(source_y % PAGE) * usize::from(page.width)
                    + usize::from(source_x % PAGE);
                (
                    page.crop_percent[index],
                    page.grazing_percent[index],
                    page.population_pressure_per_square_kilometer[index],
                )
            });
            let actual_historical = historical
                .crop_percent
                .iter()
                .copied()
                .zip(historical.grazing_percent.iter().copied())
                .zip(
                    historical
                        .population_pressure_per_square_kilometer
                        .iter()
                        .copied(),
                )
                .map(|((crop, grazing), population)| (crop, grazing, population))
                .collect::<Vec<_>>();
            assert_eq!(actual_historical, expected_historical);
            historical_root
                .push(historical.content_hash().expect("historical hash"))
                .expect("root");
        }
    }
    assert_ne!(elevation_root.finish().expect("root"), [0; 32]);
    assert_ne!(water_root.finish().expect("root"), [0; 32]);
    assert_ne!(vegetation_root.finish().expect("root"), [0; 32]);
    assert_ne!(historical_root.finish().expect("root"), [0; 32]);
}
