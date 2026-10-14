use aoe_core::{TileCoord, TileRect};
use aoe_map::{MapChunkGenerator, MovementOutcome, ResourceOverlay, find_path_with_overlay};

#[test]
fn dynamic_footprints_block_planning_without_changing_resource_revisions() {
    let terrain = flat_terrain();
    let mut overlay = ResourceOverlay::default();
    for y in 0..4 {
        for x in 0..4 {
            for node in terrain.chunk(x, y).unwrap().resources {
                overlay
                    .deplete(&terrain, node.id, node.initial_amount)
                    .unwrap();
            }
        }
    }
    let footprint = (2..120)
        .find_map(|y| {
            (2..120).find_map(|x| {
                let rect = TileRect::new(TileCoord::new(x, y), TileCoord::new(x + 4, y + 4));
                ((y - 1..=y + 4).all(|row| {
                    (x - 1..=x + 4).all(|col| {
                        terrain
                            .tile_at(TileCoord::new(col, row))
                            .is_some_and(|sample| sample.passable)
                    })
                }))
                .then_some(rect)
            })
        })
        .unwrap();
    let revision = overlay.revision();
    let snapshot = overlay.snapshot([1; 32]);
    overlay.add_footprint(footprint).unwrap();
    assert_eq!(overlay.revision(), revision);
    assert_eq!(overlay.snapshot([1; 32]), snapshot);
    let origin = TileCoord::new(footprint.min.x - 1, footprint.min.y + 1);
    assert_eq!(
        find_path_with_overlay(&terrain, &overlay, origin, footprint.min, 4096),
        MovementOutcome::InvalidDestination
    );
    let target = TileCoord::new(footprint.max.x, footprint.min.y + 1);
    let MovementOutcome::Path(path) =
        find_path_with_overlay(&terrain, &overlay, origin, target, 4096)
    else {
        panic!("detour must exist")
    };
    assert_eq!(path.tiles.first(), Some(&origin));
    assert_eq!(path.tiles.last(), Some(&target));
    assert!(path.tiles.iter().all(|tile| !footprint.contains(*tile)));
    for step in path.tiles.windows(2) {
        if step[0].x != step[1].x && step[0].y != step[1].y {
            assert!(!footprint.contains(TileCoord::new(step[0].x, step[1].y)));
            assert!(!footprint.contains(TileCoord::new(step[1].x, step[0].y)));
        }
    }
}

fn flat_terrain() -> MapChunkGenerator {
    use aoe_map::{
        ENVIRONMENT_PAGE_SAMPLES, ElevationPage, FieldPyramid, PreparedEnvironment, PyramidLevel,
        Ratio, ordered_page_root,
    };
    let zero = ElevationPage {
        level: 0,
        x: 0,
        y: 0,
        width: 2,
        height: 2,
        geographic_height_centimeters: vec![0; 4],
    };
    let overview = ElevationPage {
        level: 1,
        x: 0,
        y: 0,
        width: 1,
        height: 1,
        geographic_height_centimeters: vec![0],
    };
    let environment = PreparedEnvironment {
        samples_per_axis: 2,
        geographic_millimeters_per_sample: 128_000,
        page_samples: ENVIRONMENT_PAGE_SAMPLES,
        elevation: FieldPyramid {
            levels: vec![
                PyramidLevel {
                    samples_per_axis: 2,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&zero)).unwrap(),
                },
                PyramidLevel {
                    samples_per_axis: 1,
                    ordered_page_root: ordered_page_root(std::slice::from_ref(&overview)).unwrap(),
                },
            ],
        },
        water: None,
        vegetation: None,
        historical_land_use: None,
        hydrology_evidence: None,
    };
    MapChunkGenerator::new([3; 32], 1, 128)
        .with_prepared_elevation(
            Ratio::new(1, 1).unwrap(),
            &environment,
            vec![zero, overview],
        )
        .unwrap()
}
