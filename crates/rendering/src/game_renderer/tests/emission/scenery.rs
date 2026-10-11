//! Cliffs keep their upright source image, native size and object clearance.
use super::super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn art() -> GameArt {
    // Twenty-five slots like the pack: a 291x147 face with its hotspot in the
    // middle of a three-by-three tile footprint.
    let cliff_faces = (0..25)
        .map(|slot| GameFrame {
            atlas: crate::AtlasAddress {
                page: 2,
                uv: [0.25, 0.5, 0.125, 0.0625 + slot as f32 * 1e-4],
            },
            size: [291.0, 147.0],
            anchor: [145.0, 73.0],
        })
        .collect();
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: Default::default(),
        terrain_topology: [None; 8],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: crate::SceneryArt { cliff_faces },
    }
}

fn piece(facing: u8) -> SceneResource {
    SceneResource {
        id: 9,
        position: [10.5; 2],
        kind: SceneResource::CLIFF,
        visual_variant: 0,
        visual_family: facing,
        elevation_meters: 0.0,
    }
}

fn camera() -> SceneCamera {
    SceneCamera {
        center: [10.5, 10.5],
        zoom: 1.0,
        viewport: [1024.0, 768.0],
        focus_elevation_meters: 0.0,
    }
}

#[wasm_bindgen_test]
fn cliffs_keep_a_whole_native_face_at_every_zoom_and_share_object_clearance() {
    let art = art();
    for zoom in [1.0, 0.64] {
        let mut camera = camera();
        camera.zoom = zoom;
        let resource = piece(0);
        let selected = scene_resource_frame(&art, resource).unwrap();
        assert_eq!(selected.size, [291.0, 147.0]);
        assert_eq!(selected.anchor, [145.0, 73.0]);
        let drawn = world_sprite_frames(&art, &[], &[resource], &[], camera, 0);
        assert_eq!(drawn.len(), 1, "one whole face, no shadow or strips");
        let (sprite, frame, depth, id) = drawn[0];
        let scale = 128.0 / 96.0 * zoom as f32;
        assert!((frame.size[0] - 291.0 * scale).abs() < 1e-3);
        assert!((frame.size[1] - 147.0 * scale).abs() < 1e-3);
        assert_eq!(sprite.uv, selected.atlas.uv);
        assert_eq!(id, 9);
        let ground = crate::surface_mesh::surface_depth([10.5, 10.5, 0.0]);
        assert_eq!(depth - ground, 1.5, "shared object clearance");
        let bounds = resource_sprite_bounds(resource, selected, camera).unwrap();
        assert!((bounds[2] - bounds[0] - f64::from(frame.size[0])).abs() < 1e-3);
        assert!((bounds[3] - bounds[1] - f64::from(frame.size[1])).abs() < 1e-3);
    }
}

#[wasm_bindgen_test]
fn cliff_orientation_selects_native_faces_with_their_authored_lighting() {
    let art = art();
    for (facing, slots) in [
        (0, &[3, 15, 17, 19, 24][..]),
        (1, &[1, 2, 18][..]),
        (2, &[1, 2, 18][..]),
        (6, &[4, 5, 16][..]),
        (7, &[4, 5, 16][..]),
    ] {
        for (variant, slot) in slots.iter().enumerate() {
            let mut resource = piece(facing);
            resource.visual_variant = variant as u8;
            assert_eq!(
                scene_resource_frame(&art, resource).unwrap().atlas,
                art.scenery.cliff_faces[*slot].atlas
            );
        }
    }
    let draw = |facing| world_sprite_frames(&art, &[], &[piece(facing)], &[], camera(), 0);
    for facing in [0, 1, 2, 6, 7] {
        assert!(draw(facing)[0].0.uv[2] > 0.0, "keep authored sun direction");
    }
    for away in 3..=5 {
        assert!(draw(away).is_empty());
    }
    let mut partial = art;
    partial.scenery.cliff_faces.truncate(11);
    assert!(scene_resource_frame(&partial, piece(0)).is_none());
}
