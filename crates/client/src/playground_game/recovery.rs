//! A new document releases failed GPU resources and rebinds every input handler.
use aoe_core::Camera;
use aoe_rendering::GameRenderer;
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};

#[wasm_bindgen(module = "/src/playground_game/recovery.js")]
extern "C" {
    pub(super) fn report();
    #[wasm_bindgen(catch)]
    fn restart_renderer(reason: &str, view: &[f64]) -> Result<(), JsValue>;
}

pub(super) fn check(renderer: &GameRenderer) -> Result<(), String> {
    if let GameRenderer::WebGpu(renderer) = renderer {
        if renderer.page_retired() {
            return Err("WebGPU document retired".into());
        }
        renderer.check_health()?;
    }
    Ok(())
}

pub(super) fn restart(
    renderer: &GameRenderer,
    camera: Option<Camera>,
    reason: &str,
) -> Result<bool, JsValue> {
    let GameRenderer::WebGpu(renderer) = renderer else {
        return Ok(false);
    };
    // A normal navigation owns the next document. Never replace it with a
    // recovery reload when pagehide deliberately destroys the old device.
    if renderer.page_retired() {
        return Ok(true);
    }
    let view = camera.map(|c| [c.center[0], c.center[1], c.zoom]);
    renderer.shutdown();
    // Keep URL/DOM orchestration in the browser: Rust float-to-decimal
    // formatting alone would retain ~15 KiB of formatter functions in WASM.
    restart_renderer(reason, view.as_ref().map_or(&[], |v| v.as_slice()))?;
    Ok(true)
}
