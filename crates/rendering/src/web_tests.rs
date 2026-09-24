use super::*;
use aoe_core::ScreenPoint;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn instance_capacity_includes_selection_ring_sprites() {
    let camera = crate::SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let ring = crate::game_grid::selection_ring(camera, [0.5, 0.5], 0.0);

    assert_eq!(ring.len(), 32);
    assert_eq!(
        GPU_INSTANCE_CAPACITY,
        CAPACITY + crate::surface_mesh::MAX_SURFACE_TRIANGLES + ring.len()
    );
    assert!(ensure_instance_capacity(GPU_INSTANCE_CAPACITY, GPU_INSTANCE_CAPACITY).is_ok());
    assert!(ensure_instance_capacity(GPU_INSTANCE_CAPACITY + 1, GPU_INSTANCE_CAPACITY).is_err());
}

#[wasm_bindgen_test]
fn terrain_instance_sentinel_cannot_match_atlas_uv_rectangles() {
    let regular_atlas_frame = Sprite {
        position: [0.0; 2],
        radius: [1.0; 2],
        color: [1.0; 4],
        uv: [0.1, 0.2, 0.3, 0.4],
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
    let terrain = surface_instance(&triangle, [256.0, 128.0]);

    assert!(regular_atlas_frame.color[3] >= 0.0);
    assert!(horizontally_flipped_atlas_frame.color[3] >= 0.0);
    assert!(terrain.color[3] < 0.0);
    assert_eq!(terrain.uv, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(terrain.position, [-1.0, 1.0]);
    assert_eq!(terrain.radius, [0.0, 1.0]);
    assert_eq!(terrain.color[..2], [-0.5, 0.0]);
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
