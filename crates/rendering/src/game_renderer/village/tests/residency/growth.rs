use super::*;

async fn pause() {
    let pause = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(pause).await.unwrap();
}

async fn draw(
    renderer: &mut crate::GameRenderer,
    art: &crate::GameArt,
    actor: SceneUnit,
    camera: SceneCamera,
) {
    for _ in 0..500 {
        if renderer
            .render_world(art, &[], &[], &[actor], camera, 0, false)
            .unwrap()
        {
            return;
        }
        pause().await;
    }
    panic!(
        "{} did not present within the bounded queue wait",
        renderer.backend()
    );
}

async fn grow(renderer: &mut crate::GameRenderer, pages: u32) {
    for _ in 0..500 {
        if renderer.atlas_growth_ready().unwrap() {
            renderer.grow_game_atlas(pages).unwrap();
            return;
        }
        pause().await;
    }
    panic!("atlas growth did not reach its bounded idle point");
}

async fn pixel(renderer: &crate::GameRenderer, canvas: &web_sys::HtmlCanvasElement) -> Vec<u8> {
    match renderer {
        crate::GameRenderer::Canvas { context, .. } => {
            context
                .get_image_data(64.0, 64.0, 1.0, 1.0)
                .unwrap()
                .data()
                .0
        }
        crate::GameRenderer::WebGl(_) => native_patch_gl_pixel(canvas),
        crate::GameRenderer::WebGpu(gpu) => {
            crate::web::tests::terrain_blend::read_pixel(gpu, 2, [64, 64])
                .await
                .to_vec()
        }
    }
}

#[wasm_bindgen_test]
async fn native_atlas_growth_preserves_colour_owner_masks_and_accepts_new_pairs_on_every_backend() {
    for backend in ["canvas", "webgl2", "webgpu"] {
        let document = web_sys::window().unwrap().document().unwrap();
        let canvas: web_sys::HtmlCanvasElement = document
            .create_element("canvas")
            .unwrap()
            .dyn_into()
            .unwrap();
        canvas.set_width(128);
        canvas.set_height(128);
        document.body().unwrap().append_child(&canvas).unwrap();
        let (mut renderer, canvas) = crate::GameRenderer::new_with_backend(canvas, Some(backend))
            .await
            .unwrap();
        let mut upload = renderer.begin_game_atlas(10).unwrap();
        for layer in 0..10 {
            upload.page_buffer(layer).unwrap();
            renderer.write_game_atlas_page(&mut upload, layer).unwrap();
        }
        renderer.finish_game_atlas(upload).unwrap();
        renderer
            .update_game_atlas(8, [2, 2], [1, 1], &[150, 80, 30, 255])
            .unwrap();
        renderer
            .update_game_atlas(9, [2, 2], [1, 1], &[7, 255, 255, 0])
            .unwrap();
        let mut art = art();
        let mut frame = art.village[1][0];
        frame.size = [32.0; 2];
        frame.anchor = [16.0; 2];
        frame.stream = 1;
        art.actors = vec![Vec::new(); 167];
        art.actors[0] = vec![frame; 16];
        art.resident = vec![Some(crate::AtlasAddress {
            page: 8,
            uv: [2.0 / 2048.0, 2.0 / 2048.0, 1.0 / 2048.0, 1.0 / 2048.0],
        })];
        let camera = SceneCamera {
            viewport: [128.0; 2],
            ..camera()
        };
        let mut actor = unit(SceneUnitKind::actor(0));
        actor.visual =
            crate::SceneUnitVisual::new(false).with_owner(Some([65, 145, 245]), true, false);
        draw(&mut renderer, &art, actor, camera).await;
        assert_eq!(
            pixel(&renderer, &canvas).await,
            [65, 145, 245, 255],
            "{backend} before"
        );
        let before = renderer.memory_usage();
        grow(&mut renderer, 12).await;
        let after = renderer.memory_usage();
        let index = if backend == "canvas" { 2 } else { 0 };
        assert_eq!(
            after[index].1,
            before[index].1 + 2 * crate::GAME_ATLAS_PAGE_BYTES,
            "{backend} memory"
        );
        draw(&mut renderer, &art, actor, camera).await;
        assert_eq!(
            pixel(&renderer, &canvas).await,
            [65, 145, 245, 255],
            "{backend} retained pair"
        );
        renderer
            .update_game_atlas(10, [2, 2], [1, 1], &[255, 0, 0, 255])
            .unwrap();
        art.resident[0].as_mut().unwrap().page = 10;
        draw(&mut renderer, &art, actor, camera).await;
        assert_eq!(
            pixel(&renderer, &canvas).await,
            [255, 0, 0, 255],
            "{backend} new pair"
        );
        renderer.grow_game_atlas(12).unwrap();
        for pages in [0, 9, 10, 13, crate::MAX_GAME_ATLAS_PAGES + 2] {
            assert!(
                renderer.grow_game_atlas(pages).is_err(),
                "{backend}: {pages}"
            );
        }
        if let crate::GameRenderer::WebGpu(gpu) = &renderer {
            gpu.device.destroy();
        }
        canvas.remove();
    }
}
