//! Exact simulation fixture topology probe, without changing route budgets.
use super::*;
use crate::navigation::{DIAGONAL_COST, ORTHOGONAL_COST};
use crate::{EdgePassability, MapPackage, MapRequest};

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

fn clear(generator: &MapChunkGenerator, position: TileCoord) {
    let point = generator
        .landscape_point_with_cancel(position, &|| false)
        .unwrap()
        .unwrap();
    assert!(
        generator.landscape_reservations_at(position).route,
        "unreserved fanout at {position:?}"
    );
    assert!(
        point.tile.terrain.passable,
        "physical obstacle at {position:?}: {point:?}"
    );
    assert_ne!(
        point.tile.terrain.material,
        GroundMaterial::Mud,
        "unexpected movement-cost multiplier at {position:?}"
    );
    assert!(
        point.resource.is_none(),
        "blocking resource at {position:?}: {point:?}"
    );
    assert!(
        generator
            .object_at_with_cancel(position, &|| false)
            .unwrap()
            .is_none(),
        "object disagrees at {position:?}"
    );
    let appearance = point.tile.appearance;
    assert_eq!(
        (appearance.floor_strength, appearance.canopy_strength),
        (0, 0),
        "forest mask at {position:?}"
    );
}

#[test]
fn exact_simulation_fixture_has_clear_octile_optimal_fanout_to_every_opening() {
    let generator = exact_fixture();
    let origin = TileCoord::new(255, 255);
    let mut targets = 0;
    for dy in -1..=1 {
        for dx in -1..=1 {
            let target = generator
                .forest_opening_center_at(TileCoord::new(origin.x + dx * 192, origin.y + dy * 192))
                .unwrap();
            if origin.x.abs_diff(target.x).max(origin.y.abs_diff(target.y)) < 64 {
                continue;
            }
            targets += 1;
            let steps = origin.x.abs_diff(target.x).max(origin.y.abs_diff(target.y));
            let mut previous = origin;
            let mut cost = 0_u64;
            clear(&generator, origin);
            for step in 1..=steps {
                let position = TileCoord::new(
                    origin.x
                        + ((i64::from(target.x - origin.x) * i64::from(step)) / i64::from(steps))
                            as i32,
                    origin.y
                        + ((i64::from(target.y - origin.y) * i64::from(step)) / i64::from(steps))
                            as i32,
                );
                clear(&generator, position);
                assert_eq!(
                    generator
                        .edge_between_with_cancel(previous, position, &|| false)
                        .unwrap(),
                    EdgePassability::Passable,
                    "edge {previous:?}->{position:?}, target {target:?}"
                );
                if previous.x != position.x && previous.y != position.y {
                    for corner in [
                        TileCoord::new(previous.x, position.y),
                        TileCoord::new(position.x, previous.y),
                    ] {
                        clear(&generator, corner);
                        assert_eq!(
                            generator
                                .edge_between_with_cancel(previous, corner, &|| false)
                                .unwrap(),
                            EdgePassability::Passable
                        );
                        assert_eq!(
                            generator
                                .edge_between_with_cancel(corner, position, &|| false)
                                .unwrap(),
                            EdgePassability::Passable
                        );
                    }
                    cost += u64::from(DIAGONAL_COST);
                } else {
                    cost += u64::from(ORTHOGONAL_COST);
                }
                previous = position;
            }
            let x = u64::from(origin.x.abs_diff(target.x));
            let y = u64::from(origin.y.abs_diff(target.y));
            assert_eq!(
                cost,
                x.min(y) * u64::from(DIAGONAL_COST) + x.abs_diff(y) * u64::from(ORTHOGONAL_COST),
                "route must have exact octile-optimal cost to {target:?}"
            );
            eprintln!(
                "exact clear fanout target={target:?} tiles={steps} optimal_cost={cost}, key={:?}",
                generator.geography_key
            );
        }
    }
    assert!(targets >= 8);
}
