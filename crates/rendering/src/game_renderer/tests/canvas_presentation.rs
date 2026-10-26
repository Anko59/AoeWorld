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

#[wasm_bindgen_test]
fn damaged_canvas_frames_match_fresh_rasters_with_alpha_depth_fog_and_removals() {
    let (canvas, context) = target_canvas().unwrap();
    let mut cached = CanvasPresentation::new(128, 128);
    let mut atlas = vec![0; crate::GAME_ATLAS_BYTES];
    atlas[..4].copy_from_slice(&[20, 100, 240, 128]);
    for y in 16..48 {
        for x in 16..48 {
            let at = (y * 2048 + x) * 4;
            atlas[at..at + 4].copy_from_slice(&[
                (x * 5) as u8,
                (y * 3) as u8,
                ((x ^ y) * 4) as u8,
                255,
            ]);
        }
    }
    let camera = test_camera();
    let mut ground = triangle(
        [[0.0, 0.0], [128.0, 0.0], [64.0, 128.0]],
        [0.0, 5.0, 10.0],
        [0.4, 0.8, 0.2],
    );
    ground.texture_uv = Some(crate::AtlasAddress {
        page: 0,
        uv: [16.0 / 2048.0, 16.0 / 2048.0, 32.0 / 2048.0, 32.0 / 2048.0],
    });
    let base = WorldLayer::Surface(ground);
    for step in 0..16 {
        let mut layers = vec![base];
        if step % 4 != 3 {
            let sprite = crate::web::Sprite {
                position: [(step as f32 - 8.0) / 32.0, 0.0],
                radius: [0.125; 2],
                color: [1.0; 4],
                uv: [0.0, 0.0, 1.0 / 2048.0, 1.0 / 2048.0],
                depths: [0.0; 4],
                terrain_blend: [[0.0; 4]; 2],
                pages: [0; 4],
            };
            layers.push(WorldLayer::Sprite(
                sprite,
                crate::GameFrame {
                    atlas: crate::AtlasAddress {
                        page: 0,
                        uv: sprite.uv,
                    },
                    size: [16.0; 2],
                    anchor: [8.0; 2],
                },
                if step % 2 == 0 { 20.0 } else { 0.0 },
                7,
            ));
            if step % 5 == 0 {
                let mut overlap = *layers.last().unwrap();
                if let WorldLayer::Sprite(sprite, _, _, _) = &mut overlap {
                    sprite.color = [0.5, 1.0, 0.5, 1.0];
                }
                layers.push(overlap);
            }
        }
        if step == 8 {
            atlas[..4].copy_from_slice(&[240, 80, 20, 180]);
            cached.invalidate();
        }
        let light = if step < 10 { 1.0 } else { 0.4 };
        canvas_depth::render_canvas_world_with_fog(
            &canvas,
            &context,
            &atlas,
            &mut cached,
            &layers,
            camera,
            false,
            step > 12,
            &|_| light,
        )
        .unwrap();
        let mut fresh = CanvasPresentation::new(128, 128);
        canvas_depth::render_canvas_world_with_fog(
            &canvas,
            &context,
            &atlas,
            &mut fresh,
            &layers,
            camera,
            false,
            step > 12,
            &|_| light,
        )
        .unwrap();
        assert_eq!(cached.color_buffer, fresh.color_buffer, "frame {step}");
        assert_eq!(cached.depth_buffer, fresh.depth_buffer, "frame {step}");
    }
}
