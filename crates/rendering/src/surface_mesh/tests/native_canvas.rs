use super::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen_test]
fn canvas_keeps_an_alpha_bearing_native_terrain_diamond_aligned_to_the_tile() {
    let document = web_sys::window().unwrap().document().unwrap();
    let atlas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    atlas.set_width(GAME_ATLAS_SIDE);
    atlas.set_height(GAME_ATLAS_SIDE);
    let atlas_context: web_sys::CanvasRenderingContext2d = atlas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    atlas_context.set_fill_style_str("#ffffff");
    atlas_context.begin_path();
    atlas_context.move_to(48.0, 0.0);
    atlas_context.line_to(96.0, 24.0);
    atlas_context.line_to(48.0, 48.0);
    atlas_context.line_to(0.0, 24.0);
    atlas_context.close_path();
    atlas_context.fill();

    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(256);
    canvas.set_height(128);
    let context: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let art = test_art(GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: [
                0.0,
                0.0,
                96.0 / GAME_ATLAS_SIDE as f32,
                48.0 / GAME_ATLAS_SIDE as f32,
            ],
        },
        size: [96.0, 48.0],
        anchor: [48.0, 24.0],
    });
    let camera = SceneCamera {
        center: [0.5, 0.5],
        zoom: 1.0,
        viewport: [256.0, 128.0],
        focus_elevation_meters: 0.0,
    };
    let mut triangles = projected_surface_triangles(
        &[SceneTerrain {
            appearance: None,
            position: [0.5, 0.5],
            material: 0,
            elevation_meters: 0.0,
            surface: SceneTerrainSurface::flat(0.0),
        }],
        camera,
    );
    apply_terrain_textures(&mut triangles, &art);
    let atlases: [web_sys::HtmlCanvasElement; 5] = std::array::from_fn(|_| atlas.clone());
    for triangle in &triangles {
        draw_surface_triangle(&context, &atlases, triangle).unwrap();
    }

    for (x, y) in [(128, 34), (180, 60), (128, 94), (76, 64), (128, 64)] {
        let pixel = context
            .get_image_data(f64::from(x), f64::from(y), 1.0, 1.0)
            .unwrap();
        assert!(pixel.data().0[3] > 0, "transparent tile edge at {x},{y}");
    }
    let outside = context.get_image_data(128.0, 30.0, 1.0, 1.0).unwrap();
    assert_eq!(outside.data().0[3], 0);
}
