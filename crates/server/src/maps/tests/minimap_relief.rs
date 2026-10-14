use super::*;

fn smooth_relief(dome: bool) -> (MapPackage, MapChunkGenerator) {
    let (package, _) = coast_forest_mountain();
    let heights = (0..4)
        .flat_map(|y| {
            (0..4).map(move |x| {
                let (x, y) = (2 * x - 3, 2 * y - 3);
                if dome {
                    600_000 - 25_000 * (x * x + y * y)
                } else {
                    600_000 + 40_000 * (x + y)
                }
            })
        })
        .collect();
    let pages = [
        EnvironmentPage::Elevation(ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 4,
            height: 4,
            geographic_height_centimeters: heights,
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
            potential_biome_class: vec![14; 16],
        }),
    ];
    let mut provider = MemoryPageProvider::default();
    for page in pages {
        provider.pages.insert(page.key(), Arc::new(page));
    }
    let generator = package
        .generator_with_page_provider(Arc::new(provider))
        .expect("smooth elevation provider");
    (package, generator)
}

fn assert_smooth(raster: &[u8]) {
    let side = usize::from(u16::from_le_bytes([raster[4], raster[5]]));
    let cells = &raster[MINIMAP_HEADER_BYTES..];
    for y in 0..side {
        for x in 0..side {
            let shade = cells[y * side + x] >> 4;
            for (nx, ny) in [(x + 1, y), (x, y + 1)] {
                if nx < side && ny < side {
                    let next = cells[ny * side + nx] >> 4;
                    assert!(
                        shade.abs_diff(next) <= 1,
                        "shade step at ({x},{y}) -> ({nx},{ny}): {shade} -> {next}"
                    );
                }
            }
        }
    }
}

#[test]
fn smooth_slope_has_no_shade_steps_even_at_source_cell_and_map_edges() {
    let (package, generator) = smooth_relief(false);
    let raster = raster_of(&package, &generator);
    assert_smooth(&raster);
    assert_eq!(raster, raster_of(&package, &generator));
}

#[test]
fn smooth_dome_has_continuous_shading_and_visible_opposite_ridge_flanks() {
    let (package, generator) = smooth_relief(true);
    let raster = raster_of(&package, &generator);
    assert_smooth(&raster);
    let shades = raster[MINIMAP_HEADER_BYTES..].iter().map(|cell| cell >> 4);
    assert!(shades.clone().min().expect("dark flank") <= FLAT_SHADE - 3);
    assert!(shades.max().expect("light flank") >= FLAT_SHADE + 3);
    let side = usize::from(minimap_samples(package.estimate.tiles_per_side));
    let cell = |x, y| raster[MINIMAP_HEADER_BYTES + y * side + x] >> 4;
    assert!(cell(side / 4, side / 4) > cell(3 * side / 4, 3 * side / 4));
}
