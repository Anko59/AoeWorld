use super::*;
use aoe_rendering::{GameArt, GameFrame, SceneCamera, resource_sprite_bounds};

#[wasm_bindgen_test]
fn native_species_resource_state_depletion_precedes_actual_viewport_selection() {
    use aoe_protocol::{ResourceAmount, ResourceState};
    let (camera, scene) = test_camera();
    let mut art = resource_art(frame());
    let conifer = frame();
    let palm = GameFrame {
        anchor: [20.0, 175.0],
        size: [88.0, 168.0],
        atlas: aoe_rendering::AtlasAddress {
            uv: [0.1, 0.0, 0.1, 0.1],
            ..conifer.atlas
        },
    };
    art.tree_families = [vec![conifer; 9], vec![palm; 13]];
    let resources = [(12, 2, conifer), (14, 4, palm)].map(|(id, family, native)| SceneResource {
        kind: 1,
        visual_family: family,
        ..resource_at_sprite_center(camera, native, id, [640.0, 360.0])
    });
    let mut cache = crate::resource_state::ResourceStateCache::default();
    assert!(cache.apply(ResourceState {
        subscription_revision: 1,
        from_revision: None,
        revision: 2,
        changes: vec![
            ResourceAmount {
                id: 12,
                remaining: 100
            },
            ResourceAmount {
                id: 14,
                remaining: 100
            }
        ]
    }));
    let select = |cache: &crate::resource_state::ResourceStateCache| {
        super::super::resources::select_visible_resources(
            resources
                .into_iter()
                .filter(|resource| cache.visible(resource.id)),
            &art,
            scene,
        )
    };
    let selected = select(&cache);
    assert_eq!(selected.len(), 2);
    for (resource, native) in selected.iter().zip([conifer, palm]) {
        let Some((body, paired_shadow)) =
            aoe_rendering::scene_resource_presentation(&art, *resource)
        else {
            assert!(false, "visible native species must have a presentation");
            return;
        };
        assert_eq!(body.atlas, native.atlas);
        assert_eq!(body.anchor, native.anchor);
        assert!(
            paired_shadow.is_none(),
            "the matching native silhouette replaces broadleaf2296"
        );
    }
    assert!(cache.apply(ResourceState {
        subscription_revision: 1,
        from_revision: Some(2),
        revision: 3,
        changes: vec![
            ResourceAmount {
                id: 12,
                remaining: 0
            },
            ResourceAmount {
                id: 14,
                remaining: 0
            }
        ]
    }));
    assert!(!cache.visible(12) && !cache.visible(14));
    assert!(
        select(&cache).is_empty(),
        "depleted native tree DTOs never reach body/shadow drawing"
    );
}

fn resource_art(frame: GameFrame) -> GameArt {
    GameArt {
        walking: Vec::new(),
        standing: Vec::new(),
        grass: Vec::new(),
        terrain: std::array::from_fn(|_| Vec::new()),
        terrain_topology: [None; 7],
        terrain_world: None,
        resources: std::array::from_fn(|index| if index == 0 { vec![frame] } else { Vec::new() }),
        tree_shadows: Vec::new(),
        tree_families: Default::default(),
    }
}

fn frame() -> GameFrame {
    GameFrame {
        atlas: aoe_rendering::AtlasAddress {
            page: 2,
            uv: [0.0, 0.0, 1.0, 1.0],
        },
        size: [80.0, 100.0],
        anchor: [40.0, 80.0],
    }
}

fn test_camera() -> (Camera, SceneCamera) {
    let camera = Camera {
        center: [256.0, 256.0],
        zoom: 0.25,
        viewport: [1280.0, 720.0],
        focus_elevation_meters: 0.0,
    };
    let scene = SceneCamera {
        center: camera.center,
        zoom: camera.zoom,
        viewport: camera.viewport,
        focus_elevation_meters: camera.focus_elevation_meters,
    };
    (camera, scene)
}

fn resource_at_sprite_center(
    camera: Camera,
    frame: GameFrame,
    id: u64,
    center: [f64; 2],
) -> SceneResource {
    let contact_screen = ScreenPoint {
        x: center[0] + (f64::from(frame.anchor[0]) - f64::from(frame.size[0]) * 0.5) * camera.zoom,
        y: center[1] + (f64::from(frame.anchor[1]) - f64::from(frame.size[1]) * 0.5) * camera.zoom,
    };
    SceneResource {
        id,
        position: camera.screen_to_world_at_height(contact_screen, 0.0),
        kind: 0,
        visual_variant: 0,
        visual_family: 0,
        elevation_meters: 0.0,
    }
}

fn clipped_sprite_center(
    resource: SceneResource,
    frame: GameFrame,
    camera: SceneCamera,
) -> [f64; 2] {
    let Some([left, top, right, bottom]) = resource_sprite_bounds(resource, frame, camera) else {
        assert!(false, "selected resources have visible sprite bounds");
        return [0.0; 2];
    };
    [
        (left.max(0.0) + right.min(camera.viewport[0])) * 0.5,
        (top.max(0.0) + bottom.min(camera.viewport[1])) * 0.5,
    ]
}

#[wasm_bindgen_test]
fn dense_zoomed_out_resources_cover_all_viewport_quadrants() {
    let (camera, scene) = test_camera();
    let art = resource_art(frame());
    let mut resources = Vec::new();
    for row in 0..36 {
        for column in 0..64 {
            let center = [
                (f64::from(column) + 0.5) * scene.viewport[0] / 64.0,
                (f64::from(row) + 0.5) * scene.viewport[1] / 36.0,
            ];
            resources.push(resource_at_sprite_center(
                camera,
                frame(),
                resources.len() as u64,
                center,
            ));
        }
    }

    let selected = super::super::resources::select_visible_resources(resources, &art, scene);
    assert_eq!(
        selected.len(),
        36 * 64,
        "zoom must preserve every visible tree"
    );
    let mut quadrants = [0; 4];
    for resource in selected {
        let [x, y] = clipped_sprite_center(resource, frame(), scene);
        let quadrant = usize::from(y >= scene.viewport[1] * 0.5) * 2
            + usize::from(x >= scene.viewport[0] * 0.5);
        quadrants[quadrant] += 1;
    }
    assert!(quadrants.iter().all(|count| *count >= 200), "{quadrants:?}");
}

#[wasm_bindgen_test]
fn screen_culling_precedes_the_resource_budget_and_under_budget_nodes_are_kept() {
    let (camera, scene) = test_camera();
    let art = resource_art(frame());
    let mut resources = (0..2_048)
        .map(|id| resource_at_sprite_center(camera, frame(), id, [-100.0, 360.0]))
        .collect::<Vec<_>>();
    let visible = (0..400)
        .map(|index| {
            let column = index % 20;
            let row = index / 20;
            resource_at_sprite_center(
                camera,
                frame(),
                10_000 + index as u64,
                [
                    (f64::from(column) + 0.5) * scene.viewport[0] / 20.0,
                    (f64::from(row) + 0.5) * scene.viewport[1] / 20.0,
                ],
            )
        })
        .collect::<Vec<_>>();
    resources.extend(visible.iter().copied());

    let selected = super::super::resources::select_visible_resources(resources, &art, scene);
    assert_eq!(selected.len(), visible.len());
    assert!(selected.iter().all(|resource| resource.id >= 10_000));
    assert_eq!(
        super::super::resources::select_visible_resources(
            [
                resource_at_sprite_center(camera, frame(), 1, [600.0, 300.0]),
                resource_at_sprite_center(camera, frame(), 2, [605.0, 305.0]),
            ],
            &art,
            scene,
        )
        .len(),
        2,
        "all visible resources stay in view while under the budget"
    );
}
