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
            for variant in 0..=255 {
                assert!(
                    scene_resource_presentation(&art, fixture::resource(family, variant)).is_none()
                );
                assert!(fixture::drawn(&art, &[fixture::resource(family, variant)]).is_empty());
            }
        }
        art.tree_families[slot].clear();
        for variant in 0..=255 {
            let (body, shadow) =
                scene_resource_presentation(&art, fixture::resource(family, variant)).unwrap();
            let index = [0, 1, 2, 4, 6, 7, 9, 10, 11, 12, 13][usize::from(variant) % 11];
            assert_eq!(body.atlas, art.resources[1][index].atlas);
            assert_eq!(shadow.unwrap().atlas, art.tree_shadows[index].atlas);
        }
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
        assert!(
            drawn
                .iter()
                .any(|(_, frame, _, _)| frame.size == native.size && frame.anchor == anchor)
        );
        assert!(
            fixture::drawn(&art, &[]).is_empty(),
            "removing resource removes body and silhouette"
        );
    }
}
