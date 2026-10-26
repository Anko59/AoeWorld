use super::*;

pub(super) fn river_width(horizontal: bool, span: std::ops::Range<usize>) -> GameWorld {
    river_coverage(horizontal, span, 100)
}

pub(super) fn river_coverage(
    horizontal: bool,
    span: std::ops::Range<usize>,
    coverage: u8,
) -> GameWorld {
    river_lane(horizontal, span, coverage, None)
}

pub(super) fn river_lane(
    horizontal: bool,
    span: std::ops::Range<usize>,
    coverage: u8,
    cross: Option<usize>,
) -> GameWorld {
    let (mut package, _, _) = fixture::flat_fixture(true);
    let mut elevation = Vec::new();
    let mut water = Vec::new();
    let mut vegetation = Vec::new();
    let mut axis = 64_u16;
    package.request.requested_side_meters = 3_840;
    package.environment.samples_per_axis = axis;
    package.environment.elevation.levels.clear();
    package.environment.water.as_mut().unwrap().levels.clear();
    package.environment.vegetation = Some(FieldPyramid { levels: Vec::new() });
    loop {
        let level = elevation.len() as u8;
        let n = usize::from(axis).pow(2);
        let e = ElevationPage {
            level,
            x: 0,
            y: 0,
            width: axis as u8,
            height: axis as u8,
            geographic_height_centimeters: vec![0; n],
        };
        let w = WaterPage {
            level,
            x: 0,
            y: 0,
            width: axis as u8,
            height: axis as u8,
            ocean_coverage_percent: (0..n)
                .map(|i| {
                    if level == 0
                        && cross.is_none_or(|row| {
                            row == if horizontal {
                                i / usize::from(axis)
                            } else {
                                i % usize::from(axis)
                            }
                        })
                        && span.contains(
                            &(if horizontal {
                                i % usize::from(axis)
                            } else {
                                i / usize::from(axis)
                            }),
                        )
                    {
                        coverage
                    } else {
                        0
                    }
                })
                .collect(),
            inland_coverage_percent: vec![0; n],
        };
        let v = PotentialBiomePage {
            level,
            x: 0,
            y: 0,
            width: axis as u8,
            height: axis as u8,
            potential_biome_class: vec![28; n],
        };
        package.environment.elevation.levels.push(PyramidLevel {
            samples_per_axis: axis,
            ordered_page_root: ordered_page_root(std::slice::from_ref(&e)).unwrap(),
        });
        package
            .environment
            .water
            .as_mut()
            .unwrap()
            .levels
            .push(PyramidLevel {
                samples_per_axis: axis,
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(&w)).unwrap(),
            });
        package
            .environment
            .vegetation
            .as_mut()
            .unwrap()
            .levels
            .push(PyramidLevel {
                samples_per_axis: axis,
                ordered_page_root: ordered_biome_page_root(std::slice::from_ref(&v)).unwrap(),
            });
        elevation.push(e);
        water.push(w);
        vegetation.push(v);
        if axis == 1 {
            break;
        }
        axis /= 2;
    }
    let package = MapPackage::with_prepared_environment(
        1,
        package.request,
        Vec::new(),
        package.projection,
        package.provenance,
        package.environment,
    )
    .unwrap();
    GameWorld::from_prepared_map(package, elevation, water, vegetation, Vec::new()).unwrap()
}

/// Remove procedural resources so focused terrain tests have known free sites.
pub(super) fn clear_nodes(world: &mut GameWorld, rect: TileRect) {
    if let Terrain::Map { generator, overlay } = &mut world.terrain {
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                if let Some(node) = generator.object_at(TileCoord::new(x, y)) {
                    overlay
                        .deplete(generator, node.id, node.initial_amount)
                        .unwrap();
                }
            }
        }
    }
}
