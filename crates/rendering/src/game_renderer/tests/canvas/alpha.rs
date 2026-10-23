use super::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn solid_canvas_alpha_blends_identically_and_restores_the_context() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    canvas.set_width(2);
    canvas.set_height(1);
    let context: CanvasRenderingContext2d = canvas
        .get_context("2d")
        .unwrap()
        .unwrap()
        .dyn_into()
        .unwrap();
    context.set_fill_style_str("rgb(0,0,255)");
    context.fill_rect(0.0, 0.0, 2.0, 1.0);
    // Existing global alpha still multiplies the sprite alpha, and is restored.
    context.set_global_alpha(0.5);
    fill_solid(&context, [1.0, 0.0, 0.0, 0.5], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(context.global_alpha(), 0.5);
    context.set_global_alpha(1.0);
    fill_solid(&context, [0.0, 1.0, 0.0, 1.0], [1.0, 0.0, 1.0, 1.0]);
    let pixels = context.get_image_data(0.0, 0.0, 2.0, 1.0).unwrap().data().0;
    assert!((63..=65).contains(&pixels[0]));
    assert_eq!(pixels[1], 0);
    assert!((190..=192).contains(&pixels[2]));
    assert_eq!(pixels[3], 255);
    assert_eq!(&pixels[4..8], &[0, 255, 0, 255]);
}
