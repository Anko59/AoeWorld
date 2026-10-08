#![cfg(test)]
#[path = "species/canvas.rs"]
mod species_pixels;
#[path = "terrain_blend.rs"]
mod terrain_blend;
#[path = "terrain_filter.rs"]
mod terrain_filter;

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
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &layers,
        camera,
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&presentation.color_buffer, 64, 30), [0, 0, 255, 255]);
    assert_eq!(pixel(&presentation.color_buffer, 64, 90), [255, 0, 0, 255]);
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
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &layers,
        camera,
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    for y in 16..112 {
        for x in 16..112 {
            assert_ne!(
                pixel(&presentation.color_buffer, x, y),
                [41, 74, 36, 255],
                "crack at {x},{y}"
            );
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
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    };
    let frame = crate::GameFrame {
        atlas: crate::AtlasAddress {
            page: sprite.pages[0],
            uv: sprite.uv,
        },
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let layers = [
        WorldLayer::Surface(triangle(
            [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
            [0.0; 3],
            [1.0, 0.0, 0.0],
        )),
        WorldLayer::Sprite(sprite, frame, 100.0, 0),
    ];
    let atlas = vec![0; crate::GAME_ATLAS_BYTES];
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &layers,
        camera,
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&presentation.color_buffer, 64, 64), [255, 0, 0, 255]);
    assert_eq!(presentation.depth_buffer[64 * 128 + 64], 0.0);
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
    water.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    });
    water.tint = 4;
    let layers = [WorldLayer::Surface(water)];
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..4].copy_from_slice(&[100, 100, 100, 255]);
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(
        pixel(&presentation.color_buffer, 64, 64),
        [91, 102, 113, 255]
    );
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
        terrain_blend: [[0.0; 4]; 2],
        pages: [2, 0, 0, 0],
    };
    let frame = crate::GameFrame {
        atlas: crate::AtlasAddress {
            page: sprite.pages[0],
            uv: sprite.uv,
        },
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let atlas_len = crate::GAME_ATLAS_BYTES;
    let mut atlas = vec![0; atlas_len];
    atlas[2 * crate::GAME_ATLAS_PAGE_BYTES..2 * crate::GAME_ATLAS_PAGE_BYTES + 4]
        .copy_from_slice(&[255, 0, 0, 255]);
    atlas[2 * crate::GAME_ATLAS_PAGE_BYTES + 4..2 * crate::GAME_ATLAS_PAGE_BYTES + 8]
        .copy_from_slice(&[0, 0, 255, 255]);
    let layers = [WorldLayer::Sprite(sprite, frame, 1.0, 0)];
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());

    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");

    assert_eq!(pixel(&presentation.color_buffer, 54, 64), [0, 0, 255, 255]);
    assert_eq!(pixel(&presentation.color_buffer, 74, 64), [255, 0, 0, 255]);
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
        terrain_blend: [[0.0; 4]; 2],
        pages: [0; 4],
    };
    let frame = crate::GameFrame {
        atlas: crate::AtlasAddress {
            page: 0,
            uv: background.uv,
        },
        size: [32.0; 2],
        anchor: [16.0; 2],
    };
    let unit = crate::web::Sprite {
        radius: [0.125; 2],
        uv: [1.0 / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
        ..background
    };
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
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
            0,
        ),
        WorldLayer::Sprite(background, frame, f64::NEG_INFINITY, 0),
    ];
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let result = canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &atlas,
        &mut presentation,
        &layers,
        test_camera(),
        false,
    );
    assert!(result.is_ok(), "Canvas render failed: {result:?}");
    assert_eq!(
        pixel(&presentation.color_buffer, 54, 64),
        [70, 120, 55, 255]
    );
    assert_eq!(pixel(&presentation.color_buffer, 64, 64), [0, 0, 255, 255]);
}

#[wasm_bindgen_test]
fn canvas_reuses_image_data_and_pixels_while_updating_frame_bytes() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    let camera = test_camera();
    let no_layers = [];
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &no_layers,
        camera,
        false,
    )
    .expect("initial Canvas frame should render");

    let first_image: wasm_bindgen::JsValue =
        presentation.image_data.as_ref().unwrap().clone().into();
    let first_pixels: wasm_bindgen::JsValue =
        presentation.image_pixels.as_ref().unwrap().clone().into();
    let color_address = presentation.color_buffer.as_ptr();
    let depth_address = presentation.depth_buffer.as_ptr();
    let color_capacity = presentation.color_buffer.capacity();
    let depth_capacity = presentation.depth_buffer.capacity();
    assert_eq!(presentation.image_pixels.as_ref().unwrap().get_index(0), 41);

    let ground = [WorldLayer::Surface(triangle(
        [[20.0, 20.0], [108.0, 20.0], [64.0, 108.0]],
        [0.0; 3],
        [0.2, 0.4, 0.8],
    ))];
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &ground,
        camera,
        false,
    )
    .expect("second Canvas frame should render");

    assert!(js_sys::Object::is(
        &first_image,
        &presentation.image_data.as_ref().unwrap().clone().into(),
    ));
    assert!(js_sys::Object::is(
        &first_pixels,
        &presentation.image_pixels.as_ref().unwrap().clone().into(),
    ));
    assert_eq!(presentation.color_buffer.as_ptr(), color_address);
    assert_eq!(presentation.depth_buffer.as_ptr(), depth_address);
    assert_eq!(presentation.color_buffer.capacity(), color_capacity);
    assert_eq!(presentation.depth_buffer.capacity(), depth_capacity);

    let center = (64 * 128 + 64) * 4;
    assert_eq!(
        pixel(&presentation.color_buffer, 64, 64),
        [51, 102, 204, 255]
    );
    assert_eq!(
        presentation
            .image_pixels
            .as_ref()
            .unwrap()
            .get_index(center as u32),
        51
    );
    assert_eq!(
        presentation
            .image_pixels
            .as_ref()
            .unwrap()
            .get_index(center as u32 + 1),
        102
    );
    assert_eq!(
        presentation
            .image_pixels
            .as_ref()
            .unwrap()
            .get_index(center as u32 + 2),
        204
    );
    assert_eq!(
        presentation
            .image_pixels
            .as_ref()
            .unwrap()
            .get_index(center as u32 + 3),
        255
    );

    canvas.set_width(32);
    canvas.set_height(32);
    let small_camera = SceneCamera {
        viewport: [32.0; 2],
        ..camera
    };
    canvas_depth::render_canvas_world(
        &canvas,
        &context,
        &[],
        &mut presentation,
        &no_layers,
        small_camera,
        false,
    )
    .expect("resized Canvas frame should render");
    assert!(!js_sys::Object::is(
        &first_image,
        &presentation.image_data.as_ref().unwrap().clone().into(),
    ));
    assert!(presentation.color_buffer.capacity() < color_capacity);
    assert!(presentation.depth_buffer.capacity() < depth_capacity);
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
        appearance: 0,
        floor_strengths: None,
        points,
        color,
        tile: [0, 0],
        skirt: false,
        material: 0,
        texture_mode: 4,
        tint: 0,
        texture_uv: None,
        texture_blend: None,
        texture_tile: [0; 2],
        texture_materials: None,
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
