use super::*;
use crate::{CHUNK_TILES, GroundMaterial, LandscapeChunk};

fn chunks(generator: &MapChunkGenerator, range: std::ops::Range<i32>) -> Vec<LandscapeChunk> {
    range
        .clone()
        .flat_map(|y| range.clone().map(move |x| (x, y)))
        .map(|(x, y)| {
            generator
                .landscape_chunk_with_cancel(x, y, &|| false)
                .expect("chunk")
        })
        .collect()
}

/// Stones and open tiles of one kind of ground: (stones, tiles).
#[derive(Default)]
struct Tally {
    rock: (usize, usize),
    scree: (usize, usize),
    snow: (usize, usize),
    meadow: (usize, usize),
    below: (usize, usize),
    above: (usize, usize),
}

#[test]
fn rocks_block_their_own_tiles_on_bare_rock_and_as_scree_below_cliffs() {
    let generator = alpine(1).with_latitude(459_000_000, 60_000);
    let scenes = chunks(&generator, 6..18);
    let mut tally = Tally::default();
    let mut blocks = vec![0_usize; 24 * 24];
    for scene in &scenes {
        for tile in &scene.tiles {
            let at = tile.tile;
            let stone = scene
                .decorations
                .iter()
                .any(|d| d.tile == at && d.family as u8 == 2);
            if stone {
                // A pile blocks movement without changing the ground's shape.
                assert!(!tile.terrain.passable && tile.terrain.surface.walkable());
                assert_eq!(tile.terrain.water, crate::WaterKind::None);
                assert!(!scene.resources.iter().any(|r| r.node.tile == at));
                assert_eq!(tile.appearance.canopy_strength, 0);
            }
            // Count eligible ground under the entire pile in the density
            // denominator, including blocked tiles without a sprite anchor.
            if !generator.sample_base_tile(at).passable || tile.appearance.canopy_strength > 0 {
                continue;
            }
            let reach = [-2, 0, 2];
            let around = reach
                .iter()
                .flat_map(|dx| reach.iter().map(move |dy| (*dx, *dy)));
            let beside = around.into_iter().any(|(dx, dy)| {
                is_cliff(generator.sample_base_tile(TileCoord::new(at.x + dx, at.y + dy)))
            });
            if beside {
                let higher_cliff = reach.iter().any(|dx| {
                    reach.iter().any(|dy| {
                        let near = generator.sample_base_tile(TileCoord::new(at.x + dx, at.y + dy));
                        is_cliff(near)
                            && near.geographic_height_centimeters
                                > tile.terrain.geographic_height_centimeters
                    })
                });
                let slope = if higher_cliff {
                    &mut tally.below
                } else {
                    &mut tally.above
                };
                slope.0 += usize::from(stone);
                slope.1 += 1;
            }
            let height = tile.terrain.geographic_height_centimeters / 100;
            let slot = if beside {
                &mut tally.scree
            } else {
                match tile.terrain.material {
                    GroundMaterial::Rock => {
                        if stone {
                            let block =
                                (at.y - 6 * CHUNK_TILES) / 16 * 24 + (at.x - 6 * CHUNK_TILES) / 16;
                            blocks[block as usize] += 1;
                        }
                        &mut tally.rock
                    }
                    GroundMaterial::Snow => &mut tally.snow,
                    GroundMaterial::TemperateGrass if height < 1_000 => &mut tally.meadow,
                    _ => continue,
                }
            };
            slot.0 += usize::from(stone);
            slot.1 += 1;
        }
    }
    let share = |(stones, tiles): (usize, usize)| stones * 1000 / tiles.max(1);
    eprintln!(
        "rocks per thousand: rock={} scree={} snow={} meadow={} ({:?} {:?})",
        share(tally.rock),
        share(tally.scree),
        share(tally.snow),
        share(tally.meadow),
        tally.rock,
        tally.scree
    );
    assert!(tally.rock.1 > 5_000 && tally.scree.1 > 2_000 && tally.meadow.1 > 5_000);
    assert!(
        (15..=80).contains(&share(tally.rock)),
        "sparse on bare rock"
    );
    assert!(
        share(tally.scree) >= share(tally.rock) * 2,
        "scree is denser"
    );
    assert!(share(tally.scree) <= 250);
    assert!(tally.below.1 > 500 && tally.above.1 > 500);
    assert!(
        share(tally.below) >= share(tally.above) * 2,
        "downhill scree: {:?} vs {:?}",
        tally.below,
        tally.above
    );
    assert!(share(tally.snow) < share(tally.rock));
    assert_eq!(tally.meadow.0, 0, "no rocks in a lowland meadow");
    // Clustered, not evenly sprinkled: many 16-tile blocks of rock ground are
    // empty while others hold several rocks.
    let occupied = blocks.iter().filter(|count| **count > 0).count();
    let crowded = blocks.iter().filter(|count| **count >= 6).count();
    assert!(
        crowded >= 10 && occupied * 10 <= blocks.len() * 9,
        "{occupied} {crowded}"
    );
}

#[test]
fn loose_rock_blocks_its_ground_extent_in_chunks_and_routes() {
    use crate::{CompactChunk, DecorationFamily, EdgePassability, MovementOutcome, find_path};

    let generator = relief(1, 20, |_, _| 300);
    let scenes = chunks(&generator, 2..5);
    let (scene, rock, left, right) = scenes
        .iter()
        .flat_map(|scene| {
            scene
                .decorations
                .iter()
                .filter(|d| d.family == DecorationFamily::Stone)
                .map(move |d| (scene, d.tile))
        })
        .find_map(|(scene, at)| {
            let left = TileCoord::new(at.x - 2, at.y);
            let right = TileCoord::new(at.x + 2, at.y);
            [left, right]
                .into_iter()
                .all(|side| {
                    scene
                        .tiles
                        .iter()
                        .any(|tile| tile.tile == side && tile.terrain.passable)
                        && !scene.resources.iter().any(|r| r.node.tile == side)
                })
                .then_some((scene, at, left, right))
        })
        .expect("a loose rock with clear ground on both sides");

    let mut footprint = Vec::new();
    for dy in -1..=1 {
        for dx in -1..=1 {
            let at = TileCoord::new(rock.x + dx, rock.y + dy);
            footprint.push(at);
            let visual = generator
                .landscape_point_with_cancel(at, &|| false)
                .unwrap()
                .unwrap();
            let surface = generator
                .landscape_surface_with_cancel(at, &|| false)
                .unwrap()
                .unwrap();
            let (physical, node) = generator
                .tile_and_node_with_cancel(at, &|| false)
                .unwrap()
                .unwrap();
            assert!(
                !physical.passable,
                "rock at {rock:?} leaves {at:?} walkable"
            );
            assert!(node.is_none() && physical.surface.walkable());
            assert_eq!(visual.tile.terrain, physical);
            assert_eq!(surface.tile, visual.tile);
            assert_eq!(
                find_path(&generator, left, at, 64),
                MovementOutcome::InvalidDestination
            );
        }
    }
    assert_eq!(
        generator.edge_between(left, TileCoord::new(rock.x - 1, rock.y)),
        EdgePassability::Blocked
    );
    let physical_chunk = generator
        .physical_chunk_with_cancel(scene.x, scene.y, &|| false)
        .unwrap();
    assert_eq!(physical_chunk.tiles, scene.tiles);
    assert_eq!(
        CompactChunk::encode(scene).unwrap().decode().unwrap(),
        *scene
    );

    assert_eq!(
        find_path(&generator, left, rock, 64),
        MovementOutcome::InvalidDestination
    );
    let MovementOutcome::Path(path) = find_path(&generator, left, right, 64) else {
        panic!("clear ground is reachable past the pile");
    };
    assert!(path.tiles.iter().all(|tile| !footprint.contains(tile)));
}

#[test]
fn dry_grassland_has_a_few_rock_clusters_and_rocks_are_coordinate_stable() {
    // Steppe at 300 m.
    let generator = relief(1, 20, |_, _| 300);
    let scenes = chunks(&generator, 2..10);
    let stones = scenes
        .iter()
        .flat_map(|scene| &scene.decorations)
        .filter(|d| d.family as u8 == 2)
        .map(|d| d.tile)
        .collect::<Vec<_>>();
    let tiles = scenes.len() * 1024;
    eprintln!("steppe rocks: {} on {tiles} tiles", stones.len());
    assert!(stones.len() * 1000 >= tiles && stones.len() * 1000 <= tiles * 12);
    // Clusters: a fifth of the 16-tile blocks are bare, some hold many rocks.
    let mut blocks = vec![0_usize; 16 * 16];
    for tile in &stones {
        blocks[((tile.y - 64) / 16 * 16 + (tile.x - 64) / 16) as usize] += 1;
    }
    let bare = blocks.iter().filter(|count| **count == 0).count();
    let crowded = blocks.iter().filter(|count| **count >= 5).count();
    assert!(bare * 5 >= blocks.len() && crowded >= 8, "{bare} {crowded}");
    // The point query and the chunk agree, whatever the request order.
    for tile in stones.iter().step_by(7) {
        let point = generator
            .landscape_point_with_cancel(*tile, &|| false)
            .expect("point")
            .expect("in bounds");
        assert!(point.decoration.is_some_and(|d| d.family as u8 == 2));
    }
}

#[test]
fn loose_rocks_stay_off_a_high_cold_lake_and_its_shore() {
    let mut generator = relief(1, 13, |_, _| 2_400).with_latitude(459_000_000, 60_000);
    generator.water = Some(Arc::new(PreparedWater::new(
        2,
        [(
            (0, 0),
            WaterPage {
                level: 0,
                x: 0,
                y: 0,
                width: 2,
                height: 2,
                ocean_coverage_percent: vec![0; 4],
                inland_coverage_percent: vec![100, 0, 100, 0],
            },
        )]
        .into(),
    )));
    let scenes = chunks(&generator, 8..16);
    let mut wet = 0;
    let mut stones = 0;
    for scene in &scenes {
        for tile in &scene.tiles {
            wet += usize::from(tile.terrain.water != crate::WaterKind::None);
        }
        for rock in scene.decorations.iter().filter(|d| d.family as u8 == 2) {
            stones += 1;
            let tile = scene
                .tiles
                .iter()
                .find(|tile| tile.tile == rock.tile)
                .expect("tile");
            assert_eq!(tile.terrain.water, crate::WaterKind::None);
            let point = generator
                .landscape_point_with_cancel(rock.tile, &|| false)
                .expect("point")
                .expect("in bounds");
            assert!(point.decoration.is_some_and(|d| d.family as u8 == 2));
        }
    }
    assert!(
        wet > 1_000 && stones > 10,
        "{wet} wet tiles, {stones} stones"
    );
}

#[test]
fn physical_chunks_and_points_drop_only_decorations() {
    let generator = alpine(1).with_latitude(459_000_000, 60_000);
    let mut decorated = 0;
    for (x, y) in [(6, 6), (9, 11), (12, 14)] {
        let full = generator
            .landscape_chunk_with_cancel(x, y, &|| false)
            .unwrap();
        let physical = generator
            .physical_chunk_with_cancel(x, y, &|| false)
            .unwrap();
        decorated += full.decorations.len();
        assert!(physical.decorations.is_empty());
        assert_eq!(physical.tiles, full.tiles);
        assert_eq!(physical.resources, full.resources);
        for tile in full.tiles.iter().step_by(97) {
            let point = generator
                .landscape_point_with_cancel(tile.tile, &|| false)
                .unwrap()
                .unwrap();
            let cached = generator
                .memoized_point(tile.tile, &|| false)
                .unwrap()
                .unwrap();
            assert_eq!(cached.0, point.tile.terrain);
            assert_eq!(cached.1, point.resource.map(|resource| resource.node));
        }
    }
    assert!(decorated > 0, "the fixture must exercise skipped dressing");
}

#[test]
fn every_rock_blocked_tile_lies_under_drawn_rock_art_and_holds_no_resource() {
    use crate::DecorationFamily;

    let generator = relief(1, 20, |_, _| 300);
    let scenes = chunks(&generator, 2..10);
    let stones = scenes
        .iter()
        .flat_map(|scene| &scene.decorations)
        .filter(|d| d.family == DecorationFamily::Stone)
        .map(|d| d.tile)
        .collect::<std::collections::HashSet<_>>();
    let under_art = |at: TileCoord| {
        (-1..=1).any(|dy| (-1..=1).any(|dx| stones.contains(&TileCoord::new(at.x + dx, at.y + dy))))
    };
    let mut blocked = 0;
    for scene in &scenes {
        for tile in &scene.tiles {
            // A pile anchored outside the loaded chunks can reach the edge tiles.
            let inner = 2 * CHUNK_TILES + 2..10 * CHUNK_TILES - 2;
            if tile.terrain.passable
                || !tile.terrain.surface.walkable()
                || !inner.contains(&tile.tile.x)
                || !inner.contains(&tile.tile.y)
            {
                continue;
            }
            if scene.resources.iter().any(|r| r.node.tile == tile.tile) {
                assert!(
                    !under_art(tile.tile),
                    "a resource sits under the rocks at {:?}",
                    tile.tile
                );
                continue;
            }
            blocked += 1;
            assert!(
                under_art(tile.tile),
                "{:?} is blocked with no rock drawn",
                tile.tile
            );
        }
    }
    assert!(blocked > 0, "the steppe grows rock piles");
}
