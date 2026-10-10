use super::*;

// Yield to the browser event loop so asynchronous WebGPU notifications land.
async fn next_task() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .expect("browser window")
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10)
            .expect("timer");
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("timer promise");
}

#[wasm_bindgen_test]
async fn webgpu_destroyed_device_is_reported_instead_of_a_silent_present() {
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .expect("browser document")
        .create_element("canvas")
        .expect("canvas element")
        .dyn_into::<web_sys::HtmlCanvasElement>()
        .expect("canvas type");
    canvas.set_width(16);
    canvas.set_height(16);
    let mut renderer = Renderer::new(canvas)
        .await
        .expect("software WebGPU renderer");
    let label = renderer.adapter_label();
    assert!(label.starts_with("BrowserWebGpu"));
    assert!(
        !label.ends_with(": "),
        "empty adapter description: {label:?}"
    );
    assert!(
        renderer
            .render_sprites(&[])
            .expect("live frame")
            .did_present
    );
    renderer.device.destroy();
    // Bounded wait: the lost promise resolves asynchronously, never per frame.
    let mut failure = None;
    for _ in 0..100 {
        next_task().await;
        if let Err(message) = renderer.render_sprites(&[]) {
            failure = Some(message);
            break;
        }
    }
    let message = failure.expect("device loss must fail the next frame");
    assert!(message.contains("device lost"), "{message}");
    assert!(message.contains("reload"), "{message}");
}
