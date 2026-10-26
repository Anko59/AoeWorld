use super::*;
use crate::game_renderer::canvas_depth::CanvasPresentation;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn streamed_pages_reach_canvas_in_order_and_replace_the_atlas_only_on_finish() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    let context = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    let mut renderer = GameRenderer::Canvas {
        canvas,
        context,
        atlas: [None, None, None, None],
        source_atlas: vec![9],
        presentation: CanvasPresentation::new(128, 128),
    };
    let mut upload = renderer.begin_game_atlas().unwrap();
    upload.page_buffer(0).unwrap()[0] = 1;
    renderer.write_game_atlas_page(&mut upload, 0).unwrap();
    // Out-of-order and unfinished uploads never disturb the old atlas.
    assert!(upload.page_buffer(2).is_err());
    assert!(renderer.write_game_atlas_page(&mut upload, 1).is_err());
    assert!(renderer.finish_game_atlas(upload).is_err());
    let GameRenderer::Canvas { source_atlas, .. } = &renderer else {
        unreachable!()
    };
    assert_eq!(source_atlas, &[9]);
    let mut upload = renderer.begin_game_atlas().unwrap();
    for layer in 0..crate::GAME_ATLAS_PAGES {
        upload.page_buffer(layer).unwrap()[0] = layer as u8 + 1;
        renderer.write_game_atlas_page(&mut upload, layer).unwrap();
    }
    renderer.finish_game_atlas(upload).unwrap();
    let GameRenderer::Canvas { source_atlas, .. } = &renderer else {
        unreachable!()
    };
    assert_eq!(source_atlas.len(), crate::GAME_ATLAS_BYTES);
    for layer in 0..crate::GAME_ATLAS_PAGES as usize {
        assert_eq!(
            source_atlas[layer * crate::GAME_ATLAS_PAGE_BYTES],
            layer as u8 + 1
        );
    }
}
