use super::*;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen(inline_js = "
const observations = new WeakMap();
export function observe_patch_errors(bridge, fail) {
 const gl = bridge.gl, query = gl.getError.bind(gl), patch = gl.texSubImage3D.bind(gl);
 const state = { calls: 0, error: 0, query: gl.getError, patch: gl.texSubImage3D };
 observations.set(bridge, state);
 gl.getError = () => { state.calls++; state.error = query(); return state.error; };
 if (fail) gl.texSubImage3D = (...args) => {
  patch(...args);
  gl.bindTexture(gl.TEXTURE_2D_ARRAY, null);
  patch(...args);
 };
}
export async function wait_submitted_frame(bridge) {
 const gl = bridge.gl, deadline = performance.now() + 2000;
 gl.finish();
 for (;;) {
  // WebGL publishes fence completion after returning to the browser loop.
  await new Promise(resolve => setTimeout(resolve, 0));
  if (bridge.pendingFrames.every(sync => {
   const status = gl.clientWaitSync(sync, 0, 0);
   return status === gl.ALREADY_SIGNALED || status === gl.CONDITION_SATISFIED;
  })) return;
  if (performance.now() > deadline) throw new Error('test frame fence did not complete');
 }
}
export function inject_completed_error(bridge) {
 const gl = bridge.gl;
 gl.bindTexture(gl.TEXTURE_2D_ARRAY, null);
 gl.texSubImage3D(gl.TEXTURE_2D_ARRAY, 0, 0, 0, 0, 1, 1, 1,
  gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array(4));
}
export function hold_fences(bridge) {
 const gl = bridge.gl, wait = gl.clientWaitSync;
 gl.clientWaitSync = () => gl.TIMEOUT_EXPIRED;
 return () => { gl.clientWaitSync = wait; };
}
export function finish_patch_observation(bridge) {
 const state = observations.get(bridge), gl = bridge.gl;
 gl.getError = state.query; gl.texSubImage3D = state.patch;
 observations.delete(bridge); return new Uint32Array([state.calls, state.error]);
}
")]
extern "C" {
    fn wait_submitted_frame(bridge: &wasm_bindgen::JsValue) -> js_sys::Promise;
    fn hold_fences(bridge: &wasm_bindgen::JsValue) -> js_sys::Function;
    fn inject_completed_error(bridge: &wasm_bindgen::JsValue);
    fn observe_patch_errors(bridge: &wasm_bindgen::JsValue, fail: bool);
    fn finish_patch_observation(bridge: &wasm_bindgen::JsValue) -> Vec<u32>;
}

async fn renderer() -> (crate::GameRenderer, web_sys::HtmlCanvasElement) {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas: web_sys::HtmlCanvasElement = document
        .create_element("canvas")
        .unwrap()
        .dyn_into()
        .unwrap();
    document.body().unwrap().append_child(&canvas).unwrap();
    let (mut renderer, canvas) = crate::GameRenderer::new_with_backend(canvas, Some("webgl2"))
        .await
        .unwrap();
    let mut upload = renderer.begin_game_atlas(10).unwrap();
    for layer in 0..10 {
        upload.page_buffer(layer).unwrap();
        renderer.write_game_atlas_page(&mut upload, layer).unwrap();
    }
    renderer.finish_game_atlas(upload).unwrap();
    (renderer, canvas)
}

fn health(renderer: &crate::GameRenderer) -> Result<(), String> {
    let crate::GameRenderer::WebGl(gl) = renderer else {
        panic!("expected WebGL2");
    };
    gl.check_health()
}

#[wasm_bindgen_test]
async fn native_patch_queries_errors_once_after_upload_and_reports_failure() {
    let (mut renderer, canvas) = renderer().await;
    let crate::GameRenderer::WebGl(gl) = &renderer else {
        panic!("expected WebGL2");
    };
    let bridge = gl.test_bridge().clone();
    observe_patch_errors(&bridge, false);
    renderer
        .update_game_atlas(8, [2, 2], [1, 1], &[100, 80, 20, 255])
        .unwrap();
    assert_eq!(
        finish_patch_observation(&bridge),
        [1, 0],
        "one completed upload needs one driver error query, not a second round trip before it"
    );
    observe_patch_errors(&bridge, true);
    let error = renderer
        .update_game_atlas(8, [2, 2], [1, 1], &[100, 80, 20, 255])
        .unwrap_err();
    assert!(error.contains("WebGL2 graphics"), "{error}");
    assert_eq!(finish_patch_observation(&bridge), [1, 1282]);
    canvas.remove();
}

#[wasm_bindgen_test]
async fn native_patch_burst_checks_once_and_finishes_failed_batches() {
    let (mut renderer, canvas) = renderer().await;
    let crate::GameRenderer::WebGl(gl) = &renderer else {
        panic!("expected WebGL2");
    };
    let bridge = gl.test_bridge().clone();
    observe_patch_errors(&bridge, false);
    let result: Result<(), String> = renderer.with_game_atlas_updates(|renderer| {
        for x in 0..32 {
            renderer.update_game_atlas(8, [x, 2], [1, 1], &[90, 80, 70, 255])?;
        }
        Ok(())
    });
    result.unwrap();
    assert_eq!(finish_patch_observation(&bridge), [1, 0]);

    observe_patch_errors(&bridge, true);
    let result: Result<(), String> = renderer.with_game_atlas_updates(|renderer| {
        renderer.update_game_atlas(8, [2, 2], [1, 1], &[90, 80, 70, 255])
    });
    assert!(result.is_err(), "the burst must report its driver error");
    assert_eq!(finish_patch_observation(&bridge), [1, 1282]);

    observe_patch_errors(&bridge, false);
    let result: Result<(), String> = renderer.with_game_atlas_updates(|renderer| {
        renderer.update_game_atlas(10, [0, 0], [1, 1], &[0; 4])
    });
    assert!(result.is_err());
    assert_eq!(finish_patch_observation(&bridge), [0, 0]);
    let result: Result<(), String> = renderer.with_game_atlas_updates(|renderer| {
        renderer.update_game_atlas(8, [2, 2], [1, 1], &[90, 80, 70, 255])
    });
    result.expect("a failed closure must release its batch");
    canvas.remove();
}

#[wasm_bindgen_test]
async fn webgl_submission_defers_driver_queries_until_completion_and_latches_errors() {
    let (mut renderer, canvas) = renderer().await;
    let crate::GameRenderer::WebGl(gl) = &renderer else {
        panic!("expected WebGL2");
    };
    let bridge = gl.test_bridge().clone();
    let draw = |renderer: &mut crate::GameRenderer| {
        renderer.render_world(
            &art(),
            &[],
            &[],
            &[unit(SceneUnitKind::actor(0))],
            SceneCamera {
                viewport: [128.0; 2],
                ..camera()
            },
            0,
            false,
        )
    };
    assert!(draw(&mut renderer).unwrap());
    super::native_patch_gl_pixel(&canvas);
    wasm_bindgen_futures::JsFuture::from(wait_submitted_frame(&bridge))
        .await
        .unwrap();
    health(&renderer).unwrap();
    observe_patch_errors(&bridge, false);
    assert!(draw(&mut renderer).unwrap());
    assert_eq!(finish_patch_observation(&bridge), [0, 0]);
    super::native_patch_gl_pixel(&canvas);
    wasm_bindgen_futures::JsFuture::from(wait_submitted_frame(&bridge))
        .await
        .unwrap();
    observe_patch_errors(&bridge, false);
    health(&renderer).unwrap();
    assert_eq!(finish_patch_observation(&bridge), [1, 0]);

    assert!(draw(&mut renderer).unwrap());
    super::native_patch_gl_pixel(&canvas);
    wasm_bindgen_futures::JsFuture::from(wait_submitted_frame(&bridge))
        .await
        .unwrap();
    inject_completed_error(&bridge);
    observe_patch_errors(&bridge, false);
    assert!(health(&renderer).is_err());
    assert!(
        health(&renderer).is_err(),
        "a failed submitted frame stays failed"
    );
    assert_eq!(finish_patch_observation(&bridge), [1, 1282]);
    canvas.remove();
}

#[wasm_bindgen_test]
async fn pending_frame_error_stays_latched_through_atlas_patch() {
    let (mut renderer, canvas) = renderer().await;
    let crate::GameRenderer::WebGl(gl) = &renderer else {
        panic!("expected WebGL2");
    };
    let bridge = gl.test_bridge().clone();
    assert!(
        renderer
            .render_world(
                &art(),
                &[],
                &[],
                &[unit(SceneUnitKind::actor(0))],
                SceneCamera {
                    viewport: [128.0; 2],
                    ..camera()
                },
                0,
                false,
            )
            .unwrap()
    );
    let release = hold_fences(&bridge);
    inject_completed_error(&bridge);
    renderer
        .update_game_atlas(8, [2, 2], [1, 1], &[100, 80, 20, 255])
        .expect("an error from the unfinished frame is not an atlas failure");
    assert!(
        health(&renderer).is_err(),
        "the frame error must stay latched"
    );
    release.call0(&wasm_bindgen::JsValue::NULL).unwrap();
    canvas.remove();
}
