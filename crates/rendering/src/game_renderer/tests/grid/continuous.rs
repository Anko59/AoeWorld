use super::*;

#[wasm_bindgen_test]
fn canvas_grid_continuous_diagonal_alpha_gap_and_css_width_toggle() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let context = context(&canvas).unwrap();
    let mut c = camera();
    c.center = [0.0; 2];
    let b = TileRect::from_xywh(-4, -4, 8, 8);
    let pixel = |x, y| context.get_image_data(x, y, 1.0, 1.0).unwrap().data().0;
    let clear = || {
        context.set_fill_style_str("rgb(41,74,36)");
        context.fill_rect(0.0, 0.0, 128.0, 128.0);
    };
    clear();
    game_grid::draw_grid(&context, [canvas.width(), canvas.height()], c, b);
    for x in (16..=48).step_by(2) {
        let on = pixel(f64::from(x), f64::from(x / 2 + 32));
        assert!(
            on[1] >= 66 && on[1] < 74,
            "continuous alpha diagonal at {x}: {on:?}"
        );
        assert!(
            on[0] > 31 && on[2] > 31,
            "stroke retains background rather than opaque grid color"
        );
        assert_eq!(on[3], 255);
        assert_eq!(
            &pixel(f64::from(x), f64::from(x / 2 + 35))[..],
            &[41, 74, 36, 255]
        );
    }
    clear();
    for x in (16..=48).step_by(2) {
        assert_eq!(
            &pixel(f64::from(x), f64::from(x / 2 + 32))[..],
            &[41, 74, 36, 255]
        );
    }
    // The same 1 CSS-pixel geometry occupies two backing pixels at scale two,
    // not a thinner one-backing-pixel stroke. No caller transform is retained.
    canvas.set_width(256);
    canvas.set_height(256);
    context.set_fill_style_str("rgb(41,74,36)");
    context.fill_rect(0.0, 0.0, 256.0, 256.0);
    game_grid::draw_grid(&context, [canvas.width(), canvas.height()], c, b);
    let on = pixel(64.0, 96.0);
    assert!(on[1] < 74);
    assert_eq!(&pixel(64.0, 103.0)[..], &[41, 74, 36, 255]);
}

#[wasm_bindgen_test]
fn canvas_grid_crossing_composites_each_strip_and_rejects_invalid_scale_without_state_changes() {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    let context = context(&canvas).unwrap();
    let mut c = camera();
    c.center = [-3.0 / 256.0, -1.0 / 256.0];
    let b = TileRect::from_xywh(-4, -4, 8, 8);
    for backing in [128, 256] {
        canvas.set_width(backing);
        canvas.set_height(backing);
        context.set_fill_style_str("rgb(41,74,36)");
        context.fill_rect(0.0, 0.0, f64::from(backing), f64::from(backing));
        context.set_line_width(7.0);
        context.set_line_cap("round");
        game_grid::draw_grid(&context, [canvas.width(), canvas.height()], c, b);
        let point = f64::from(backing / 2);
        let on = context
            .get_image_data(point, point, 1.0, 1.0)
            .unwrap()
            .data()
            .0;
        // Canvas edge coverage is antialiased; the two strokes still composite
        // independently like GPU strips (.36 nominal opacity). A combined path
        // would union coverage and give green ~67, not this double-alpha range.
        assert!(
            (61..=65).contains(&on[1]),
            "crossing at backing {backing}: {on:?}"
        );
        assert!(on[0] > 31 && on[2] > 31);
        assert_eq!(on[3], 255);
        assert_eq!(context.line_width(), 7.0);
        assert_eq!(context.line_cap(), "round");
        context.fill_rect(0.0, 0.0, f64::from(backing), f64::from(backing));
        let mut invalid = c;
        invalid.viewport[0] = f64::MIN_POSITIVE;
        assert!((f64::from(backing) / invalid.viewport[0]).is_infinite());
        game_grid::draw_grid(&context, [canvas.width(), canvas.height()], invalid, b);
        let off = context
            .get_image_data(point, point, 1.0, 1.0)
            .unwrap()
            .data()
            .0;
        assert_eq!(&off[..], &[41, 74, 36, 255]);
        assert_eq!(context.line_width(), 7.0);
        assert_eq!(context.line_cap(), "round");
    }
}
