use super::*;

// Retain the former allocation/order as an independent reference, without
// copying the production projection/draw implementation into a test.
#[derive(Clone, Copy)]
enum Materialized {
    Resource(SceneResource),
    Unit(SceneUnit),
}

#[wasm_bindgen_test]
fn lazy_objects_preserve_materialized_order_and_every_packet_bit() {
    let mut art = fixture::art();
    art.walking = (0..80).map(|i| fixture::frame(1200 + i)).collect();
    art.standing = art.walking.clone();
    let resources = [
        SceneResource {
            id: 12,
            ..fixture::resource(2, 255)
        },
        SceneResource {
            id: 14,
            kind: 255,
            ..fixture::resource(4, 0)
        },
        SceneResource {
            id: 16,
            ..fixture::resource(4, 255)
        },
        SceneResource {
            id: 18,
            ..fixture::resource(3, 240)
        },
        SceneResource {
            id: 20,
            kind: 2,
            ..fixture::resource(2, 2)
        },
        SceneResource {
            id: 22,
            position: [10000.0; 2],
            ..fixture::resource(2, 0)
        },
        SceneResource {
            id: 24,
            ..fixture::resource(0, 39)
        },
    ];
    let units = [
        SceneUnit {
            id: EntityId(100),
            position: [0.5; 2],
            moving: true,
            facing: 3,
            selected: false,
            elevation_meters: 0.0,
        },
        SceneUnit {
            id: EntityId(102),
            position: [0.5; 2],
            moving: false,
            facing: 6,
            selected: true,
            elevation_meters: 0.0,
        },
    ];
    let native = art.tree_families.clone();
    for conifer_count in [9, 0, 5] {
        for palm_count in [13, 0, 1] {
            art.tree_families = [
                native[0][..conifer_count].to_vec(),
                native[1][..palm_count].to_vec(),
            ];
            let mut objects = Vec::new();
            for resource in resources {
                if scene_resource_presentation(&art, resource).is_some() {
                    objects.push(Materialized::Resource(resource));
                }
            }
            objects.extend(units.into_iter().map(Materialized::Unit));
            let expected = objects
                .into_iter()
                .flat_map(|object| match object {
                    Materialized::Resource(resource) => {
                        world_sprite_frames(&art, &[], &[resource], &[], fixture::camera(), 9)
                    }
                    Materialized::Unit(unit) => {
                        world_sprite_frames(&art, &[], &[], &[unit], fixture::camera(), 9)
                    }
                })
                .collect::<Vec<_>>();
            let actual = world_sprite_frames(&art, &[], &resources, &units, fixture::camera(), 9);
            assert_eq!(actual.len(), expected.len());
            for ((packet, frame, depth, id), (old_packet, old_frame, old_depth, old_id)) in
                actual.iter().zip(&expected)
            {
                assert_eq!(bytemuck::bytes_of(packet), bytemuck::bytes_of(old_packet));
                assert_eq!(frame.atlas, old_frame.atlas);
                assert_eq!(
                    frame.size.map(f32::to_bits),
                    old_frame.size.map(f32::to_bits)
                );
                assert_eq!(
                    frame.anchor.map(f32::to_bits),
                    old_frame.anchor.map(f32::to_bits)
                );
                assert_eq!(depth.to_bits(), old_depth.to_bits());
                assert_eq!(id, old_id);
            }
            // Resource shadow then body, exact equal-depth ties; units remain after
            // all retained resource objects, independently of iterator size hints.
            let mut expected_ids = Vec::new();
            for resource in resources {
                if resource.id != 22 && scene_resource_presentation(&art, resource).is_some() {
                    expected_ids.extend([resource.id; 2]);
                }
            }
            expected_ids.extend([100, 100, 102, 102]);
            assert_eq!(
                actual.iter().map(|(_, _, _, id)| *id).collect::<Vec<_>>(),
                expected_ids
            );
            assert!(actual.iter().all(|(_, _, depth, _)| *depth == 1.0));
        }
    }
}
