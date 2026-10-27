//! Generated elevation pages make the workload independent of missing-source
//! fallback terrain. Objects/resources still use the ordinary seeded generator.
use crate::GameWorld;
use aoe_map::{
    ENVIRONMENT_PAGE_SAMPLES, ElevationPage, EnvironmentalProvenance, FieldPyramid, MapPackage,
    MapRequest, PreparedEnvironment, ProjectionMetadata, PyramidLevel, WaterPage,
    ordered_page_root, ordered_water_page_root,
};

pub(super) fn build(request: MapRequest) -> Result<GameWorld, Box<dyn std::error::Error>> {
    let pages = vec![
        ElevationPage {
            level: 0,
            x: 0,
            y: 0,
            width: 2,
            height: 2,
            geographic_height_centimeters: vec![0; 4],
        },
        ElevationPage {
            level: 1,
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            geographic_height_centimeters: vec![0],
        },
    ];
    // A southern sea leaves all hundred villages on land and gives the navy
    // real water navigation. Each coarser level averages the same coverage.
    let mut water = Vec::new();
    let mut axis = 32_u8;
    let mut coverage: Vec<u8> = (0..axis)
        .flat_map(|y| (0..axis).map(move |_| if y >= 30 { 100 } else { 0 }))
        .collect();
    loop {
        water.push(WaterPage {
            level: water.len() as u8,
            x: 0,
            y: 0,
            width: axis,
            height: axis,
            ocean_coverage_percent: coverage.clone(),
            inland_coverage_percent: vec![0; coverage.len()],
        });
        if axis == 1 {
            break;
        }
        let old = usize::from(axis);
        axis /= 2;
        coverage = (0..usize::from(axis))
            .flat_map(|y| {
                let coverage = &coverage;
                (0..usize::from(axis)).map(move |x| {
                    ((u16::from(coverage[2 * y * old + 2 * x])
                        + u16::from(coverage[2 * y * old + 2 * x + 1])
                        + u16::from(coverage[(2 * y + 1) * old + 2 * x])
                        + u16::from(coverage[(2 * y + 1) * old + 2 * x + 1]))
                        / 4) as u8
                })
            })
            .collect();
    }
    let water_levels = water
        .iter()
        .map(|page| {
            Ok(PyramidLevel {
                samples_per_axis: u16::from(page.width),
                ordered_page_root: ordered_water_page_root(std::slice::from_ref(page))?,
            })
        })
        .collect::<Result<Vec<_>, aoe_map::EnvironmentError>>()?;
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: request.requested_side_meters * 1_000 / 2,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(&pages[..1])?,
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(&pages[1..])?,
                },
            ],
        },
        water: Some(FieldPyramid {
            levels: water_levels,
        }),
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    let package = MapPackage::with_prepared_environment(
        1,
        request,
        Vec::new(),
        ProjectionMetadata::default(),
        EnvironmentalProvenance::default(),
        environment,
    )?;
    Ok(GameWorld::from_prepared_map(
        package,
        pages,
        water,
        Vec::new(),
        Vec::new(),
    )?)
}

/// Eight additional normal deposits leave room for the mature reservation-aware
/// economy. Even 100 complete kits plus these stands stay below the existing
/// 4,096 starting-node bound. No simulation capacity or work floor changes.
pub(super) fn mature_resources(
    world: &mut GameWorld,
    center: aoe_core::TileCoord,
    spots: &[aoe_core::TileCoord],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut tiles: Vec<_> = spots
        .iter()
        .map(|tile| {
            let (dx, dy) = (tile.x - center.x, tile.y - center.y);
            (dx * dx + dy * dy, tile.x, tile.y)
        })
        .filter(|(d, x, y)| *d >= 144 && x.rem_euclid(3) == 0 && y.rem_euclid(3) == 0)
        .collect();
    tiles.sort_unstable();
    let mut inserted = 0;
    for (_, x, y) in tiles {
        let tile = aoe_core::TileCoord::new(x, y);
        if !world.tile_free(tile) || world.building_at(tile).is_some() {
            continue;
        }
        if [(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
            !world.tile_passable(aoe_core::TileCoord::new(tile.x + dx, tile.y + dy))
        }) {
            continue;
        }
        let node = aoe_map::ResourceNode::starting(tile, aoe_map::ResourceKind::Gold)
            .ok_or("invalid mature deposit tile")?;
        if let crate::Terrain::Map { generator, overlay } = &mut world.terrain
            && overlay.insert_starting_resource(generator, node).is_ok()
        {
            inserted += 1;
            if inserted == 8 {
                break;
            }
        }
    }
    if let Ok(cache) = world.resource_chunks.get_mut() {
        cache.clear();
    }
    world.resource_collision_changed();
    if inserted != 8 {
        return Err(
            format!("mature stand near {center:?} has only {inserted} extra deposits").into(),
        );
    }
    Ok(())
}
