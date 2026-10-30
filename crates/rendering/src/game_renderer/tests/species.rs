use super::*;
use crate::game_renderer::species_fixture as fixture;
#[path = "species/ordering.rs"]
mod ordering;

#[wasm_bindgen_test]
fn reviewed_species_subsets_and_broadleaf_pairing_share_selection_and_draw() {
    let mut art = fixture::art();
    for variant in 0..=255 {
        for (family, slot, approved) in [
            (2, 0, &[1, 2, 3, 4, 7, 8][..]),
            (4, 1, &[0, 1, 2, 3, 5, 6, 8, 10, 11, 12][..]),
        ] {
            let resource = fixture::resource(family, variant);
            let (body, shadow) = scene_resource_presentation(&art, resource).unwrap();
            let index = approved[usize::from(variant) % approved.len()];
            assert_eq!(body.atlas, art.tree_families[slot][index].atlas);
            assert!(shadow.is_none(), "2296 cannot pair with another species");
            assert_eq!(
                scene_resource_frame(&art, resource).unwrap().atlas,
                body.atlas
            );
            let drawn = fixture::drawn(&art, &[resource]);
            assert_eq!(drawn.len(), 2);
            assert!(
                drawn
                    .iter()
                    .all(|(_, frame, _, id)| frame.atlas == body.atlas && *id == 7)
            );
            assert_eq!(drawn[0].0.color, [0.0, 0.0, 0.0, 0.2]);
        }
        for family in [0, 1, 3, 5, 255] {
            let resource = fixture::resource(family, variant);
            let index = if family == 0 {
                crate::resource_frame_index(1, variant, 14).unwrap()
            } else {
                [0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13][usize::from(variant) % 11]
            };
            let (body, shadow) = scene_resource_presentation(&art, resource).unwrap();
            assert_eq!(body.atlas, art.resources[1][index].atlas);
            assert_eq!(shadow.unwrap().atlas, art.tree_shadows[index].atlas);
        }
        for kind in [0, 2, 3] {
            for family in [0, 2, 4, 255] {
                let resource = SceneResource {
                    kind,
                    ..fixture::resource(family, variant)
                };
                let (body, shadow) = scene_resource_presentation(&art, resource).unwrap();
                assert_eq!(body.atlas, art.resources[usize::from(kind)][0].atlas);
                assert!(shadow.is_none());
            }
        }
    }
    for (family, slot, expected) in [(2, 0, 9), (4, 1, 13)] {
        let original = art.tree_families[slot].clone();
        for length in 1..=expected + 1 {
            if length == expected {
                continue;
            }
            art.tree_families[slot] = vec![original[0]; length];
            // An incomplete family is unavailable art, not a reason to drop trees.
            assert_broadleaf_fallback(&art, family);
        }
        art.tree_families[slot].clear();
        assert_broadleaf_fallback(&art, family);
        art.tree_families[slot] = original;
    }
}

#[wasm_bindgen_test]
fn native_signed_and_outside_anchors_preserve_body_overhang_and_removal() {
    let mut art = fixture::art();
    let camera = fixture::camera();
    let projection = Camera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    for (family, slot, index, anchor) in [(2, 0, 1, [-8.0, 32.0]), (4, 1, 0, [20.0, 175.0])] {
        let native = GameFrame {
            stream: 0,
            size: [88.0, 168.0],
            anchor,
            ..art.tree_families[slot][index]
        };
        art.tree_families[slot][index] = native;
        let resource = SceneResource {
            position: projection.screen_to_world_at_height(
                ScreenPoint {
                    x: if family == 2 { -4.0 } else { 132.0 },
                    y: 96.0,
                },
                12.0,
            ),
            elevation_meters: 12.0,
            ..fixture::resource(family, 0)
        };
        let body = scene_resource_frame(&art, resource).unwrap();
        assert_eq!(body.anchor, anchor);
        assert!(
            resource_sprite_bounds(resource, body, camera).is_some(),
            "offscreen contact, visible foliage"
        );
        let drawn = fixture::drawn(&art, &[resource]);
        // Native 96-pixel-tile art grows with the 128-pixel world tile.
        let world = |pixels: f32| pixels * (128.0 / 96.0);
        assert!(
            drawn
                .iter()
                .any(|(_, frame, _, _)| frame.size == native.size.map(world)
                    && frame.anchor == anchor.map(world))
        );
        assert!(
            fixture::drawn(&art, &[]).is_empty(),
            "removing resource removes body and silhouette"
        );
    }
}

fn assert_broadleaf_fallback(art: &GameArt, family: u8) {
    for variant in 0..=255 {
        let (body, shadow) =
            scene_resource_presentation(art, fixture::resource(family, variant)).unwrap();
        let index = [0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13][usize::from(variant) % 11];
        assert_eq!(body.atlas, art.resources[1][index].atlas);
        assert_eq!(shadow.unwrap().atlas, art.tree_shadows[index].atlas);
        assert!(!fixture::drawn(art, &[fixture::resource(family, variant)]).is_empty());
    }
}

#[wasm_bindgen_test]
fn dressing_variants_keep_native_anchors_scale_and_order_with_trees_and_units() {
    let mut art = fixture::dressing_art();
    {
        let (kind, count) = (20, 5);
        let frames = (0..count)
            .map(|i| GameFrame {
                stream: 0,
                anchor: [-3.0, 25.0],
                size: [29.0, 20.0],
                ..fixture::frame(i * 16)
            })
            .collect::<Vec<_>>();
        art.scenery.stumps = frames[..3].to_vec();
        art.scenery.logs = frames[3..5].to_vec();
        art.scenery.snags = vec![fixture::frame(1984)];
        let mut seen = vec![false; count];
        for variant in 0..=255 {
            for orientation in 0..8 {
                let resource = SceneResource {
                    visual_variant: variant,
                    visual_family: orientation,
                    ..fixture::dressing(kind)
                };
                let selected = scene_resource_frame(&art, resource).unwrap();
                let index = frames
                    .iter()
                    .position(|f| f.atlas == selected.atlas)
                    .unwrap();
                seen[index] = true;
                assert_eq!(selected.anchor, [-3.0, 25.0]);
                for zoom in [1.0, 0.25] {
                    let camera = SceneCamera {
                        zoom,
                        ..fixture::camera()
                    };
                    let drawn = world_sprite_frames(&art, &[], &[resource], &[], camera, 0);
                    let (_, native, depth, _) = drawn.last().unwrap();
                    let scale = 128.0 / 96.0 * zoom as f32;
                    assert_eq!(native.size, selected.size.map(|p| p * scale));
                    assert_eq!(native.anchor, selected.anchor.map(|p| p * scale));
                    assert_eq!(*depth, 2.5);
                    assert!(resource_sprite_bounds(resource, selected, camera).is_some());
                }
            }
        }
        assert!(seen.into_iter().all(|v| v));
    }
    art.walking = vec![fixture::frame(640); 50];
    art.standing = art.walking.clone();
    fixture::unit_rows(&mut art);
    let resources = [
        fixture::dressing(20),
        SceneResource {
            id: 9,
            ..fixture::resource(0, 0)
        },
    ];
    let unit = SceneUnit {
        kind: Default::default(),
        id: EntityId(11),
        position: [0.5; 2],
        visual: crate::SceneUnitVisual::new(false).with_selected(false),
        facing: 0,
        elevation_meters: 0.0,
    };
    let layers = direct_world_layers([], &art, &[], &resources, &[unit], fixture::camera(), 0);
    let ids = layers
        .iter()
        .filter_map(|l| match l {
            WorldLayer::Sprite(_, _, _, id) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [7, 7, 9, 9, 11, 11],
        "equal-depth scenery/tree/unit tie order"
    );
    let unit = SceneUnit {
        kind: Default::default(),
        position: [0.4; 2],
        ..unit
    };
    let layers = direct_world_layers([], &art, &[], &resources, &[unit], fixture::camera(), 0);
    let ids = layers
        .iter()
        .filter_map(|l| match l {
            WorldLayer::Sprite(_, _, _, id) => Some(*id),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(ids, [11, 11, 7, 7, 9, 9], "ground depth before stable IDs");
}

#[wasm_bindgen_test]
fn absent_or_incomplete_dressing_never_substitutes_harvestable_art() {
    for kind in [18, 20] {
        assert!(scene_resource_frame(&fixture::art(), fixture::dressing(kind)).is_none());
        let mut art = fixture::dressing_art();
        match kind {
            18 => {
                art.scenery.shrubs.pop();
            }
            _ => {
                art.scenery.stumps.pop();
            }
        }
        assert!(scene_resource_frame(&art, fixture::dressing(kind)).is_none());
    }
}

#[wasm_bindgen_test]
fn broadleaf_tree_variants_keep_paired_shadows_and_limit_bare_trees() {
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [640.0, 480.0],
        focus_elevation_meters: 0.0,
    };
    let make_frame = |index: usize| GameFrame {
        stream: 0,
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [index as f32 / 1000.0, 0.0, 0.01, 0.01],
        },
        size: [32.0, 48.0],
        anchor: [16.0, 48.0],
    };
    let mut art = synthetic_art();
    art.resources[1] = (0..14).map(make_frame).collect();
    art.tree_shadows = (0..14).map(make_frame).collect();
    let terrain = SceneTerrain {
        appearance: None,
        position: [0.5, 0.5],
        material: 0,
        elevation_meters: 0.0,
        surface: SceneTerrainSurface::flat(0.0),
    };
    let trees = (0..256_u16)
        .map(|variant| SceneResource {
            highlight: 0,
            visual_family: 0,
            id: u64::from(variant),
            position: [0.5, 0.5],
            kind: 1,
            visual_variant: variant as u8,
            elevation_meters: 0.0,
        })
        .collect::<Vec<_>>();

    let sprites = world_sprite_frames(&art, &[terrain], &trees, &[], camera, 0);
    assert_eq!(sprites.len(), 512);
    let bare_trees = sprites
        .as_chunks::<2>()
        .0
        .iter()
        .filter(|pair| {
            let frame = (pair[1].1.atlas.uv[0] * 1000.0).round() as usize;
            matches!(frame, 3 | 5 | 8)
        })
        .count();
    assert_eq!(bare_trees, 18);
    for pair in sprites.as_chunks::<2>().0 {
        assert_eq!(pair[0].1.atlas.uv[0], pair[1].1.atlas.uv[0]);
    }
}
