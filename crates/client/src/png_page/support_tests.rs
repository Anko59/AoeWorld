//! Adapters exercise the production JavaScript decoder with the existing Rust fixtures.
use js_sys::{Promise, Uint8Array};
use wasm_bindgen::{JsValue, prelude::wasm_bindgen};
use wasm_bindgen_futures::JsFuture;
pub(super) const SIDE: usize = 2048;
pub(super) const PIXEL_BYTES: usize = SIDE * SIDE * 4;
pub(super) const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
pub(super) const ADAM7: [Pass; 7] = [
    Pass::new(0, 0, 8, 8),
    Pass::new(4, 0, 8, 8),
    Pass::new(0, 4, 4, 8),
    Pass::new(2, 0, 4, 4),
    Pass::new(0, 2, 2, 4),
    Pass::new(1, 0, 2, 2),
    Pass::new(0, 1, 1, 2),
];

#[derive(Clone, Copy)]
pub(super) struct Pass {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) step_x: usize,
    pub(super) step_y: usize,
}

impl Pass {
    const fn new(x: usize, y: usize, step_x: usize, step_y: usize) -> Self {
        Self {
            x,
            y,
            step_x,
            step_y,
        }
    }
}

#[wasm_bindgen(module = "/src/png_page/decoder.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = validate_header)]
    pub(super) fn validate_header(bytes: &[u8]) -> Result<(), JsValue>;
    #[wasm_bindgen(catch, js_name = filtered_size)]
    fn size_js(width: usize, height: usize, interlaced: bool) -> Result<usize, JsValue>;
    #[wasm_bindgen(catch, js_name = reconstruct)]
    fn reconstruct_js(
        bytes: &[u8],
        width: usize,
        height: usize,
        interlaced: bool,
    ) -> Result<Uint8Array, JsValue>;
    #[wasm_bindgen(js_name = inflate)]
    fn inflate_js(bytes: &[u8], limit: usize) -> Promise;
}
pub(super) fn filtered_size(
    width: usize,
    height: usize,
    interlaced: bool,
) -> Result<usize, JsValue> {
    size_js(width, height, interlaced)
}
pub(super) fn pass_size(length: usize, start: usize, step: usize) -> usize {
    if length <= start {
        0
    } else {
        (length - start).div_ceil(step)
    }
}
pub(super) fn reconstruct(
    bytes: &[u8],
    width: usize,
    height: usize,
    interlaced: bool,
) -> Result<Vec<u8>, JsValue> {
    reconstruct_js(bytes, width, height, interlaced).map(|bytes| bytes.to_vec())
}
pub(super) async fn inflate(bytes: &[u8], limit: usize) -> Result<Vec<u8>, JsValue> {
    JsFuture::from(inflate_js(bytes, limit))
        .await
        .map(|value| Uint8Array::new(&value).to_vec())
}
