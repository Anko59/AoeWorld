use super::*;

fn render(
    canvas: &web_sys::HtmlCanvasElement,
    context: &web_sys::CanvasRenderingContext2d,
    presentation: &mut CanvasPresentation,
    side: u32,
    layers: &[WorldLayer],
) {
    canvas.set_width(side);
    canvas.set_height(side);
    let camera = SceneCamera {
        viewport: [f64::from(side); 2],
        ..test_camera()
    };
    canvas_depth::render_canvas_world(canvas, context, &[], presentation, layers, camera, false)
        .expect("Canvas frame should render");
}

fn backing(presentation: &CanvasPresentation) -> wasm_bindgen::JsValue {
    presentation.image_pixels.as_ref().unwrap().buffer().into()
}

#[wasm_bindgen_test]
fn canvas_resize_cycles_reuse_the_js_image_backing() {
    let Some((canvas, context)) = target_canvas() else {
        assert!(false, "browser canvas is unavailable");
        return;
    };
    let mut presentation = CanvasPresentation::new(canvas.width(), canvas.height());
    render(&canvas, &context, &mut presentation, 128, &[]);
    let first_backing = backing(&presentation);
    let first_image: wasm_bindgen::JsValue =
        presentation.image_data.as_ref().unwrap().clone().into();

    let ground = [WorldLayer::Surface(triangle(
        [[10.0, 10.0], [110.0, 10.0], [60.0, 110.0]],
        [0.0; 3],
        [0.2, 0.4, 0.8],
    ))];
    for _ in 0..3 {
        render(&canvas, &context, &mut presentation, 120, &ground);
        assert!(js_sys::Object::is(&first_backing, &backing(&presentation)));
        assert!(!js_sys::Object::is(
            &first_image,
            &presentation.image_data.as_ref().unwrap().clone().into(),
        ));
        assert_eq!(
            presentation.image_pixels.as_ref().unwrap().length(),
            120 * 120 * 4
        );
        let shown = context
            .get_image_data(60.0, 30.0, 1.0, 1.0)
            .expect("presented pixel should read back")
            .data()
            .0;
        assert_eq!(shown, [51, 102, 204, 255]);
        let shown = context
            .get_image_data(2.0, 118.0, 1.0, 1.0)
            .expect("presented background should read back")
            .data()
            .0;
        assert_eq!(shown, [41, 74, 36, 255]);

        render(&canvas, &context, &mut presentation, 128, &[]);
        assert!(js_sys::Object::is(&first_backing, &backing(&presentation)));
        assert_eq!(
            presentation.image_pixels.as_ref().unwrap().length(),
            128 * 128 * 4
        );
    }

    // A large shrink or any growth still releases the old browser array.
    render(&canvas, &context, &mut presentation, 32, &[]);
    assert!(!js_sys::Object::is(&first_backing, &backing(&presentation)));
    let small_backing = backing(&presentation);
    render(&canvas, &context, &mut presentation, 48, &[]);
    assert!(!js_sys::Object::is(&small_backing, &backing(&presentation)));
}
