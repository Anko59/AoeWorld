//! WebGL2 middle tier sharing the WebGPU sprite ABI and depth normalization.
//! The parent must replace a context-bound canvas before Canvas 2D recovery.
use crate::web::{Sprite, normalize_depths};
use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

const MAX_INSTANCE_BYTES: usize = 64 * 1024 * 1024;
const INSTANCE_BYTES: usize = std::mem::size_of::<Sprite>();
const _: () = assert!(INSTANCE_BYTES == 96);

// Local modules are copied by wasm-bindgen into pkg/snippets, served by the
// existing web static-file service. No wgpu GL feature or web-sys GL features.
#[wasm_bindgen(module = "/src/game_renderer/webgl/bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = AoeWebGl)]
    type GlBridge;
    #[wasm_bindgen(constructor, catch, js_class = AoeWebGl)]
    fn create(canvas: &HtmlCanvasElement) -> Result<GlBridge, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn upload(this: &GlBridge, pixels: &Uint8Array) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn resize(this: &GlBridge, width: u32, height: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn render(this: &GlBridge, instances: &[u8]) -> Result<bool, JsValue>;
    #[wasm_bindgen(method)]
    fn dispose(this: &GlBridge);
}

pub struct WebGlRenderer {
    bridge: GlBridge,
}

impl WebGlRenderer {
    pub(super) fn new(canvas: &HtmlCanvasElement) -> Result<Self, String> {
        Ok(Self {
            bridge: GlBridge::create(canvas).map_err(error)?,
        })
    }

    pub(super) fn upload(&mut self, pixels: &[u8]) -> Result<(), String> {
        if pixels.len() != (crate::GAME_ATLAS_SIDE * crate::GAME_ATLAS_SIDE * 4) as usize {
            return Err("Invalid WebGL2 game atlas size".to_owned());
        }
        self.bridge.upload(&Uint8Array::from(pixels)).map_err(error)
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.bridge.resize(width, height).map_err(error)
    }

    /// Normalizes caller-owned depths in place without changing layer order.
    /// Pass fresh world depths, not instances normalized by an earlier call.
    /// False means no frame was submitted (zero backing size or lost context).
    pub(super) fn render(&mut self, sprites: &mut [Sprite]) -> Result<bool, String> {
        sprites
            .len()
            .checked_mul(INSTANCE_BYTES)
            .filter(|bytes| *bytes <= MAX_INSTANCE_BYTES)
            .ok_or_else(|| "WebGL2 instance buffer exceeds 64 MiB".to_owned())?;
        normalize_depths(sprites);
        // wasm-bindgen borrows a memory view for this synchronous import.
        // The bridge uploads immediately and never retains the packet or awaits.
        // The bridge independently rejects oversized packets before allocating
        // exactly their byte length; no extra capacity or retained CPU packet.
        self.bridge
            .render(bytemuck::cast_slice(sprites))
            .map_err(error)
    }
}

impl Drop for WebGlRenderer {
    fn drop(&mut self) {
        self.bridge.dispose();
    }
}

fn error(value: JsValue) -> String {
    value
        .as_string()
        .unwrap_or_else(|| "WebGL2 graphics operation failed".to_owned())
}
