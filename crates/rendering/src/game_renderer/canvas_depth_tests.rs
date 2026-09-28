#![cfg(test)]

use super::*;
use crate::surface_mesh::{ProjectedSurfaceTriangle, SurfacePoint};
use aoe_core::ScreenPoint;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn canvas_depth_resolves_crossing_surfaces_at_each_pixel() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let camera = test_camera();
    let layers = [
        WorldLayer::Surface(triangle(
            [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
            [0.0, 0.0, 20.0],
            [1.0, 0.0, 0.0],
        )),
        WorldLayer::Surface(triangle(
            [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
            [10.0; 3],
            [0.0, 0.0, 1.0],
        )),
    ];
    let (mut color, mut depth) = (Vec::new(), Vec::new());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut color,
        &mut depth,
        &layers,
        camera,
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&color, 64, 30), [0, 0, 255, 255]);
    assert_eq!(pixel(&color, 64, 90), [255, 0, 0, 255]);
}

#[wasm_bindgen_test]
fn canvas_continuous_shared_triangle_edges_leave_no_background_cracks() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let camera = test_camera();
    let corners = [[16.0, 16.0], [112.0, 16.0], [112.0, 112.0], [16.0, 112.0]];
    let layers = [
        WorldLayer::Surface(triangle(
            [corners[0], corners[1], corners[2]],
            [0.0; 3],
            [0.4, 0.8, 0.2],
        )),
        WorldLayer::Surface(triangle(
            [corners[0], corners[2], corners[3]],
            [0.0; 3],
            [0.2, 0.5, 0.8],
        )),
    ];
    let (mut color, mut depth) = (Vec::new(), Vec::new());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut color,
        &mut depth,
        &layers,
        camera,
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    for y in 16..112 {
        for x in 16..112 {
            assert_ne!(pixel(&color, x, y), [41, 74, 36, 255], "crack at {x},{y}");
        }
    }
}

#[wasm_bindgen_test]
fn transparent_sprite_texels_do_not_occlude_terrain() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let camera = test_camera();
    let sprite = crate::web::Sprite {
        position: [0.0; 2],
        radius: [0.25; 2],
        color: [1.0; 4],
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        depths: [0.0; 4],
    };
    let frame = crate::GameFrame {
        uv: sprite.uv,
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let layers = [
        WorldLayer::Surface(triangle(
            [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
            [0.0; 3],
            [1.0, 0.0, 0.0],
        )),
        WorldLayer::Sprite(sprite, frame, 100.0),
    ];
    let atlas = vec![0; crate::GAME_ATLAS_SIDE as usize * crate::GAME_ATLAS_SIDE as usize * 4];
    let (mut color, mut depth) = (Vec::new(), Vec::new());

    let result = canvas_depth::render_canvas_world(
        &canvas, &context, &atlas, &mut color, &mut depth, &layers, camera, false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&color, 64, 64), [255, 0, 0, 255]);
    assert_eq!(depth[64 * 128 + 64], 0.0);
}

#[wasm_bindgen_test]
fn canvas_terrain_samples_the_native_atlas_and_applies_water_tint() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let mut water = triangle(
        [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
        [0.0; 3],
        [0.0; 3],
    );
    water.texture_uv = Some([0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0]);
    water.tint = 4;
    let layers = [WorldLayer::Surface(water)];
    let mut atlas = vec![0; crate::GAME_ATLAS_SIDE as usize * crate::GAME_ATLAS_SIDE as usize * 4];
    atlas[..4].copy_from_slice(&[100, 100, 100, 255]);
    let (mut color, mut depth) = (Vec::new(), Vec::new());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut color,
        &mut depth,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&color, 64, 64), [91, 102, 113, 255]);
}

#[wasm_bindgen_test]
fn canvas_sprite_flip_samples_atlas_texels_in_mirrored_order() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let sprite = crate::web::Sprite {
        position: [0.0; 2],
        radius: [0.25; 2],
        color: [1.0; 4],
        uv: [2.0 / 2048.0, 0.0, -2.0 / 2048.0, 1.0 / 2048.0],
        depths: [1.0; 4],
    };
    let frame = crate::GameFrame {
        uv: sprite.uv,
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let atlas_len = crate::GAME_ATLAS_SIDE as usize * crate::GAME_ATLAS_SIDE as usize * 4;
    let mut atlas = vec![0; atlas_len];
    atlas[..4].copy_from_slice(&[255, 0, 0, 255]);
    atlas[4..8].copy_from_slice(&[0, 0, 255, 255]);
    let layers = [WorldLayer::Sprite(sprite, frame, 1.0)];
    let (mut color, mut depth) = (Vec::new(), Vec::new());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut color,
        &mut depth,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&color, 54, 64), [0, 0, 255, 255]);
    assert_eq!(pixel(&color, 74, 64), [255, 0, 0, 255]);
}

#[wasm_bindgen_test]
fn canvas_flat_background_is_textured_and_stays_behind_world_sprites() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let background = crate::web::Sprite {
        position: [0.0; 2],
        radius: [0.25; 2],
        color: [1.0; 4],
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        depths: [0.0; 4],
    };
    let frame = crate::GameFrame {
        uv: background.uv,
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let unit = crate::web::Sprite {
        radius: [0.125; 2],
        uv: [1.0 / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        ..background
    };
    let mut atlas = vec![0; crate::GAME_ATLAS_SIDE as usize * crate::GAME_ATLAS_SIDE as usize * 4];
    atlas[..4].copy_from_slice(&[70, 120, 55, 255]);
    atlas[4..8].copy_from_slice(&[0, 0, 255, 255]);
    let layers = [
        WorldLayer::Sprite(
            unit,
            crate::GameFrame {
                size: [16.0; 2],
                ..frame
            },
            0.0,
        ),
        WorldLayer::Sprite(background, frame, f64::NEG_INFINITY),
    ];
    let (mut color, mut depth) = (Vec::new(), Vec::new());
    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut color,
        &mut depth,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");
    assert_eq!(pixel(&color, 54, 64), [70, 120, 55, 255]);
    assert_eq!(pixel(&color, 64, 64), [0, 0, 255, 255]);
}

fn target_canvas() -> Option<(
    web_sys::HtmlCanvasElement,
    web_sys::CanvasRenderingContext2d,
)> {
    let document = web_sys::window()?.document()?;
    let canvas = document
        .create_element("canvas")
        .ok()?
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .ok()?;
    canvas.set_width(128);
    canvas.set_height(128);
    let context = super::context(&canvas).ok()?;
    Some((canvas, context))
}

fn test_camera() -> SceneCamera {
    SceneCamera {
        center: [0.0; 2],
        zoom: 1.0,
        viewport: [128.0; 2],
        focus_elevation_meters: 0.0,
    }
}

fn triangle(
    screen: [[f64; 2]; 3],
    elevation: [f64; 3],
    color: [f32; 3],
) -> ProjectedSurfaceTriangle {
    let points = std::array::from_fn(|index| SurfacePoint {
        world: [0.0, 0.0, elevation[index]],
        screen: ScreenPoint {
            x: screen[index][0],
            y: screen[index][1],
        },
    });
    ProjectedSurfaceTriangle {
        points,
        color,
        tile: [0, 0],
        skirt: false,
        material: 0,
        texture_mode: 4,
        tint: 0,
        texture_uv: None,
        pickable: true,
        order: 0,
    }
}

fn pixel(image: &[u8], x: u32, y: u32) -> [u8; 4] {
    let offset = (y as usize * 128 + x as usize) * 4;
    [
        image[offset],
        image[offset + 1],
        image[offset + 2],
        image[offset + 3],
    ]
}
