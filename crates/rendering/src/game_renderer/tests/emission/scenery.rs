//! Cliff faces belong to terrain geometry; loose rocks remain upright objects.
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
        terrain_topology: [None; 12],
        water_depth: Default::default(),
        resources: Default::default(),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
        scenery: crate::SceneryArt {
            cliff_faces,
            rocks: Vec::new(),
        },
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
fn cliffs_are_drawn_by_the_surface_mesh_without_an_overlapping_billboard() {
    let art = art();
    for zoom in [1.0, 0.64, 0.25] {
        let mut camera = camera();
        camera.zoom = zoom;
        for facing in [0, 1, 2, 6, 7] {
            assert!(world_sprite_frames(&art, &[], &[piece(facing)], &[], camera, 0).is_empty());
        }
    }
}

#[wasm_bindgen_test]
fn cliff_frame_slots_are_mesh_textures_and_never_random_corner_objects() {
    let art = art();
    for facing in 0..8 {
        for variant in 0..=255 {
            let mut resource = piece(facing);
            resource.visual_variant = variant;
            assert!(scene_resource_frame(&art, resource).is_none());
        }
    }
}

#[wasm_bindgen_test]
fn loose_rocks_keep_native_frames_and_favor_large_piles_at_both_zooms() {
    let mut art = art();
    let rock = |variant| SceneResource {
        id: 3,
        position: [10.5, 10.5],
        kind: SceneResource::ROCK,
        visual_variant: variant,
        visual_family: 0,
        elevation_meters: 0.0,
    };
    assert!(scene_resource_frame(&art, rock(0)).is_none());
    art.scenery.rocks = [
        [58.0, 37.0],
        [75.0, 36.0],
        [67.0, 33.0],
        [75.0, 44.0],
        [56.0, 34.0],
        [85.0, 46.0],
        [78.0, 42.0],
    ]
    .into_iter()
    .enumerate()
    .map(|(slot, size)| GameFrame {
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [0.5 + slot as f32 * 0.01, 0.25, 0.03, 0.02],
        },
        size,
        anchor: [size[0] * 0.5, size[1] * 0.75],
    })
    .collect();
    let mut large = 0;
    let mut seen = [false; 7];
    for variant in 0..=255 {
        let selected = scene_resource_frame(&art, rock(variant)).unwrap();
        let slot = art
            .scenery
            .rocks
            .iter()
            .position(|frame| frame.atlas == selected.atlas)
            .expect("unaltered native art");
        assert_eq!(selected.size, art.scenery.rocks[slot].size);
        assert_eq!(selected.anchor, art.scenery.rocks[slot].anchor);
        seen[slot] = true;
        large += usize::from(matches!(slot, 3 | 5 | 6));
        for zoom in [1.0, 0.64] {
            let mut camera = camera();
            camera.zoom = zoom;
            let sprites = world_sprite_frames(&art, &[], &[rock(variant)], &[], camera, 0);
            assert_eq!(sprites.len(), 2, "shadow and body");
            let (body, frame, depth, _) = sprites[1];
            let scale = 128.0 / 96.0 * zoom as f32;
            assert!((frame.size[0] - selected.size[0] * scale).abs() < 1e-3);
            assert!((frame.anchor[1] - selected.anchor[1] * scale).abs() < 1e-3);
            assert_eq!(body.uv, selected.atlas.uv);
            assert_eq!(body.color, [1.0; 4]);
            assert_eq!(
                depth - crate::surface_mesh::surface_depth([10.5, 10.5, 0.0]),
                1.5
            );
            let bounds = resource_sprite_bounds(rock(variant), selected, camera).unwrap();
            assert!((bounds[2] - bounds[0] - f64::from(frame.size[0])).abs() < 1e-3);
        }
    }
    assert!(seen.into_iter().all(|present| present));
    assert!(
        large >= 192,
        "at least three quarters are large piles: {large}"
    );
    art.scenery.rocks.truncate(6);
    assert!(scene_resource_frame(&art, rock(0)).is_none());
}
