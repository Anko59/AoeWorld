use super::*;
use aoe_core::ScreenPoint;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn instance_buffer_grows_at_capacity_boundaries_with_layer_pressure() {
    let Some(window) = web_sys::window() else {
        assert!(false, "browser window is unavailable");
        return;
    };
    let Some(document) = window.document() else {
        assert!(false, "browser document is unavailable");
        return;
    };
    let Ok(element) = document.create_element("canvas") else {
        assert!(false, "canvas element cannot be created");
        return;
    };
    let Ok(canvas) = element.dyn_into::<web_sys::HtmlCanvasElement>() else {
        assert!(false, "canvas element has the wrong type");
        return;
    };
    canvas.set_width(64);
    canvas.set_height(64);
    let Ok(mut renderer) = Renderer::new(canvas).await else {
        assert!(false, "software WebGPU renderer is unavailable");
        return;
    };
    let camera = crate::SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [64.0, 64.0],
        focus_elevation_meters: 0.0,
    };
    let surfaces = vec![capacity_surface(); crate::surface_mesh::MAX_SURFACE_TRIANGLES];
    let objects = vec![solid_sprite(); CAPACITY];
    let ring = crate::game_grid::selection_ring(camera, [0.5, 0.5], 0.0)
        .into_iter()
        .map(|(sprite, _)| sprite)
        .collect::<Vec<_>>();
    let grid = crate::game_grid::grid_sprites(camera);
    assert_eq!(ring.len(), crate::game_grid::SELECTION_RING_SPRITES);
    assert!(!grid.is_empty());
    let mut sprites = objects;
    sprites.extend(ring);
    sprites.extend(grid);
    let Ok(supported) = instance_buffer::required_capacity(&surfaces, &sprites) else {
        assert!(
            false,
            "supported object, grid, and ring lists exceed the instance bound"
        );
        return;
    };
    assert!(supported > instance_buffer::INITIAL_CAPACITY);
    sprites.resize(supported + 1, solid_sprite());

    for (offset, expected_capacity) in [
        (supported - 1, supported - 1),
        (supported, supported),
        (supported + 1, supported + 1),
    ] {
        let sprite_count = offset - surfaces.len();
        let Ok(required) = instance_buffer::required_instance_count(surfaces.len(), sprite_count)
        else {
            assert!(false, "supported instance count was rejected");
            return;
        };
        assert_eq!(required, offset);
        let Ok(counters) =
            renderer.render_world_layers(&surfaces, &sprites[..sprite_count], [0.0; 4])
        else {
            assert!(false, "supported instance frame was rejected");
            return;
        };
        assert_eq!(counters.draw_calls, 1);
        assert_eq!(renderer.instances.capacity(), expected_capacity);
        assert_eq!(
            counters.gpu_buffer_bytes,
            expected_capacity * std::mem::size_of::<Sprite>()
        );
    }
    assert!(instance_buffer::required_instance_count(instance_buffer::MAX_CAPACITY, 0).is_ok());
    assert!(
        instance_buffer::required_instance_count(instance_buffer::MAX_CAPACITY + 1, 0).is_err()
    );
}

#[wasm_bindgen_test]
fn terrain_instance_sentinel_cannot_match_atlas_uv_rectangles() {
    let regular_atlas_frame = Sprite {
        position: [0.0; 2],
        radius: [1.0; 2],
        color: [1.0; 4],
        uv: [0.1, 0.2, 0.3, 0.4],
        depths: [0.0; 4],
    };
    let horizontally_flipped_atlas_frame = Sprite {
        uv: [0.4, 0.2, -0.3, 0.4],
        ..regular_atlas_frame
    };
    let triangle = ProjectedSurfaceTriangle {
        points: [
            surface_point([0.0, 0.0]),
            surface_point([128.0, 0.0]),
            surface_point([64.0, 64.0]),
        ],
        color: [0.2, 0.3, 0.4],
        tile: [0, 0],
        skirt: false,
        material: 0,
        texture_mode: 1,
        tint: 4,
        texture_uv: Some([0.1, 0.2, 0.3, 0.4]),
        pickable: true,
        order: 0,
    };
    let terrain = surface_instance(&triangle, [256.0, 128.0], 0.0);

    assert!(regular_atlas_frame.color[3] >= 0.0);
    assert!(horizontally_flipped_atlas_frame.color[3] >= 0.0);
    assert!(terrain.color[3] < 0.0);
    assert_eq!(terrain.uv, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(terrain.position, [-1.0, 1.0]);
    assert_eq!(terrain.radius, [0.0, 1.0]);
    assert_eq!(terrain.color[..2], [-0.5, 0.0]);
}

#[wasm_bindgen_test]
fn depth_buffer_encoding_places_the_nearest_world_surface_first() {
    let mut background = solid_sprite();
    background.depths = [-2.0; 4];
    let mut foreground = solid_sprite();
    foreground.depths = [2.0; 4];
    let mut instances = [background, foreground];

    normalize_depths(&mut instances);

    assert_eq!(instances[0].depths[0], 1.0);
    assert_eq!(instances[1].depths[0], 0.0);
}

#[wasm_bindgen_test]
fn surface_instances_preserve_each_projected_vertex_depth() {
    let mut triangle = capacity_surface();
    triangle.points[0].world = [0.0, 0.0, 0.0];
    triangle.points[1].world = [0.0, 0.0, 1.0];
    triangle.points[2].world = [0.0, 0.0, 2.0];

    let instance = surface_instance(&triangle, [64.0, 64.0], 0.0);

    assert_eq!(instance.depths, [0.0, 2.0, 4.0, 0.0]);
}

#[wasm_bindgen_test]
fn cliff_skirts_depth_behind_walkable_surface_at_an_exact_tie() {
    let mut skirt = capacity_surface();
    skirt.skirt = true;
    for point in &mut skirt.points {
        point.world = [4.0, 8.0, 0.0];
    }

    let instance = surface_instance(&skirt, [64.0, 64.0], 12.0);

    assert_eq!(instance.depths, [-0.01, -0.01, -0.01, 0.0]);
}

#[wasm_bindgen_test]
fn overlay_only_depths_remain_finite_without_world_geometry() {
    let mut overlay = solid_sprite();
    overlay.depths = [f32::INFINITY; 4];
    let mut instances = [overlay];

    normalize_depths(&mut instances);

    assert_eq!(instances[0].depths, [0.0; 4]);
}

fn surface_point(screen: [f64; 2]) -> crate::surface_mesh::SurfacePoint {
    crate::surface_mesh::SurfacePoint {
        world: [0.0, 0.0, 0.0],
        screen: ScreenPoint {
            x: screen[0],
            y: screen[1],
        },
    }
}

fn capacity_surface() -> ProjectedSurfaceTriangle {
    ProjectedSurfaceTriangle {
        points: [
            surface_point([0.0, 0.0]),
            surface_point([64.0, 0.0]),
            surface_point([32.0, 64.0]),
        ],
        color: [0.2, 0.3, 0.4],
        tile: [0, 0],
        skirt: false,
        material: 0,
        texture_mode: 4,
        tint: 0,
        texture_uv: Some([0.1, 0.2, 0.3, 0.4]),
        pickable: true,
        order: 0,
    }
}

fn solid_sprite() -> Sprite {
    Sprite {
        position: [0.0, 0.0],
        radius: [0.01, 0.01],
        color: [1.0; 4],
        uv: [0.0, 0.0, 0.1, 0.1],
        depths: [0.0; 4],
    }
}
