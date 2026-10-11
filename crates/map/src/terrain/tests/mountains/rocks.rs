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
}

#[test]
fn rocks_lie_on_bare_rock_and_as_scree_below_cliffs_and_never_block() {
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
                // A rock is dressing on open ground: nothing else stands there.
                assert!(tile.terrain.passable && !is_cliff(tile.terrain));
                assert!(!scene.resources.iter().any(|r| r.node.tile == at));
                assert_eq!(tile.appearance.canopy_strength, 0);
            }
            if !tile.terrain.passable || tile.appearance.canopy_strength > 0 {
                continue;
            }
            let reach = [-2, 0, 2];
            let around = reach
                .iter()
                .flat_map(|dx| reach.iter().map(move |dy| (*dx, *dy)));
            let beside = around.into_iter().any(|(dx, dy)| {
                is_cliff(generator.sample_base_tile(TileCoord::new(at.x + dx, at.y + dy)))
            });
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
