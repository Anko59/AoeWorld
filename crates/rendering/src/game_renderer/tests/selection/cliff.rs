use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, SurfacePoint};
use aoe_core::ScreenPoint;

fn triangle(skirt: bool, height: f64) -> ProjectedSurfaceTriangle {
    let point = |x: f64, y: f64| SurfacePoint {
        world: [1.0, 1.0, height],
        screen: ScreenPoint { x, y },
    };
    ProjectedSurfaceTriangle {
        appearance: 0,
        floor_strengths: None,
        mix: [0; 6],
        points: [point(100.0, 10.0), point(156.0, 10.0), point(128.0, 70.0)],
        color: [0.2, 0.2, 0.2],
        tile: [1, 1],
        skirt,
        material: 4,
        texture_mode: 4,
        tint: 3,
        texture_uv: None,
        texture_blend: None,
        texture_tile: [0; 2],
        texture_materials: None,
        pickable: !skirt,
        order: 0,
    }
}

#[wasm_bindgen_test]
fn only_a_nearer_cliff_face_hides_a_resource_under_the_pointer() {
    let pointer = ScreenPoint { x: 128.0, y: 32.0 };
    // Resource depth 4: a cliff face rising to depth 2 + 2 * 8 is in front.
    assert!(crate::surface_mesh::cliff_face_in_front(
        &[triangle(true, 8.0)],
        pointer,
        4.0
    ));
    // A cliff behind the resource, ordinary ground, or no surface does not.
    assert!(!crate::surface_mesh::cliff_face_in_front(
        &[triangle(true, 0.0)],
        pointer,
        4.0
    ));
    assert!(!crate::surface_mesh::cliff_face_in_front(
        &[triangle(false, 8.0)],
        pointer,
        4.0
    ));
    assert!(!crate::surface_mesh::cliff_face_in_front(&[], pointer, 4.0));
}
