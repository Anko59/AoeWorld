use super::shoreline::coast;
use super::*;

#[test]
fn fish_are_deterministic_in_chunks_and_points_on_coasts_and_lakes() {
    for lake in [false, true] {
        let generator = coast(71, &|x, _| {
            let wet = if x >= 16 { 100 } else { 0 };
            if lake { (0, wet) } else { (wet, 0) }
        });
        let mut shore = 0;
        let mut deep = 0;
        for cy in 1..7 {
            for cx in 3..7 {
                let chunk = generator.chunk(cx, cy).unwrap();
                assert_eq!(chunk, generator.chunk(cx, cy).unwrap());
                for node in chunk.resources {
                    if !matches!(node.object, ObjectKind::ShoreFish | ObjectKind::DeepFish) {
                        continue;
                    }
                    assert_eq!(generator.object_at(node.tile), Some(node));
                    assert_eq!(generator.resource_by_id(node.id), Some(node));
                    let tile = generator.tile_at(node.tile).unwrap();
                    assert!(matches!(tile.water, WaterKind::Lake | WaterKind::Ocean));
                    match node.object {
                        ObjectKind::ShoreFish => {
                            shore += 1;
                            assert_eq!(node.initial_amount, 200);
                            assert!([(0, -1), (-1, 0), (1, 0), (0, 1)].iter().any(|(dx, dy)| {
                                generator
                                    .tile_at(TileCoord::new(node.tile.x + dx, node.tile.y + dy))
                                    .is_some_and(|t| t.passable && t.water == WaterKind::None)
                            }));
                        }
                        ObjectKind::DeepFish => {
                            deep += 1;
                            assert_eq!(
                                node.initial_amount,
                                if node.visual_variant % 5 == 4 {
                                    200
                                } else {
                                    225
                                }
                            );
                            assert_eq!(tile.material, GroundMaterial::DeepWater);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }
        assert!(
            shore > 0 && deep > 0,
            "shore={shore}, deep={deep}, lake={lake}"
        );
    }
}

#[test]
fn narrow_water_has_no_fish() {
    let generator = coast(71, &|x, _| (0, if x == 16 { 100 } else { 0 }));
    for cy in 2..5 {
        for cx in 3..5 {
            let chunk = generator.chunk(cx, cy).unwrap();
            assert!(
                chunk.resources.iter().all(|node| !matches!(
                    node.object,
                    ObjectKind::ShoreFish | ObjectKind::DeepFish
                )),
                "narrow-water resources: {:?}",
                chunk.resources
            );
        }
    }
}
