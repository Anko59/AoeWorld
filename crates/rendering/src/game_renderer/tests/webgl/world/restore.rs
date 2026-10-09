//! Real browser context loss/restoration retains typed pixels and Rust admission.
use super::*;

#[wasm_bindgen(inline_js = "
export async function lose_typed_context(canvas, bridge) {
    const gl = canvas.getContext('webgl2');
    const extension = gl.getExtension('WEBGL_lose_context');
    if (!extension) throw new Error('actual context-loss extension unavailable');
    const owner = bridge.atlasPixels;
    await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error('context loss event timeout')), 2500);
        canvas.addEventListener('webglcontextlost', () => {
            clearTimeout(timeout); resolve();
        }, {once: true});
        extension.loseContext();
    });
    return {extension, owner};
}
export async function restore_typed_context(canvas, bridge, token) {
    await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error('context restoration event timeout')), 2500);
        canvas.addEventListener('webglcontextrestored', () => {
            clearTimeout(timeout);
            if (bridge.atlasPixels !== token.owner) reject(new Error('restoration changed atlas owner'));
            else if (bridge.restoreError) reject(bridge.restoreError);
            else resolve();
        }, {once: true});
        // Restoration becomes legal after the lost-event dispatch completes.
        setTimeout(() => token.extension.restoreContext(), 0);
    });
}
")]
extern "C" {
    #[wasm_bindgen(catch)]
    async fn lose_typed_context(
        canvas: &HtmlCanvasElement,
        bridge: &JsValue,
    ) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch)]
    async fn restore_typed_context(
        canvas: &HtmlCanvasElement,
        bridge: &JsValue,
        token: &JsValue,
    ) -> Result<JsValue, JsValue>;
}

#[wasm_bindgen_test]
async fn typed_world_context_events_restore_same_pixels_and_private_capability() {
    let data = fixture::Fixture::new();
    let constructed = data.constructed();
    let canvas = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .create_element("canvas")
        .unwrap()
        .dyn_into::<HtmlCanvasElement>()
        .unwrap();
    canvas.set_width(128);
    canvas.set_height(128);
    let mut renderer = WebGlRenderer::new(&canvas).unwrap();
    renderer
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    let key = data.art.terrain_world;
    let case = fixture::cases()[4];
    let faces = fixture::faces(&data.art, case, false);
    let expected = draw_world(&canvas, &mut renderer, &faces, fixture::probes(case));
    let token = lose_typed_context(&canvas, renderer.test_bridge())
        .await
        .unwrap();
    assert_eq!(renderer.world_atlas, key);
    let mut packets = faces.iter().map(instance).collect::<Vec<_>>();
    assert!(!renderer.render(&mut packets).unwrap());
    restore_typed_context(&canvas, renderer.test_bridge(), &token)
        .await
        .unwrap();
    assert_eq!(renderer.world_atlas, key);
    let restored = draw_world(&canvas, &mut renderer, &faces, fixture::probes(case));
    assert_eq!(restored, expected);

    // A newly constructed backend cannot inherit a prior context's capability.
    drop(renderer);
    let mut replacement = WebGlRenderer::new(&canvas).unwrap();
    assert_eq!(replacement.world_atlas, None);
    replacement.upload(&data.atlas).unwrap();
    assert_eq!(replacement.world_atlas, None);
    let raw = draw_world(&canvas, &mut replacement, &faces, fixture::probes(case));
    let legacy = draw_world(
        &canvas,
        &mut replacement,
        &fixture::legacy(&faces),
        fixture::probes(case),
    );
    assert_eq!(raw, legacy);
    replacement
        .upload_terrain_atlas(&constructed, &data.art)
        .unwrap();
    assert_eq!(replacement.world_atlas, key);
    assert_eq!(
        draw_world(&canvas, &mut replacement, &faces, fixture::probes(case)),
        expected
    );
}
