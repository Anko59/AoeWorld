//! Exact simulation fixture topology probe: forests never wall off the start.
use super::*;
use crate::{EdgePassability, MapPackage, MapRequest};
use std::collections::VecDeque;

fn exact_fixture() -> MapChunkGenerator {
    let package = MapPackage::new(
        1,
        MapRequest {
            requested_side_meters: 30_720,
            ..MapRequest::default()
        },
        Vec::new(),
    )
    .unwrap();
    let original = package.generator();
    assert_eq!(original.width_tiles, 512);
    let mut generator = flat(original.procedural_seed, original.width_tiles);
    generator.geography_key = original.geography_key;
    let elevation = Arc::make_mut(generator.elevation.as_mut().unwrap());
    elevation.compression = Ratio::new(1, 1).unwrap();
    elevation
        .pages
        .get_mut(&(0, 0))
        .unwrap()
        .geographic_height_centimeters[0] = 0;
    generator.biome = Some(Arc::new(PreparedBiome::new(
        1,
        [(
            (0, 0),
            PotentialBiomePage {
                level: 0,
                x: 0,
                y: 0,
                width: 1,
                height: 1,
                potential_biome_class: vec![9],
            },
        )]
        .into(),
    )));
    generator
}

#[test]
fn exact_simulation_fixture_start_reaches_every_map_edge_on_open_ground() {
    let generator = exact_fixture();
    let side = generator.width_tiles;
    let origin = TileCoord::new(255, 255);
    let walkable = |position: TileCoord| {
        generator
            .landscape_point_with_cancel(position, &|| false)
            .unwrap()
            .is_some_and(|point| point.tile.terrain.passable && point.resource.is_none())
    };
    assert!(walkable(origin));
    assert!(generator.landscape_reservations_at(origin).start);
    let mut seen = vec![false; (side * side) as usize];
    let mut queue = VecDeque::from([origin]);
    seen[(origin.y * side + origin.x) as usize] = true;
    let mut reached = 0usize;
    while let Some(tile) = queue.pop_front() {
        reached += 1;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = TileCoord::new(tile.x + dx, tile.y + dy);
            if next.x < 0 || next.y < 0 || next.x >= side || next.y >= side {
                continue;
            }
            let index = (next.y * side + next.x) as usize;
            if !seen[index] && walkable(next) {
                assert_eq!(
                    generator
                        .edge_between_with_cancel(tile, next, &|| false)
                        .unwrap(),
                    EdgePassability::Passable
                );
                seen[index] = true;
                queue.push_back(next);
            }
        }
    }
    let mut trees = 0;
    let edge = |x: i32, y: i32| seen[(y * side + x) as usize];
    assert!((0..side).any(|i| edge(i, 0)) && (0..side).any(|i| edge(i, side - 1)));
    assert!((0..side).any(|i| edge(0, i)) && (0..side).any(|i| edge(side - 1, i)));
    for y in 0..side {
        for x in 0..side {
            let point = generator
                .landscape_point_with_cancel(TileCoord::new(x, y), &|| false)
                .unwrap()
                .unwrap();
            trees += usize::from(point.resource.is_some());
        }
    }
    eprintln!("reached={reached} blockers={trees} of {}", side * side);
    assert!(trees > 10_000, "fixture must contain forests");
    // Unreached walkable tiles are single gaps inside dense stands.
    assert!((reached + trees) * 100 >= 97 * (side * side) as usize);
}
