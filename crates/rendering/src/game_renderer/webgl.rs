//! WebGL2 middle tier sharing the WebGPU sprite ABI and depth normalization.
//! The parent must replace a context-bound canvas before Canvas 2D recovery.
use crate::web::{Sprite, normalize_depths};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

const MAX_INSTANCE_BYTES: usize = 64 * 1024 * 1024;
const INSTANCE_BYTES: usize = std::mem::size_of::<Sprite>();
const _: () = assert!(INSTANCE_BYTES == 112);

// Local modules are copied by wasm-bindgen into pkg/snippets, served by the
// existing web static-file service. No wgpu GL feature or web-sys GL features.
#[wasm_bindgen(module = "/src/game_renderer/webgl/bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = AoeWebGl)]
    type GlBridge;
    #[wasm_bindgen(constructor, catch, js_class = AoeWebGl)]
    fn create(canvas: &HtmlCanvasElement, sources: &JsValue) -> Result<GlBridge, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn upload(this: &GlBridge, pixels: &[u8], memory: &JsValue) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn patch(
        this: &GlBridge,
        page: u32,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn resize(this: &GlBridge, width: u32, height: u32) -> Result<(), JsValue>;
    #[wasm_bindgen(method, catch)]
    fn render(
        this: &GlBridge,
        instances: &[u8],
        black: bool,
        effects: bool,
    ) -> Result<bool, JsValue>;
    #[wasm_bindgen(method)]
    fn dispose(this: &GlBridge);
    #[wasm_bindgen(method, getter, js_name = bufferBytes)]
    fn buffer_bytes(this: &GlBridge) -> usize;
    #[wasm_bindgen(method, getter, js_name = textureBytes)]
    fn texture_bytes(this: &GlBridge) -> usize;
}

#[wasm_bindgen(module = "/src/game_renderer/webgl/shaders.js")]
extern "C" {
    #[wasm_bindgen(js_name = terrainShaders)]
    fn terrain_shaders() -> JsValue;
}

#[wasm_bindgen(module = "/src/game_renderer/webgl/coverage.js")]
extern "C" {
    #[wasm_bindgen(js_name = coverageShaders)]
    fn coverage_shaders(sources: &JsValue) -> JsValue;
}

pub struct WebGlRenderer {
    bridge: GlBridge,
    source_atlas: Vec<u8>,
}

impl WebGlRenderer {
    pub(super) fn update(
        &mut self,
        page: u32,
        origin: [u32; 2],
        size: [u32; 2],
        pixels: &[u8],
    ) -> Result<(), String> {
        super::atlas_upload::patch::copy(&mut self.source_atlas, page, origin, size, pixels)?;
        self.bridge
            .patch(page, origin[0], origin[1], size[0], size[1], pixels)
            .map_err(error)
    }
    pub(super) fn buffer_bytes(&self) -> usize {
        self.bridge.buffer_bytes()
    }

    pub(super) fn texture_bytes(&self) -> usize {
        self.bridge.texture_bytes()
    }

    pub(super) fn source_bytes(&self) -> usize {
        self.source_atlas.capacity()
    }

    pub(super) fn new(canvas: &HtmlCanvasElement) -> Result<Self, String> {
        Ok(Self {
            bridge: GlBridge::create(canvas, &coverage_shaders(&terrain_shaders()))
                .map_err(error)?,
            source_atlas: Vec::new(),
        })
    }

    #[cfg(test)]
    pub(super) fn upload(&mut self, pixels: &[u8]) -> Result<(), String> {
        self.upload_owned(pixels.to_vec())
    }

    pub(super) fn upload_owned(&mut self, pixels: Vec<u8>) -> Result<(), String> {
        if crate::atlas_pages(pixels.len()).is_none() {
            return Err("Invalid WebGL2 game atlas size".to_owned());
        }
        // The synchronous import borrows a WASM view. Keep its allocation alive
        // for restoration; JS refreshes the view if linear memory grows.
        self.bridge
            .upload(&pixels, &wasm_bindgen::memory())
            .map_err(error)?;
        self.source_atlas = pixels;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn test_bridge(&self) -> &JsValue {
        self.bridge.as_ref()
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) -> Result<(), String> {
        self.bridge.resize(width, height).map_err(error)
    }

    /// Normalizes caller-owned depths in place without changing layer order.
    /// Pass fresh world depths, not instances normalized by an earlier call.
    /// False means no frame was submitted (zero backing size or lost context).
    pub(super) fn render(&mut self, sprites: &mut [Sprite]) -> Result<bool, String> {
        self.render_with_clear(sprites, false)
    }

    pub(super) fn render_with_clear(
        &mut self,
        sprites: &mut [Sprite],
        black: bool,
    ) -> Result<bool, String> {
        sprites
            .len()
            .checked_mul(INSTANCE_BYTES)
            .filter(|bytes| *bytes <= MAX_INSTANCE_BYTES)
            .ok_or_else(|| "WebGL2 instance buffer exceeds 64 MiB".to_owned())?;
        let effects = normalize_depths(sprites);
        // wasm-bindgen borrows a memory view for this synchronous import.
        // The bridge uploads immediately and never retains the packet or awaits.
        // The bridge independently rejects oversized packets before allocating
        // exactly their byte length; no extra capacity or retained CPU packet.
        self.bridge
            .render(bytemuck::cast_slice(sprites), black, effects)
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
