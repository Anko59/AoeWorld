use super::*;

#[wasm_bindgen(inline_js = "
export function webgl_composite_turns() {
    return new Promise(resolve => requestAnimationFrame(() =>
        requestAnimationFrame(() => setTimeout(resolve, 0))));
}
")]
extern "C" {
    fn webgl_composite_turns() -> js_sys::Promise;
}

#[wasm_bindgen_test]
async fn webgl_static_sprite_and_placement_pixels_survive_idle_compositor_turns() {
    let (canvas, mut renderer) = target(&atlas(&[[65, 145, 245, 255]]));
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .append_child(&canvas)
        .unwrap();
    for color in [[1.0; 4], [1.0, 1.0, 1.0, 0.5], [3.0, 0.2, 0.2, 0.5]] {
        let mut body = sprite(0.0, 1.0);
        body.color = color;
        render(&mut renderer, &mut [body]);
        let displayed = pixel(&canvas, 64, 64);
        assert!(displayed[2] > 0);
        // The production client intentionally skips unchanged frames. A late
        // capture must retain the same native sprite/ghost without a redraw.
        wasm_bindgen_futures::JsFuture::from(webgl_composite_turns())
            .await
            .unwrap();
        assert_pixel(&canvas, 64, 64, displayed);
    }
    canvas.remove();
}
