//! Opaque graphics-only imports. Rust remains the sole atlas-admission owner.
use js_sys::Promise;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

#[wasm_bindgen(module = "/src/web/bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = AoeWebGpu)]
    pub(crate) type GpuBridge;
    #[wasm_bindgen(js_name = createWebGpu, catch)]
    pub(crate) fn create(
        canvas: &HtmlCanvasElement,
        shader: &str,
        capacity: u32,
    ) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, getter, js_name = adapterLabel)]
    pub(crate) fn adapter_label(this: &GpuBridge) -> String;
    #[cfg(test)]
    #[wasm_bindgen(method, getter, js_name = format)]
    pub(crate) fn format(this: &GpuBridge) -> String;
    #[wasm_bindgen(method, catch, js_name = ensureCapacity)]
    pub(crate) fn ensure_capacity(this: &GpuBridge, required: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = uploadAtlas)]
    pub(crate) fn upload_atlas(this: &GpuBridge, pixels: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    pub(crate) fn resize(this: &GpuBridge, width: u32, height: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch, js_name = renderPreparedInstances)]
    pub(crate) fn render(this: &GpuBridge, bytes: &[u8], clear: &[f64]) -> Result<bool, JsValue>;
    #[cfg(test)]
    #[wasm_bindgen(method, js_name = destroyDevice)]
    pub(crate) fn destroy(this: &GpuBridge);
    #[wasm_bindgen(method)]
    pub(crate) fn dispose(this: &GpuBridge);

    #[cfg(test)]
    #[wasm_bindgen(method, catch, js_name = createTestReadback)]
    pub(crate) fn create_readback(this: &GpuBridge, count: u32) -> Result<TestReadback, JsValue>;
}

// The reader is a plain object, not an exported bridge-module class.
#[cfg(test)]
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = Object)]
    pub(crate) type TestReadback;
    #[wasm_bindgen(method, catch)]
    pub(crate) fn read(
        this: &TestReadback,
        count: u32,
        points: &js_sys::Array,
        clear: &[f64],
    ) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, js_name = dispose)]
    pub(crate) fn dispose_readback(this: &TestReadback);
}

pub(crate) fn error(value: JsValue) -> String {
    value.as_string().unwrap_or_else(|| {
        js_sys::Reflect::get(&value, &JsValue::from_str("message"))
            .ok()
            .and_then(|message| message.as_string())
            .unwrap_or_else(|| "WebGPU graphics operation failed".to_owned())
    })
}
