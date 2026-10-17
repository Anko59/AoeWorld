use super::*;
use wasm_bindgen::JsCast;

const RING: [u8; 4] = [242, 217, 89, 255];

fn flat_field(side: i32) -> Vec<SceneTerrain> {
    (0..side * side)
        .map(|index| SceneTerrain {
            appearance: None,
            position: [f64::from(index % side) + 0.5, f64::from(index / side) + 0.5],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        })
        .collect()
}

#[wasm_bindgen_test]
fn town_center_ellipse_lower_arc_is_drawn_over_flat_ground_on_canvas() {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.create_element("canvas").ok())
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .expect("browser canvas");
    let (width, height) = (640_u32, 480_u32);
    canvas.set_width(width);
    canvas.set_height(height);
    let context = crate::game_renderer::context(&canvas).expect("2d context");
    let camera = SceneCamera {
        center: [8.0, 8.0],
        zoom: 1.0,
        viewport: [f64::from(width), f64::from(height)],
        focus_elevation_meters: 0.0,
    };
    let center = SceneUnit {
        kind: SceneUnitKind::TownCenter,
        id: aoe_core::EntityId(1),
        position: [8.0, 8.0],
        moving: false,
        facing: 0,
        selected: true,
        elevation_meters: 0.0,
    };
    let surfaces = projected_surface_triangles(&flat_field(16), camera);
    let layers = ordered_world_layers(surfaces, Vec::new(), &[center], camera);
    let mut presentation = CanvasPresentation::new(width, height);
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &layers,
        camera,
        false,
    )
    .expect("canvas render");
    let at = |x: u32, y: u32| {
        let offset = (y as usize * width as usize + x as usize) * 4;
        presentation.color_buffer[offset..offset + 4].to_vec()
    };
    // Radii are [128, 64] pixels at zoom one; the arc crosses each column of
    // the lower half once, so one pixel in that stretch carries the colour.
    let (cx, cy) = (width / 2, height / 2);
    for (dx, below) in [(0_i32, 64_u32), (60, 56), (-60, 56), (90, 45), (-90, 45)] {
        let x = (cx as i32 + dx) as u32;
        let found = (cy + below - 3..=cy + below + 3).any(|y| at(x, y) == RING);
        assert!(found, "lower arc missing in column offset {dx}");
    }
}

#[wasm_bindgen_test]
fn canvas_scans_only_the_ellipse_bounding_box_not_twice_its_size() {
    let camera = SceneCamera {
        center: [8.0, 8.0],
        zoom: 1.0,
        viewport: [640.0, 480.0],
        focus_elevation_meters: 0.0,
    };
    let (sprite, depth) = crate::game_grid::selection_ring(camera, [8.0, 8.0], 0.0)[0];
    let bounds = canvas_depth::canvas_layer_bounds(
        &WorldLayer::Selection(sprite, depth),
        camera.viewport,
        640,
        480,
    )
    .expect("visible ring");
    // Radii are [24, 8] pixels plus the one-pixel stroke half width.
    assert!(bounds[2] - bounds[0] <= 2 * 25 + 2, "{bounds:?}");
    assert!(bounds[3] - bounds[1] <= 2 * 9 + 2, "{bounds:?}");
}
