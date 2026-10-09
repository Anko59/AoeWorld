use super::*;

#[path = "terrain_blend/readback.rs"]
mod readback;
use readback::PixelReadback;

#[wasm_bindgen_test]
async fn webgpu_crossfades_the_same_native_material_pixel_as_canvas() {
    let mut renderer = surface_renderer().await;
    check_surface_pixel(&mut renderer, 0, true, [82, 86, 86, 255]).await;
    renderer.device.destroy();
}

#[wasm_bindgen_test]
async fn webgpu_procedural_rock_snow_ice_mud_and_shore_match_shared_kernel() {
    // One device/atlas for every probe: context churn is not this pixel contract.
    let mut renderer = surface_renderer().await;
    for tint in (5..=10).chain([12]).chain(21..=26) {
        check_surface_pixel(
            &mut renderer,
            tint,
            false,
            crate::surface_mesh::procedural_tint([255, 0, 0, 255], tint),
        )
        .await;
    }
    renderer.device.destroy();
}

pub(super) async fn surface_renderer() -> Renderer {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer = Renderer::new(canvas)
        .await
        .expect("software WebGPU renderer");
    let diagnostic = renderer.render_sprites(&[]).unwrap();
    assert_eq!(diagnostic.atlas_pages, 1);
    assert_eq!(diagnostic.atlas_bytes, 8 * 8 * 4);
    let mut atlas = vec![0; (3 * crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize];
    atlas[..12].copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    let page_bytes = (crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize;
    atlas[page_bytes..page_bytes + 4].copy_from_slice(&[0, 255, 0, 255]);
    atlas[page_bytes * 2..page_bytes * 2 + 12]
        .copy_from_slice(&[0, 0, 255, 255, 255, 255, 0, 255, 0, 0, 0, 128]);
    renderer.upload_game_atlas(&atlas).unwrap();
    renderer
}

async fn check_surface_pixel(renderer: &mut Renderer, tint: u8, blend: bool, expected: [u8; 4]) {
    let rect = |x: f32| crate::AtlasAddress {
        page: 0,
        uv: [x / 2048.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
    };
    let mut triangle = capacity_surface();
    triangle.points = [[16.0, 16.0], [112.0, 16.0], [16.0, 112.0]].map(surface_point);
    triangle.texture_uv = Some(rect(0.0));
    triangle.tint = tint;
    triangle.texture_blend = blend.then_some([rect(1.0), rect(2.0)]);
    let instance = surface_instance(&triangle, [128.0; 2], 0.0);
    assert_eq!(instance.color[3], if blend { -3.0 } else { -1.0 });
    renderer
        .render_world_layers(&[triangle], &[], [0.0, 0.0, 0.0, 1.0])
        .unwrap();

    let pixel = read_pixel(renderer, 1, [48, 48]).await;
    assert_pixel(pixel, expected);
}

pub(super) fn assert_pixel(pixel: [u8; 4], expected: [u8; 4]) {
    for (actual, expected) in pixel.into_iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "unexpected GPU blend pixel: {pixel:?}"
        );
    }
}

pub(super) async fn read_pixel(renderer: &Renderer, count: u32, point: [u32; 2]) -> [u8; 4] {
    // Execute the same pipeline/instances into an explicit GPU attachment. A
    // buffer copy reads actual shader pixels without compositor canvas expiry.
    let mut readback = PixelReadback::<1>::new(renderer);
    readback
        .read(renderer, count, [point], [0.0, 0.0, 0.0, 1.0])
        .await[0]
}
