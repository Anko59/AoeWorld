//! Cliff faces belong to terrain geometry; loose rocks remain upright objects.
use super::super::*;
use wasm_bindgen_test::wasm_bindgen_test;

fn art() -> GameArt {
    // Twenty-five slots like the pack: a 291x147 face with its hotspot in the
    // middle of a three-by-three tile footprint.
    let cliff_faces = (0..25)
        .map(|slot| GameFrame {
            stream: 0,
            atlas: crate::AtlasAddress {
                page: 2,
                uv: [0.25, 0.5, 0.125, 0.0625 + slot as f32 * 1e-4],
            },
            size: [291.0, 147.0],
            anchor: [145.0, 73.0],
        })
        .collect();
    GameArt {
        resident: Vec::new(),
        building_clocks: Vec::new(),
        environment: Vec::new(),
        actors: Vec::new(),
        actor_rows: Vec::new(),
        buildings: std::array::from_fn(|_| Vec::new()),
        village: Default::default(),
        fishing: Default::default(),
        compositions: Vec::new(),
        foundation: Default::default(),
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
            ..Default::default()
        },
    }
}

fn piece(facing: u8) -> SceneResource {
    SceneResource {
        highlight: 0,
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
fn rally_flags_keep_a_visible_pole_and_banner_without_optional_original_art() {
    let mut art = art();
    let mut flag = piece(0);
    flag.kind = SceneResource::RALLY_FLAG;
    for variant in 0..10 {
        flag.visual_variant = variant;
        let bounds = scene_resource_frame(&art, flag).expect("fallback flag bounds");
        assert!(bounds.size[1] >= 30.0);
        let sprites = world_sprite_frames(&art, &[], &[flag], &[], camera(), 0);
        assert_eq!(sprites.len(), 2, "pole and banner on every backend");
        assert!(
            sprites.iter().all(|(_, _, depth, _)| *depth
                > crate::surface_mesh::surface_depth([10.5, 10.5, 0.0]) + 3.0),
            "a building rally target must not hide its owner's flag"
        );
        assert!(sprites.iter().all(|(s, _, _, _)| s.color[3] == 1.0));
        let mut layers: Vec<_> = sprites
            .into_iter()
            .map(|(s, f, d, id)| WorldLayer::Sprite(s, f, d, id))
            .collect();
        crate::game_renderer::fog::shade_objects(&mut layers, &art, &[flag], &[], 0, &|_| 0.0);
        assert!(
            layers
                .iter()
                .all(|l| matches!(l,WorldLayer::Sprite(s,_,_,_) if s.color[2]>0.0)),
            "an owner's flag stays visible beyond explored terrain"
        );
    }
    let native = GameFrame {
        stream: 0,
        atlas: crate::AtlasAddress {
            page: 2,
            uv: [0.2, 0.3, 0.02, 0.03],
        },
        size: [30.0, 42.0],
        anchor: [10.0, 40.0],
    };
    art.scenery.rally_flags = vec![native; 10];
    assert_eq!(
        scene_resource_frame(&art, flag).unwrap().atlas,
        native.atlas
    );
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
fn decorative_rocks_use_small_native_frames_distinct_from_ore_at_both_zooms() {
    let mut art = art();
    let rock = |variant| SceneResource {
        highlight: 0,
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
        stream: 0,
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
    assert_eq!(seen, [true, false, true, false, true, false, false]);
    assert!(
        large == 0,
        "decorative rocks must never use tall pile art: {large}"
    );
    art.scenery.rocks.truncate(6);
    assert!(scene_resource_frame(&art, rock(0)).is_none());
}

#[wasm_bindgen_test]
fn yellow_dressing_is_never_presented_as_a_gold_like_object() {
    let art = crate::game_renderer::species_fixture::dressing_art();
    let mut resource = piece(0);
    resource.kind = SceneResource::GRASS;
    for variant in 0..=255 {
        resource.visual_variant = variant;
        assert!(scene_resource_frame(&art, resource).is_none());
    }
}

#[wasm_bindgen_test]
fn mediterranean_dressing_plants_are_suppressed_like_meadow_grass() {
    let art = crate::game_renderer::species_fixture::dressing_art();
    for family in [SceneResource::SHRUB, SceneResource::GRASS] {
        for variant in 0..=255 {
            let mut resource = piece(0);
            resource.kind = family;
            resource.visual_variant = variant;
            assert!(scene_resource_frame(&art, resource).is_none());
            assert!(world_sprite_frames(&art, &[], &[resource], &[], camera(), 0).is_empty());
        }
    }
    let mut wood = piece(0);
    wood.kind = SceneResource::DEADWOOD;
    assert!(scene_resource_frame(&art, wood).is_some());
}
